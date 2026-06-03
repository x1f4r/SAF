// Central app state — the web port of the macOS app's AppStore. Provides
// session/login, REST polling, the live event stream, and command actions.
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api, APIError } from "./api";
import { EventStream } from "./events";
import { Fmt } from "./format";
import { Notify } from "./notify";
import type {
  AccountsResponse,
  Alert,
  BotStatus,
  CommandDefinition,
  FlipRecord,
  LiveEvent,
  ProfitSeries,
  ProfitSummary,
  SaleRecord,
  SessionInfo,
} from "./types";

export type Tab =
  | "dashboard" | "accounts" | "flips" | "profit"
  | "queue" | "logs" | "commands" | "blacklist" | "config" | "diagnostics" | "settings";

export interface DiagEvent {
  id: number;
  ts: number;
  level: "info" | "warn" | "error";
  message: string;
}

export interface Toast {
  id: number;
  icon: string;
  tint: string;
  title: string;
  detail?: string;
}

interface StoreValue {
  phase: "loading" | "login" | "live";
  session: SessionInfo | null;
  tab: Tab;
  setTab: (t: Tab) => void;

  status?: BotStatus;
  accounts?: AccountsResponse;
  profit?: ProfitSummary;
  series?: ProfitSeries;
  bought: FlipRecord[];
  sold: SaleRecord[];
  logs: string[];
  events: LiveEvent[];
  commands: CommandDefinition[];
  alerts: Alert[];
  diag: DiagEvent[];

  streamConnected: boolean;
  reachable: boolean;
  lastError?: string;
  toast?: Toast;

  isHalted: boolean;
  running: boolean;
  runningCount: number;
  configuredCount: number;
  connectedCount: number;
  readyCount: number;
  weekProfit: number;
  ppHour: number;
  connectionState: "disconnected" | "connecting" | "connected" | "degraded";

  logDiag: (level: DiagEvent["level"], message: string) => void;

  login: (password: string) => Promise<boolean>;
  logout: () => void;
  refresh: () => Promise<void>;
  runRestart: () => void;
  runControl: (action: string) => void;
  runCommand: (name: string, options?: Record<string, unknown>, label?: string) => void;
  runButton: (button: string, title: string) => void;
  runLine: (line: string) => void;
  runTransfer: (from: string, to: string, amount: string, stopSource: boolean) => void;
  showToast: (t: Omit<Toast, "id">) => void;
}

const Ctx = createContext<StoreValue | null>(null);
export const useStore = () => {
  const v = useContext(Ctx);
  if (!v) throw new Error("useStore outside provider");
  return v;
};

let toastSeq = 1;

// Diagnostics history survives reloads via localStorage (capped per the contract).
const DIAG_KEY = "saf_diag_history";
const DIAG_PERSIST_CAP = 500;

