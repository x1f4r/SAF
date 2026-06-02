# SAF Dashboard (macOS)

A native macOS app for monitoring and controlling a running SAF bot: live
profit and flip analytics, item lists with official SkyBlock icons, per-account
stats, the action queue, a live log console, and the full command surface (the
same commands available in Discord). It talks to an API embedded in the `saf`
runtime over an automatically managed SSH tunnel, so nothing is exposed to the
public internet.

```
 macOS app ──ssh -L──▶ 127.0.0.1:8787 (loopback on the bot host) ──▶ saf run-live
   REST + WebSocket            embedded API, bearer-token auth
```

## How it works

- The bot runs an HTTP + WebSocket API bound to `127.0.0.1` inside the live
  runtime (feature `api`, part of `production-runtime`). It reads the in-memory
  stat providers and a persistent ledger, and drives commands through the exact
  same path the Discord gateway uses.
- The app opens an SSH tunnel with your key, forwards a local port to the bot's
  loopback API, and polls REST endpoints while streaming live buy/sell/state
  events over a WebSocket. The tunnel reconnects automatically and survives VPS
  reboots.
- Profit history is persisted to `SavedData/dashboard-ledger.jsonl` on the bot
  host, so the dashboard shows lifetime totals and charts that survive restarts.

## Enabling the API on the bot host

The deployed `saf` binary already includes the API (it ships in the
`production-runtime` bundle). Turn it on once with the helper, from any machine
that can SSH to the host:

```bash
scripts/setup-dashboard-api.sh <ssh-destination>      # e.g. saf-vps or root@host
# add --remote-dir /root/SAF if SAF is not in /opt/saf
```

It sets `SAF_API_ENABLED=1`, generates `SAF_API_TOKEN`, sets `SAF_API_PORT`
(default `8787`) in the host's `.env`, restarts SAF, and prints the token.

To do it manually, add to the host's `.env` and restart:

```bash
SAF_API_ENABLED=1
SAF_API_PORT=8787
SAF_API_TOKEN=<openssl rand -hex 24>
```

## Building the app

Requires Xcode (Swift 6, macOS 14+ SDK). No third-party dependencies.

```bash
scripts/build-macos-app.sh --install --run
```

This compiles the Swift package, assembles `SAF Dashboard.app` (icon from
`assets/saf-icon.png`), installs it to `/Applications`, and launches it.

## Connecting

On first launch the app asks for:

- **SSH destination** — an `~/.ssh/config` alias (e.g. `saf-vps`) or `user@host`.
  Leave blank to connect directly to `127.0.0.1` (bot running locally, or you
  manage your own tunnel/VPN).
- **API token** — the value printed by the setup helper.
- *Advanced* — explicit identity file, SSH port, and remote/local ports.

Click **Test** to verify, then **Connect**. The connection profile is stored in
`~/Library/Application Support/SAF Dashboard/`; the token is stored in the
macOS Keychain. Neither is ever written to the repository.

## Switching servers

The app keeps no bot state of its own — everything lives on the host. Moving to
a new VPS is: deploy SAF there, run `setup-dashboard-api.sh` against the new
host, then update the **SSH destination** (and token) in the app's Connection
tab. Copy `SavedData/` to the new host if you want profit history to carry over.

## Security

- The API binds `127.0.0.1` only and is unreachable except through your SSH
  tunnel. The bearer token is defense-in-depth on shared hosts.
- The app shells the system `ssh` with `BatchMode` and your existing key — no
  passwords are stored, and host keys go through the normal `known_hosts` flow.

## Hosting the web dashboard from the app

The macOS app can launch the self-hosted **web** dashboard for you. On the
**Connection** tab, under *Host a Web Dashboard*, pick a port + password and
click **Start Web Server** — the app runs the bundled `web/` Docker context with
`docker compose` and the container connects to the bot *through this Mac*
(`host.docker.internal`), so other devices on your network can use the dashboard
at `http://localhost:<port>`. Requires Docker Desktop and an active connection to
the bot. Use **Open in Browser** / **Stop** to manage it.

## Keyboard shortcuts

`⌘1`–`⌘8` switch between Dashboard, Accounts, Flips, Profit, Queue, Console,
Commands, and Connection.
