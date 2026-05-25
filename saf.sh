#!/usr/bin/env bash
set -Eeuo pipefail

APP_DIR="${SAF_APP_DIR:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)}"
SESSION="${SAF_TMUX_SESSION:-saf}"
SERVICE="${SAF_SERVICE_NAME:-saf.service}"
RESTART_DELAY="${SAF_TMUX_RESTART_DELAY:-5}"
PAUSE_FILE="${SAF_PAUSE_FILE:-$APP_DIR/.saf-paused}"
COMMAND_INBOX_FILE="${SAF_COMMAND_INBOX_FILE:-$APP_DIR/.saf-commands.jsonl}"

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

RUST_BIN="${SAF_RUST_BIN:-$APP_DIR/bin/saf}"
CONFIG_FILE="${SAF_CONFIG_FILE:-${SAF_CONFIG:-$APP_DIR/config.json5}}"
EXAMPLE_CONFIG_FILE="${SAF_EXAMPLE_CONFIG_FILE:-$APP_DIR/config.example.json5}"

export SAF_APP_DIR="$APP_DIR"
export SAF_TMUX_SESSION="$SESSION"
export SAF_PAUSE_FILE="$PAUSE_FILE"
export SAF_COMMAND_INBOX_FILE="$COMMAND_INBOX_FILE"
export SAF_RUST_BIN="$RUST_BIN"
export SAF_CONFIG_FILE="$CONFIG_FILE"
export SAF_RUNTIME="${SAF_RUNTIME:-rust}"
export SAF_DISABLE_TERMINAL="${SAF_DISABLE_TERMINAL:-1}"

usage() {
    cat <<USAGE
Usage: ./saf.sh [start|stop|pause|hard-stop|restart|manual|managed|send|transfer|blacklist|rust-live|rust-live-dry-run|rust-live-preflight|rust-live-setup|rust-live-smoke|rust-live-acceptance|rust-minecraft-auth|rust-cofl-auth-link|rust-check|rust-full-gate|rust-azalea-check|rust-production-check|rust-production-clippy|rust-production-test|status|attach|logs|console|supervise]

Commands:
  start      Start or resume the configured accounts while keeping the Discord controller online.
  stop       Pause all accounts but keep the Discord controller online.
  pause      Alias for stop.
  hard-stop  Fully stop the service/tmux session. Use only for maintenance.
  restart    Restart the systemd service when installed, otherwise restart tmux.
  manual     Stop the service and start tmux with terminal input enabled.
  managed    Stop any tmux session and return to the normal supervised service.
  send       Send one command line into the running SAF controller, e.g. ./saf.sh send '/cofl switchregion US'.
  transfer   Move coins between co-op accounts through the shared bank, e.g. ./saf.sh transfer MainAccount AltAccount all.
  blacklist  Update live buy/relist blacklists without restarting, e.g. ./saf.sh blacklist add buy item-enchant LAVA_SHELL_NECKLACE THE_ONE:5.
  rust-live  Run the Rust live runtime with live market queue writes enabled.
  rust-live-dry-run
             Run the Rust live runtime without writing market actions to the queue.
  rust-live-preflight
             Check Rust live runtime feature flags, config, and required credential presence.
  rust-live-setup
             Run the full Rust live preflight and print required setup actions.
  rust-live-smoke
             Run env-gated Rust live smoke probes: base, discord, discord-gateway, cofl, login, inventory, or full.
  rust-live-acceptance
             Run the full Rust promotion gate: production check, production tests, full preflight, and full live smoke.
  rust-minecraft-auth
             Warm the Azalea Microsoft auth cache for an account, e.g. ./saf.sh rust-minecraft-auth MainAccount.
  rust-cofl-auth-link
             Print a SkyCofl authmod login link for an account, e.g. ./saf.sh rust-cofl-auth-link MainAccount.
  rust-check Validate formatting, tests, and the local or example config.
  rust-full-gate
             Run the full local green gate: Rust fmt/test/clippy, feature checks, and production-runtime tests.
  rust-azalea-check Check and test the feature-gated native Azalea adapter with nightly Rust.
  rust-production-check
             Validate the full Rust production feature bundle with nightly Rust.
  rust-production-clippy
             Lint the full Rust production feature bundle with warnings denied.
  rust-production-test
             Test the full Rust production feature bundle with pinned nightly Rust.
  status     Show systemd, tmux, and Rust runtime status.
  attach     Attach to the tmux session.
  logs       Follow logs/latest.log.
  console    Print the current tmux pane.
  supervise  Internal systemd mode: keep the tmux session alive.
USAGE
}

