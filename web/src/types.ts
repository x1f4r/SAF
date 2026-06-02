// Mirrors the bot API JSON (camelCase) — see the macOS app's Models.swift.

export interface BotStatus {
  ok: boolean;
  version?: string;
  name?: string;
  startedAtMs?: number;
  uptimeMs?: number;
  marketMode?: string;
  halted: boolean;
  paused: boolean;
  configured: string[];
  running: string[];
  defaultIgn?: string;
  commandInbox?: string;
  stateBaseDir?: string;
}

export interface AccountStats {
  bought: number;
  sold: number;
  totalProfit: number;
  userFinderFlips: number;
  profitPerHour?: number | null;
  purse?: number | null;
  startedAtMs?: number | null;
  coflDelayMs?: number | null;
  coflPingMs?: number | null;
  coflTier?: string | null;
  coflExpiresAt?: number | null;
  cookieExpiresAt?: number | null;
  hypixelPingMs?: number | null;
  auctionSlotsUsed?: number | null;
  auctionSlotsMax?: number | null;
}

export type AccountStatus = "offline" | "connecting" | "online";

export interface AccountInfo {
  ign: string;
  running: boolean;
  queueSize: number;
  connectionId?: string | null;
  coflConnected?: boolean;
  hasCookie?: boolean;
  status?: AccountStatus;
  ready?: boolean;
  reason?: string | null;
  stats: AccountStats;
  headUrl?: string | null;
}

export interface AccountsResponse {
  configured: string[];
  running: string[];
  defaultIgn?: string;
  readyCount?: number;
  connectedCount?: number;
  accounts: AccountInfo[];
}

export interface Alert {
  ts: string;
  level: "warn" | "error";
  message: string;
}

export interface ProfitFigures {
  totalProfit: number;
  bought: number;
  sold: number;
  userFinderFlips: number;
  profitPerHour?: number | null;
  purse?: number | null;
}

export interface ProfitAccount { ign: string; summary: ProfitFigures; }

export interface LifetimeFigures { totalProfit: number; bought: number; sold: number; }

export interface ProfitSummary {
  totalProfit: number;
  bought: number;
  sold: number;
  purse: number;
  accounts: ProfitAccount[];
  lifetime?: LifetimeFigures;
}

export interface ProfitPoint { ts: number; profit: number; cumulative: number; count: number; }
export interface ProfitSeries { bucketMs: number; points: ProfitPoint[]; }

export interface FlipRecord {
  ts: number;
  account: string;
  item: string;
  weirdItemName?: string;
  tag?: string | null;
  price: number;
  targetPrice: number;
  profit: number;
  finder: string;
  volume?: number | null;
  profitPercentage?: number | null;
  buyKind?: string;
  buySpeedMs?: number | null;
  auctionId: string;
}

export interface SaleRecord {
  ts: number;
  account: string;
  item: string;
  buyer: string;
  price: number;
}

export interface QueueEntry {
  action: any;
  state: string;
  priority: number;
}

export interface QueueResponse { ign: string; queue: QueueEntry[]; bidData?: any; }

export interface CommandChoice { name: string; value: string; }
export interface CommandOption {
  name: string;
  description: string;
  kind: string;
  required: boolean;
  choices: CommandChoice[];
}
export interface CommandDefinition { name: string; description: string; options: CommandOption[]; }

export interface ConfirmAction { title: string; message: string; button: string; }
export interface CommandResult {
  ok?: boolean;
  outcome?: any;
  error?: string;
  message?: string;
  requiresConfirmation?: boolean;
  confirm?: ConfirmAction;
  action?: string;
}

export interface LiveEvent {
  id: number;
  type: string;
  ts: number;
  raw: any;
}

// Gateway session / connection info (web-only).
export interface SessionInfo {
  authenticated: boolean;
  needsPassword: boolean;
  upstream?: string;
  transport?: string;
  connected?: boolean;
  botName?: string;
}

// Derived helpers
export function displayProfit(p?: ProfitSummary): number {
  return p?.lifetime?.totalProfit ?? p?.totalProfit ?? 0;
}
export function displayBought(p?: ProfitSummary): number {
  return Math.max(p?.lifetime?.bought ?? 0, p?.bought ?? 0);
}
export function displaySold(p?: ProfitSummary): number {
  return Math.max(p?.lifetime?.sold ?? 0, p?.sold ?? 0);
}

// Queue entry field accessors (action is arbitrary JSON).
export function entryAuctionId(e: QueueEntry): string | undefined {
  return e.action?.auctionID ?? e.action?.auctionId;
}
export function entryItemName(e: QueueEntry): string | undefined {
  return e.action?.itemName ?? e.action?.item_name;
}
export function entryPrice(e: QueueEntry): number | undefined {
  const p = e.action?.price;
  return typeof p === "number" ? p : undefined;
}
