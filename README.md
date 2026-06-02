# WARNING: NOT BAN-SAFE

SAF has not been tested thoroughly and should not be treated as safe for use on
live accounts. It still needs fixes, validation, and hardening in many areas.
Use it only with the expectation that bugs, broken workflows, and account-risking
behavior may still exist.

# SAF

SAF is a Rust automation runtime for Hypixel SkyBlock auction flipping. It can
connect Minecraft accounts, consume SkyCofl flip events, route Discord/operator
commands, evaluate buy and relist safety rules, persist auction queue state, and
drive auction-house workflows when explicitly run in live mode.

The public repository is Rust-only. Historical non-Rust code is not part of
this repo.

## What It Does

- Loads a JSON5 bot configuration with one or more Minecraft accounts.
- Routes local command-inbox, Discord slash-command, and dashboard actions.
- Connects to SkyCofl with feature-gated WebSocket support.
- Connects to Minecraft with feature-gated native Azalea support.
- Opens, buys, claims, lists, relists, delists, and reconciles auction state.
- Applies buy/relist blocklists, skip thresholds, listing price guards, and
  dry-run/live market action boundaries.
- Runs supervised through a tmux session and optional systemd service.
- Provides offline unit tests and opt-in live smoke probes for external systems.

## Desktop Dashboard (macOS)

`SAFDashboard/` is a native macOS app for monitoring and controlling a running
bot: live profit/flip analytics with official item icons, per-account stats, the
action queue, a live log console, and the full command surface. It connects to an
API embedded in the `saf` runtime (feature `api`) over an auto-managed SSH tunnel
— nothing is exposed to the public internet.

```bash
scripts/setup-dashboard-api.sh <ssh-destination>   # enable the API on the host
scripts/build-macos-app.sh --install --run         # build + install the app
```

See [docs/DASHBOARD.md](docs/DASHBOARD.md) for setup, security, and switching hosts.

## Workspace

- `crates/saf-core`: config, typed IDs, command routing, auction math, blocklist
  policy, market workflow planning, runtime traits, and persisted state shapes.
- `crates/saf-cofl`: SkyCofl wire formats, flip parsing, socket links, command
  encoding, HTTP/WebSocket clients behind features.
- `crates/saf-discord`: Discord command and dashboard models, webhook support,
  and Serenity gateway support behind features.
- `crates/saf-minecraft`: recorded Minecraft adapter contracts and native
  Azalea integration behind the `azalea-native` feature.
- `crates/saf-app`: `saf` binary, CLI commands, live runtime wiring, preflight
  checks, inbox processing, and operational helpers.

## Prerequisites

- Git.
- Rust through `rustup`. The repo pins nightly in `rust-toolchain.toml` because
  the native Minecraft feature currently depends on nightly-compatible crates.
- A C/C++ build toolchain for native Rust dependencies.
- `tmux` for supervised local/VPS runs.
- Optional live systems: Minecraft accounts, Microsoft auth cache, SkyCofl
  access, and a Discord application/bot token.

## Quick Start

These commands do not run live Minecraft, Discord, SkyCofl, or market actions.

```bash
git clone https://github.com/x1f4r/SAF.git saf
cd saf
cp config.example.json5 config.json5
cp .env.example .env
cargo fmt --all --check
cargo test --workspace
./saf.sh rust-check
```

Edit `config.json5` and `.env` on the machine that will run SAF.

## Configuration

Start from:

- `config.example.json5`: account names, runtime settings, buy/relist policy,
  blocklists, listing rules, and blank SkyCofl session state.
- `.env.example`: machine-specific environment values and runtime overrides.

Use neutral placeholders while sharing examples:

```json5
igns: ["MainAccount", "AltAccount"],
defaultIgn: "MainAccount",
startDefaultOnly: true,
session: "",
```

Common environment variables:

