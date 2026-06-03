import { useEffect, useRef, useState } from "react";
import { useStore } from "../store";
import { Fmt } from "../format";
import { Notify } from "../notify";
import {
  Page,
  PageHeader,
  Surface,
  SectionLabel,
  GhostButton,
  CodeBlock,
  VRule,
} from "../components/UI";
import { Icon } from "../components/Icon";

function StatusItem({ label, value, color, icon }: {
  label: string; value: string; color: string; icon: string;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <span style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        width: 32, height: 32, borderRadius: 999,
        background: `color-mix(in srgb, ${color} 15%, transparent)`, color,
      }}>
        <Icon name={icon} size={15} />
      </span>
      <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
        <span style={{ fontSize: 9, fontWeight: 700, letterSpacing: 0.4, textTransform: "uppercase", color: "var(--text-3)" }}>
          {label}
        </span>
        <span style={{ fontSize: 12.5, fontWeight: 700, color: "var(--text-1)" }}>{value}</span>
      </div>
    </div>
  );
}

function InfoRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 16 }}>
      <span style={{ fontSize: 12, fontWeight: 700, color: "var(--text-2)" }}>{label}</span>
      <span className={mono ? "mono" : "num"} style={{ fontSize: 12.5, fontWeight: 600, color: "var(--text-1)" }}>
        {value}
      </span>
    </div>
  );
}

function NotificationsToggle() {
  const [on, setOn] = useState(Notify.enabled);
  if (!Notify.supported) return null;
  return (
    <Surface>
      <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
        <span style={{ display: "inline-flex", alignItems: "center", justifyContent: "center", width: 32, height: 32, borderRadius: 999, background: "color-mix(in srgb, var(--accent) 15%, transparent)", color: "var(--accent)" }}>
          <Icon name="bell.fill" size={15} />
        </span>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 13, fontWeight: 700, color: "var(--text-1)" }}>Desktop notifications</div>
          <div style={{ fontSize: 11.5, fontWeight: 600, color: "var(--text-3)" }}>Buys, sells, and errors when this tab is in the background.</div>
        </div>
        <button onClick={async () => { if (on) { Notify.disable(); setOn(false); } else { setOn(await Notify.enable()); } }}
          style={{ width: 44, height: 26, borderRadius: 999, border: "none", cursor: "pointer", padding: 3, background: on ? "var(--brand-grad-h)" : "rgba(255,255,255,0.1)", display: "flex", justifyContent: on ? "flex-end" : "flex-start", transition: "all 0.15s" }}>
          <span style={{ width: 20, height: 20, borderRadius: "50%", background: "#fff" }} />
        </button>
      </div>
    </Surface>
  );
}

