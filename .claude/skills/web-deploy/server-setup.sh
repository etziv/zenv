#!/bin/bash
# server-setup.sh - one-time bootstrap of a deploy server (self-hosted runner path), run as root, idempotent.
# The BOSS runs it (it creates a user, a sudoers rule and a service: agents may not), from the project folder:
#   ssh root@<server> 'bash -s' -- <project> [runner-user] < .claude/skills/web-deploy/server-setup.sh
# With the GitHub runner too (registration token, valid one hour; owner/repo of the PRIVATE repo). The token
# travels on stdin with the script, never in a command line:
#   T=$(gh api -X POST repos/<owner>/<repo>/actions/runners/registration-token -q .token)
#   { printf 'RUNNER_TOKEN=%q REPO=%q\n' "$T" "<owner>/<repo>"; cat .claude/skills/web-deploy/server-setup.sh; } \
#     | ssh root@<server> 'bash -s -- <project>'
#
# What it does (prints each step):
#   1. /root/<project> (deploy.sh, compose, .env, token, state) and /home/<project>/src: owned by root, so the
#      runner user can change neither deploy.sh nor the source that gets built.
#   2. Runner user (default ghrunner) WITHOUT the docker group (docker group = root on this host); it refuses
#      a user in sudo/wheel/admin/lxd/disk/root.
#   3. /etc/sudoers.d/<project>-deploy: that user may run exactly /root/<project>/deploy.sh as root, checked by visudo.
#   4. Empty secret files with mode 600 (.env, .deploy-token); it prints how to fill them over ssh stdin.
#   5. With RUNNER_TOKEN + REPO: GitHub Actions runner in /home/<user>/actions-runner-<project>, label
#      deploy-<project>, installed as a service.
set -euo pipefail
P="${1:?usage: bash -s -- <project> [runner-user]}"
U="${2:-ghrunner}"
[[ "$P" =~ ^[a-z0-9][a-z0-9-]*$ ]] || { echo "ERROR: project slug must be lower case letters, digits, '-': $P"; exit 1; }
[[ "$U" =~ ^[a-z_][a-z0-9_-]*$ ]] || { echo "ERROR: bad user name: $U"; exit 1; }
(( EUID == 0 )) || { echo "ERROR: run as root"; exit 1; }
W="/root/$P"; S="/home/$P/src"

echo "==> 1. folders (root-owned)"
for d in "$W" "/home/$P" "$S"; do [[ -L "$d" ]] && { echo "ERROR: $d is a symlink; remove it first"; exit 1; }; done
if getent passwd | cut -d: -f6 | grep -qx "/home/$P"; then
    echo "ERROR: /home/$P is a user's home folder; deploy source must not live there"; exit 1
fi
mkdir -p "$W" "$S"
# an earlier setup may have left the source to some user: take ALL of it back (chown -R does not follow symlinks)
chown root:root "$W" "/home/$P"; chown -R root:root "$S"; chmod -R go-w "$S"; chmod 700 "$W"; chmod 755 "$S" "/home/$P"

echo "==> 2. runner user $U (no docker group)"
id "$U" >/dev/null 2>&1 || useradd --create-home --shell /bin/bash "$U"
if id -nG "$U" | tr ' ' '\n' | grep -qxE 'sudo|wheel|admin|lxd|disk|root'; then
    echo "ERROR: $U is in a privileged group ($(id -nG "$U")); use another user"; exit 1
fi
REMOVED=0
if id -nG "$U" | tr ' ' '\n' | grep -qx docker; then gpasswd -d "$U" docker >/dev/null; REMOVED=1; echo "    removed $U from the docker group"; fi

echo "==> 3. sudoers: $U may run only $W/deploy.sh"
command -v sudo >/dev/null || { echo "ERROR: sudo is not installed (apt-get install sudo)"; exit 1; }
SUD="/etc/sudoers.d/${P}-deploy"
umask 077
printf '%s ALL=(root) NOPASSWD: %s/deploy.sh\n' "$U" "$W" > "$SUD.tmp"
visudo -cf "$SUD.tmp" >/dev/null || { rm -f "$SUD.tmp"; echo "ERROR: sudoers line rejected by visudo"; exit 1; }
chmod 440 "$SUD.tmp"; mv "$SUD.tmp" "$SUD"

echo "==> 4. secret files (600, root)"
for f in .env .deploy-token; do [[ -e "$W/$f" ]] || : > "$W/$f"; chown root:root "$W/$f"; chmod 600 "$W/$f"; done
# Laravel reads its .env from the app root: when it exists, the same owner and mode
if [[ -f "$S/.env" ]]; then chown root:root "$S/.env"; chmod 600 "$S/.env"; fi
umask 022

if [[ -n "${RUNNER_TOKEN:-}" && -n "${REPO:-}" ]]; then
    echo "==> 5. GitHub Actions runner for $REPO"
    case "$(uname -m)" in x86_64) A=x64 ;; aarch64|arm64) A=arm64 ;; *) echo "ERROR: unsupported CPU $(uname -m)"; exit 1 ;; esac
    R="/home/$U/actions-runner-$P"
    if [[ ! -f "$R/.runner" ]]; then
        V=$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p')
        [[ -n "$V" ]] || { echo "ERROR: could not read the latest runner version"; exit 1; }
        install -d -o "$U" -g "$U" "$R"
        curl -fsSL "https://github.com/actions/runner/releases/download/v$V/actions-runner-linux-$A-$V.tar.gz" | sudo -u "$U" tar xz -C "$R"
        (cd "$R" && ./bin/installdependencies.sh)   # from inside the runner dir, or it misses its files
        (cd "$R" && sudo -u "$U" ./config.sh --unattended --replace --url "https://github.com/$REPO" \
            --token "$RUNNER_TOKEN" --labels "deploy-$P" --name "$P-$(hostname)")
    else
        echo "    already configured in $R"
    fi
    # outside the check above: a rerun finishes a service install that failed the first time
    (cd "$R" && { [[ -f .service ]] || ./svc.sh install "$U"; } && ./svc.sh start)
fi
# a running runner keeps its old groups until restarted
if (( REMOVED )); then
    for svc in $(systemctl list-units --type=service --all --plain --no-legend 'actions.runner.*' 2>/dev/null | awk '{print $1}'); do
        systemctl restart "$svc" && echo "    restarted $svc (drops the docker group)"
    done
fi
echo "    sudo rights of $U (must list only $W/deploy.sh):"; sudo -l -U "$U" | sed -n '/may run/,$p' | sed 's/^/    /'

cat <<EOF

DONE. Next, from the project folder on your PC (values go over ssh stdin: never in a command line or chat):
  ssh root@<server> "umask 077; cat > $W/.env" < <your filled .env>        (binary: Rust/Go)
  ssh root@<server> "umask 077; cat > $S/.env" < <your filled .env>        (Laravel: the app reads it there)
  ssh root@<server> "umask 077; cat > $W/.deploy-token" < <file with the read-only GitHub token>
  ssh root@<server> "cat > $W/deploy.sh && chmod 755 $W/deploy.sh" < docker/deploy.sh
  ssh root@<server> "cat > $W/docker-compose.yml" < docker/docker-compose.yml   (root config: never taken from the repo)
Then the first deploy: task.mjs deploy (after the DEPLOY card), or on the server: $W/deploy.sh
EOF
