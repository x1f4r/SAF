#!/usr/bin/env bash
# One-shot setup for the SAF Dashboard API on a bot host.
#
# Enables the loopback API in the remote .env (SAF_API_ENABLED / SAF_API_TOKEN /
# SAF_API_PORT), restarts SAF, and prints the token to paste into the macOS app.
#
#   scripts/setup-dashboard-api.sh <ssh-destination> [--remote-dir DIR] [--port N] [--token TOK]
#
# <ssh-destination> is an ~/.ssh/config alias (e.g. saf-vps) or user@host.
# The API binds 127.0.0.1 only; the app reaches it through an SSH tunnel.
set -Eeuo pipefail

DEST="${1:-}"
if [[ -z "$DEST" || "$DEST" == -* ]]; then
    echo "Usage: scripts/setup-dashboard-api.sh <ssh-destination> [--remote-dir DIR] [--port N] [--token TOK]" >&2
    exit 2
fi
shift

REMOTE_DIR="${SAF_DEPLOY_REMOTE_DIR:-/opt/saf}"
PORT="8787"
TOKEN=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --remote-dir) REMOTE_DIR="${2:?}"; shift 2 ;;
        --port) PORT="${2:?}"; shift 2 ;;
        --token) TOKEN="${2:?}"; shift 2 ;;
        *) echo "unknown flag: $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$TOKEN" ]]; then
    if command -v openssl >/dev/null 2>&1; then
        TOKEN="$(openssl rand -hex 24)"
    else
        TOKEN="$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')"
    fi
fi

echo "==> configuring SAF dashboard API on $DEST ($REMOTE_DIR), port $PORT"
ssh "$DEST" bash -s -- "$REMOTE_DIR" "$PORT" "$TOKEN" <<'REMOTE'
set -Eeuo pipefail
REMOTE_DIR="$1"; PORT="$2"; TOKEN="$3"
cd "$REMOTE_DIR" || { echo "remote dir $REMOTE_DIR not found" >&2; exit 1; }
touch .env

set_env() {
    key="$1"; value="$2"; tmp="$(mktemp)"
    awk -v key="$key" -v value="$value" '
        BEGIN { done = 0 }
        $0 ~ "^(export[[:space:]]+)?" key "=" { print key "=" value; done = 1; next }
        { print }
        END { if (!done) print key "=" value }' .env > "$tmp"
    cat "$tmp" > .env; rm -f "$tmp"
}

set_env SAF_API_ENABLED 1
set_env SAF_API_TOKEN "$TOKEN"
set_env SAF_API_PORT "$PORT"
echo "updated $REMOTE_DIR/.env"

if [ -x ./saf.sh ]; then
    ./saf.sh restart || ./saf.sh start || true
    ./saf.sh status || true
fi
REMOTE

cat <<DONE

==> Done. The dashboard API is enabled on $DEST (loopback 127.0.0.1:$PORT).

In SAF Dashboard:
  SSH destination : $DEST
  Remote API port : $PORT
  API token       : $TOKEN

Keep this token private. To rotate it, re-run this script (optionally with --token).
DONE
