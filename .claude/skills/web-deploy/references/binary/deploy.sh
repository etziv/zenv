#!/bin/bash
# ============================================================================
# Deploy script - compiled web service (Rust/Axum or Go), Docker, BLUE-GREEN, zero downtime.
#
# Two colors of the app (app_blue, app_green) behind Caddy; only one serves.
#   1. git sync (container alpine/git, server needs no git) -> commit NEW_REV
#   2. image <PROJECT>:<NEW_REV>: already built by the ci `prebuild` job right after the merge
#      (`--build-only`), otherwise built now while the live color keeps serving
#   3. start the IDLE color on the new image, wait healthy, optional SMOKE_PATH on it
#      (nothing has changed for users yet: a failure here stops the idle color, exit 3)
#   4. switch: write upstream/active.caddy and `caddy reload` (graceful: in-flight requests finish)
#   5. health through the proxy (failure -> switch back), record state, then stop the old color
#      with SIGTERM (it finishes its in-flight requests)
# Rollback = the same flow with the previous image: no rebuild, seconds.
#
# Requirements in the app: GET /health returns 200 when ready, OUTSIDE any
# host-check middleware (the proxy calls it by container name), and graceful
# shutdown on SIGTERM (Rust: axum::serve(...).with_graceful_shutdown; Go:
# http.Server.Shutdown on SIGTERM; see SKILL.md).
# docker/Dockerfile = references/binary/Dockerfile.rust or Dockerfile.go from the kit.
# Migrations must stay compatible with the previous release (both run around
# the switch); destructive changes ship in two deploys (expand -> contract).
#
# Exit codes: 0 = deployed and healthy (or --build-only done) | 2 = unhealthy, needs attention |
# 3 = refused or incomplete, the previous version still serves.
#
# Flags:
#   --ref <sha>    deploy a specific commit on BRANCH
#   --rollback     run the image of the commit before the last deploy (no rebuild)
#   --no-fetch     skip git sync, use the source as it is
#   --build        force an image rebuild even if <PROJECT>:<sha> exists
#   --build-only   sync + build the image, touch nothing live (ci prebuild job)
#   --logs         tail logs after the deploy
# ============================================================================

set -Eeuo pipefail

# ===================== KONFIGURASI (EDIT BAGIAN INI) =======================
PROJECT="myapp"                 # slug (lower case, no spaces) - same as in docker-compose.yml
REPO="owner/repo"               # GitHub owner/repo
BRANCH="main"                   # branch to deploy
REQUIRED_ENV=""                 # app .env vars that must be set (space separated, names only)
# Rust: --build-arg BIN=<cargo bin> | Go: --build-arg PKG=./cmd/server. BUILD_JOBS caps the compiler's
# parallel jobs: the build runs next to production (and maybe its database), never on every core.
BUILD_ARGS="--build-arg BIN=app --build-arg BUILD_JOBS=4"
HEALTH_PATH="/health"           # checked inside the new color and through the proxy
KEEP_IMAGES=3                   # image tags kept for rollback
LOCK_WAIT=900                   # seconds a deploy waits for a running prebuild (and the reverse)
# Pinned by digest (the git image receives the token and writes the source). Newer: docker buildx imagetools
# inspect <image>:<tag> -> the sha256 of the index.
GIT_IMAGE="alpine/git:v2.54.0@sha256:ac6c828f15d82775d204d3d75112e1d34af8945da80ccf922713c9891ba68e7a"
export CADDY_IMAGE="caddy:2.11.7-alpine@sha256:e70b164da81ef698b3ad19af799ea6086ce6d9e48ed4079eb9fe26f6fbf14ee5"
# 0 = a changed docker/deploy.sh in the repo stops the deploy until the boss re-uploads it (one command,
# printed). 1 = it replaces itself and runs as root: then whoever can merge to BRANCH (an agent included) or
# write the source tree is root on this server. Keep 0 unless the server holds nothing worth protecting.
SELF_UPDATE=0
# ===========================================================================
# Command-line level config beats the repo's own .git/config (core.fsmonitor there would run a command).
GIT_SAFE=(-e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=core.fsmonitor -e GIT_CONFIG_VALUE_0=false)

WORK_DIR="/root/${PROJECT}"     # root-owned: deploy.sh, compose, .env, token, state
export SRC_DIR="/home/${PROJECT}/src"
export ENV_FILE="${WORK_DIR}/.env"          # app settings (compose env_file); no secret of the deploy itself
TOKEN_FILE="${WORK_DIR}/.deploy-token"      # read-only GitHub token, chmod 600, never in a container env
mkdir -p "$WORK_DIR/upstream"

