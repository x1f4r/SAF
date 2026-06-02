import { useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { Toast } from "./components/Toast";
import { useStore, type Tab } from "./store";
import { AccountsView } from "./views/AccountsView";
import { CommandsView } from "./views/CommandsView";
import { ConnectionView } from "./views/ConnectionView";
import { DashboardView } from "./views/DashboardView";
import { FlipsView } from "./views/FlipsView";
import { LoginView } from "./views/LoginView";
import { LogsView } from "./views/LogsView";
import { ProfitView } from "./views/ProfitView";
import { QueueView } from "./views/QueueView";

const VIEWS: Record<Tab, () => JSX.Element> = {
  dashboard: DashboardView,
  accounts: AccountsView,
  flips: FlipsView,
  profit: ProfitView,
  queue: QueueView,
  logs: LogsView,
  commands: CommandsView,
  settings: ConnectionView,
};

const ORDER: Tab[] = ["dashboard", "accounts", "flips", "profit", "queue", "logs", "commands", "settings"];

export function App() {
  const store = useStore();
  // ⌘/Ctrl+1–8 (and plain 1–8) switch tabs.
  useEffect(() => {
    if (store.phase !== "live") return;
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const n = parseInt(e.key, 10);
      if (n >= 1 && n <= ORDER.length) store.setTab(ORDER[n - 1]);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store.phase, store]);

  const View = VIEWS[store.tab];
  return (
    <>
      <div className="app-bg" />
      <div style={{ position: "relative", zIndex: 1, height: "100%" }}>
        {store.phase === "loading" && (
          <div style={{ height: "100%", display: "flex", alignItems: "center", justifyContent: "center" }}>
            <div style={{ fontSize: 48, fontWeight: 900, background: "var(--brand-grad)", WebkitBackgroundClip: "text", backgroundClip: "text", color: "transparent" }}>SAF</div>
          </div>
        )}
        {store.phase === "login" && <LoginView />}
        {store.phase === "live" && (
          <div style={{ display: "flex", height: "100%" }}>
            <Sidebar />
            <div style={{ flex: 1, minWidth: 0, height: "100%" }}><View /></div>
          </div>
        )}
      </div>
      <Toast />
    </>
  );
}
