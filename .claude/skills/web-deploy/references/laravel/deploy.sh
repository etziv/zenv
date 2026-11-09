#!/bin/bash
# ============================================================================
# Generic Laravel deploy script - Docker + BIND-MOUNT, ZERO-DOWNTIME untuk
# update biasa. Framework-agnostic: Inertia/React (Octane), Filament, API.
#
# Source di-bind-mount ke /app, jadi kode PHP/Blade langsung live
# (opcache.validate_timestamps=1) tanpa rebuild & tanpa restart container.
# Langkah ditentukan OTOMATIS dari git diff antar-commit (.deploy-changed).
#
# Versi 2 (zero-downtime), beda dari versi 1:
#   - Vite di-build di salinan terpisah, lalu asset baru DIGABUNG ke
#     public/build dan manifest.json diganti ATOMIK di akhir. Tidak ada lagi
#     jeda "Vite manifest not found" selama build. Asset lama disimpan
#     KEEP_ASSET_DAYS hari supaya tab browser yang masih terbuka tidak 404.
#   - Composer install ke .vendor-new (salinan vendor), lalu ditukar dengan
#     rename. Request tidak pernah melihat vendor setengah ter-update.
#   - .env TIDAK lagi lewat env_file compose: Laravel membaca /app/.env langsung
#     (bind-mount). Ganti nilai .env = refresh config cache, TANPA recreate.
#     Recreate hanya untuk key yang dipakai compose sendiri (port, dst.) dan
#     itu ditangani `up -d` per service. Compose lama (masih env_file) tetap
#     didukung: perilakunya sama dengan versi 1 (recreate).
#   - reverb/scheduler di-restart HANYA kalau vendor, config/ atau .env berubah.
#   - --ref <sha>  : deploy commit tertentu (dipakai CI: commit yang lolos tes).
#   - --rollback   : deploy ulang commit sebelum deploy terakhir.
#   - lock (cegah dua deploy bersamaan) + health check di akhir (exit 2 kalau gagal); ditolak sebelum
#     ada yang berubah = exit 3.
#
# Yang MASIH menimbulkan restart (jarang): image di-rebuild (docker/Dockerfile,
# entrypoint.sh, php/ berubah) dan key .env yang dipakai compose (mis. APP_PORT).
#
# Cara pakai:
#   1. Edit blok KONFIGURASI di bawah sesuai project.
#   2. Taruh deploy.sh + Dockerfile + docker-compose.yml + entrypoint.sh +
#      php/opcache.ini di folder docker/ project (di repo github).
#   3. Di server: salin deploy.sh ke /root/<PROJECT>/deploy.sh, chmod +x.
#   4. Buat /home/<PROJECT>/src/.env (isi secret), chmod 600, LF.
#   5. Jalankan: /root/<PROJECT>/deploy.sh
#
# Flag:
#   --ref <sha>  deploy commit tertentu di BRANCH (harus sudah ada di BRANCH)
#   --rollback   deploy ulang commit sebelum deploy terakhir (migration TIDAK di-undo)
#   --no-fetch   skip git sync, pakai source yang ada
#   --build      PAKSA rebuild image (default: auto dari git diff)
#   --no-build   PAKSA skip rebuild image
#   --logs       tail log service utama setelah deploy
# ============================================================================

set -euo pipefail

# ===================== KONFIGURASI (EDIT BAGIAN INI) =======================
PROJECT="myapp"                           # slug project (huruf kecil, tanpa spasi)
REPO="owner/repo"                         # GitHub owner/repo
BRANCH="main"                             # branch yang di-deploy
REQUIRED_ENV="APP_KEY"                    # var .env wajib ada (pisah spasi, nama saja)
APP_SERVICE="app"                         # nama service utama di docker-compose
OCTANE=0                                  # 1 = stack Octane (inertia-react): octane:reload
                                          # 0 = php-server (filament): queue:restart