DO_FETCH=1; FORCE_BUILD=0; BUILD_ONLY=0; TAIL_LOGS=0; REF=""; ROLLBACK=0; CADDY_CHANGED=0
ORIG_ARGS=("$@")
while (( $# )); do
    case "$1" in
        --ref)        REF="${2:-}"; shift ;;
        --rollback)   ROLLBACK=1 ;;
        --no-fetch)   DO_FETCH=0 ;;
        --build)      FORCE_BUILD=1 ;;
        --build-only) BUILD_ONLY=1 ;;
        --logs)       TAIL_LOGS=1 ;;
        *) echo "ERROR: unknown flag: $1"; exit 3 ;;
    esac
    shift
done
if [[ -n "$REF" && ! "$REF" =~ ^[0-9a-f]{7,40}$ ]]; then echo "ERROR: --ref must be a commit SHA: $REF"; exit 3; fi
if (( ROLLBACK && BUILD_ONLY )); then echo "ERROR: --rollback and --build-only exclude each other"; exit 3; fi

if command -v flock >/dev/null 2>&1 && [[ -z "${DEPLOY_LOCKED:-}" ]]; then
    exec 9>"$WORK_DIR/.deploy.lock"
    flock -w "$LOCK_WAIT" 9 || { echo "ERROR: another deploy or build of $PROJECT still runs after ${LOCK_WAIT}s."; exit 3; }
    export DEPLOY_LOCKED=1
fi