export function ConnectionView() {
  const store = useStore();

  // The store does not expose a lastRefresh timestamp, so capture the wall-clock
  // time whenever fresh data lands (status/reachable change).
  const [lastSync, setLastSync] = useState<number | null>(null);
  const seen = useRef<unknown>(null);
  useEffect(() => {
    if (store.reachable && store.status !== seen.current) {
      seen.current = store.status;
      setLastSync(Date.now());
    }
  }, [store.reachable, store.status]);

  const transport = store.session?.transport ?? "—";
  const upstream = store.session?.upstream ?? store.session?.transport ?? "—";

  return (
    <Page gap={18}>
      <PageHeader title="Connection" subtitle="How this dashboard reaches your bot" />

      {/* Status strip */}
      <Surface>
        <div style={{ display: "flex", alignItems: "center", gap: 20 }}>
          <StatusItem
            label="Tunnel"
            value={transport}
            color={store.reachable ? "var(--profit)" : "var(--warn)"}
            icon="lock.shield.fill"
          />
          <VRule height={38} />
          <StatusItem
            label="API"
            value={store.reachable ? "Reachable" : "Unreachable"}
            color={store.reachable ? "var(--profit)" : "var(--warn)"}
            icon="antenna.radiowaves.left.and.right"
          />
          <VRule height={38} />
          <StatusItem
            label="Live feed"
            value={store.streamConnected ? "Streaming" : "Off"}
            color={store.streamConnected ? "var(--profit)" : "var(--text-3)"}
            icon="dot.radiowaves.left.and.right"
          />
          <VRule height={38} />
          <StatusItem
            label="Bot"
            value={store.status?.name ?? "—"}
            color="var(--accent)"
            icon="bolt.fill"
          />
          <span style={{ flex: 1 }} />
          {lastSync != null && (
            <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 2 }}>
              <span style={{ fontSize: 9, fontWeight: 700, letterSpacing: 0.4, textTransform: "uppercase", color: "var(--text-3)" }}>
                Last sync
              </span>
              <span className="num" style={{ fontSize: 12, fontWeight: 600, color: "var(--text-2)" }}>
                {Fmt.clock(lastSync)}
              </span>
            </div>
          )}
        </div>
      </Surface>

      {/* Gateway (read-only) */}
      <Surface padding={22}>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <SectionLabel
            title="Gateway"
            icon="server.rack"
            trailing={
              <div style={{ display: "flex", gap: 10 }}>
                <GhostButton title="Restart bot" icon="arrow.clockwise" danger
                  disabled={!store.session?.transport?.includes("ssh")}
                  onClick={() => store.runRestart()} />
                <GhostButton title="Log out" icon="lock.fill" onClick={() => store.logout()} />
                <GhostButton title="Refresh" icon="arrow.clockwise" onClick={() => store.refresh()} />
              </div>
            }
          />

          <div style={{
            fontSize: 11.5, fontWeight: 600, color: "var(--text-3)", lineHeight: 1.5,
            display: "flex", alignItems: "center", gap: 7,
          }}>
            <Icon name="lock.shield.fill" size={12} style={{ color: "var(--text-3)" }} />
            Restarting the bot needs the SSH tunnel transport — it's unavailable on a direct connection.
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <InfoRow label="Upstream" value={upstream} mono />
            <InfoRow label="Transport" value={transport} />
            <InfoRow label="Market mode" value={store.status?.marketMode ?? "—"} />
            <InfoRow label="Accounts" value={Fmt.int(store.configuredCount)} />
          </div>

          <div style={{
            fontSize: 12, fontWeight: 600, color: "var(--text-3)", lineHeight: 1.5,
            padding: "11px 14px", borderRadius: 10,
            background: "rgba(255,255,255,0.03)", border: "1px solid var(--stroke)",
          }}>
            Connection settings are configured server-side, through the container's environment
            (<span className="mono" style={{ color: "var(--accent)" }}>BOT_API_URL</span> /{" "}
            <span className="mono" style={{ color: "var(--accent)" }}>SSH_DEST</span> /{" "}
            <span className="mono" style={{ color: "var(--accent)" }}>BOT_API_TOKEN</span>). They cannot
            be edited from the browser — change the env and restart the container to apply new values.
          </div>
        </div>
      </Surface>

      {/* Notifications */}
      <NotificationsToggle />

      {/* Setup help */}
      <Surface>
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          <SectionLabel title="Set up the bot API" icon="wrench.and.screwdriver.fill" />
          <span style={{ fontSize: 12, fontWeight: 600, color: "var(--text-2)", lineHeight: 1.5 }}>
            On any machine with SSH access to the bot host, run the helper to enable the API,
            generate a token, and restart SAF:
          </span>
          <CodeBlock text="scripts/setup-dashboard-api.sh <ssh-destination>" />
          <span style={{ fontSize: 11.5, fontWeight: 600, color: "var(--text-3)", lineHeight: 1.5 }}>
            The printed token goes in the container's{" "}
            <span className="mono" style={{ color: "var(--accent)" }}>BOT_API_TOKEN</span>. Switching to a
            new host just means updating the env (BOT_API_URL / SSH_DEST / BOT_API_TOKEN) and restarting the
            container — your bot's saved state lives on the server.
          </span>
        </div>
      </Surface>
    </Page>
  );
}
