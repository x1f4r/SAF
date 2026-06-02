// Faithful to the Claude Design handoff (views_a.jsx DashboardView): weekly
// net-profit hero with the gold coin glyph, market mode as plain text, a single
// running indicator, and clean icon-circle activity rows (no pill spam).
import { ProfitArea } from "../components/Charts";
import { Icon } from "../components/Icon";
import { AccountAvatar, ItemIcon } from "../components/ItemIcon";
import {
  Coin, EmptyState, Hairline, ListRow, Metric, Page, PageHeader, PrimaryButton,
  SectionLabel, StatusDot, VRule,
} from "../components/UI";
import { Fmt, prettyFinder } from "../format";
import { useStore } from "../store";
import { displayBought, displayProfit, displaySold, type AccountInfo, type FlipRecord, type LiveEvent } from "../types";

const pnl = (v: number) => (v >= 0 ? "var(--profit)" : "var(--loss)");
const linkBtn: React.CSSProperties = { background: "none", border: "none", cursor: "pointer", color: "var(--accent)", fontSize: 12, fontWeight: 600 };

function FlipRowContent({ flip }: { flip: FlipRecord }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, width: "100%" }}>
      <ItemIcon tag={flip.tag} size={34} />
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: "var(--t1)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{flip.item}</div>
        <div style={{ display: "flex", alignItems: "center", gap: 7, marginTop: 2 }}>
          <span style={{ fontSize: 11, fontWeight: 500, color: "var(--accent2)" }}>{prettyFinder(flip.finder)}</span>
          <span style={{ color: "var(--t3)" }}>·</span>
          <span style={{ fontSize: 11, fontWeight: 500, color: "var(--t3)" }}>{Fmt.relative(flip.ts)}</span>
        </div>
      </div>
      <div style={{ textAlign: "right" }}>
        <div className="num" style={{ fontSize: 13.5, fontWeight: 700, color: pnl(flip.profit) }}>{Fmt.signedCoins(flip.profit)}</div>
        <div className="num" style={{ fontSize: 11, fontWeight: 500, color: "var(--t3)", marginTop: 2 }}>{Fmt.coins(flip.price)}</div>
      </div>
    </div>
  );
}

function ActivityRowContent({ event }: { event: LiveEvent }) {
  const r = event.raw || {};
  const map: Record<string, [string, string, string, string]> = {
    purchase: ["cart.fill", "var(--profit)", `Bought ${r.item ?? "item"}`, Fmt.signedCoins(r.profit ?? 0)],
    sold: ["checkmark.seal.fill", "var(--gold)", `Sold ${r.item ?? "item"}`, Fmt.coins(r.price ?? 0)],
    claim: ["tray.and.arrow.down.fill", "var(--accent)", "Claimed sale", Fmt.coins(r.coins ?? 0)],
    state: ["bolt.fill", "var(--warn)", "State changed", ""],
  };
  const [icon, tint, title, detail] = map[event.type] ?? ["bell.fill", "var(--accent2)", r.notification?.title ?? "Notification", ""];
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, width: "100%" }}>
      <div style={{ width: 24, height: 24, borderRadius: "50%", background: `color-mix(in srgb, ${tint} 14%, transparent)`, display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0 }}>
        <Icon name={icon} size={11} style={{ color: tint }} />
      </div>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 12, fontWeight: 600, color: "var(--t1)", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title}</div>
        <div className="num" style={{ fontSize: 10, fontWeight: 500, color: "var(--t3)", marginTop: 1 }}>{Fmt.clock(event.ts)}</div>
      </div>
      {detail && <span className="num" style={{ fontSize: 12, fontWeight: 700, color: tint }}>{detail}</span>}
    </div>
  );
}

function AccountRowContent({ a }: { a: AccountInfo }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, width: "100%" }}>
      <AccountAvatar url={a.headUrl} size={28} />
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 12.5, fontWeight: 600, color: "var(--t1)" }}>{a.ign}</div>
        <div className="num" style={{ fontSize: 10.5, fontWeight: 500, color: "var(--t3)", marginTop: 1 }}>{a.stats.bought} bought · {a.stats.sold} sold</div>
      </div>
      <StatusDot color={a.running ? "var(--profit)" : "var(--t3)"} pulse={a.running} />
    </div>
  );
}

function BotControls() {
  const s = useStore();
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
        <StatusDot color={s.running ? "var(--profit)" : "var(--warn)"} pulse={s.running} />
        <span style={{ fontSize: 12.5, fontWeight: 600, color: s.running ? "var(--profit)" : "var(--warn)" }}>{s.running ? "Running" : "Halted"}</span>
      </div>
      {s.running
        ? <PrimaryButton title="Stop Bot" icon="pause.fill" gradient="linear-gradient(90deg,#FFB23E,#E0892B)" onClick={() => s.runControl("stop")} />
        : <PrimaryButton title="Start Bot" icon="play.fill" gradient="linear-gradient(90deg,#37E0A6,#2BB98A)" onClick={() => s.runControl("start")} />}
    </div>
  );
}