LONG_RUNNING="scheduler reverb"           # service yang menahan kode framework di memori;
                                          # di-restart HANYA kalau vendor/config/.env berubah.
                                          # Service yang tidak ada di compose diabaikan.
HEALTH_URL="http://localhost:8080/up"     # dicek dari DALAM container app setelah deploy
KEEP_ASSET_DAYS=7                         # simpan asset Vite lama sekian hari
# Di-pin dengan digest (image ini menerima token dan menulis source).
GIT_IMAGE="alpine/git:v2.54.0@sha256:ac6c828f15d82775d204d3d75112e1d34af8945da80ccf922713c9891ba68e7a"
# 0 = deploy.sh yang berubah di repo menghentikan deploy sampai bos meng-upload ulang (perintahnya dicetak).
# 1 = mengganti dirinya sendiri dan jalan sebagai root: siapa pun yang bisa merge ke BRANCH (termasuk agent)
# atau menulis source (app yang dibobol, paket npm jahat; source di-mount rw) jadi root di server ini.
SELF_UPDATE=0
# ===========================================================================
# Config level command-line mengalahkan .git/config repo (core.fsmonitor di sana bisa menjalankan perintah).
GIT_SAFE=(-e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=core.fsmonitor -e GIT_CONFIG_VALUE_0=false)

WORK_DIR="/root/${PROJECT}"               # lokasi operasional (deploy.sh, compose, state)
# export: dipakai compose untuk interpolasi ${SRC_DIR} (bind-mount) & ${ENV_FILE}.
export SRC_DIR="/home/${PROJECT}/src"     # lokasi source code (git repo)
export ENV_FILE="${SRC_DIR}/.env"         # Laravel membacanya dari root app, jadi tetap di sini
# Token GitHub read-only, sendirian, chmod 600: TIDAK di .env (app PHP bisa membaca .env-nya sendiri).
TOKEN_FILE="${WORK_DIR}/.deploy-token"
mkdir -p "$WORK_DIR"

# --- Parse flags -----------------------------------------------------------
DO_FETCH=1; FORCE_BUILD=-1; TAIL_LOGS=0; REF=""; ROLLBACK=0   # FORCE_BUILD: -1 auto | 1 paksa | 0 skip
ORIG_ARGS=("$@")
while (( $# )); do
    case "$1" in
        --ref)      REF="${2:-}"; shift ;;
        --rollback) ROLLBACK=1 ;;
        --no-fetch) DO_FETCH=0 ;;
        --build)    FORCE_BUILD=1 ;;
        --no-build) FORCE_BUILD=0 ;;
        --logs)     TAIL_LOGS=1 ;;
        *) echo "Unknown flag: $1"; exit 3 ;;
    esac
    shift
done
if (( ROLLBACK )); then
    [[ -f "$WORK_DIR/.deploy-prev" ]] || { echo "ERROR: tidak ada catatan deploy sebelumnya untuk rollback."; exit 3; }
    REF=$(cat "$WORK_DIR/.deploy-prev")
    echo "==> ROLLBACK ke $REF (migration TIDAK di-undo; cek skema DB kalau ada migration di antaranya)."
fi
if [[ -n "$REF" && ! "$REF" =~ ^[0-9a-f]{7,40}$ ]]; then
    echo "ERROR: --ref harus SHA commit (hex 7-40 karakter): $REF"; exit 3
fi

# --- Lock: cegah dua deploy bersamaan (mis. CI + manual) --------------------
if command -v flock >/dev/null 2>&1 && [[ -z "${DEPLOY_LOCKED:-}" ]]; then
    exec 9>"$WORK_DIR/.deploy.lock"
    flock -n 9 || { echo "ERROR: deploy lain sedang berjalan untuk $PROJECT."; exit 3; }
    export DEPLOY_LOCKED=1
fi