require_tmux() {
    if ! command -v tmux >/dev/null 2>&1; then
        echo "tmux is not installed. Install it with: sudo apt install tmux" >&2
        exit 1
    fi
}

service_exists() {
    command -v systemctl >/dev/null 2>&1 || return 1
    systemctl show "$SERVICE" >/dev/null 2>&1 || return 1
    [[ "$(systemctl show -p LoadState --value "$SERVICE" 2>/dev/null)" != "not-found" ]]
}

in_service_mode() {
    [[ "${SAF_TMUX_SUPERVISOR:-0}" == "1" ]]
}

lowercase() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

wait_for_session_state() {
    local state="$1"
    local timeout="${2:-20}"
    for _ in $(seq 1 "$timeout"); do
        if [[ "$state" == "running" ]]; then
            session_exists && return 0
        else
            ! session_exists && return 0
        fi
        sleep 1
    done
    return 1
}

delegate_to_systemd() {
    local action="$1"
    [[ "${SAF_NO_SYSTEMD:-0}" == "1" ]] && return 1
    in_service_mode && return 1
    service_exists || return 1

    case "$action" in
        start)
            clear_paused
            if [[ "$(systemctl is-active "$SERVICE" 2>/dev/null || true)" == "active" ]]; then
                wait_for_session_state running 20 || true
                resume_accounts_signal || true
            else
                systemctl start "$SERVICE"
            fi
            wait_for_session_state running 20 || true
            ;;
        restart)
            clear_paused
            systemctl restart "$SERVICE"
            wait_for_session_state running 20 || true
            ;;
        *)
            return 1
            ;;
    esac

    status
    exit 0
}

session_exists() {
    require_tmux
    tmux has-session -t "$SESSION" 2>/dev/null
}

resume_accounts_signal() {
    queue_fixed_terminal_command "start"
}

pause_accounts_signal() {
    queue_fixed_terminal_command "stop"
}

mark_paused() {
    mkdir -p "$APP_DIR"
    : > "$PAUSE_FILE"
}

clear_paused() {
    rm -f "$PAUSE_FILE"
}

prepare_runtime() {
    mkdir -p "$APP_DIR/logs"
    if [[ -x "$RUST_BIN" ]]; then
        return 0
    fi
    if [[ "${SAF_ALLOW_CARGO_ON_RUST_RUNTIME:-0}" == "1" ]] && command -v cargo >/dev/null 2>&1; then
        return 0
    fi
    echo "SAF requires a prebuilt Rust executable at $RUST_BIN for supervised starts." >&2
    echo "Build on a stronger machine and upload it, or set SAF_ALLOW_CARGO_ON_RUST_RUNTIME=1 for local diagnostics." >&2
    exit 1
}

start_session() {
    require_tmux
    prepare_runtime
    if session_exists; then
        echo "tmux session '$SESSION' is already running."
        return 0
    fi

    tmux new-session -d -s "$SESSION" -c "$APP_DIR" "$APP_DIR/scripts/run-in-tmux.sh"
    echo "Started SAF in tmux session '$SESSION'."
}

start_manual_session() {
    require_tmux
    prepare_runtime

    if service_exists && ! in_service_mode; then
        systemctl stop "$SERVICE" || true
        wait_for_session_state stopped 20 || true
    fi

    if session_exists; then
        stop_session
    fi

    tmux new-session -d -s "$SESSION" -c "$APP_DIR" "SAF_DISABLE_TERMINAL=0 \"$APP_DIR/scripts/run-in-tmux.sh\""
    echo "Started SAF in manual tmux session '$SESSION' with terminal input enabled."
    echo "Attach with: ./saf.sh attach"
    echo "Send a command with: ./saf.sh send '/cofl switchregion US'"
}

start_managed_service() {
    require_tmux
    clear_paused
    if session_exists; then
        stop_session
    fi

    if service_exists && ! in_service_mode; then
        systemctl start "$SERVICE"
        wait_for_session_state running 20 || true
        status
        exit 0
    fi

    start_session
    status
}

