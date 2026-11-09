#!/bin/sh
# deploy-ssh-gate.sh - forced command for the CI deploy key (SSH option only).
# The key can run NOTHING except deploy.sh with a validated commit or --rollback.
# Install on the server:
#   install -o root -g root -m 755 deploy-ssh-gate.sh /usr/local/bin/<PROJECT>-deploy-gate
#   (not under /root: sshd runs the forced command as the deploy user, who cannot enter /root)
#   In ~<deploy-user>/.ssh/authorized_keys, prefix the key line with:
#   command="/usr/local/bin/<PROJECT>-deploy-gate",no-port-forwarding,no-agent-forwarding,no-X11-forwarding,no-pty ssh-ed25519 AAAA... ci-deploy
# The deploy user needs passwordless sudo for exactly /root/<PROJECT>/deploy.sh (see BOSS-GUIDE.md).
set -eu
PROJECT="myapp"
DEPLOY="/root/${PROJECT}/deploy.sh"

set -- ${SSH_ORIGINAL_COMMAND:-}
case "${1:-} ${2:-}" in
    "deploy --rollback")
        [ "$#" -eq 2 ] || { echo "rejected"; exit 1; }
        exec sudo -n "$DEPLOY" --rollback ;;
    "deploy --ref")
        [ "$#" -eq 3 ] || { echo "rejected"; exit 1; }
        echo "$3" | grep -Eq '^[0-9a-f]{7,40}$' || { echo "rejected: bad sha"; exit 1; }
        exec sudo -n "$DEPLOY" --ref "$3" ;;
    *)
        echo "rejected: only 'deploy --ref <sha>' or 'deploy --rollback' are allowed"; exit 1 ;;
esac
