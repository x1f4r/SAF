// SAF Dashboard gateway.
//
// Serves the built SPA and proxies the browser's same-origin /api (REST) and
// /events (WebSocket) to the bot's API, injecting the bearer token server-side
// so it never reaches the browser. Protects the dashboard with an optional
// password (session cookie). Upstream is either a direct URL or an SSH tunnel
// the gateway manages itself.
import crypto from "node:crypto";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import express from "express";
import { WebSocketServer, WebSocket } from "ws";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(__dirname, "..", "dist");

const PORT = parseInt(process.env.PORT || "8090", 10);
const PASSWORD = (process.env.DASHBOARD_PASSWORD || "").trim();
const AUTH_REQUIRED = PASSWORD.length > 0;
const SECRET = (process.env.SESSION_SECRET || crypto.randomBytes(32).toString("hex")).trim();
const TOKEN = (process.env.BOT_API_TOKEN || "").trim();
const COOKIE = "saf_session";
const SESSION_TTL_MS = 7 * 24 * 3600 * 1000;

function log(...a) { console.log("[gateway]", ...a); }

// ---- Upstream resolution: direct URL or managed SSH tunnel ----
let upstreamHost = "127.0.0.1";
let upstreamPort = 8787;
let transport = "direct";

function startTunnel() {
  const dest = process.env.SSH_DEST || process.env.SSH_HOST;
  const remotePort = parseInt(process.env.REMOTE_PORT || "8787", 10);
  const localPort = parseInt(process.env.SSH_LOCAL_PORT || "18787", 10);
  upstreamHost = "127.0.0.1";
  upstreamPort = localPort;
  transport = `ssh:${dest}`;
  const args = [
    "-N", "-T",
    "-o", "ExitOnForwardFailure=yes", "-o", "ServerAliveInterval=15",
    "-o", "ServerAliveCountMax=3", "-o", "StrictHostKeyChecking=accept-new",
    "-o", "BatchMode=yes",
  ];
  if (process.env.SSH_PORT) args.push("-p", process.env.SSH_PORT);
  if (process.env.SSH_KEY) args.push("-i", process.env.SSH_KEY, "-o", "IdentitiesOnly=yes");
  args.push("-L", `127.0.0.1:${localPort}:127.0.0.1:${remotePort}`, dest);

  let attempt = 0;
  const launch = () => {
    log(`ssh tunnel → ${dest} (127.0.0.1:${localPort} → :${remotePort})`);
    const proc = spawn("ssh", args, { stdio: ["ignore", "inherit", "inherit"] });
    proc.on("exit", (code) => {
      attempt++;
      const delay = Math.min(2000 * attempt, 20000);
      log(`ssh exited (${code}); reconnecting in ${delay}ms`);
      setTimeout(launch, delay);
    });
  };
  launch();
}

function resolveUpstream() {
  if (process.env.BOT_API_URL) {
    const u = new URL(process.env.BOT_API_URL);
    upstreamHost = u.hostname;
    upstreamPort = parseInt(u.port || "8787", 10);
    transport = `direct:${u.host}`;
    log(`upstream: ${transport}`);
  } else if (process.env.SSH_DEST || process.env.SSH_HOST) {
    startTunnel();
  } else {
    log("WARNING: no BOT_API_URL or SSH_DEST set; defaulting to 127.0.0.1:8787");
  }
}
resolveUpstream();

const upstreamHttp = () => `http://${upstreamHost}:${upstreamPort}`;

// ---- Session cookie helpers ----
function sign(value) {
  return crypto.createHmac("sha256", SECRET).update(value).digest("base64url");
}
function makeCookie() {
  const payload = `${Date.now()}`;
  return `${payload}.${sign(payload)}`;
}
function validCookie(raw) {
  if (!raw) return false;
  const [payload, mac] = raw.split(".");
  if (!payload || !mac) return false;
  const expected = sign(payload);
  if (mac.length !== expected.length || !crypto.timingSafeEqual(Buffer.from(mac), Buffer.from(expected))) return false;
  return Date.now() - Number(payload) < SESSION_TTL_MS;
}
function parseCookies(header = "") {
  return Object.fromEntries(header.split(";").map((c) => {
    const i = c.indexOf("=");
    return i < 0 ? [c.trim(), ""] : [c.slice(0, i).trim(), decodeURIComponent(c.slice(i + 1).trim())];
  }).filter(([k]) => k));
}
function isAuthed(req) {
  if (!AUTH_REQUIRED) return true;
  return validCookie(parseCookies(req.headers.cookie || "")[COOKIE]);
}