# --- Validasi env file & required vars -------------------------------------
[[ -f "$ENV_FILE" ]] || { echo "ERROR: $ENV_FILE tidak ditemukan"; exit 3; }
if grep -q $'\r' "$ENV_FILE"; then
    echo "ERROR: $ENV_FILE memakai CRLF (Windows). Jalankan: sed -i 's/\r\$//' $ENV_FILE  (lihat E2)"; exit 3
fi
[[ "$(stat -c %a "$ENV_FILE")" == 600 ]] || { echo "ERROR: $ENV_FILE harus mode 600 (berisi rahasia). Jalankan: chmod 600 $ENV_FILE"; exit 3; }
missing=()
for var in $REQUIRED_ENV; do
    grep -qE "^${var}=.+" "$ENV_FILE" || missing+=("$var")
done
if (( ${#missing[@]} > 0 )); then
    echo "ERROR: variable berikut kosong di $ENV_FILE:"
    printf '  - %s\n' "${missing[@]}"
    exit 3
fi

TOKEN=$(tr -d '\r\n "' < "$TOKEN_FILE" 2>/dev/null || true)
if (( DO_FETCH )) && [[ -z "$TOKEN" ]]; then
    echo "ERROR: $TOKEN_FILE tidak ada atau kosong (dibutuhkan untuk git sync; atau pakai --no-fetch)."; exit 3
fi
export GH_TOKEN="$TOKEN"   # dioper ke container lewat NAMA: nilainya tidak muncul di command line (ps)

# --- Sync source via git (container alpine/git) + catat file berubah -------
# Token dibaca credential helper dari env saat git memintanya: tidak ada di URL, argv, atau .git/config.
# OLD_REV -> NEW_REV di-diff ke /src/.deploy-changed (dasar keputusan langkah).
# --ref: commit HARUS sudah ada di BRANCH (cegah deploy commit sembarang).
if (( DO_FETCH )); then
    echo "==> Sync source dari github via git container..."
    docker run --rm \
        -v "${SRC_DIR}:/src" \
        -e GH_TOKEN \
        "${GIT_SAFE[@]}" \
        -e GH_REPO="$REPO" \
        -e GH_BRANCH="$BRANCH" \
        -e GH_REF="$REF" \
        --entrypoint sh \
        "$GIT_IMAGE" -c '
            set -e
            git config --global --add safe.directory /src
            cd /src
            if [ ! -d .git ]; then
                git init -q
                git remote add origin "https://github.com/${GH_REPO}.git"
            fi
            OLD_REV=$(git rev-parse HEAD 2>/dev/null || true)
            git -c credential.helper= -c "credential.helper=!f() { echo username=x-access-token; echo \"password=\$GH_TOKEN\"; }; f" \
                fetch --quiet "https://github.com/${GH_REPO}.git" \
                "${GH_BRANCH}:refs/remotes/origin/${GH_BRANCH}"
            if [ -n "$GH_REF" ]; then
                NEW_REV=$(git rev-parse --verify "${GH_REF}^{commit}" 2>/dev/null) \
                    || { echo "ERROR: commit $GH_REF tidak ditemukan"; exit 1; }
                git merge-base --is-ancestor "$NEW_REV" "refs/remotes/origin/${GH_BRANCH}" \
                    || { echo "ERROR: commit $GH_REF tidak ada di branch ${GH_BRANCH}"; exit 1; }
            else
                NEW_REV=$(git rev-parse "refs/remotes/origin/${GH_BRANCH}")
            fi
            git reset --hard --quiet "$NEW_REV"
            git clean -fd -e .env -e ".env.*" -e .deploy-changed -e .deploy-revs >/dev/null
            if [ -n "$OLD_REV" ] && git cat-file -e "${OLD_REV}^{commit}" 2>/dev/null; then
                git diff --name-only "$OLD_REV" "$NEW_REV" > /src/.deploy-changed
            else
                echo "__FIRST_DEPLOY__" > /src/.deploy-changed
            fi
            echo "$OLD_REV $NEW_REV" > /src/.deploy-revs
            git log -1 --format="    Deployed: %h - %s (%cr)"
        ' || { echo "ERROR: git sync ditolak (--ref salah, atau commit tidak ada di $BRANCH)."; exit 3; }
fi

# --- Baca daftar file berubah ----------------------------------------------
CHANGED=""
[[ -f "$SRC_DIR/.deploy-changed" ]] && CHANGED=$(cat "$SRC_DIR/.deploy-changed")
changed()      { echo "$CHANGED" | grep -qE "$1"; }   # true kalau ada file cocok pola
first_deploy() { echo "$CHANGED" | grep -q '__FIRST_DEPLOY__'; }

echo ""
echo "==> File berubah:"
if [[ -n "$CHANGED" ]]; then echo "$CHANGED" | sed 's/^/    /'; else echo "    (tidak ada / --no-fetch)"; fi
echo ""

# --- Sync docker/ config ke WORK_DIR (selalu; source sudah ter-sync) -------
DOCKER_SRC="$SRC_DIR/docker"
[[ -f "$DOCKER_SRC/docker-compose.yml" ]] || { echo "ERROR: $DOCKER_SRC/docker-compose.yml tidak ada (source corrupt?)"; exit 3; }
# root mengikuti path ini (cp, cmp, build, mount): symlink bisa menunjuk file khusus root.
for f in "" deploy.sh docker-compose.yml Dockerfile entrypoint.sh; do
    [[ -L "$DOCKER_SRC${f:+/$f}" ]] && { echo "ERROR: docker/$f di repo adalah symlink; ditolak, tidak ada yang berubah."; exit 3; }
done
# deploy.sh dan docker-compose.yml sama-sama bertindak sebagai root (compose menentukan privileged, volumes).
# Source di-mount rw ke app (RCE di app bisa mengubah keduanya di repo): SELF_UPDATE=0 berhenti SEBELUM menyalin
# apa pun dan minta bos meng-upload ulang.
if ! (( SELF_UPDATE )); then
    STALE=""; for f in deploy.sh docker-compose.yml; do cmp -s "$DOCKER_SRC/$f" "$WORK_DIR/$f" || STALE+=" $f"; done
    if [[ -n "$STALE" ]]; then
        echo "ERROR: di repo,$STALE beda dari $WORK_DIR (SELF_UPDATE=0). Review perubahannya, lalu dari folder proyek:"
        for f in $STALE; do echo "  ssh root@<server> \"cat > $WORK_DIR/$f\" < docker/$f"; done
        exit 3
    fi
fi
cp "$DOCKER_SRC/docker-compose.yml" "$DOCKER_SRC/Dockerfile" \
   "$DOCKER_SRC/entrypoint.sh" "$WORK_DIR/"
# Self-update (SELF_UPDATE=1): copy + re-exec versi baru.
# Pakai --no-fetch supaya .deploy-changed hasil fetch pertama TIDAK hilang.
if (( SELF_UPDATE )) && ! cmp -s "$DOCKER_SRC/deploy.sh" "$WORK_DIR/deploy.sh"; then
    echo "==> deploy.sh ter-update, re-exec versi baru..."
    cp "$DOCKER_SRC/deploy.sh" "$WORK_DIR/deploy.sh"
    chmod +x "$WORK_DIR/deploy.sh"
    REEXEC=()
    for a in "${ORIG_ARGS[@]}"; do [[ "$a" == "--no-fetch" ]] || REEXEC+=("$a"); done
    exec "$WORK_DIR/deploy.sh" --no-fetch "${REEXEC[@]}"
fi

COMPOSE_FILE="$WORK_DIR/docker-compose.yml"
export BUILD_CONTEXT="$SRC_DIR"           # build context = lokasi source code
COMPOSE="docker compose -p $PROJECT -f $COMPOSE_FILE --env-file $ENV_FILE"
RUN="$COMPOSE run --rm --no-deps"         # helper: container one-off, tak start deps
SERVICES=$($COMPOSE config --services 2>/dev/null || true)
has_service() { echo "$SERVICES" | grep -qx "$1"; }

app_running() { $COMPOSE ps --status running --services 2>/dev/null | grep -qx "$APP_SERVICE"; }
# Artisan/sh DI CONTAINER YANG RUNNING (exec), fallback one-off (run) kalau belum up.
# Cache Laravel/Filament HARUS di-generate container yang melayani request (E30).
app_artisan() {
    if app_running; then $COMPOSE exec -T "$APP_SERVICE" php artisan "$@"
    else $RUN --entrypoint php "$APP_SERVICE" artisan "$@"; fi
}
app_sh() {
    if app_running; then $COMPOSE exec -T "$APP_SERVICE" sh -c "$1"
    else $RUN --entrypoint sh "$APP_SERVICE" -c "$1"; fi
}

# --- .env: key mana yang berubah? (hash per key, nilai asli TIDAK disimpan) --
USES_ENV_FILE=0
grep -qE '^[[:space:]]*env_file:' "$COMPOSE_FILE" && USES_ENV_FILE=1
ENV_KEYS_FILE="$WORK_DIR/.env.deployed-keys"
CUR_ENV_KEYS=$(grep -E '^[A-Za-z_][A-Za-z0-9_]*=' "$ENV_FILE" | while IFS='=' read -r k v; do
    printf '%s=%s\n' "$k" "$(printf '%s' "$v" | sha256sum | cut -c1-16)"; done | sort)
CHANGED_ENV=""
if [[ -f "$ENV_KEYS_FILE" ]]; then
    CHANGED_ENV=$(comm -3 "$ENV_KEYS_FILE" <(echo "$CUR_ENV_KEYS") | sed 's/^\t//' | cut -d= -f1 | sort -u | tr '\n' ' ')
elif [[ -f "$WORK_DIR/.env.deployed-md5" ]] && [[ "$(cat "$WORK_DIR/.env.deployed-md5")" != "$(md5sum "$ENV_FILE" | cut -d' ' -f1)" ]]; then
    CHANGED_ENV="(unknown: upgrade dari deploy.sh v1)"
fi
env_changed() { [[ -n "$CHANGED_ENV" ]]; }
env_changed_match() { echo " $CHANGED_ENV " | grep -qE "$1"; }
RECREATE=""
if env_changed; then
    echo "==> .env berubah: $CHANGED_ENV"
    if (( USES_ENV_FILE )); then
        echo "    compose masih pakai env_file -> container di-recreate (pola v1, ada downtime singkat)."
        RECREATE="--force-recreate"
    else
        echo "    Laravel membaca /app/.env langsung -> cukup refresh config (tanpa recreate)."
        echo "    Key yang dipakai compose sendiri (port dll.) ditangani 'up -d' per service."
    fi
fi

# --- Build image HANYA kalau runtime Docker berubah ------------------------
NEED_BUILD=0
docker image inspect "${PROJECT}:latest" >/dev/null 2>&1 || NEED_BUILD=1
changed '^docker/(Dockerfile|entrypoint\.sh|php/)' && NEED_BUILD=1
first_deploy && NEED_BUILD=1
(( FORCE_BUILD == 1 )) && NEED_BUILD=1
(( FORCE_BUILD == 0 )) && NEED_BUILD=0
if (( NEED_BUILD )); then
    CACHEBUST="$(date +%s)"
    echo "==> Runtime Docker berubah -> build image (CACHEBUST=$CACHEBUST)."
    echo "    Container lama tetap melayani selama build; restart singkat saat 'up -d'."
    CACHEBUST="$CACHEBUST" $COMPOSE build
else
    echo "==> Runtime Docker tak berubah -> skip build."
fi

# --- Composer: install ke .vendor-new lalu tukar (tanpa vendor setengah jadi) -
# vendor/ hidup di SRC_DIR (bind-mount menutupi vendor image).
# COMPOSER_VENDOR_DIR=.vendor-new: kedalaman folder sama dengan vendor/, jadi
# path relatif di autoload tetap benar setelah di-rename.
NEED_COMPOSER=0
[[ -f "$SRC_DIR/vendor/autoload.php" ]] || NEED_COMPOSER=1
changed '^composer\.lock$' && NEED_COMPOSER=1
first_deploy && NEED_COMPOSER=1
COMPOSER_DONE=0
# Gagal di sini atau di Vite: kode baru SUDAH live (bind-mount), jadi script TIDAK berhenti.
# Langkah sisanya (up, migrate, cache, reload) tetap jalan supaya PHP konsisten; vendor/asset
# lama tetap dipakai; deploy ditandai GAGAL (exit 3) di akhir.
DEPLOY_WARN=()
if (( NEED_COMPOSER )); then
    COMPOSER_FLAGS="--no-dev --prefer-dist --no-interaction --no-progress --optimize-autoloader"
    if [[ -f "$SRC_DIR/vendor/autoload.php" ]]; then
        echo "==> Composer: install ke .vendor-new lalu tukar atomik..."
        app_sh "set -e; cd /app; rm -rf .vendor-new .vendor-old; cp -a vendor .vendor-new; \
            COMPOSER_VENDOR_DIR=.vendor-new composer install $COMPOSER_FLAGS --no-scripts; \
            mv vendor .vendor-old && mv .vendor-new vendor; \
            php artisan package:discover --ansi || true; rm -rf .vendor-old" \
            || { DEPLOY_WARN+=("composer install gagal: vendor LAMA tetap dipakai"); app_sh "rm -rf /app/.vendor-new" || true; }
    else
        echo "==> Composer: install pertama ke source..."
        $RUN --entrypoint composer "$APP_SERVICE" install $COMPOSER_FLAGS
    fi
    COMPOSER_DONE=1
else
    echo "==> Composer tak berubah -> skip."
fi

# --- Frontend (Vite): build di salinan, gabung asset, manifest diganti atomik -
# public/build TIDAK pernah dikosongkan: asset ber-hash baru ditambahkan, lalu
# manifest.json diganti dengan rename (atomik di filesystem yang sama).
# HAPUS blok ini kalau project tidak punya frontend build (tidak ada package.json).
NEED_FE=0
[[ -f "$SRC_DIR/public/build/manifest.json" ]] || NEED_FE=1
changed '^(resources/|package\.json$|package-lock\.json$|vite\.config\.(js|ts|mjs)$|tailwind\.config\.|postcss\.config\.)' && NEED_FE=1
changed '^composer\.(json|lock)$' && NEED_FE=1     # CSS paket (Filament dll) di-import dari vendor
env_changed_match ' VITE_' && NEED_FE=1            # VITE_* di-bake ke bundle saat build
first_deploy && NEED_FE=1
if [[ -f "$SRC_DIR/package.json" ]] && (( NEED_FE )); then
    echo "==> Build frontend (Vite) di salinan terpisah..."
    docker run --rm \
        -v "${SRC_DIR}:/src" \
        -v "${PROJECT}-npm-cache:/root/.npm" \
        -e KEEP_DAYS="$KEEP_ASSET_DAYS" \
        node:26-alpine sh -c '
            set -e
            mkdir -p /work && cd /src
            tar -cf - --exclude=./node_modules --exclude=./vendor --exclude=./.vendor-new \
                --exclude=./public/build --exclude=./.git --exclude=./storage . | (cd /work && tar -xf -)
            ln -s /src/vendor /work/vendor
            cd /work
            npm ci --no-audit --no-fund
            npm run build
            [ -f public/build/manifest.json ] || { echo "ERROR: build tidak menghasilkan public/build/manifest.json"; exit 1; }
            mkdir -p /src/public/build
            cd /work/public/build
            find . -type f ! -name manifest.json | while read -r f; do
                mkdir -p "/src/public/build/$(dirname "$f")"
                cp -p "$f" "/src/public/build/$f"
            done
            cp manifest.json /src/public/build/.manifest.new
            mv -f /src/public/build/.manifest.new /src/public/build/manifest.json
            node -e "
                const fs = require(\"fs\"), path = require(\"path\");
                const dir = \"/src/public/build\", keep = new Set([\"manifest.json\"]);
                const m = JSON.parse(fs.readFileSync(dir + \"/manifest.json\", \"utf8\"));
                for (const e of Object.values(m)) for (const f of [e.file, ...(e.css || []), ...(e.assets || [])]) if (f) keep.add(f);
                const limit = Date.now() - (+process.env.KEEP_DAYS || 7) * 864e5;
                let n = 0;
                (function walk(d) { for (const x of fs.readdirSync(d, { withFileTypes: true })) {
                    const p = path.join(d, x.name), rel = path.relative(dir, p).split(path.sep).join(\"/\");
                    if (x.isDirectory()) walk(p);
                    else if (!keep.has(rel) && fs.statSync(p).mtimeMs < limit) { fs.unlinkSync(p); n++; }
                } })(dir);
                console.log(\"    asset lama dihapus: \" + n);
            "
        ' || DEPLOY_WARN+=("build frontend gagal: asset LAMA tetap dipakai (manifest tidak diganti)")
else
    echo "==> Frontend tak berubah / tak ada -> skip Vite."
fi

# --- Start / recreate container --------------------------------------------
# Tanpa --force-recreate: compose hanya recreate service yang definisinya berubah
# (image baru, port baru, dll). Update kode biasa = tidak ada container yang disentuh.
echo "==> Start container (recreate hanya yang definisinya berubah)..."
$COMPOSE up -d --remove-orphans $RECREATE

# --- Migration HANYA kalau ada migration baru ------------------------------
# JANGAN PERNAH fresh/refresh/wipe - DB berisi data produksi.
# Zero-downtime: migration jalan saat kode baru sudah live. Untuk perubahan
# yang merusak kode lama (rename/drop kolom), pecah jadi 2 deploy (expand -> contract).
if first_deploy || changed '^database/migrations/'; then
    echo "==> Jalankan migration..."
    app_artisan migrate --force
else
    echo "==> Tak ada migration baru -> skip."
fi

# --- Filament Shield: auto-discover permission untuk resource/page baru ----
NEED_SHIELD=0
[[ -d "$SRC_DIR/app/Filament" ]] && [[ ! -f "$SRC_DIR/storage/.shield-generated" ]] && NEED_SHIELD=1
changed '^app/Filament/' && NEED_SHIELD=1
first_deploy && NEED_SHIELD=1
if (( NEED_SHIELD )) && [[ -d "$SRC_DIR/vendor/bezhansalleh" ]]; then
    echo "==> Filament Shield: generate permission untuk resource/page baru..."
    app_artisan filament:optimize-clear || true
    app_artisan shield:generate --all --panel=admin --option=permissions --ignore-existing-policies -n || true
    app_artisan permission:cache-reset || true
    app_sh "touch /app/storage/.shield-generated" || true
else
    echo "==> Filament Shield tak perlu -> skip."
fi

# --- Refresh cache Laravel (URUTAN KRITIS, lihat E29) ----------------------
echo "==> Refresh cache Laravel..."
app_artisan optimize:clear || true
if [[ -d "$SRC_DIR/vendor/filament/filament" ]]; then   # stack non-Filament: lewati (tanpa log error)
    app_artisan filament:optimize-clear || true
    app_artisan filament:optimize || true
fi
app_artisan config:cache || true
app_artisan route:cache || true
app_artisan view:cache || true
app_artisan event:cache || true

# --- Reload proses long-running (graceful, TANPA restart container app) ----
RESTART_LONG=0
(( COMPOSER_DONE )) && RESTART_LONG=1
env_changed && RESTART_LONG=1
changed '^config/' && RESTART_LONG=1
if (( OCTANE )); then
    echo "==> octane:reload (worker diganti bertahap, request tetap dilayani)..."
    $COMPOSE exec -T "$APP_SERVICE" php artisan octane:reload 2>/dev/null || true
fi
if has_service queue || (( ! OCTANE )); then
    echo "==> queue:restart (worker selesai job lalu ganti kode)..."
    app_artisan queue:restart || true
fi
if (( RESTART_LONG )); then
    for s in $LONG_RUNNING; do
        has_service "$s" || continue
        echo "==> restart $s (vendor/config/.env berubah)..."
        $COMPOSE restart "$s" || true
    done
else
    echo "==> $LONG_RUNNING tak perlu restart (hanya kode app yang berubah)."
fi

# SAFETY NET (E28): bind-mount butuh opcache.validate_timestamps=1. Kalau Off,
# HTTP menyajikan kode beku -> restart app (satu-satunya restart yang dipaksa).
if (( ! OCTANE )); then
    OPCACHE_VT=$($COMPOSE exec -T "$APP_SERVICE" \
        php -r 'echo ini_get("opcache.validate_timestamps") ? "1" : "0";' 2>/dev/null || echo "1")
    if [[ "$OPCACHE_VT" == "0" ]]; then
        echo "==> opcache.validate_timestamps=Off -> restart app. Saran: './deploy.sh --build' sekali."
        $COMPOSE restart "$APP_SERVICE"
    fi
fi

# --- Health check (dari dalam container app) --------------------------------
echo "==> Health check $HEALTH_URL ..."
HEALTHY=0
for _ in $(seq 1 20); do
    if $COMPOSE exec -T "$APP_SERVICE" sh -c "curl -fsS -o /dev/null '$HEALTH_URL'" 2>/dev/null; then HEALTHY=1; break; fi
    sleep 3
done

# --- Catat state deploy (rollback & deteksi .env berikutnya) ----------------
echo "$CUR_ENV_KEYS" > "$ENV_KEYS_FILE"; chmod 600 "$ENV_KEYS_FILE"
rm -f "$WORK_DIR/.env.deployed-md5"
if [[ -f "$SRC_DIR/.deploy-revs" ]]; then
    read -r OLD_REV NEW_REV < "$SRC_DIR/.deploy-revs" || true
    if (( HEALTHY )) && [[ -n "${OLD_REV:-}" && "${OLD_REV:-}" != "${NEW_REV:-}" ]]; then
        echo "$OLD_REV" > "$WORK_DIR/.deploy-prev"
    fi
    (( HEALTHY )) && echo "${NEW_REV:-}" > "$WORK_DIR/.deploy-current"
    rm -f "$SRC_DIR/.deploy-revs"
fi

echo "==> Hapus image dangling..."
docker image prune -f >/dev/null

echo ""
echo "==> Status container:"
$COMPOSE ps

echo ""
if (( ${#DEPLOY_WARN[@]} > 0 )); then
    echo "DEPLOY TIDAK LENGKAP:"; printf '  - %s\n' "${DEPLOY_WARN[@]}"
    echo "App tetap jalan dengan cache yang konsisten. Perbaiki penyebabnya lalu deploy ulang (atau --rollback)."
    exit 3
fi
if (( ! HEALTHY )); then
    echo "DEPLOY SELESAI TAPI HEALTH CHECK GAGAL ($HEALTH_URL)."
    echo "Cek: $COMPOSE logs --tail 100 $APP_SERVICE"
    echo "Rollback: $WORK_DIR/deploy.sh --rollback"
    exit 2
fi
echo "Deploy selesai. App di: $(grep '^APP_URL=' "$ENV_FILE" | cut -d= -f2-)"
echo "Source code: $SRC_DIR"

if (( TAIL_LOGS )); then
    echo ""
    echo "==> Tailing log $APP_SERVICE (Ctrl+C untuk berhenti)..."
    exec $COMPOSE logs -f "$APP_SERVICE"
fi
