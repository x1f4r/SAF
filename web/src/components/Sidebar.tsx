import { useStore, type Tab } from "../store";
import { Icon } from "./Icon";
import { StatusDot } from "./UI";

const TABS: { id: Tab; title: string; icon: string }[] = [
  { id: "dashboard", title: "Dashboard", icon: "dashboard" },
  { id: "accounts", title: "Accounts", icon: "accounts" },
  { id: "inventory", title: "Inventory", icon: "cube" },
  { id: "auctions", title: "Auctions", icon: "tag" },
  { id: "flips", title: "Flips", icon: "flips" },
  { id: "profit", title: "Profit", icon: "profit" },
  { id: "queue", title: "Queue", icon: "queue" },
  { id: "logs", title: "Console", icon: "console" },
  { id: "commands", title: "Commands", icon: "command" },
  { id: "config", title: "Config", icon: "slider.horizontal.3" },
  { id: "blacklist", title: "Blacklist", icon: "nosign" },
  { id: "diagnostics", title: "Diagnostics", icon: "activity" },
  { id: "settings", title: "Connection", icon: "connection" },
];

export function Sidebar() {
  const store = useStore();
  return (
    <div style={{
      width: 232, flex: "none", display: "flex", flexDirection: "column",
      background: "var(--sidebar)", borderRight: "1px solid var(--stroke)",
    }}>
      {/* Brand */}
      <div style={{ display: "flex", alignItems: "center", gap: 11, padding: "26px 18px 22px" }}>
        <div style={{
          width: 38, height: 38, borderRadius: 11, background: "var(--brand-grad)",
          display: "flex", alignItems: "center", justifyContent: "center",
          boxShadow: "0 3px 10px rgba(94,200,255,0.5)",
        }}>
          <Icon name="bolt" size={18} strokeWidth={2.6} style={{ color: "rgba(0,0,0,0.85)" }} />
        </div>
        <div>
          <div style={{ fontSize: 20, fontWeight: 900, color: "var(--text-1)", lineHeight: 1 }}>SAF</div>
          <div style={{ fontSize: 10.5, fontWeight: 700, color: "var(--text-3)", marginTop: 2 }}>Auction Flipper</div>
        </div>
      </div>

      {/* Nav */}
      <div style={{ display: "flex", flexDirection: "column", gap: 4, padding: "0 12px" }}>
        {TABS.map((t) => {
          const sel = store.tab === t.id;
          return (
            <button key={t.id} onClick={() => store.setTab(t.id)} className="nav-row" data-sel={sel}
              style={{
                display: "flex", alignItems: "center", gap: 11, padding: "9px 12px", border: 0,
                borderRadius: 11, textAlign: "left", fontSize: 13.5,
                fontWeight: sel ? 700 : 600,
                color: sel ? "rgba(0,0,0,0.85)" : "var(--text-2)",
                background: sel ? "var(--brand-grad-h)" : "transparent",
                boxShadow: sel ? "0 2px 8px rgba(94,200,255,0.35)" : "none",
              }}>
              <Icon name={t.icon} size={14} style={{ width: 22 }} />
              <span>{t.title}</span>
            </button>
          );
        })}
      </div>

      <div style={{ flex: 1 }} />

      {/* Connection badge */}
      <div style={{ padding: "0 12px 14px" }}>
        <div className="surface" style={{ padding: 12, display: "flex", alignItems: "center", gap: 10 }}>
          <StatusDot color={store.reachable ? "var(--profit)" : "var(--warn)"} pulse />
          <div style={{ minWidth: 0, flex: 1 }}>
            <div style={{ fontSize: 12, fontWeight: 700, color: "var(--text-1)" }}>
              {store.reachable ? "Connected" : "Connecting…"}
            </div>
            <div style={{ fontSize: 10.5, fontWeight: 600, color: "var(--text-3)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {store.status?.name ?? store.session?.upstream ?? "SAF"}
            </div>
          </div>
          {store.streamConnected && <Icon name="live" size={11} style={{ color: "var(--profit)" }} />}
        </div>
      </div>
    </div>
  );
}

export const SIDEBAR_CSS = `.nav-row:hover[data-sel="false"] { background: rgba(255,255,255,0.06) !important; color: var(--text-1) !important; }`;