pause_accounts() {
    require_tmux
    mark_paused

    if service_exists && ! in_service_mode; then
        if [[ "$(systemctl is-active "$SERVICE" 2>/dev/null || true)" != "active" ]]; then
            systemctl start "$SERVICE"
        fi
        wait_for_session_state running 20 || true
        pause_accounts_signal || true
        status
        exit 0
    fi

    if session_exists; then
        pause_accounts_signal || true
    else
        start_session
    fi
    status
}

stop_session() {
    require_tmux
    if ! session_exists; then
        echo "tmux session '$SESSION' is not running."
        return 0
    fi

    tmux send-keys -t "$SESSION" C-c || true
    for _ in $(seq 1 20); do
        if ! session_exists; then
            echo "Stopped tmux session '$SESSION'."
            return 0
        fi
        sleep 1
    done

    tmux kill-session -t "$SESSION" || true
    echo "Killed tmux session '$SESSION'."
}

hard_stop() {
    if service_exists && ! in_service_mode && [[ "${SAF_NO_SYSTEMD:-0}" != "1" ]]; then
        systemctl stop "$SERVICE"
        wait_for_session_state stopped 20 || true
        status
        exit 0
    fi

    stop_session
    status
}

status() {
    echo "SAF directory: $APP_DIR"
    echo "runtime: ${SAF_RUNTIME:-rust}"
    if service_exists; then
        echo "systemd: $(systemctl is-active "$SERVICE" 2>/dev/null || true) ($SERVICE)"
    else
        echo "systemd: not installed ($SERVICE)"
    fi

    require_tmux
    if session_exists; then
        echo "tmux: running ($SESSION)"
    else
        echo "tmux: stopped ($SESSION)"
    fi

    if [[ -f "$APP_DIR/Cargo.toml" ]]; then
        echo "rust:"
        echo "  workspace: present"
        echo "  runtime binary: $RUST_BIN"
        if [[ -x "$RUST_BIN" ]]; then
            echo "  binary: prebuilt"
        elif [[ -x "$APP_DIR/target/debug/saf" || -x "$APP_DIR/target/release/saf" ]]; then
            echo "  binary: local target"
        else
            echo "  binary: not built"
        fi
    fi
}

send_command() {
    [[ "$#" -gt 0 ]] || { echo "Usage: ./saf.sh send '<command>'"; exit 1; }
    rust_enqueue_command terminal "$*" >/dev/null
    echo "Queued SAF command: $*"
}

transfer_command() {
    [[ "$#" -ge 2 ]] || { echo "Usage: ./saf.sh transfer <from> <to> [amount|all]"; exit 1; }
    local from="$1"
    local to="$2"
    local amount="${3:-all}"
    rust_enqueue_command transfer "$from" "$to" "$amount" >/dev/null
    echo "Queued SAF coin transfer: $from -> $to ($amount)"
}

blacklist_command() {
    [[ "$#" -gt 0 ]] || { echo "Usage: ./saf.sh blacklist <list|add|remove> ..."; exit 1; }
    rust_enqueue_command terminal "blacklist $*" >/dev/null
    echo "Queued SAF blacklist command: blacklist $*"
}

rust_check() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo is not installed." >&2
        exit 1
    fi
    local check_config
    check_config="$(config_for_local_check)"
    (cd "$APP_DIR" && cargo fmt --all --check && cargo test --workspace)
    (cd "$APP_DIR" && cargo run -q -p saf-app -- --config "$check_config" check-config)
}

config_for_local_check() {
    if [[ -f "$CONFIG_FILE" ]]; then
        printf '%s\n' "$CONFIG_FILE"
        return 0
    fi
    if [[ -f "$EXAMPLE_CONFIG_FILE" ]]; then
        printf '%s\n' "$EXAMPLE_CONFIG_FILE"
        return 0
    fi
    echo "No config file found. Copy config.example.json5 to config.json5, or set SAF_CONFIG_FILE." >&2
    exit 1
}

run_nightly_cargo() {
    local reason="$1"
    shift
    if ! command -v rustup >/dev/null 2>&1; then
        echo "rustup is required for $reason." >&2
        exit 1
    fi
    local cargo_path
    local rustdoc_path
    local rustc_path
    cargo_path="$(rustup which --toolchain nightly cargo)"
    rustdoc_path="$(rustup which --toolchain nightly rustdoc)"
    rustc_path="$(rustup which --toolchain nightly rustc)"
    (cd "$APP_DIR" && CARGO_TARGET_DIR="$APP_DIR/target/nightly" RUSTC="$rustc_path" RUSTDOC="$rustdoc_path" "$cargo_path" "$@")
}

