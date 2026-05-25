#!/usr/bin/env bash
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REMOTE_HOST="${SAF_DEPLOY_HOST:-saf-vps}"
REMOTE_DIR="${SAF_DEPLOY_REMOTE_DIR:-/opt/saf}"
LOCAL_BINARY="${SAF_DEPLOY_BINARY:-$APP_DIR/target/x86_64-unknown-linux-gnu/release/saf}"
REMOTE_NEXT="$REMOTE_DIR/bin/saf.next"
SSH_CONNECT_TIMEOUT="${SAF_DEPLOY_CONNECT_TIMEOUT:-20}"
DEPLOY_RUNTIME="${SAF_DEPLOY_RUNTIME:-rust}"
DEPLOY_MINECRAFT="${SAF_DEPLOY_MINECRAFT:-azalea}"
SSH_OPTS=(
    -o "ConnectTimeout=$SSH_CONNECT_TIMEOUT"
    -o ServerAliveInterval=10
    -o ServerAliveCountMax=2
)

usage() {
    cat <<EOF
Usage: scripts/deploy-vps-prebuilt.sh [--host HOST] [--remote-dir DIR] [--binary PATH] [--runtime rust|preserve] [--minecraft azalea|preserve] [--skip-source-update]

Uploads an already-built Linux saf binary, fast-forwards the VPS checkout, and restarts the service.
No build runs on the VPS.
By default, the remote .env is updated to start the Rust runtime with the uploaded binary and native Azalea Minecraft.

Environment overrides:
  SAF_DEPLOY_HOST
  SAF_DEPLOY_REMOTE_DIR
  SAF_DEPLOY_BINARY
  SAF_DEPLOY_CONNECT_TIMEOUT
  SAF_DEPLOY_RUNTIME
  SAF_DEPLOY_MINECRAFT
EOF
}

SKIP_SOURCE_UPDATE=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --host)
            REMOTE_HOST="${2:?missing host}"
            shift 2
            ;;
        --remote-dir)
            REMOTE_DIR="${2:?missing remote dir}"
            REMOTE_NEXT="$REMOTE_DIR/bin/saf.next"
            shift 2
            ;;
        --binary)
            LOCAL_BINARY="${2:?missing binary path}"
            shift 2
            ;;
        --runtime)
            DEPLOY_RUNTIME="${2:?missing runtime}"
            shift 2
            ;;
        --minecraft)
            DEPLOY_MINECRAFT="${2:?missing minecraft mode}"
            shift 2
            ;;
        --skip-source-update)
            SKIP_SOURCE_UPDATE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$DEPLOY_RUNTIME" in
    rust|preserve)
        ;;
    *)
        echo "Invalid deploy runtime: $DEPLOY_RUNTIME" >&2
        echo "Use --runtime rust or --runtime preserve." >&2
        exit 2
        ;;
esac

case "$DEPLOY_MINECRAFT" in
    azalea|preserve)
        ;;
    *)
        echo "Invalid deploy Minecraft mode: $DEPLOY_MINECRAFT" >&2
        echo "Use --minecraft azalea or --minecraft preserve." >&2
        exit 2
        ;;
esac

if [[ ! -x "$LOCAL_BINARY" ]]; then
    echo "Missing executable Linux binary: $LOCAL_BINARY" >&2
    echo "Build it locally first, for example:" >&2
    echo "  rustup run nightly cargo zigbuild -p saf-app --features production-runtime --release --target x86_64-unknown-linux-gnu" >&2
    exit 1
fi

if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$LOCAL_BINARY"
fi

ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" "mkdir -p '$REMOTE_DIR/bin'"
scp "${SSH_OPTS[@]}" "$LOCAL_BINARY" "$REMOTE_HOST:$REMOTE_NEXT"
ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" bash -s -- "$REMOTE_DIR" "$SKIP_SOURCE_UPDATE" "$DEPLOY_RUNTIME" "$DEPLOY_MINECRAFT" <<'REMOTE'
set -euo pipefail

REMOTE_DIR="$1"
SKIP_SOURCE_UPDATE="$2"
DEPLOY_RUNTIME="$3"
DEPLOY_MINECRAFT="$4"
cd "$REMOTE_DIR"

set_env_value() {
    key="$1"
    value="$2"
    touch .env
    tmp_file="$(mktemp)"
    awk -v key="$key" -v value="$value" '
        BEGIN { updated = 0 }
        $0 ~ "^(export[[:space:]]+)?" key "=" {
            print key "=" value
            updated = 1
            next
        }
        { print }
        END {
            if (!updated) {
                print key "=" value
            }
        }
    ' .env > "$tmp_file"
    cat "$tmp_file" > .env
    rm -f "$tmp_file"
}

if [ "$SKIP_SOURCE_UPDATE" -eq 0 ] && [ -d .git ]; then
    backup_dir="$(mktemp -d)"
    restore_paths=".env config.json5 SavedData"
    for path in $restore_paths; do
        if [ -e "$path" ]; then
            mkdir -p "$backup_dir/$(dirname "$path")"
            cp -a "$path" "$backup_dir/$path"
        fi
    done
    git fetch origin main
    git reset --hard origin/main
    for path in $restore_paths; do
        if [ -e "$backup_dir/$path" ]; then
            rm -rf "$path"
            mkdir -p "$(dirname "$path")"
            cp -a "$backup_dir/$path" "$path"
        fi
    done
    rm -rf "$backup_dir"
fi

install -m 755 bin/saf.next bin/saf
rm -f bin/saf.next
if [ "$DEPLOY_RUNTIME" != "preserve" ]; then
    set_env_value SAF_RUNTIME "$DEPLOY_RUNTIME"
    if [ "$DEPLOY_RUNTIME" = "rust" ]; then
        set_env_value SAF_RUST_BIN "$REMOTE_DIR/bin/saf"
    fi
fi
if [ "$DEPLOY_MINECRAFT" != "preserve" ]; then
    set_env_value SAF_RUST_MINECRAFT "$DEPLOY_MINECRAFT"
fi
./saf.sh restart
./saf.sh status
REMOTE
