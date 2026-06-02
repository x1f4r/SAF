// Accounts — honest per-account status (connected / not ready + reason) with an
// all/connected-only toggle so it never shows accounts as flipping when they're not.
import { useState } from "react";
import { AccountAvatar } from "../components/ItemIcon";
import {
  ActionChip, EmptyState, Hairline, MetaChip, Metric, Page, PageHeader, Pill,
  SegmentedPicker, StatusDot, Surface, VRule,
} from "../components/UI";
import { Fmt } from "../format";
import { useStore } from "../store";
import type { AccountInfo } from "../types";
import { ControlSheet } from "./ControlSheet";

type Filter = "connected" | "all";

function statusBadge(a: AccountInfo): { label: string; color: string; filled: boolean } {
  const status = a.status ?? (a.running ? "online" : "offline");
  if (status === "offline") return { label: "Offline", color: "var(--t3)", filled: false };
  if (status === "connecting") return { label: "Connecting", color: "var(--warn)", filled: false };
  if (a.ready) return { label: "Ready", color: "var(--profit)", filled: true };
  return { label: a.reason ?? "Not ready", color: "var(--warn)", filled: false };
}

export function AccountsView() {
  const store = useStore();
  const all = store.accounts?.accounts ?? [];
  const [filter, setFilter] = useState<Filter>("connected");

  const connected = all.filter((a) => (a.status ?? (a.running ? "online" : "offline")) !== "offline");
  const shown = filter === "connected" ? connected : all;
  const hidden = all.length - connected.length;

  return (
    <Page>
      <PageHeader title="Accounts"
        subtitle={`${store.connectedCount} connected · ${store.readyCount} ready of ${store.configuredCount} configured`}
        trailing={
          <SegmentedPicker<Filter> value={filter}
            options={[{ value: "connected", label: "Connected" }, { value: "all", label: "All" }]}
            onChange={setFilter} width={220} />
        } />

      {shown.length === 0 ? (
        <EmptyState
          icon="person.2"
          text={all.length === 0 ? "No accounts reported by the bot." : "No accounts are connected right now. Check Diagnostics for why."}
          height={260} />
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(420px, 1fr))", gap: 20 }}>
          {shown.map((a) => <AccountCard key={a.ign} account={a} />)}
        </div>
      )}

      {filter === "connected" && hidden > 0 && (
        <button onClick={() => setFilter("all")} style={{ alignSelf: "flex-start", background: "none", border: "none", color: "var(--accent)", fontSize: 12.5, fontWeight: 600, cursor: "pointer" }}>
          {hidden} not connected — show all
        </button>
      )}
    </Page>
  );
}

function AccountCard({ account }: { account: AccountInfo }) {
  const store = useStore();
  const s = account.stats;
  const status = account.status ?? (account.running ? "online" : "offline");
  const badge = statusBadge(account);
  const offline = status === "offline";
  const [control, setControl] = useState(false);

  return (
    <Surface padding={20} style={offline ? { opacity: 0.62 } : undefined}>
      <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 13 }}>
          <AccountAvatar url={account.headUrl} size={44} />
          <div style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}>
            <div style={{ fontSize: 17, fontWeight: 700, color: "var(--t1)" }}>{account.ign}</div>
            <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
              <Pill text={badge.label} color={badge.color} filled={badge.filled} />
              {s.coflTier && <Pill text={s.coflTier} color="var(--gold)" />}
            </div>
          </div>
          <div style={{ flex: 1 }} />
          <StatusDot color={account.ready ? "var(--profit)" : status === "offline" ? "var(--t3)" : "var(--warn)"} pulse={account.ready} />
        </div>

        {/* Why this account can't flip */}
        {!account.ready && !offline && account.reason && (
          <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "9px 11px", borderRadius: 10, background: "color-mix(in srgb, var(--warn) 9%, transparent)", border: "1px solid color-mix(in srgb, var(--warn) 25%, transparent)" }}>
            <span style={{ width: 7, height: 7, borderRadius: "50%", background: "var(--warn)" }} />
            <span style={{ fontSize: 12, fontWeight: 600, color: "var(--warn)" }}>{account.reason}</span>
          </div>
        )}

        <div style={{ display: "flex", alignItems: "flex-start" }}>
          <Metric label="Profit" value={Fmt.coins(s.totalProfit)} color="var(--profit)" />
          <VRule height={30} />
          <Metric label="Profit/hr" value={Fmt.coins(s.profitPerHour ?? 0)} color="var(--accent)" />
          <VRule height={30} />
          <Metric label="Purse" value={s.purse != null ? Fmt.coins(s.purse) : "—"} color="var(--gold)" />
          <VRule height={30} />
          <Metric label="Bought" value={Fmt.int(s.bought)} />
          <VRule height={30} />
          <Metric label="Sold" value={Fmt.int(s.sold)} />
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 16, flexWrap: "wrap" }}>
          <MetaChip icon={account.coflConnected ? "antenna.radiowaves.left.and.right" : "antenna.radiowaves.left.and.right"} text={account.coflConnected ? "SkyCofl connected" : "SkyCofl offline"} />
          <MetaChip icon="seal" text={account.hasCookie ? "Cookie active" : "No cookie"} />
          {s.coflPingMs != null && <MetaChip icon="wifi" text={`${s.coflPingMs}ms`} />}
          {s.auctionSlotsUsed != null && s.auctionSlotsMax != null && (
            <MetaChip icon="tag.fill" text={`${s.auctionSlotsUsed}/${s.auctionSlotsMax} slots`} />
          )}
          <MetaChip icon="list.bullet" text={`${account.queueSize} queued`} />
        </div>

        <Hairline />

        <div className="flow">
          <ActionChip title="Control" icon="slider.horizontal.3" onClick={() => setControl(true)} />
          <ActionChip title="Reconcile" icon="arrow.triangle.2.circlepath"
            onClick={() => store.runCommand("reconcile", { username: account.ign }, `Reconcile ${account.ign}`)} />
          <ActionChip title="Claim Sold" icon="tray.and.arrow.down"
            onClick={() => store.runCommand("claim_sold", { username: account.ign }, "Claim sold")} />
          <ActionChip title="Bids" icon="hand.raised.fill"
            onClick={() => store.runCommand("bids", { username: account.ign }, "Collect bids")} />
          <ActionChip title="Sell Inv" icon="shippingbox.fill"
            onClick={() => store.runButton(`saf:confirmSellInventory:${account.ign}:0`, "Queue inventory listings")} />
          <ActionChip title="Delist All" icon="trash.fill" variant="danger"
            onClick={() => store.runButton(`saf:confirmDelistAll:${account.ign}`, "Delist everything")} />
          {account.running ? (
            <ActionChip title="Stop" icon="pause.fill" variant="warn"
              onClick={() => store.runCommand("stop_bot", { username: account.ign }, `Stop ${account.ign}`)} />
          ) : (
            <ActionChip title="Start" icon="play.fill" variant="good"
              onClick={() => store.runCommand("start_bot", { username: account.ign }, `Start ${account.ign}`)} />
          )}
        </div>
      </div>
      {control && <ControlSheet account={account} onClose={() => setControl(false)} />}
    </Surface>
  );
}