rust_binary_available() {
    [[ -x "$RUST_BIN" ]]
}

rust_runtime_selected() {
    [[ "$(lowercase "${SAF_RUNTIME:-}")" == "rust" ]]
}

cargo_fallback_allowed_for_direct_command() {
    [[ -f "$APP_DIR/Cargo.toml" && "${SAF_TMUX_CHILD:-0}" != "1" && "${SAF_TMUX_SUPERVISOR:-0}" != "1" ]]
}

require_prebuilt_or_allow_cargo() {
    local command_name="$1"
    if rust_runtime_selected &&
        ! rust_binary_available &&
        [[ "${SAF_ALLOW_CARGO_ON_RUST_RUNTIME:-0}" != "1" ]] &&
        ! cargo_fallback_allowed_for_direct_command; then
        echo "$command_name requires the prebuilt Rust executable at $RUST_BIN when SAF_RUNTIME=rust." >&2
        echo "Build on a stronger machine and upload bin/saf, or set SAF_ALLOW_CARGO_ON_RUST_RUNTIME=1 for local diagnostics." >&2
        exit 1
    fi
}

run_rust_binary() {
    (cd "$APP_DIR" && "$RUST_BIN" --config "$CONFIG_FILE" "$@")
}

rust_enqueue_command() {
    if rust_binary_available; then
        run_rust_binary enqueue-command --file "$COMMAND_INBOX_FILE" "$@" && return 0
        return 1
    fi

    if command -v cargo >/dev/null 2>&1; then
        (cd "$APP_DIR" && cargo run -q -p saf-app -- enqueue-command --file "$COMMAND_INBOX_FILE" "$@") && return 0
        return 1
    fi

    echo "Cannot queue command: neither the Rust binary nor cargo is available." >&2
    return 1
}

queue_fixed_terminal_command() {
    local line="$1"
    if rust_enqueue_command terminal "$line" >/dev/null; then
        return 0
    fi
    mkdir -p "$(dirname "$COMMAND_INBOX_FILE")"
    printf '{"type":"terminal","line":"%s","createdAt":%s}\n' "$line" "$(date +%s000)" >> "$COMMAND_INBOX_FILE"
}

rust_azalea_check() {
    run_nightly_cargo "the nightly Azalea check" check -p saf-minecraft --features azalea-native
    run_nightly_cargo "the nightly Azalea tests" test -p saf-minecraft --features azalea-native
}

rust_production_check() {
    run_nightly_cargo "the nightly production-runtime check" check -p saf-app --features production-runtime
}

rust_production_clippy() {
    run_nightly_cargo "the nightly production-runtime clippy" clippy -p saf-app --features production-runtime --all-targets -- -D warnings
}

rust_production_test() {
    run_nightly_cargo "the nightly production-runtime test" test -p saf-app --features production-runtime "$@"
}

rust_full_gate() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "cargo is not installed." >&2
        exit 1
    fi
    local check_config
    check_config="$(config_for_local_check)"

    (
        cd "$APP_DIR"
        cargo fmt --all --check
        cargo test --workspace
        cargo clippy --workspace --all-targets -- -D warnings
        cargo run -q -p saf-app -- --config "$check_config" check-config
        cargo check --workspace
        cargo check -p saf-app --features live-cofl
        cargo check -p saf-app --features live-discord
        cargo test -p saf-discord --features serenity-controller
        cargo check -p saf-app --features discord-webhook
        cargo check -p saf-discord --features webhook-notifier
    )
    rust_azalea_check
    rust_production_check
    rust_production_clippy
    rust_production_test
    (cd "$APP_DIR" && cargo run -q -p saf-app -- --config "$check_config" run-live --once --disable-cofl --market-actions dry-run)
    SAF_ALLOW_CARGO_SMOKE_ON_RUST_RUNTIME=1 rust_live_smoke

    if command -v git >/dev/null 2>&1; then
        git -C "$APP_DIR" diff --check HEAD
    fi
}