```bash
SAF_RUNTIME=rust
SAF_RUST_BIN=/opt/saf/bin/saf
SAF_CONFIG_FILE=/opt/saf/config.json5
SAF_DEFAULT_IGN=MainAccount
SAF_START_DEFAULT_ONLY=1
SAF_ONLY_IGNS=MainAccount
SAF_RUST_MINECRAFT=azalea
SAF_DISCORD_BOT_ENABLED=1
SAF_DISCORD_TOKEN=<discord-bot-token>
SAF_DISCORD_CLIENT_ID=<discord-application-id>
SAF_DISCORD_GUILD_ID=<discord-guild-id>
SAF_DISCORD_ALLOWED_IDS=<discord-user-id>
SAF_COFL_SESSION=<cofl-session>
SAF_COFL_SOCKET_MAINACCOUNT=wss://sky.coflnet.com/modsocket?SId=<cofl-session>
```

## CLI Basics

The binary is named `saf`.

```bash
cargo run -p saf-app -- --help
cargo run -p saf-app -- --config config.example.json5 check-config
cargo run -p saf-app -- route-command '/cofl s minProfit 30m' \
  --configured MainAccount,AltAccount \
  --running MainAccount \
  --default-ign MainAccount
cargo run -p saf-app -- simulate-flip \
  '{"id":"auction-1","itemName":"Example Item","startingBid":"30m","target":"60m"}'
```

`saf.sh` wraps common local and VPS operations:

```bash
./saf.sh status
./saf.sh rust-check
./saf.sh rust-live-preflight --full
./saf.sh rust-live-dry-run
./saf.sh rust-live
./saf.sh send 'MainAccount ping'
./saf.sh transfer MainAccount AltAccount all
```

## Dry-Run And Live Modes

`run-live` defaults to dry-run market actions. Dry-run mode may process command
inbox and recorded/offline runtime paths, but it does not write live market queue
actions or drive buying/listing mutations.

Use live mode only after configuration and preflight are clean:

```bash
./saf.sh rust-live-preflight --full
./saf.sh rust-live-dry-run
./saf.sh rust-live
```

`rust-live` enables live market queue writes and can interact with Minecraft,
SkyCofl, Discord, and Hypixel auction workflows when the matching feature bundle
and credentials are configured.

## Build

Debug build:

```bash
cargo build -p saf-app
```

Release build without the full live feature bundle:

```bash
cargo build -p saf-app --release
```

Production feature bundle:

```bash
./saf.sh rust-production-check
./saf.sh rust-production-test
cargo build -p saf-app --features production-runtime --release
```

The production bundle enables Discord webhooks, SkyCofl HTTP/WebSocket support,
Discord gateway support, and native Azalea Minecraft support.

## Tests

Minimum local gate:

```bash
cargo fmt --all --check
cargo test --workspace
```

Broader local checks:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p saf-app --features live-cofl
cargo check -p saf-app --features live-discord
cargo check -p saf-app --features discord-webhook
./saf.sh rust-full-gate
```

Live smoke probes are skipped by default and should be enabled only for the
surface being validated:

```bash
SAF_LIVE_SMOKE=1 ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_COFL=1 ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_LOGIN=1 SAF_RUST_MINECRAFT=azalea ./saf.sh rust-production-test --test live_smoke
```

## VPS / systemd / tmux

The docs use `/opt/saf` as the neutral deployment path.

```bash
sudo mkdir -p /opt/saf
sudo chown "$USER":"$USER" /opt/saf
git clone https://github.com/x1f4r/SAF.git /opt/saf
cd /opt/saf
cp config.example.json5 config.json5
cp .env.example .env
```

For low-compute VPS hosts, build on another machine and upload a prebuilt
`saf` binary to `/opt/saf/bin/saf`.

Install the service:

```bash
sudo cp scripts/saf.service /etc/systemd/system/saf.service
sudo systemctl daemon-reload
sudo systemctl enable --now saf.service
./saf.sh status
```

Useful commands:

```bash
./saf.sh start
./saf.sh stop
./saf.sh restart
./saf.sh attach
./saf.sh console
./saf.sh logs
```

## More Documentation

- [Operations](docs/OPERATIONS.md)
- [Testing](docs/TESTING.md)
- [Rust architecture](docs/RUST_REWRITE.md)
- [Auction flow](docs/AUCTION_FLOW.md)
- [Blacklist rules](docs/BLACKLISTS.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
