import { Icon } from "../components/Icon";
import { EmptyState, Page, PageHeader, SectionLabel } from "../components/UI";
import { Fmt } from "../format";
import { useStore } from "../store";

const STATE_COLOR: Record<string, string> = {
  connected: "var(--profit)", connecting: "var(--warn)", degraded: "var(--gold)", disconnected: "var(--loss)",
};

function HealthItem({ icon, label, value, color }: { icon: string; label: string; value: string; color: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <div style={{ width: 32, height: 32, borderRadius: "50%", background: `color-mix(in srgb, ${color} 15%, transparent)`, display: "flex", alignItems: "center", justifyContent: "center" }}>
        <Icon name={icon} size={15} style={{ color }} />
      </div>
      <div>
        <div style={{ fontSize: 9, fontWeight: 700, letterSpacing: "0.4px", textTransform: "uppercase", color: "var(--t3)" }}>{label}</div>
        <div style={{ fontSize: 12.5, fontWeight: 700, color: "var(--t1)" }}>{value}</div>
      </div>
    </div>
  );
}

export function DiagnosticsView() {
  const s = useStore();
  const cs = s.connectionState;

  function reportText() {
    return [
      `# SAF Dashboard diagnostics — ${new Date().toISOString()}`,
      `connection=${cs} reachable=${s.reachable} liveFeed=${s.streamConnected} bot=${s.status?.name ?? "—"} accountsReady=${s.readyCount}/${s.configuredCount}`,
      "", "## Connection log",
      ...s.diag.map((d) => `${new Date(d.ts).toISOString()} ${d.level.toUpperCase()} ${d.message}`),
      "", "## Bot alerts",
      ...s.alerts.map((a) => `${a.ts} ${a.level.toUpperCase()} ${a.message}`),
    ].join("\n");
  }

  function copyLog() {
    navigator.clipboard?.writeText(reportText());
    s.showToast?.({ icon: "copy", tint: "var(--accent)", title: "Diagnostics copied", detail: "Paste it into a bug report" });
  }

  function downloadLog() {
    const blob = new Blob([reportText()], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `saf-diagnostics-${new Date().toISOString().replace(/:/g, "-")}.txt`;
    a.click();
    URL.revokeObjectURL(url);
    s.showToast?.({ icon: "check", tint: "var(--profit)", title: "Report downloaded", detail: a.download });
  }

  const reportBtn = {
    display: "inline-flex", alignItems: "center", gap: 6, padding: "8px 13px", borderRadius: 999,
    border: "1px solid var(--stroke)", background: "transparent", color: "var(--t2)", fontSize: 13, fontWeight: 600,
  } as const;

  return (
    <Page>
      <PageHeader title="Diagnostics" subtitle="Connection health, bot alerts, and a troubleshooting log."
        trailing={<div style={{ display: "flex", gap: 10 }}>
          <button onClick={copyLog} style={reportBtn}><Icon name="copy" size={12} />Copy report</button>
          <button onClick={downloadLog} style={reportBtn}><Icon name="arrow.down.to.line" size={12} />Download report</button>
        </div>} />

      {/* Health strip */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 26 }}>
        <HealthItem icon="link" label="Connection" value={cs[0].toUpperCase() + cs.slice(1)} color={STATE_COLOR[cs]} />
        <HealthItem icon="antenna.radiowaves.left.and.right" label="API" value={s.reachable ? "Reachable" : "Unreachable"} color={s.reachable ? "var(--profit)" : "var(--loss)"} />
        <HealthItem icon="dot.radiowaves.left.and.right" label="Live feed" value={s.streamConnected ? "Streaming" : "Off"} color={s.streamConnected ? "var(--profit)" : "var(--warn)"} />
        <HealthItem icon="bolt.fill" label="Bot" value={s.status?.name ?? "—"} color="var(--accent)" />
        <HealthItem icon="person.2.fill" label="Accounts ready" value={`${s.readyCount}/${s.configuredCount}`} color={s.readyCount > 0 ? "var(--profit)" : "var(--warn)"} />
      </div>

      <div style={{ display: "flex", gap: 40, alignItems: "flex-start" }}>
        {/* Bot alerts */}
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 12 }}>
          <SectionLabel title="Bot Alerts" icon="exclamationmark.triangle.fill" trailing={<span style={{ fontSize: 11, fontWeight: 600, color: "var(--t3)", textTransform: "none" }}>{s.alerts.length}</span>} />
          {s.alerts.length === 0
            ? <EmptyState icon="checkmark.seal.fill" text="No warnings or errors from the bot." height={140} />
            : <div>{s.alerts.slice().reverse().slice(0, 60).map((a, i) => {
                const color = a.level === "error" ? "var(--loss)" : "var(--warn)";
                return (
                  <div key={i} style={{ display: "flex", gap: 10, padding: "8px 4px", borderBottom: "1px solid color-mix(in srgb, var(--stroke) 50%, transparent)" }}>
                    <span style={{ width: 7, height: 7, borderRadius: "50%", background: color, marginTop: 5, flexShrink: 0 }} />
                    <div style={{ minWidth: 0, flex: 1 }}>
                      <div className="mono" style={{ fontSize: 11.5, color: "var(--t1)", wordBreak: "break-word" }}>{stripPrefix(a.message)}</div>
                      <div style={{ fontSize: 10, color: "var(--t3)", marginTop: 1 }}>{relTime(a.ts)}</div>
                    </div>
                  </div>
                );
              })}</div>}
        </div>

        {/* Connection log */}
        <div style={{ width: 360, flexShrink: 0, display: "flex", flexDirection: "column", gap: 12 }}>
          <SectionLabel title="Connection Log" icon="waveform.path.ecg" />
          {s.diag.length === 0
            ? <EmptyState icon="waveform.path.ecg" text="No connection events yet." height={140} />
            : <div>{s.diag.slice(0, 80).map((d) => {
                const color = d.level === "error" ? "var(--loss)" : d.level === "warn" ? "var(--warn)" : "var(--accent)";
                return (
                  <div key={d.id} style={{ display: "flex", gap: 9, padding: "7px 4px", borderBottom: "1px solid color-mix(in srgb, var(--stroke) 50%, transparent)" }}>
                    <span style={{ width: 7, height: 7, borderRadius: "50%", background: color, marginTop: 4, flexShrink: 0 }} />
                    <div style={{ minWidth: 0, flex: 1 }}>
                      <div style={{ fontSize: 12, color: "var(--t1)" }}>{d.message}</div>
                      <div className="num" style={{ fontSize: 10, color: "var(--t3)", marginTop: 1 }}>{Fmt.clock(d.ts)}</div>
                    </div>
                  </div>
                );
              })}</div>}
        </div>
      </div>
    </Page>
  );
}

function relTime(iso: string): string {
  const t = Date.parse(iso);
  return isNaN(t) ? iso : Fmt.relative(t);
}

// Drop the leading "<ts> LEVEL target:" so the alert reads cleanly.
function stripPrefix(message: string): string {
  const m = message.match(/^\S+\s+(WARN|ERROR)\s+(.*)$/);
  return m ? m[2] : message;
}
