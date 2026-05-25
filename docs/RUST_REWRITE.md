# Rust Architecture

SAF is currently a Rust workspace. The live runtime, CLI, tests, and deployment
helpers in this repository are all Rust-first.

## Crates

- `saf-core`: shared domain logic. It owns config parsing, account selection,
  command routing, auction math, listing duration/price rules, blocklist policy,
  GUI/window abstractions, runtime traits, and saved queue state.
- `saf-cofl`: SkyCofl integration. It parses envelopes, builds socket URLs,
  handles command wire formats, and exposes optional HTTP/WebSocket clients.
- `saf-discord`: Discord integration. It defines slash commands, dashboard
  components, command planning, webhook notifications, and optional Serenity
  gateway support.
- `saf-minecraft`: Minecraft integration. It provides recorded adapter behavior
  for tests and optional native Azalea support for live accounts.
- `saf-app`: the `saf` binary. It wires the runtime, preflight checks, config
  session persistence, command inbox processing, live loop, and operational CLI.

## Feature Flags

`saf-app` exposes these features:

- `discord-webhook`: Discord webhook notifications.
- `live-cofl`: SkyCofl HTTP and WebSocket clients.
- `live-discord`: Discord slash-command gateway.
- `live-minecraft`: native Azalea Minecraft client.
- `production-runtime`: all live features together.

Default builds compile the offline/runtime core without external live clients.

## Runtime Model

The live loop combines:

- Configured accounts and startup account selection.
- A JSONL command inbox for local/scripted commands.
- Optional Discord gateway and webhook notifications.
- Optional SkyCofl socket clients per account.
- Optional native Minecraft clients per account.
- A persisted market queue and saved auction data.
- Runtime providers for stats, inventory, active auctions, queues, and notifiers.

Long-running `run-live` starts reading the command inbox at the current file end
so service restarts do not replay old commands. One-shot and smoke-test runs
read their provided inbox from the beginning.

## Market Safety

`--market-actions dry-run` is the default. It allows offline rehearsals and
runtime reporting without live market mutations.

`--market-actions live` enables persisted market queue writes and live Minecraft
market actions. It fails closed when required live preflight checks are missing.

Safety checks include:

- Buy skip policy and buy blocklists before final purchase clicks.
- Enchantment and item+enchantment blocklists after the auction item is visible.
- Pending-buy tracking through BIN, nugget, and bed purchase flows.
- Listing price guards before automated listing clicks.
- Unique inventory UUID requirements for purchased-item relists.
- Recovery paths for sold, expired, bought, and draft auctions.
- Dry-run protection for cached-only auction discovery paths.

## External Auth Surfaces

Discord:

```bash
SAF_DISCORD_BOT_ENABLED=1
SAF_DISCORD_TOKEN=<discord-bot-token>
SAF_DISCORD_ALLOWED_IDS=<discord-user-id>
```

SkyCofl:

```bash
SAF_COFL_SESSION=<cofl-session>
SAF_COFL_SOCKET_MAINACCOUNT=wss://sky.coflnet.com/modsocket?SId=<cofl-session>
```

Minecraft:

```bash
SAF_RUST_MINECRAFT=azalea
./saf.sh rust-minecraft-auth MainAccount
```

## Important CLI Commands

```bash
cargo run -p saf-app -- --help
cargo run -p saf-app -- --config config.example.json5 check-config
cargo run -p saf-app -- parse-inbox .saf-commands.jsonl
cargo run -p saf-app -- process-inbox .saf-commands.jsonl --running MainAccount
cargo run -p saf-app -- simulate-flip '{"id":"auction-1","itemName":"Example Item","startingBid":"30m","target":"60m"}'
cargo run -p saf-app -- route-command 'MainAccount ping' --configured MainAccount,AltAccount --running MainAccount --default-ign MainAccount
cargo run -p saf-app -- plan-command 'inventory' --configured MainAccount,AltAccount --running MainAccount --default-ign MainAccount
cargo run -p saf-app -- state snapshot <account-uuid> --base-dir .
```

Feature-gated live helpers:

```bash
./saf.sh rust-live-preflight --full
./saf.sh rust-live-dry-run
./saf.sh rust-live
./saf.sh rust-cofl-auth-link MainAccount
./saf.sh rust-minecraft-auth MainAccount
```

## Validation

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p saf-app --features live-cofl
cargo check -p saf-app --features live-discord
cargo check -p saf-app --features discord-webhook
./saf.sh rust-production-check
./saf.sh rust-production-test
```

Use `./saf.sh rust-full-gate` before deployment when the machine can compile the
full feature set.