rust_minecraft_is_azalea() {
    local mode="${SAF_RUST_MINECRAFT:-}"
    mode="${mode#"${mode%%[![:space:]]*}"}"
    mode="${mode%"${mode##*[![:space:]]}"}"
    [[ "$(lowercase "$mode")" == "azalea" ]]
}

rust_live() {
    local market_actions="$1"
    mkdir -p "$APP_DIR/logs"
    if rust_binary_available; then
        run_rust_binary run-live \
            --command-inbox "$COMMAND_INBOX_FILE" \
            --state-base-dir "$APP_DIR" \
            --market-actions "$market_actions"
        return 0
    fi
    require_prebuilt_or_allow_cargo "rust-live"
    if rust_minecraft_is_azalea; then
        run_nightly_cargo "SAF_RUST_MINECRAFT=azalea" run -p saf-app --features production-runtime -- run-live \
            --command-inbox "$COMMAND_INBOX_FILE" \
            --state-base-dir "$APP_DIR" \
            --market-actions "$market_actions"
    else
        if ! command -v cargo >/dev/null 2>&1; then
            echo "cargo is not installed." >&2
            exit 1
        fi
        (cd "$APP_DIR" && cargo run -p saf-app --features live-cofl,discord-webhook,live-discord -- run-live \
            --command-inbox "$COMMAND_INBOX_FILE" \
            --state-base-dir "$APP_DIR" \
            --market-actions "$market_actions")
    fi
}

rust_live_preflight() {
    local extra_args=("$@")
    mkdir -p "$APP_DIR/logs"
    if rust_binary_available; then
        run_rust_binary live-preflight \
            --command-inbox "$COMMAND_INBOX_FILE" \
            --state-base-dir "$APP_DIR" \
            "${extra_args[@]}"
        return 0
    fi
    require_prebuilt_or_allow_cargo "rust-live-preflight"
    if [[ " ${extra_args[*]} " == *" --full "* ]] || rust_minecraft_is_azalea; then
        run_nightly_cargo "full Rust live preflight" run -p saf-app --features production-runtime -- live-preflight \
            --command-inbox "$COMMAND_INBOX_FILE" \
            --state-base-dir "$APP_DIR" \
            "${extra_args[@]}"
    else
        if ! command -v cargo >/dev/null 2>&1; then
            echo "cargo is not installed." >&2
            exit 1
        fi
        (cd "$APP_DIR" && cargo run -p saf-app --features live-cofl,discord-webhook,live-discord -- live-preflight \
            --command-inbox "$COMMAND_INBOX_FILE" \
            --state-base-dir "$APP_DIR" \
            "${extra_args[@]}")
    fi
}

rust_live_setup() {
    rust_live_preflight --full "$@"
}

rust_live_smoke_usage() {
    cat <<USAGE >&2
Usage: ./saf.sh rust-live-smoke [base|discord|discord-gateway|cofl|login|inventory|full] [cargo-test-args...]

Examples:
  ./saf.sh rust-live-smoke
  ./saf.sh rust-live-smoke cofl
  ./saf.sh rust-live-smoke login
  ./saf.sh rust-live-smoke full
USAGE
}

rust_live_smoke() {
    if rust_runtime_selected && rust_binary_available && [[ "${SAF_ALLOW_CARGO_SMOKE_ON_RUST_RUNTIME:-0}" != "1" ]]; then
        echo "rust-live-smoke runs the Cargo test harness and can compile the workspace." >&2
        echo "Run it on the local build machine. On a prebuilt low-compute VPS, use rust-live-setup plus rust-live-dry-run/rust-live instead." >&2
        exit 1
    fi

    local probe="${1:-base}"
    if [[ "$#" -gt 0 ]]; then
        shift
    fi

    case "$probe" in
        -h|--help|help)
            rust_live_smoke_usage
            return 0
            ;;
    esac

    export SAF_LIVE_SMOKE=1

    case "$probe" in
        base)
            ;;
        discord)
            export SAF_LIVE_SMOKE_DISCORD=1
            ;;
        discord-gateway)
            export SAF_LIVE_SMOKE_DISCORD_GATEWAY=1
            ;;
        cofl)
            export SAF_LIVE_SMOKE_COFL=1
            ;;
        login)
            export SAF_LIVE_SMOKE_LOGIN=1
            if ! rust_minecraft_is_azalea; then
                export SAF_RUST_MINECRAFT=azalea
            fi
            ;;
        inventory)
            export SAF_LIVE_SMOKE_INVENTORY=1
            if ! rust_minecraft_is_azalea; then
                export SAF_RUST_MINECRAFT=azalea
            fi
            ;;
        full)
            export SAF_LIVE_SMOKE_FULL=1
            if ! rust_minecraft_is_azalea; then
                export SAF_RUST_MINECRAFT=azalea
            fi
            rust_live_setup
            ;;
        *)
            echo "Unknown Rust live smoke probe: $probe" >&2
            rust_live_smoke_usage
            exit 2
            ;;
    esac

    rust_production_test --test live_smoke "$@"
}

