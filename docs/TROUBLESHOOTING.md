# Troubleshooting

## First Checks

On a supervised host:

```bash
cd /opt/saf
./saf.sh status
./saf.sh console
tail -n 200 logs/latest.log
tail -n 200 logs/tmux-console.log
```

Check the checkout state:

```bash
git rev-parse --short HEAD
git status --short
```

## Config Does Not Load

Validate the config directly:

```bash
cargo run -p saf-app -- --config config.json5 check-config
```

If this is a fresh clone, create the local config first:

```bash
cp config.example.json5 config.json5
```

Put account names, tokens, sessions, and IDs in `config.json5` or `.env`.

## Service Or tmux Is Not Running

```bash
./saf.sh status
sudo systemctl status saf.service
./saf.sh restart
```

If the service is not installed:

```bash
sudo cp scripts/saf.service /etc/systemd/system/saf.service
sudo systemctl daemon-reload
sudo systemctl enable --now saf.service
```

## Prebuilt Binary Is Missing

When `SAF_RUNTIME=rust` is set, supervised starts expect an executable at
`SAF_RUST_BIN`, usually `/opt/saf/bin/saf`.

Fix by uploading a release binary or, for local diagnostics only, setting:

```bash
SAF_ALLOW_CARGO_ON_RUST_RUNTIME=1
```

Low-compute VPS hosts should use a prebuilt binary rather than compiling the
workspace in place.

## Discord Commands Do Not Work

Run:

```bash
./saf.sh rust-live-preflight --require-discord
```

Check `.env` for:

```bash
SAF_DISCORD_BOT_ENABLED=1
SAF_DISCORD_TOKEN=<discord-bot-token>
SAF_DISCORD_CLIENT_ID=<discord-application-id>
SAF_DISCORD_GUILD_ID=<discord-guild-id>
SAF_DISCORD_ALLOWED_IDS=<discord-user-id>
```

`SAF_DISCORD_ALLOWED_IDS` must contain numeric Discord user IDs for command
execution.

## SkyCofl Connects But No Flips Are Bought

Search logs:

```bash
rg -i 'cofl|settings|auth|flip|opening|blocked|tracked Cofl flip' logs
```

Common causes:

- SkyCofl settings/auth are not loaded yet.
- The flip is blocked by skip policy or `doNotBuy`.
- Live market mode is not enabled.
- Native Minecraft is not enabled or authenticated.

Fetch a login link:

```bash
./saf.sh rust-cofl-auth-link MainAccount
```

Then rerun:

```bash
./saf.sh rust-live-preflight --require-cofl
```

## Minecraft Is Not Ready

Check native mode and auth:

```bash
SAF_RUST_MINECRAFT=azalea ./saf.sh rust-minecraft-auth MainAccount
./saf.sh rust-live-preflight --require-minecraft
```

Useful log patterns:

```bash
rg -i 'connected Minecraft account|play state|locraw|private island|reconnect|kicked|disconnect' logs
```

Expected healthy startup evidence includes a connected account, a play-state
event, startup location checks, and an account-ready notification.

## Nothing Is Being Bought

Useful checks:

```bash
./saf.sh blacklist list
rg -i 'received flip|opening|blocked|too poor|settings|purchase|bed|nugget' logs
```

Verify the run mode. `rust-live-dry-run` will not perform live market writes.
`rust-live` can perform live market actions after preflight passes.

## Bought Item Did Not List

Search:

```bash
rg -i 'failed to list|queueing listing|inventory item|listing-status-unclear|unsafe listing|relist' logs
```

Common causes:

- `doNotRelist` blocked the item.
- The item could not be matched by unique inventory UUID.
- The final listing price guard rejected the queue entry.
- The Auction House GUI changed or timed out.
- Auction slots are full and the item is queued for later.

## Queue Or SavedData Looks Wrong

Inspect queue state with the CLI:

```bash
cargo run -p saf-app -- state snapshot <account-uuid> --base-dir .
```

For a live host, inspect the saved data on the host.

## Testing A Fix

Before publishing or deploying:

```bash
cargo fmt --all --check
cargo test --workspace
git diff --check
```

For broader runtime changes:

```bash
./saf.sh rust-full-gate
./saf.sh rust-live-preflight --full
```
