# SAF Dashboard — Web (self-hosted)

A self-hosted, browser-based version of the SAF Dashboard with the same UI and
capabilities as the macOS app — live profit/flip analytics with official item
icons, per-account stats, the action queue, a log console, and the full command
palette. Runs anywhere via Docker (Windows, Linux, macOS). Tested in Chromium
and Firefox.

```
 browser ──/api (REST) + /events (WS)──▶ gateway ──▶ bot API ( /v1/* )
            dashboard session cookie       injects the bearer token server-side
```

The gateway serves the built SPA and proxies the browser's same-origin requests
to the bot's API, adding the bearer token so it never reaches the browser. The
dashboard is protected by its own password.

## Quick start

```bash
cd web
cp .env.example .env          # set BOT_API_TOKEN + a DASHBOARD_PASSWORD
docker compose up -d --build
# open http://localhost:8090
```

`BOT_API_TOKEN` is the token printed by `scripts/setup-dashboard-api.sh` on the
bot host (it enables the bot's API). Then point the gateway at the bot with
**one** of:

- **Direct** — `BOT_API_URL=http://host.docker.internal:8787` (Docker Desktop)
  or run with `network_mode: host` and `http://127.0.0.1:8787` (Linux, bot on
  the same host).
- **SSH tunnel** — set `SSH_DEST` (and mount a key via `SSH_KEY` + a compose
  volume). The gateway opens and maintains the tunnel itself.

## Security

- The bot's API stays loopback-only on its host; the gateway is the only thing
  that talks to it, and it holds the token server-side.
- The dashboard requires a password (`DASHBOARD_PASSWORD`); sessions are signed
  cookies. Put the gateway behind TLS (a reverse proxy) or reach it over an SSH
  tunnel if exposing it beyond localhost.

## Develop

```bash
npm install
npm run dev          # Vite dev server on :5180, proxying /api + /events to :8090
# in another shell, run the gateway against a bot:
BOT_API_URL=http://127.0.0.1:8787 BOT_API_TOKEN=… DASHBOARD_PASSWORD= npm start
```

## Layout

- `src/` — the React SPA (Vite + TypeScript). `theme.css` ports the macOS app's
  design tokens; `components/` holds the shared primitives and charts; `views/`
  are the eight screens.
- `server/index.mjs` — the gateway (static SPA + REST/WS proxy + auth + tunnel).
- `Dockerfile`, `docker-compose.yml` — build + run.

Keyboard: `1`–`8` switch tabs.