export function DashboardView() {
  const s = useStore();
  const cum = s.series?.points?.[s.series.points.length - 1]?.cumulative ?? 0;
  const live = s.status?.marketMode === "live";
  const modeWord = live ? "live" : "dry-run";
  const accountsList = s.accounts?.accounts ?? [];

  const subtitle = (
    <span>{s.runningCount}/{s.configuredCount} accounts running&nbsp;&nbsp;·&nbsp;&nbsp;market{" "}
      <span style={{ color: live ? "var(--warn)" : "var(--profit)", fontWeight: 600 }}>{modeWord}</span>
    </span>
  );

  return (
    <Page>
      <PageHeader title="Dashboard" subtitle={subtitle} trailing={<BotControls />} />

      {/* metric strip — weekly hero with coin glyph */}
      <div style={{ display: "flex", alignItems: "flex-start", gap: 0 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 11, fontWeight: 600, letterSpacing: "0.8px", textTransform: "uppercase", color: "var(--t3)" }}>Net Profit · 7 Days</div>
          <div style={{ display: "flex", alignItems: "center", gap: 11, marginTop: 8 }}>
            <span className="num" style={{ fontSize: 36, fontWeight: 800, color: "var(--profit)", lineHeight: 1 }}>{Fmt.coins(s.weekProfit)}</span>
            <Coin size={26} />
          </div>
          <div className="num" style={{ marginTop: 5, fontSize: 11, fontWeight: 500, color: "var(--t3)" }}>lifetime {Fmt.coins(displayProfit(s.profit))} · {Fmt.int(displayBought(s.profit))} flips</div>
        </div>
        <VRule height={52} />
        <Metric label="Profit / hr" value={Fmt.coins(s.ppHour)} sub="this session" color="var(--accent)" />
        <VRule height={52} />
        <Metric label="Bought" value={Fmt.int(displayBought(s.profit))} sub={`${s.bought.length} tracked`} />
        <VRule height={52} />
        <Metric label="Sold" value={Fmt.int(displaySold(s.profit))} sub={`${s.sold.length} tracked`} />
        <VRule height={52} />
        <Metric label="Purse" value={Fmt.coins(s.profit?.purse ?? 0)} sub="liquid" color="var(--gold)" />
        <VRule height={52} />
        <Metric label="Accounts" value={`${s.runningCount}/${s.configuredCount}`} sub={s.running ? "active" : "halted"} subColor={s.running ? "var(--profit)" : "var(--warn)"} />
      </div>
      <Hairline />

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <SectionLabel title="Cumulative Profit" icon="chart.line.uptrend.xyaxis"
          trailing={<span className="num" style={{ fontSize: 14, fontWeight: 700, color: "var(--profit)" }}>{Fmt.signedCoins(cum)}</span>} />
        <Surface18><ProfitArea points={s.series?.points ?? []} height={244} /></Surface18>
      </div>

      <div style={{ display: "flex", gap: 40, alignItems: "flex-start" }}>
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 12 }}>
          <SectionLabel title="Recent Buys" icon="cart.fill"
            trailing={<button onClick={() => s.setTab("flips")} style={linkBtn}>View all</button>} />
          {s.bought.length === 0
            ? <EmptyState icon="cart.fill" text="No purchases recorded yet." height={160} />
            : <div>{s.bought.slice(0, 8).map((f, i) => <ListRow key={f.auctionId + f.ts} separator={i < Math.min(7, s.bought.length - 1)}><FlipRowContent flip={f} /></ListRow>)}</div>}
        </div>
        <div style={{ width: 320, flexShrink: 0, display: "flex", flexDirection: "column", gap: 26 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <SectionLabel title="Live Activity" icon="dot.radiowaves.left.and.right" />
            {s.events.length === 0
              ? <EmptyState icon="waveform.path.ecg" text="Waiting for activity…" height={120} />
              : <div>{s.events.slice(0, 7).map((e, i) => <ListRow key={e.id} separator={i < Math.min(6, s.events.length - 1)}><ActivityRowContent event={e} /></ListRow>)}</div>}
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <SectionLabel title="Accounts" icon="person.2.fill"
              trailing={<button onClick={() => s.setTab("accounts")} style={linkBtn}>Manage</button>} />
            {accountsList.length === 0
              ? <EmptyState icon="person.fill" text="No accounts." height={90} />
              : <div>{accountsList.slice(0, 5).map((a, i) => <ListRow key={a.ign} separator={i < Math.min(4, accountsList.length - 1)}><AccountRowContent a={a} /></ListRow>)}</div>}
          </div>
        </div>
      </div>
    </Page>
  );
}

// Local thin wrapper so the chart sits in one quiet surface (padding 18).
function Surface18({ children }: { children: React.ReactNode }) {
  return <div className="surface" style={{ padding: 18, borderRadius: 20 }}>{children}</div>;
}
