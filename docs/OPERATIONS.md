# Operations

This guide covers local and VPS operation for the Rust runtime.

## Runtime Files

- `config.json5`: account names, policy settings, and SkyCofl session state.
- `.env`: tokens, IDs, host-specific paths, and feature flags.
- `SavedData/`, `logs/`, `.saf-commands.jsonl`, and local reports.

Start every machine from the tracked templates:

```bash
cp config.example.json5 config.json5
cp .env.example .env
```

## Local Runtime Checks

Offline checks:

```bash
cargo fmt --all --check
cargo test --workspace
./saf.sh rust-check
```

Feature checks:

```bash
cargo check -p saf-app --features live-cofl
cargo check -p saf-app --features live-discord
cargo check -p saf-app --features discord-webhook
./saf.sh rust-production-check
./saf.sh rust-production-test
```

`./saf.sh rust-full-gate` runs the broad local gate: formatting, tests, clippy,
feature checks, production-runtime checks, a one-shot dry-run live loop, smoke
tests, and whitespace checks.

## Running Locally

Dry-run rehearsal:

```bash
./saf.sh rust-live-preflight --full
./saf.sh rust-live-dry-run
```

Live market mode:

```bash
./saf.sh rust-live
```

Live mode can connect to Minecraft, Discord, and SkyCofl and can mutate auction
state. Run it only after dry-run behavior and full preflight are acceptable.

Useful local operator commands:

```bash
./saf.sh send 'MainAccount ping'
./saf.sh send 'MainAccount inventory'
./saf.sh transfer MainAccount AltAccount all
./saf.sh blacklist list
```

## VPS Layout

The public documentation uses `/opt/saf` as the deployment directory:

```bash
sudo mkdir -p /opt/saf
sudo chown "$USER":"$USER" /opt/saf
git clone https://github.com/x1f4r/SAF.git /opt/saf
cd /opt/saf
cp config.example.json5 config.json5
cp .env.example .env
```

For supervised runs, the service starts `./saf.sh supervise`, which keeps a tmux
session named `saf` alive.

```bash
sudo cp scripts/saf.service /etc/systemd/system/saf.service
sudo systemctl daemon-reload
sudo systemctl enable --now saf.service
./saf.sh status
```

Service and tmux commands:

```bash
./saf.sh start
./saf.sh stop
./saf.sh hard-stop
./saf.sh restart
./saf.sh attach
./saf.sh console
./saf.sh logs
```

Detach from tmux with `Ctrl-b`, then `d`.

## Prebuilt VPS Runtime

On low-compute VPS hosts, do not compile the workspace in place. Build a release
binary on a stronger machine, upload it to `/opt/saf/bin/saf`, and set:

```bash
SAF_RUNTIME=rust
SAF_RUST_BIN=/opt/saf/bin/saf
SAF_RUST_MINECRAFT=azalea
SAF_CONFIG_FILE=/opt/saf/config.json5
```

When `SAF_RUNTIME=rust` and the binary is missing, supervised starts fail before
falling back to Cargo. Use `SAF_ALLOW_CARGO_ON_RUST_RUNTIME=1` only for local
diagnostics on a machine intended to compile the workspace.

The helper below uploads an already-built Linux binary, fast-forwards the remote
checkout, preserves runtime files, installs `bin/saf`, updates `.env`,
and restarts the service:

```bash
scripts/deploy-vps-prebuilt.sh --host saf-vps --remote-dir /opt/saf
```

Use `--runtime preserve` or `--minecraft preserve` when a deployment should not
change those existing `.env` values.

## Discord Gateway

Set Discord values in `.env`, not in tracked files:

```bash
SAF_DISCORD_BOT_ENABLED=1
SAF_DISCORD_TOKEN=<discord-bot-token>
SAF_DISCORD_CLIENT_ID=<discord-application-id>
SAF_DISCORD_GUILD_ID=<discord-guild-id>
SAF_DISCORD_ALLOWED_IDS=<discord-user-id>[,<discord-user-id>]
SAF_EXTERNAL_BACKEND_ENABLED=0
```

`SAF_DISCORD_ALLOWED_IDS` is the command allow-list. Leave it empty only when
command registration should work but every interaction should be denied.

The command surface includes account start/stop/status, command sending,
SkyCofl controls, queue inspection, listing/delisting, auction reconciliation,
inventory snapshots, bank/coin transfer helpers, dashboard buttons, and
diagnostics.

## SkyCofl Auth

Use one of:

```bash
SAF_COFL_SESSION=<cofl-session>
SAF_COFL_SOCKET_MAINACCOUNT=wss://sky.coflnet.com/modsocket?SId=<cofl-session>
```

If `config.session` is blank and the default socket path is used, the runtime can
generate and persist a session value in `config.json5` before
connecting.

To request a SkyCofl browser authorization link without starting the full
runtime:

```bash
./saf.sh rust-cofl-auth-link MainAccount
```

## Minecraft Auth

Native Minecraft automation requires the production feature bundle and:

```bash
SAF_RUST_MINECRAFT=azalea
```

Warm the Microsoft auth cache before live startup:

```bash
./saf.sh rust-minecraft-auth MainAccount
```

Optional per-account cache key:

```bash
SAF_MICROSOFT_CACHE_MAINACCOUNT=<cache-key> ./saf.sh rust-minecraft-auth MainAccount
```

`SAF_MINECRAFT_SERVER` defaults to `mc.hypixel.net`.

## Account Scope

`config.json5` controls the configured accounts:

```json5
igns: ["MainAccount", "AltAccount"],
defaultIgn: "MainAccount",
startDefaultOnly: true,
```

Environment overrides:

```bash
SAF_DEFAULT_IGN=MainAccount
SAF_START_DEFAULT_ONLY=1
SAF_ONLY_IGNS=MainAccount
```

`SAF_START_DEFAULT_ONLY` limits initial startup while keeping other configured
accounts available for explicit start commands. `SAF_ONLY_IGNS` hard-limits the
process to the listed accounts.

## Promotion Flow

Recommended order:

```bash
cargo fmt --all --check
cargo test --workspace
./saf.sh rust-full-gate
./saf.sh rust-live-preflight --full
./saf.sh rust-live-dry-run
./saf.sh rust-live
```

Run live smoke probes only on machines with the required external credentials
and account access.