rust_live_acceptance() {
    rust_live_setup
    rust_full_gate
    rust_live_smoke full "$@"
}

rust_minecraft_auth() {
    if ! rust_minecraft_is_azalea; then
        export SAF_RUST_MINECRAFT=azalea
    fi
    if rust_binary_available; then
        run_rust_binary minecraft-auth "$@"
        return 0
    fi
    require_prebuilt_or_allow_cargo "rust-minecraft-auth"
    run_nightly_cargo "Azalea Microsoft auth warmup" run -p saf-app --features production-runtime -- minecraft-auth "$@"
}

rust_cofl_auth_link() {
    if rust_binary_available; then
        run_rust_binary cofl-auth-link "$@"
        return 0
    fi
    require_prebuilt_or_allow_cargo "rust-cofl-auth-link"
    run_nightly_cargo "SkyCofl auth link helper" run -p saf-app --features production-runtime -- cofl-auth-link "$@"
}

supervise() {
    export SAF_TMUX_SUPERVISOR=1
    trap 'stop_session; exit 0' INT TERM

    while true; do
        start_session
        while session_exists; do
            sleep 5
        done
        echo "tmux session '$SESSION' exited; restarting in ${RESTART_DELAY}s."
        sleep "$RESTART_DELAY"
    done
}

cmd="${1:-start}"
case "$cmd" in
    start)
        delegate_to_systemd start || { clear_paused; start_session; resume_accounts_signal || true; }
        ;;
    stop|pause)
        pause_accounts
        ;;
    hard-stop|shutdown)
        hard_stop
        ;;
    restart)
        delegate_to_systemd restart || { clear_paused; stop_session; start_session; }
        ;;
    manual|interactive)
        start_manual_session
        ;;
    managed|daemon)
        start_managed_service
        ;;
    send)
        shift
        send_command "$@"
        ;;
    transfer)
        shift
        transfer_command "$@"
        ;;
    blacklist)
        shift
        blacklist_command "$@"
        ;;
    rust-live)
        rust_live live
        ;;
    rust-live-dry-run)
        rust_live dry-run
        ;;
    rust-live-preflight)
        shift
        rust_live_preflight "$@"
        ;;
    rust-live-setup|rust-live-doctor)
        shift
        rust_live_setup "$@"
        ;;
    rust-live-smoke)
        shift
        rust_live_smoke "$@"
        ;;
    rust-live-acceptance)
        shift
        rust_live_acceptance "$@"
        ;;
    rust-minecraft-auth)
        shift
        rust_minecraft_auth "$@"
        ;;
    rust-cofl-auth-link)
        shift
        rust_cofl_auth_link "$@"
        ;;
    rust-check)
        rust_check
        ;;
    rust-full-gate)
        rust_full_gate
        ;;
    rust-azalea-check)
        rust_azalea_check
        ;;
    rust-production-check)
        rust_production_check
        ;;
    rust-production-clippy)
        rust_production_clippy
        ;;
    rust-production-test)
        shift
        rust_production_test "$@"
        ;;
    status)
        status
        ;;
    attach)
        require_tmux
        session_exists || { echo "tmux session '$SESSION' is not running. Start it with ./saf.sh start"; exit 1; }
        tmux attach-session -t "$SESSION"
        ;;
    logs)
        mkdir -p "$APP_DIR/logs"
        touch "$APP_DIR/logs/latest.log"
        tail -n 200 -f "$APP_DIR/logs/latest.log"
        ;;
    console)
        require_tmux
        session_exists || { echo "tmux session '$SESSION' is not running."; exit 1; }
        tmux capture-pane -pt "$SESSION" -S -200
        ;;
    supervise)
        supervise
        ;;
    start-session)
        start_session
        ;;
    stop-session)
        stop_session
        ;;
    help|-h|--help)
        usage
        ;;
    *)
        usage
        exit 1
        ;;
esac
