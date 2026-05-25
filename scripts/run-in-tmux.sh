#!/usr/bin/env bash
set -Eeuo pipefail

APP_DIR="${SAF_APP_DIR:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)}"
SESSION="${SAF_TMUX_SESSION:-saf}"

cd "$APP_DIR"
mkdir -p logs

if [[ -z "${HOME:-}" ]]; then
    home_dir=""
    if command -v getent >/dev/null 2>&1; then
        home_dir="$(getent passwd "$(id -u)" | cut -d: -f6 || true)"
    fi
    if [[ -z "$home_dir" ]]; then
        home_dir="$(cd ~ && pwd -P 2>/dev/null || true)"
    fi
    export HOME="${home_dir:-/root}"
fi

if [[ -f "$APP_DIR/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "$APP_DIR/.env"
    set +a
fi

export SAF_DISABLE_TERMINAL="${SAF_DISABLE_TERMINAL:-1}"
export SAF_TMUX_CHILD=1

echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] Starting SAF in tmux session '$SESSION' from $APP_DIR"
runtime="$(printf '%s' "${SAF_RUNTIME:-rust}" | tr '[:upper:]' '[:lower:]')"
case "$runtime" in
    rust)
        "$APP_DIR/saf.sh" rust-live 2>&1 | tee -a logs/tmux-console.log
        ;;
    *)
        echo "Unknown SAF_RUNTIME=${SAF_RUNTIME}. Use rust." >&2
        exit 2
        ;;
esac
