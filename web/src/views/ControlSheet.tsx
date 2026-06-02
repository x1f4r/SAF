// Per-account control modal — a full-screen dim overlay with a centered card that
// drives every per-account action (lifecycle, market, bank, transfer) through the store.
import { useState } from "react";
import { AccountAvatar } from "../components/ItemIcon";
import { Icon } from "../components/Icon";
import {
  ActionChip, Hairline, PrimaryButton, SectionLabel, StatusDot, Surface,
} from "../components/UI";
import { useStore } from "../store";
import type { AccountInfo } from "../types";

function statusBadge(a: AccountInfo): { label: string; color: string; filled: boolean } {
  const status = a.status ?? (a.running ? "online" : "offline");
  if (status === "offline") return { label: "Offline", color: "var(--t3)", filled: false };
  if (status === "connecting") return { label: "Connecting", color: "var(--warn)", filled: false };
  if (a.ready) return { label: "Ready", color: "var(--profit)", filled: true };
  return { label: a.reason ?? "Not ready", color: "var(--warn)", filled: false };
}

// A small selectable chip (used for the duration / region quick actions).
function QuickChip({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button onClick={onClick} style={{
      fontSize: 12, fontWeight: 700, color: "var(--t2)", padding: "6px 12px", borderRadius: 999,
      background: "rgba(255,255,255,0.04)", border: "1px solid var(--stroke)", whiteSpace: "nowrap",
      transition: "all 0.12s ease",
    }}
      onMouseEnter={(e) => { e.currentTarget.style.background = "rgba(94,200,255,0.14)"; e.currentTarget.style.color = "var(--accent)"; e.currentTarget.style.borderColor = "rgba(94,200,255,0.4)"; }}
      onMouseLeave={(e) => { e.currentTarget.style.background = "rgba(255,255,255,0.04)"; e.currentTarget.style.color = "var(--t2)"; e.currentTarget.style.borderColor = "var(--stroke)"; }}
    >{label}</button>
  );
}

// A flat pill toggle, on = accent-filled.
function Toggle({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button onClick={() => onChange(!on)} style={{
      display: "inline-flex", alignItems: "center", gap: 8, padding: "7px 12px", borderRadius: 999,
      fontSize: 12, fontWeight: 700, color: on ? "rgba(0,0,0,0.85)" : "var(--t2)",
      background: on ? "var(--accent)" : "rgba(255,255,255,0.04)",
      border: `1px solid ${on ? "transparent" : "var(--stroke)"}`,
    }}>
      <span style={{
        width: 14, height: 14, borderRadius: "50%", flex: "none",
        background: on ? "rgba(0,0,0,0.55)" : "var(--t3)",
        boxShadow: on ? "0 0 6px rgba(0,0,0,0.3)" : undefined,
      }} />
      {label}
    </button>
  );
}

function Field({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <input className="field" value={value} placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)} style={{ flex: 1, minWidth: 0 }} />
  );
}