function loadDiagHistory(): DiagEvent[] {
  try {
    const raw = JSON.parse(localStorage.getItem(DIAG_KEY) || "[]");
    return Array.isArray(raw) ? (raw as DiagEvent[]) : [];
  } catch {
    return [];
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<StoreValue["phase"]>("loading");
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [tab, setTab] = useState<Tab>("dashboard");

  const [status, setStatus] = useState<BotStatus>();
  const [accounts, setAccounts] = useState<AccountsResponse>();
  const [profit, setProfit] = useState<ProfitSummary>();
  const [series, setSeries] = useState<ProfitSeries>();
  const [bought, setBought] = useState<FlipRecord[]>([]);
  const [sold, setSold] = useState<SaleRecord[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [events, setEvents] = useState<LiveEvent[]>([]);
  const [commands, setCommands] = useState<CommandDefinition[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [diag, setDiag] = useState<DiagEvent[]>(loadDiagHistory);

  const [streamConnected, setStreamConnected] = useState(false);
  const [reachable, setReachable] = useState(false);
  const [lastError, setLastError] = useState<string>();
  const [toast, setToast] = useState<Toast>();

  const tickRef = useRef(0);
  const streamRef = useRef<EventStream | null>(null);
  // Start the id counter past any restored history so reloaded ids stay unique.
  const diagSeq = useRef(diag.reduce((max, d) => Math.max(max, d.id), 0) + 1);
  const wasReachable = useRef(false);

  const logDiag = useCallback((level: DiagEvent["level"], message: string) => {
    const event = { id: diagSeq.current++, ts: Date.now(), level, message };
    setDiag((prev) => [event, ...prev].slice(0, 300));
    // Persist a deeper history (cap 500) so Diagnostics survives a reload.
    try {
      const history = [event, ...loadDiagHistory()].slice(0, DIAG_PERSIST_CAP);
      localStorage.setItem(DIAG_KEY, JSON.stringify(history));
    } catch {
      /* storage full / unavailable — keep the in-memory log only */
    }
  }, []);

  const showToast = useCallback((t: Omit<Toast, "id">) => {
    const full = { ...t, id: toastSeq++ };
    setToast(full);
    setTimeout(() => setToast((cur) => (cur?.id === full.id ? undefined : cur)), 3600);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [s, a, p] = await Promise.all([api.status(), api.accounts(), api.profit()]);
      setStatus(s);
      setAccounts(a);
      setProfit(p);
      setReachable(true);
      setLastError(undefined);
      if (!wasReachable.current) { logDiag("info", "Connected to the bot."); wasReachable.current = true; }
      const tick = tickRef.current;
      if (tick % 3 === 0) {
        api.profitSeries("all", 0, 1800).then(setSeries).catch(() => {});
        api.boughtFlips(200).then(setBought).catch(() => {});
        api.soldFlips(200).then(setSold).catch(() => {});
        api.alerts(120).then(setAlerts).catch(() => {});
      }
      api.logs(300).then(setLogs).catch(() => {});
      setCommands((cur) => {
        if (cur.length === 0) api.commands().then(setCommands).catch(() => {});
        return cur;
      });
    } catch (e) {
      setReachable(false);
      if (e instanceof APIError && e.status === 401) {
        logDiag("warn", "Session expired — please sign in again.");
        setPhase("login");
        return;
      }
      const msg = e instanceof Error ? e.message : String(e);
      setLastError(msg);
      if (wasReachable.current) { logDiag("error", "Lost connection to the bot: " + msg); wasReachable.current = false; }
    }
  }, [logDiag]);

  const applyEvent = useCallback((event: LiveEvent) => {
    setEvents((prev) => [event, ...prev].slice(0, 250));
    if (event.type === "purchase") {
      const r = event.raw as FlipRecord;
      setBought((prev) => [r, ...prev].slice(0, 400));
      showToast({
        icon: "cart", tint: "var(--profit)", title: `Bought ${r.item}`,
        detail: `${Fmt.coins(r.price)} → ${Fmt.signedCoins(r.profit)} profit`,
      });
      Notify.show(`Bought ${r.item}`, `${Fmt.coins(r.price)} → ${Fmt.signedCoins(r.profit)} profit`, `buy-${r.auctionId}`);
    } else if (event.type === "sold") {
      const r = event.raw as SaleRecord;
      setSold((prev) => [r, ...prev].slice(0, 400));
      showToast({ icon: "seal", tint: "var(--gold)", title: `Sold ${r.item}`, detail: Fmt.coins(r.price) });
      Notify.show(`Sold ${r.item}`, Fmt.coins(r.price), "sold");
    } else if (event.type === "state") {
      setStatus((cur) => (cur ? { ...cur, halted: !!event.raw.halted, paused: !!event.raw.paused } : cur));
    } else if (event.type === "notification") {
      const n = event.raw.notification;
      const title = n?.title ?? "Notification";
      const kind = String(n?.kind ?? "").toLowerCase();
      if (kind.includes("error") || kind.includes("warn")) {
        Notify.show(`SAF: ${title}`, String(n?.body ?? ""), "alert");
      }
    }
  }, [showToast]);

  // Boot: check session.
  const boot = useCallback(async () => {
    try {
      const s = await api.session();
      setSession(s);
      if (s.authenticated) {
        setPhase("live");
      } else {
        setPhase("login");
      }
    } catch {
      // No gateway session endpoint → assume direct/live.
      setPhase("live");
    }
  }, []);

  useEffect(() => { boot(); }, [boot]);

  // Live loop: poll + stream while authenticated.
  useEffect(() => {
    if (phase !== "live") return;
    let cancelled = false;
    refresh();
    const stream = new EventStream();
    stream.onEvent = applyEvent;
    let lastStreamState: boolean | null = null;
    stream.onState = (connected) => {
      setStreamConnected(connected);
      if (lastStreamState !== null && lastStreamState !== connected) {
        logDiag(connected ? "info" : "warn", connected ? "Live feed connected." : "Live feed dropped — reconnecting.");
        // Reconcile on reconnect: a dropped feed may have missed buys/sells, so
        // pull a fresh full snapshot the moment the link is back.
        if (connected) refresh();
      }
      lastStreamState = connected;
    };
    stream.start();
    streamRef.current = stream;
    const timer = setInterval(() => {
      if (cancelled) return;
      tickRef.current++;
      refresh();
    }, 3000);
    return () => {
      cancelled = true;
      clearInterval(timer);
      stream.stop();
      streamRef.current = null;
    };
  }, [phase, refresh, applyEvent, logDiag]);

  const login = useCallback(async (password: string) => {
    try {
      const res = await api.login(password);
      if (res.ok) {
        const s = await api.session();
        setSession(s);
        setPhase("live");
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, []);

  const logout = useCallback(() => {
    api.logout().catch(() => {});
    setPhase("login");
  }, []);

  const runRestart = useCallback(() => {
    if (!window.confirm("Restart the bot now? Every account will disconnect and come back up.")) return;
    logDiag("warn", "Restart requested from the dashboard.");
    api.restart()
      .then((res) => {
        showToast({
          icon: "arrow.clockwise", tint: "var(--accent)", title: "Restart initiated",
          detail: res?.message ?? "The bot is restarting over SSH.",
        });
        logDiag("info", res?.message ?? "Restart initiated over SSH.");
      })
      .catch((e) => {
        const msg = String(e?.message ?? e);
        showToast({ icon: "x", tint: "var(--loss)", title: "Restart failed", detail: msg });
        logDiag("error", "Restart failed: " + msg);
      });
  }, [logDiag, showToast]);

  const runControl = useCallback((action: string) => {
    api.control(action)
      .then(() => {
        showToast({
          icon: action === "start" ? "play" : "pause",
          tint: action === "start" ? "var(--profit)" : "var(--warn)",
          title: action === "start" ? "Bot started" : "Bot stopped",
        });
        refresh();
      })
      .catch((e) => showToast({ icon: "x", tint: "var(--loss)", title: "Failed", detail: String(e?.message ?? e) }));
  }, [refresh, showToast]);

  const runCommand = useCallback((name: string, options: Record<string, unknown> = {}, label?: string) => {
    api.execute(name, options)
      .then((res) => {
        if (res.requiresConfirmation && res.confirm) {
          showToast({ icon: "alert", tint: "var(--warn)", title: res.confirm.title, detail: "Confirm in Commands" });
        } else {
          showToast({ icon: "check", tint: "var(--accent)", title: label ?? `/${name}`, detail: "Sent" });
        }
        refresh();
      })
      .catch((e) => showToast({ icon: "x", tint: "var(--loss)", title: "Command failed", detail: String(e?.message ?? e) }));
  }, [refresh, showToast]);

  const runButton = useCallback((button: string, title: string) => {
    api.executeButton(button)
      .then(() => { showToast({ icon: "check", tint: "var(--accent)", title, detail: "Sent" }); refresh(); })
      .catch((e) => showToast({ icon: "x", tint: "var(--loss)", title: "Failed", detail: String(e?.message ?? e) }));
  }, [refresh, showToast]);

  const runLine = useCallback((line: string) => {
    api.executeLine(line)
      .then(() => { showToast({ icon: "terminal", tint: "var(--accent)", title: line, detail: "Sent" }); refresh(); })
      .catch((e) => showToast({ icon: "x", tint: "var(--loss)", title: "Failed", detail: String(e?.message ?? e) }));
  }, [refresh, showToast]);

  const runTransfer = useCallback((from: string, to: string, amount: string, stopSource: boolean) => {
    api.transfer(from, to, amount, stopSource)
      .then(() => { showToast({ icon: "check", tint: "var(--accent)", title: `Transfer ${from} → ${to}`, detail: `${amount} sent` }); refresh(); })
      .catch((e) => showToast({ icon: "x", tint: "var(--loss)", title: "Transfer failed", detail: String(e?.message ?? e) }));
  }, [refresh, showToast]);

  const connectedCount = accounts?.connectedCount
    ?? (accounts?.accounts ?? []).filter((a) => a.status === "online").length;
  const readyCount = accounts?.readyCount
    ?? (accounts?.accounts ?? []).filter((a) => a.ready).length;
  const connectionState: StoreValue["connectionState"] = !reachable
    ? (phase === "live" ? "connecting" : "disconnected")
    : (streamConnected ? "connected" : "degraded");

  const value = useMemo<StoreValue>(() => ({
    phase, session, tab, setTab,
    status, accounts, profit, series, bought, sold, logs, events, commands, alerts, diag,
    streamConnected, reachable, lastError, toast,
    isHalted: status?.halted ?? true,
    running: !(status?.halted ?? true),
    runningCount: status?.running.length ?? 0,
    configuredCount: status?.configured.length ?? accounts?.configured.length ?? 0,
    connectedCount, readyCount, connectionState,
    weekProfit: (() => {
      const cutoff = Date.now() - 7 * 86400_000;
      return bought.filter((f) => f.ts >= cutoff).reduce((x, f) => x + f.profit, 0);
    })(),
    ppHour: (accounts?.accounts ?? []).reduce((x, a) => x + (a.running ? a.stats.profitPerHour ?? 0 : 0), 0),
    login, logout, refresh, runRestart, runControl, runCommand, runButton, runLine, runTransfer, showToast, logDiag,
  }), [phase, session, tab, status, accounts, profit, series, bought, sold, logs, events,
    commands, alerts, diag, streamConnected, reachable, lastError, toast, connectedCount, readyCount, connectionState,
    login, logout, refresh, runRestart, runControl, runCommand, runButton, runLine, runTransfer, showToast, logDiag]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
