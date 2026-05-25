# Testing

## Minimum Gate

Run these before committing:

```bash
cargo fmt --all --check
cargo test --workspace
```

`./saf.sh rust-check` runs the same minimum gate and validates `config.json5`.
When `config.json5` is absent, it validates the tracked `config.example.json5`
so a fresh clone can still prove the basic setup.

## Broader Local Gate

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo check -p saf-app --features live-cofl
cargo check -p saf-app --features live-discord
cargo check -p saf-app --features discord-webhook
cargo check -p saf-discord --features webhook-notifier
./saf.sh rust-production-check
./saf.sh rust-production-test
```

`./saf.sh rust-full-gate` combines formatting, tests, clippy, feature checks,
production-runtime checks, a one-shot dry-run `run-live`, base smoke coverage,
and `git diff --check`.

## Test Layout

- `saf-core`: config import, account selection, command routing, queue state,
  blocklist decisions, relist pricing, item metadata, GUI slot logic, and market
  workflow state machines.
- `saf-cofl`: SkyCofl envelope parsing, socket links, command execution safety,
  telemetry, inventory/scoreboard upload shapes, and text parsing.
- `saf-discord`: slash command definitions, dashboard components, allow-list
  planning, command routing, and destructive-action confirmation flows.
- `saf-minecraft`: recorded client contracts and Azalea-native parsing,
  windows, scoreboard, inventory, signs, and disconnect behavior.
- `saf-app`: live runtime orchestration, preflight checks, command inbox
  processing, Discord gateway routing, SkyCofl lifecycle, market queue
  execution, auction-window flows, startup readiness, and smoke probes.

## Live Smoke Probes

Live probes are skipped unless explicitly enabled. Enable only the surface being
validated and run them only where the required credentials and accounts are
available.

```bash
SAF_LIVE_SMOKE=1 ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_DISCORD=1 ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_COFL=1 ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_LOGIN=1 SAF_RUST_MINECRAFT=azalea ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_INVENTORY=1 SAF_RUST_MINECRAFT=azalea ./saf.sh rust-production-test --test live_smoke
SAF_LIVE_SMOKE=1 SAF_LIVE_SMOKE_FULL=1 SAF_RUST_MINECRAFT=azalea ./saf.sh rust-production-test --test live_smoke
```

The full probe expects `SAF_LIVE_SMOKE_IGN`, Discord gateway credentials, a
numeric Discord allow-list, SkyCofl auth, and Azalea Minecraft auth.

## When To Add Tests

Add or update focused tests when changing:

- Auction GUI names, slots, or click planning.
- Buy-path latency decisions.
- Claim, listing, relisting, delisting, and reconciliation behavior.
- Blocklist parsing or live reload behavior.
- Discord command routing, dashboard state, or confirmation flows.
- SkyCofl socket parsing, auth, execute-command handling, or telemetry.
- Minecraft startup, location checks, inventory parsing, or reconnect logic.
- Persisted queue or saved-data file shapes.