// ---- App ----
const app = express();
app.use(express.json({ limit: "256kb" }));

app.get("/api/session", (req, res) => {
  res.json({
    authenticated: isAuthed(req),
    needsPassword: AUTH_REQUIRED,
    upstream: transport,
    transport,
    botName: "SAF",
  });
});

app.post("/api/login", (req, res) => {
  if (!AUTH_REQUIRED) return res.json({ ok: true });
  const pw = String(req.body?.password || "");
  const ok = pw.length === PASSWORD.length && crypto.timingSafeEqual(Buffer.from(pw), Buffer.from(PASSWORD));
  if (!ok) return res.status(401).json({ ok: false, error: "unauthorized", message: "Wrong password." });
  res.setHeader("Set-Cookie", `${COOKIE}=${makeCookie()}; HttpOnly; SameSite=Lax; Path=/; Max-Age=${SESSION_TTL_MS / 1000}`);
  res.json({ ok: true });
});

app.post("/api/logout", (_req, res) => {
  res.setHeader("Set-Cookie", `${COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0`);
  res.json({ ok: true });
});

// REST proxy: /api/<x> → <upstream>/v1/<x>
app.all("/api/*", async (req, res) => {
  if (!isAuthed(req)) return res.status(401).json({ error: "unauthorized", message: "Not logged in." });
  const rest = req.originalUrl.replace(/^\/api\//, "");
  const target = `${upstreamHttp()}/v1/${rest}`;
  try {
    const init = {
      method: req.method,
      headers: { Authorization: `Bearer ${TOKEN}` },
    };
    if (!["GET", "HEAD"].includes(req.method) && req.body && Object.keys(req.body).length) {
      init.headers["Content-Type"] = "application/json";
      init.body = JSON.stringify(req.body);
    }
    const upstream = await fetch(target, init);
    const buf = Buffer.from(await upstream.arrayBuffer());
    res.status(upstream.status);
    const ct = upstream.headers.get("content-type");
    if (ct) res.setHeader("Content-Type", ct);
    res.send(buf);
  } catch (e) {
    res.status(502).json({ error: "upstream_unreachable", message: String(e?.message ?? e) });
  }
});

// Static SPA + history fallback
app.use(express.static(DIST));
app.get("*", (_req, res) => res.sendFile(path.join(DIST, "index.html")));

const server = app.listen(PORT, () => {
  log(`listening on :${PORT} (auth ${AUTH_REQUIRED ? "required" : "OPEN"})`);
  if (!TOKEN) log("WARNING: BOT_API_TOKEN is empty — upstream calls will be rejected.");
});

// ---- WebSocket proxy: /events ↔ <upstream>/v1/events ----
const wss = new WebSocketServer({ noServer: true });
server.on("upgrade", (req, socket, head) => {
  if (!req.url?.startsWith("/events")) { socket.destroy(); return; }
  if (AUTH_REQUIRED && !validCookie(parseCookies(req.headers.cookie || "")[COOKIE])) { socket.destroy(); return; }
  // Connect upstream FIRST and only complete the browser handshake once the bot
  // feed is actually open. That way the client's `onopen` genuinely means
  // "streaming": its reconnect backoff stays honest and there's no connect/drop
  // flapping while the bot is down (the gateway just keeps the client waiting,
  // or fails the upgrade, instead of accepting-then-immediately-closing).
  const upstream = new WebSocket(`ws://${upstreamHost}:${upstreamPort}/v1/events`, {
    headers: { Authorization: `Bearer ${TOKEN}` },
    handshakeTimeout: 8000,
  });
  let upgraded = false;
  const abort = () => {
    if (!upgraded) { try { socket.destroy(); } catch {} }
    try { upstream.close(); } catch {}
  };
  upstream.on("error", abort);
  socket.on("error", abort);
  socket.on("close", () => { try { upstream.close(); } catch {} });
  upstream.once("open", () => {
    upgraded = true;
    wss.handleUpgrade(req, socket, head, (client) => {
      const closeBoth = () => { try { client.close(); } catch {} try { upstream.close(); } catch {} };
      upstream.on("message", (d, isBin) => client.readyState === WebSocket.OPEN && client.send(d, { binary: isBin }));
      client.on("message", (d, isBin) => upstream.readyState === WebSocket.OPEN && upstream.send(d, { binary: isBin }));
      upstream.on("close", closeBoth);
      client.on("close", closeBoth);
      client.on("error", closeBoth);
    });
  });
});
