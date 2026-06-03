// REST client. The browser calls same-origin /api/* (the gateway proxies to the
// bot's /v1/* and injects the bearer token). The dashboard session cookie
// authenticates the browser to the gateway.
import type {
  AccountsResponse,
  Alert,
  AuctionsResponse,
  BotStatus,
  CommandDefinition,
  CommandResult,
  ConfigPatchResult,
  ConfigResponse,
  FlipRecord,
  InventoryResponse,
  ProfitSeries,
  ProfitSummary,
  QueueResponse,
  SaleRecord,
  SessionInfo,
} from "./types";

export class APIError extends Error {
  status: number;
  // Structured error fields the bot returns (e.g. forbidden_fields → forbidden[]).
  code?: string;
  forbidden?: string[];
  constructor(status: number, message: string, extra?: { code?: string; forbidden?: string[] }) {
    super(message);
    this.status = status;
    this.code = extra?.code;
    this.forbidden = extra?.forbidden;
  }
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { credentials: "include", ...init });
  if (!res.ok) {
    let msg = res.statusText;
    let extra: { code?: string; forbidden?: string[] } | undefined;
    try {
      const body = await res.json();
      msg = body.message || body.error || msg;
      extra = { code: body.error, forbidden: body.forbidden };
    } catch {
      /* ignore */
    }
    throw new APIError(res.status, msg, extra);
  }
  return (await res.json()) as T;
}

function post(path: string, body?: unknown): Promise<CommandResult> {
  return req<CommandResult>(path, {
    method: "POST",
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
}

function patchReq<T>(path: string, body: unknown): Promise<T> {
  return req<T>(path, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export const api = {
  // Gateway session (web-only)
  session: () => req<SessionInfo>("/api/session"),
  login: (password: string) => post("/api/login", { password }),
  logout: () => post("/api/logout"),

  // Bot API (proxied)
  status: () => req<BotStatus>("/api/status"),
  accounts: () => req<AccountsResponse>("/api/accounts"),
  profit: () => req<ProfitSummary>("/api/profit"),
  profitSeries: (ign = "all", sinceMs = 0, bucketSec = 1800) =>
    req<ProfitSeries>(`/api/profit/${encodeURIComponent(ign)}/series?since=${sinceMs}&bucket=${bucketSec}`),
  boughtFlips: (limit = 200, account?: string) =>
    req<{ kind: string; flips: FlipRecord[] }>(
      `/api/flips?kind=bought&limit=${limit}${account ? `&account=${encodeURIComponent(account)}` : ""}`
    ).then((r) => r.flips),
  soldFlips: (limit = 200, account?: string) =>
    req<{ kind: string; flips: SaleRecord[] }>(
      `/api/flips?kind=sold&limit=${limit}${account ? `&account=${encodeURIComponent(account)}` : ""}`
    ).then((r) => r.flips),
  queue: (ign: string) => req<QueueResponse>(`/api/accounts/${encodeURIComponent(ign)}/queue`),
  getInventory: (ign: string) =>
    req<InventoryResponse>(`/api/accounts/${encodeURIComponent(ign)}/inventory`),
  getAuctions: (ign: string) =>
    req<AuctionsResponse>(`/api/accounts/${encodeURIComponent(ign)}/auctions`),
  logs: (lines = 300) => req<{ lines: string[] }>(`/api/logs?lines=${lines}`).then((r) => r.lines),
  alerts: (lines = 120) => req<{ alerts: Alert[] }>(`/api/alerts?lines=${lines}`).then((r) => r.alerts),
  commands: () => req<{ commands: CommandDefinition[] }>("/api/commands").then((r) => r.commands),

  execute: (command: string, options: Record<string, unknown> = {}) =>
    post("/api/command", Object.keys(options).length ? { command, options } : { command }),
  transfer: (from: string, to: string, amount: string, stopSource = true) =>
    post("/api/command", { transfer: { from, to, amount, stop_source: stopSource } }),
  executeLine: (line: string) => post("/api/command", { line }),
  executeButton: (button: string) => post("/api/command", { button }),
  control: (action: string) => post(`/api/control/${encodeURIComponent(action)}`),

  // Config editor (gateway proxies /api/config → bot's /v1/config)
  getConfig: () => req<ConfigResponse>("/api/config"),
  patchConfig: (body: Record<string, unknown>) => patchReq<ConfigPatchResult>("/api/config", body),

  // Restart the bot over the SSH tunnel (gateway-native route)
  restart: () => post("/api/restart"),
};
