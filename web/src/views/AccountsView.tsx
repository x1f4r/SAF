// Port of AccountsView.swift — a responsive grid of account cards.
import { useStore } from "../store";
import type { AccountInfo } from "../types";
import { Fmt } from "../format";
import {
  Page, PageHeader, EmptyState, Surface, Pill, StatusDot, Metric, VRule,
  MetaChip, Hairline, ActionChip,
} from "../components/UI";
import { AccountAvatar } from "../components/ItemIcon";

export function AccountsView() {
  const store = useStore();
  const accounts = store.accounts?.accounts ?? [];

  return (
    <Page>
      <PageHeader
        title="Accounts"
        subtitle={`${store.runningCount} running of ${store.configuredCount} configured`}
      />

      {accounts.length === 0 ? (
        <EmptyState icon="person.2" text="No accounts reported by the bot." height={280} />
      ) : (
        <div style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(420px, 1fr))",
          gap: 20,
        }}>
          {accounts.map((a) => <AccountCard key={a.ign} account={a} />)}
        </div>
      )}
    </Page>
  );
}

function AccountCard({ account }: { account: AccountInfo }) {
  const store = useStore();
  const s = account.stats;
  const runColor = account.running ? "var(--profit)" : "var(--text-3)";

  return (
    <Surface padding={20}>
      <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 13 }}>
          <AccountAvatar url={account.headUrl} size={44} />
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <div style={{ fontSize: 17, fontWeight: 700, color: "var(--text-1)" }}>{account.ign}</div>
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <Pill
                text={account.running ? "Running" : "Idle"}
                color={account.running ? "var(--profit)" : "var(--text-3)"}
                filled={account.running}
              />
              {s.coflTier && <Pill text={s.coflTier} color="var(--gold)" />}
            </div>
          </div>
          <div style={{ flex: 1 }} />
          <StatusDot color={runColor} pulse={account.running} />
        </div>

        <div style={{ display: "flex", alignItems: "flex-start" }}>
          <Metric label="Profit" value={Fmt.coins(s.totalProfit)} color="var(--profit)" />
          <VRule height={30} />
          <Metric label="Profit/hr" value={Fmt.coins(s.profitPerHour ?? 0)} color="var(--accent)" />
          <VRule height={30} />
          <Metric label="Purse" value={Fmt.coins(s.purse ?? 0)} color="var(--gold)" />
          <VRule height={30} />
          <Metric label="Bought" value={Fmt.int(s.bought)} />
          <VRule height={30} />
          <Metric label="Sold" value={Fmt.int(s.sold)} />
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          <MetaChip icon="antenna.radiowaves.left.and.right" text={s.coflPingMs != null ? `${s.coflPingMs}ms cofl` : "—"} />
          <MetaChip icon="wifi" text={s.hypixelPingMs != null ? `${s.hypixelPingMs}ms mc` : "—"} />
          {s.auctionSlotsUsed != null && s.auctionSlotsMax != null && (
            <MetaChip icon="tag.fill" text={`${s.auctionSlotsUsed}/${s.auctionSlotsMax} slots`} />
          )}
          <MetaChip icon="list.bullet" text={`${account.queueSize} queued`} />
        </div>

        <Hairline />

        <div className="flow">
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
    </Surface>
  );
}