[[ -f "$ENV_FILE" ]] || { echo "ERROR: $ENV_FILE not found"; exit 3; }
if grep -q $'\r' "$ENV_FILE"; then echo "ERROR: $ENV_FILE uses CRLF. Run: sed -i 's/\r\$//' $ENV_FILE"; exit 3; fi
[[ "$(stat -c %a "$ENV_FILE")" == 600 ]] || { echo "ERROR: $ENV_FILE must be mode 600 (it holds secrets). Run: chmod 600 $ENV_FILE"; exit 3; }
missing=()
for var in $REQUIRED_ENV; do grep -qE "^${var}=.+" "$ENV_FILE" || missing+=("$var"); done
if (( ${#missing[@]} > 0 )) && ! (( ROLLBACK )); then
    echo "ERROR: empty in $ENV_FILE:"; printf '  - %s\n' "${missing[@]}"; exit 3
fi
# Until the switch, any unhandled failure leaves the live color serving: exit 3.
trap 'echo "ERROR: step failed at line $LINENO; the live version was not touched."; exit 3' ERR

COMPOSE_FILE="$WORK_DIR/docker-compose.yml"
COMPOSE="docker compose -p $PROJECT -f $COMPOSE_FILE --env-file $ENV_FILE"
state() { cat "$WORK_DIR/$1" 2>/dev/null || true; }
ACTIVE=$(state .deploy-color); [[ "$ACTIVE" == blue || "$ACTIVE" == green ]] || ACTIVE=""
# State file lost (a restore without dotfiles, a cleanup): the proxy's own upstream says which color is live,
# so the next deploy never recreates the serving one.
[[ -n "$ACTIVE" ]] || case "$(state upstream/active.caddy)" in
    "to app_blue:8080") ACTIVE=blue ;; "to app_green:8080") ACTIVE=green ;; esac
CUR_IMAGE=$(state .deploy-image)
# compose interpolates both colors' images on every call: give each its recorded image
export BLUE_IMAGE GREEN_IMAGE
BLUE_IMAGE=$(state .image-blue); GREEN_IMAGE=$(state .image-green)

# --- Rollback: previous image, no git sync, no build -----------------------
if (( ROLLBACK )); then
    TARGET_IMAGE=$(state .deploy-prev-image)
    [[ -n "$TARGET_IMAGE" ]] || { echo "ERROR: no previous deploy recorded."; exit 3; }
    docker image inspect "$TARGET_IMAGE" >/dev/null 2>&1 || { echo "ERROR: image $TARGET_IMAGE is gone; deploy that commit with --ref instead."; exit 3; }
    echo "==> ROLLBACK to $TARGET_IMAGE (migrations are NOT undone)."
    DO_FETCH=0
fi

# --- Git sync: alpine/git container, token from TOKEN_FILE -------------------
if (( DO_FETCH )); then
    TOKEN=$(tr -d '\r\n "' < "$TOKEN_FILE" 2>/dev/null || true)
    [[ -n "$TOKEN" ]] || { echo "ERROR: $TOKEN_FILE is missing or empty (or use --no-fetch)."; exit 3; }
    export GH_TOKEN="$TOKEN"    # passed by NAME below: the value is in no command line (ps) and no URL
    echo "==> Sync source from GitHub via git container..."
    docker run --rm -v "${SRC_DIR}:/src" -e GH_TOKEN -e GH_REPO="$REPO" "${GIT_SAFE[@]}" -e GH_BRANCH="$BRANCH" -e GH_REF="$REF" \
        --entrypoint sh "$GIT_IMAGE" -c '
            set -e
            git config --global --add safe.directory /src
            cd /src
            if [ ! -d .git ]; then git init -q; git remote add origin "https://github.com/${GH_REPO}.git"; fi
            # the helper reads $GH_TOKEN only when git asks: the token is in no URL and no argv
            git -c credential.helper= -c "credential.helper=!f() { echo username=x-access-token; echo \"password=\$GH_TOKEN\"; }; f" \
                fetch --quiet "https://github.com/${GH_REPO}.git" "${GH_BRANCH}:refs/remotes/origin/${GH_BRANCH}"
            if [ -n "$GH_REF" ]; then
                NEW_REV=$(git rev-parse --verify "${GH_REF}^{commit}" 2>/dev/null) || { echo "ERROR: commit $GH_REF not found"; exit 1; }
                git merge-base --is-ancestor "$NEW_REV" "refs/remotes/origin/${GH_BRANCH}" \
                    || { echo "ERROR: commit $GH_REF is not on ${GH_BRANCH}"; exit 1; }
            else
                NEW_REV=$(git rev-parse "refs/remotes/origin/${GH_BRANCH}")
            fi
            git reset --hard --quiet "$NEW_REV"
            git clean -fdq
            git log -1 --format="    Source: %h - %s (%cr)"
        ' || { echo "ERROR: git sync refused (bad --ref, or commit not on $BRANCH)."; exit 3; }
    unset GH_TOKEN TOKEN
fi

# --- docker/ files: self-update check, validate the Caddyfile, install -----
DOCKER_SRC="$SRC_DIR/docker"
# A rollback runs on the INSTALLED files: the source may hold a commit that was never deployed (a prebuild,
# a refused deploy), and a changed deploy.sh there must not block the emergency path.
if ! (( ROLLBACK )); then
    [[ -f "$DOCKER_SRC/docker-compose.yml" ]] || { echo "ERROR: $DOCKER_SRC/docker-compose.yml missing"; exit 3; }
    # root follows these paths (cp, cmp, docker build -f, bind mounts): a symlink could point at a root-only file.
    for f in "" deploy.sh docker-compose.yml Dockerfile Dockerfile.dockerignore Caddyfile; do
        [[ -L "$DOCKER_SRC${f:+/$f}" ]] && { echo "ERROR: docker/$f in the repo is a symlink; refused, nothing changed."; exit 3; }
    done
    # deploy.sh and the compose file both act as root here (compose decides privileged, volumes, pid): with
    # SELF_UPDATE=0 neither comes from the repo until the boss re-uploads it.
    if ! (( SELF_UPDATE || BUILD_ONLY )); then   # a build uses only the Dockerfile, no root config
        STALE=""; for f in deploy.sh docker-compose.yml; do cmp -s "$DOCKER_SRC/$f" "$WORK_DIR/$f" || STALE+=" $f"; done
        if [[ -n "$STALE" ]]; then
            echo "ERROR: in the repo,$STALE differs from $WORK_DIR (SELF_UPDATE=0). Review the change, then from the project folder:"
            for f in $STALE; do echo "  ssh root@<server> \"cat > $WORK_DIR/$f\" < docker/$f"; done
            exit 3
        fi
    fi
    if (( SELF_UPDATE )) && ! cmp -s "$DOCKER_SRC/deploy.sh" "$WORK_DIR/deploy.sh"; then
        echo "==> deploy.sh updated, re-exec..."
        cp "$DOCKER_SRC/deploy.sh" "$WORK_DIR/deploy.sh"; chmod 755 "$WORK_DIR/deploy.sh"
        REEXEC=(); for a in "${ORIG_ARGS[@]}"; do [[ "$a" == "--no-fetch" ]] || REEXEC+=("$a"); done
        exec "$WORK_DIR/deploy.sh" --no-fetch "${REEXEC[@]}"
    fi
fi
if ! (( BUILD_ONLY || ROLLBACK )); then
    cmp -s "$DOCKER_SRC/Caddyfile" "$WORK_DIR/Caddyfile" 2>/dev/null || CADDY_CHANGED=1
    # A broken Caddyfile would only show at the next proxy restart (a reboot): validate before installing it.
    if (( CADDY_CHANGED )); then
        CHK=$(mktemp -d); echo "to app_blue:8080" > "$CHK/active.caddy"
        docker run --rm -v "$DOCKER_SRC/Caddyfile:/etc/caddy/Caddyfile:ro" -v "$CHK:/etc/caddy/upstream:ro" "$CADDY_IMAGE" \
            caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null \
            || { rm -rf "$CHK"; echo "ERROR: the new Caddyfile does not validate; nothing changed."; exit 3; }
        rm -rf "$CHK"
    fi
    # cp (not mv): the Caddyfile is bind-mounted as a single file, its inode must stay the same.
    cp "$DOCKER_SRC/docker-compose.yml" "$DOCKER_SRC/Dockerfile" "$DOCKER_SRC/Caddyfile" "$WORK_DIR/"
fi

# --- Image for this commit (prebuilt by ci, or built now) --------------------
if (( ! ROLLBACK )); then
    REV=$(docker run --rm -v "${SRC_DIR}:/src" "${GIT_SAFE[@]}" --entrypoint sh "$GIT_IMAGE" -c \
        'git config --global --add safe.directory /src; cd /src; git rev-parse --short=12 HEAD 2>/dev/null || echo nogit')
    TARGET_IMAGE="${PROJECT}:${REV}"
    if (( FORCE_BUILD )) || ! docker image inspect "$TARGET_IMAGE" >/dev/null 2>&1; then
        echo "==> Build $TARGET_IMAGE (the live version keeps serving during the build)..."
        # The build context is the repo root: docker/Dockerfile.dockerignore (allow-list) keeps the rest out.
        # shellcheck disable=SC2086
        DOCKER_BUILDKIT=1 docker build $BUILD_ARGS -t "$TARGET_IMAGE" -f "$DOCKER_SRC/Dockerfile" "$SRC_DIR"
    else
        echo "==> Image $TARGET_IMAGE already exists -> no build."
    fi
fi
if (( BUILD_ONLY )); then echo "Build only: $TARGET_IMAGE ready, nothing live changed."; exit 0; fi

# --- Anything to do? ---------------------------------------------------------
ENV_KEYS_FILE="$WORK_DIR/.env.deployed-keys"
CUR_ENV_KEYS=$(grep -E '^[A-Za-z_][A-Za-z0-9_]*=' "$ENV_FILE" | while IFS='=' read -r k v; do
    printf '%s=%s\n' "$k" "$(printf '%s' "$v" | sha256sum | cut -c1-16)"; done | sort)
container_of() { { $COMPOSE ps -q "$1" 2>/dev/null || true; } | head -1; }
if [[ -n "$ACTIVE" && "$TARGET_IMAGE" == "$CUR_IMAGE" && "$(state .env.deployed-keys)" == "$CUR_ENV_KEYS" \
      && -n "$(container_of "app_$ACTIVE")" && -n "$(container_of proxy)" ]] && ! (( CADDY_CHANGED )); then
    echo "Already live: $TARGET_IMAGE on $ACTIVE, .env unchanged. Nothing to do."
    exit 0
fi

wait_healthy() {
    local id status
    for _ in $(seq 1 120); do
        id=$(container_of "$1")
        status=$( [[ -n "$id" ]] && docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$id" 2>/dev/null || echo missing)
        [[ "$status" == "healthy" ]] && return 0
        sleep 1
    done
    return 1
}
# Optional SMOKE_PATH in .env (a cheap read that touches the database; /health does not). A host-checked
# route: SMOKE_HOST = a host name the app allows (default localhost). It needs credentials? Feed them to curl
# on stdin from the container's own env (`printf 'user = "%s:%s"' ... | curl -K - ...`), never argv.
SMOKE_PATH=$( { grep "^SMOKE_PATH=" "$ENV_FILE" || true; } | cut -d= -f2- | tr -d '\r"')
SMOKE_HOST=$( { grep "^SMOKE_HOST=" "$ENV_FILE" || true; } | cut -d= -f2- | tr -d '\r"')
smoke() {
    [[ -z "$SMOKE_PATH" ]] && return 0
    docker exec "$(container_of "$1")" curl -fsS -o /dev/null -H "Host: ${SMOKE_HOST:-localhost}" "http://localhost:8080${SMOKE_PATH}"
}
# Point the proxy at one color: a new upstream file, then a graceful reload (or the first proxy start).
switch_to() {
    printf 'to app_%s:8080\n' "$1" > "$WORK_DIR/upstream/active.caddy.new"
    mv -f "$WORK_DIR/upstream/active.caddy.new" "$WORK_DIR/upstream/active.caddy"
    # up first: a no-op while the proxy's compose config is unchanged; a changed APP_BIND/APP_PORT or CADDY_IMAGE
    # recreates it (seconds without the proxy, the only restart a deploy can cause). Then a graceful reload.
    $COMPOSE up -d --no-deps proxy
    for _ in 1 2 3 4 5 6 7 8 9; do   # a just-started proxy needs a moment before its admin API answers
        $COMPOSE exec -T proxy caddy reload --config /etc/caddy/Caddyfile >/dev/null 2>&1 && return 0
        sleep 1
    done
    $COMPOSE exec -T proxy caddy reload --config /etc/caddy/Caddyfile
}
proxy_healthy() {
    for _ in $(seq 1 15); do
        $COMPOSE exec -T proxy wget -q -O /dev/null "http://localhost:8080${HEALTH_PATH}" 2>/dev/null && return 0
        sleep 2
    done
    return 1
}

# --- Start the idle color on the new image (users still on the live one) -----
if [[ "$ACTIVE" == blue ]]; then IDLE=green; else IDLE=blue; fi
echo "==> Start $IDLE on $TARGET_IMAGE (live: ${ACTIVE:-none})..."
if [[ "$IDLE" == blue ]]; then BLUE_IMAGE="$TARGET_IMAGE"; else GREEN_IMAGE="$TARGET_IMAGE"; fi
if ! { $COMPOSE up -d --no-deps --force-recreate "app_$IDLE" && wait_healthy "app_$IDLE" && smoke "app_$IDLE"; }; then
    echo "ERROR: $IDLE is not healthy (or its smoke request failed) on $TARGET_IMAGE."
    $COMPOSE logs --tail 50 "app_$IDLE" || true
    $COMPOSE stop "app_$IDLE" >/dev/null 2>&1 || true
    echo "DEPLOY REFUSED: ${ACTIVE:-nothing} still serves${ACTIVE:+ $CUR_IMAGE}."
    exit 3
fi
echo "$TARGET_IMAGE" > "$WORK_DIR/.image-$IDLE"

# --- Switch -------------------------------------------------------------------
echo "==> Switch the proxy to $IDLE (graceful reload)..."
if ! switch_to "$IDLE" || ! proxy_healthy; then
    echo "ERROR: the proxy does not serve $IDLE healthily."
    if [[ -n "$ACTIVE" ]] && switch_to "$ACTIVE" && proxy_healthy; then
        $COMPOSE stop "app_$IDLE" >/dev/null 2>&1 || true
        echo "DEPLOY REFUSED: switched back, $ACTIVE still serves $CUR_IMAGE."; exit 3
    fi
    echo "Needs attention: check '$COMPOSE ps' and the proxy logs."; exit 2
fi
echo "$IDLE" > "$WORK_DIR/.deploy-color"
echo "$TARGET_IMAGE" > "$WORK_DIR/.deploy-image"
if [[ -n "$CUR_IMAGE" && "$CUR_IMAGE" != "$TARGET_IMAGE" ]]; then echo "$CUR_IMAGE" > "$WORK_DIR/.deploy-prev-image"; fi
echo "$CUR_ENV_KEYS" > "$ENV_KEYS_FILE"; chmod 600 "$ENV_KEYS_FILE"
trap 'echo "ERROR: step failed at line $LINENO after the switch ($IDLE serves). Rollback: $WORK_DIR/deploy.sh --rollback"; exit 2' ERR

# --- Retire the old color: SIGTERM, it finishes its in-flight requests -------
if [[ -n "$ACTIVE" ]]; then
    echo "==> Stop $ACTIVE (drains in-flight requests)..."
    $COMPOSE stop "app_$ACTIVE" >/dev/null 2>&1 || echo "WARN: could not stop $ACTIVE; it no longer gets traffic."
fi

# --- Prune old images -----------------------------------------------------------
KEEP="$TARGET_IMAGE $CUR_IMAGE $(state .deploy-prev-image)"
docker image ls "$PROJECT" --format '{{.Repository}}:{{.Tag}}' | { grep -v ':latest$' || true; } | tail -n +$((KEEP_IMAGES + 1)) | while read -r img; do
    [[ " $KEEP " == *" $img "* ]] || docker image rm "$img" >/dev/null 2>&1 || true
done
docker image prune -f >/dev/null

echo ""; $COMPOSE ps; echo ""
echo "Deploy done: $TARGET_IMAGE on $IDLE"
(( TAIL_LOGS )) && exec $COMPOSE logs -f
exit 0