export function ControlSheet({ account, onClose }: { account: AccountInfo; onClose: () => void }) {
  const store = useStore();
  const ign = account.ign;
  const badge = statusBadge(account);

  const [bankAmount, setBankAmount] = useState("all");
  const [bankPersonal, setBankPersonal] = useState(false);

  const otherAccounts = (store.accounts?.accounts ?? []).filter((a) => a.ign !== ign);
  const [dest, setDest] = useState(otherAccounts[0]?.ign ?? "");
  const [transferAmount, setTransferAmount] = useState("all");
  const [keepRunning, setKeepRunning] = useState(false);

  const restart = async () => {
    store.runCommand("stop_bot", { username: ign }, `Restart ${ign}`);
    store.runCommand("start_bot", { username: ign }, `Restart ${ign}`);
  };

  const deposit = () =>
    store.runCommand("bank", bankPersonal ? { username: ign, amount: bankAmount, personal: true } : { username: ign, amount: bankAmount }, "Deposit");
  const withdraw = () =>
    store.runCommand("bank", bankPersonal ? { username: ign, amount: bankAmount, withdraw: true, personal: true } : { username: ign, amount: bankAmount, withdraw: true }, "Withdraw");
  const withdrawAllBlocked = bankAmount.trim().toLowerCase() === "all";

  const labelStyle = { fontSize: 12, fontWeight: 700, color: "var(--t3)", whiteSpace: "nowrap" as const };

  return (
    <div onClick={onClose} style={{
      position: "fixed", inset: 0, zIndex: 100, background: "rgba(0,0,0,0.5)",
      display: "flex", alignItems: "center", justifyContent: "center", padding: 24,
    }}>
      <div onClick={(e) => e.stopPropagation()} style={{ width: 480, maxWidth: "100%", maxHeight: "90vh", display: "flex" }}>
        <Surface padding={0} style={{ display: "flex", flexDirection: "column", width: "100%", overflow: "hidden" }}>
          {/* Header */}
          <div style={{ display: "flex", alignItems: "center", gap: 13, padding: "20px 22px" }}>
            <AccountAvatar url={account.headUrl} size={44} />
            <div style={{ display: "flex", flexDirection: "column", gap: 5, minWidth: 0, flex: 1 }}>
              <div style={{ fontSize: 18, fontWeight: 800, color: "var(--t1)" }}>{ign}</div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <StatusDot color={badge.color} pulse={account.ready} />
                <span style={{ fontSize: 12.5, fontWeight: 700, color: badge.color }}>{badge.label}</span>
              </div>
              <div style={{ fontSize: 11.5, fontWeight: 600, color: account.hasCookie ? "var(--t2)" : "var(--warn)" }}>
                Booster cookie: {account.hasCookie ? "active" : "none"}
              </div>
            </div>
            <button onClick={onClose} style={{
              display: "inline-flex", alignItems: "center", justifyContent: "center",
              width: 30, height: 30, borderRadius: 999, border: "1px solid var(--stroke)",
              background: "rgba(255,255,255,0.04)", color: "var(--t2)", flex: "none",
            }}>
              <Icon name="x" size={15} />
            </button>
          </div>

          <Hairline />

          {/* Scrollable body */}
          <div style={{ overflowY: "auto", padding: "20px 22px", display: "flex", flexDirection: "column", gap: 22 }}>
            {/* LIFECYCLE */}
            <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
              <SectionLabel title="Lifecycle" icon="bolt.fill" />
              <div className="flow">
                {account.running ? (
                  <ActionChip title="Stop" icon="pause.fill" variant="warn"
                    onClick={() => store.runCommand("stop_bot", { username: ign }, `Stop ${ign}`)} />
                ) : (
                  <ActionChip title="Start" icon="play.fill" variant="good"
                    onClick={() => store.runCommand("start_bot", { username: ign }, `Start ${ign}`)} />
                )}
                <ActionChip title="Restart" icon="arrow.clockwise" onClick={restart} />
              </div>

              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <span style={labelStyle}>Stop after</span>
                {(["10m", "30m", "1h", "3h"] as const).map((d) => (
                  <QuickChip key={d} label={d}
                    onClick={() => store.runCommand("timeout", { duration: d, username: ign }, `Stop after ${d}`)} />
                ))}
              </div>

              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <span style={labelStyle}>Start in</span>
                {(["10m", "30m", "1h"] as const).map((d) => (
                  <QuickChip key={d} label={d}
                    onClick={() => store.runCommand("start_in", { duration: d, username: ign }, `Start in ${d}`)} />
                ))}
              </div>

              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <span style={labelStyle}>Switch region</span>
                {(["US", "EU"] as const).map((r) => (
                  <QuickChip key={r} label={r}
                    onClick={() => store.runCommand("cofl", { command: `switchregion ${r}`, username: ign }, `Switch region ${r}`)} />
                ))}
              </div>
            </div>

            <Hairline />

            {/* MARKET */}
            <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
              <SectionLabel title="Market" icon="tag.fill" />
              <div className="flow">
                <ActionChip title="Reconcile" icon="arrow.triangle.2.circlepath"
                  onClick={() => store.runButton(`saf:reconcile:${ign}`, "Reconcile")} />
                <ActionChip title="Claim Sold" icon="tray.and.arrow.down"
                  onClick={() => store.runCommand("claim_sold", { username: ign }, "Claim sold")} />
                <ActionChip title="Collect Bids" icon="hand.raised.fill"
                  onClick={() => store.runButton(`saf:bids:${ign}`, "Bids")} />
                <ActionChip title="Sell Inventory" icon="shippingbox.fill"
                  onClick={() => store.runButton(`saf:confirmSellInventory:${ign}:0`, "Queue listings")} />
                <ActionChip title="Delist All" icon="trash.fill" variant="danger"
                  onClick={() => store.runButton(`saf:confirmDelistAll:${ign}`, "Delist all")} />
                <ActionChip title="Clear Queue" icon="trash.fill" variant="danger"
                  onClick={() => store.runCommand("clear_queue", { username: ign }, "Clear queue")} />
                <ActionChip title="Clear Data" icon="trash.fill" variant="danger"
                  onClick={() => store.runButton(`saf:confirmClearData:${ign}`, "Clear data")} />
              </div>
            </div>

            <Hairline />

            {/* BANK */}
            <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
              <SectionLabel title="Bank" icon="creditcard.fill" />
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <Field value={bankAmount} onChange={setBankAmount} placeholder="all" />
                <Toggle on={bankPersonal} onChange={setBankPersonal} label="Personal" />
              </div>
              <div className="flow">
                <PrimaryButton title="Deposit" icon="arrow.down.to.line" onClick={deposit} />
                <PrimaryButton title="Withdraw" icon="arrow.up.to.line" onClick={withdraw} disabled={withdrawAllBlocked} />
              </div>
              {withdrawAllBlocked && (
                <span style={{ fontSize: 11, fontWeight: 600, color: "var(--warn)" }}>
                  Withdraw needs a specific amount — “all” isn’t accepted.
                </span>
              )}
            </div>

            {/* TRANSFER */}
            {otherAccounts.length >= 1 && (store.accounts?.accounts.length ?? 0) >= 2 && (
              <>
                <Hairline />
                <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
                  <SectionLabel title="Transfer" icon="arrow.left.arrow.right" />
                  <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                    <span style={labelStyle}>To</span>
                    <select className="field" value={dest} onChange={(e) => setDest(e.target.value)}
                      style={{ flex: 1, minWidth: 120 }}>
                      {otherAccounts.map((a) => <option key={a.ign} value={a.ign}>{a.ign}</option>)}
                    </select>
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <Field value={transferAmount} onChange={setTransferAmount} placeholder="all" />
                    <Toggle on={keepRunning} onChange={setKeepRunning} label="Keep source running" />
                  </div>
                  <div className="flow">
                    <PrimaryButton title="Transfer" icon="paperplane.fill" disabled={!dest}
                      onClick={() => store.runTransfer(ign, dest, transferAmount, !keepRunning)} />
                  </div>
                </div>
              </>
            )}
          </div>
        </Surface>
      </div>
    </div>
  );
}
