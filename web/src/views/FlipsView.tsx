// Ported from SAFDashboard/Views/FlipsView.swift.
import { useState, type ReactNode } from "react";
import { useStore } from "../store";
import { Fmt, prettyFinder } from "../format";
import type { FlipRecord, SaleRecord } from "../types";
import {
  Page, PageHeader, SearchField, SegmentedPicker, Metric, VRule, Hairline,
  ListRow, EmptyState,
} from "../components/UI";
import { ItemIcon } from "../components/ItemIcon";

type Mode = "bought" | "sold";

const pnl = (p: number) => (p >= 0 ? "var(--profit)" : "var(--loss)");

// Constrains a table cell to a fixed width, or flexes when width == 0.
function Col({ width, trailing, children, style }: {
  width: number; trailing?: boolean; children: ReactNode; style?: React.CSSProperties;
}) {
  return (
    <div style={{
      ...(width === 0 ? { flex: 1 } : { width }),
      display: "flex", alignItems: "center", minWidth: 0,
      justifyContent: trailing ? "flex-end" : "flex-start",
      ...style,
    }}>
      {children}
    </div>
  );
}

function TableHeader({ columns }: { columns: [string, number][] }) {
  return (
    <div style={{ display: "flex", gap: 12, padding: "0 6px 8px" }}>
      {columns.map(([label, width], idx) => (
        <Col key={label} width={width} trailing={idx === columns.length - 1}>
          <span style={{
            fontSize: 9.5, fontWeight: 600, letterSpacing: 0.7,
            textTransform: "uppercase", color: "var(--text-3)",
          }}>{label}</span>
        </Col>
      ))}
    </div>
  );
}

const boughtCols: [string, number][] = [
  ["Item", 0], ["Finder", 86], ["Bought", 92], ["Target", 92], ["Profit", 100], ["When", 84],
];
const soldCols: [string, number][] = [
  ["Item", 0], ["Buyer", 160], ["Price", 120], ["When", 84],
];

function BoughtRow({ flip }: { flip: FlipRecord }) {
  const w = boughtCols.map(([, width]) => width);
  return (
    <div style={{ display: "flex", gap: 12, alignItems: "center", flex: 1, padding: "9px 6px" }}>
      <Col width={w[0]}>
        <div style={{ display: "flex", gap: 11, alignItems: "center", minWidth: 0 }}>
          <ItemIcon tag={flip.tag} size={30} />
          <span style={{
            fontSize: 12.5, fontWeight: 600, color: "var(--text-1)",
            whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
          }}>{flip.item}</span>
        </div>
      </Col>
      <Col width={w[1]}>
        <span style={{ fontSize: 11.5, fontWeight: 500, color: "var(--accent2)" }}>{prettyFinder(flip.finder)}</span>
      </Col>
      <Col width={w[2]}>
        <span className="num" style={{ fontSize: 12, fontWeight: 500, color: "var(--text-2)" }}>{Fmt.coins(flip.price)}</span>
      </Col>
      <Col width={w[3]}>
        <span className="num" style={{ fontSize: 12, fontWeight: 500, color: "var(--text-2)" }}>{Fmt.coins(flip.targetPrice)}</span>
      </Col>
      <Col width={w[4]}>
        <span className="num" style={{ fontSize: 12.5, fontWeight: 700, color: pnl(flip.profit) }}>{Fmt.signedCoins(flip.profit)}</span>
      </Col>
      <Col width={w[5]} trailing>
        <span style={{ fontSize: 11, fontWeight: 500, color: "var(--text-3)" }}>{Fmt.relative(flip.ts)}</span>
      </Col>
    </div>
  );
}

function SoldRow({ sale }: { sale: SaleRecord }) {
  const w = soldCols.map(([, width]) => width);
  return (
    <div style={{ display: "flex", gap: 12, alignItems: "center", flex: 1, padding: "10px 6px" }}>
      <Col width={w[0]}>
        <span style={{
          fontSize: 12.5, fontWeight: 600, color: "var(--text-1)",
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}>{sale.item}</span>
      </Col>
      <Col width={w[1]}>
        <span style={{
          fontSize: 12, fontWeight: 500, color: "var(--text-2)",
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}>{sale.buyer}</span>
      </Col>
      <Col width={w[2]}>
        <span className="num" style={{ fontSize: 12.5, fontWeight: 700, color: "var(--gold)" }}>{Fmt.coins(sale.price)}</span>
      </Col>
      <Col width={w[3]} trailing>
        <span style={{ fontSize: 11, fontWeight: 500, color: "var(--text-3)" }}>{Fmt.relative(sale.ts)}</span>
      </Col>
    </div>
  );
}

export function FlipsView() {
  const store = useStore();
  const [mode, setMode] = useState<Mode>("bought");
  const [query, setQuery] = useState("");
  const q = query.toLowerCase();

  const filteredBought = q
    ? store.bought.filter((f) => f.item.toLowerCase().includes(q) || f.finder.toLowerCase().includes(q))
    : store.bought;
  const filteredSold = q
    ? store.sold.filter((s) => s.item.toLowerCase().includes(q) || s.buyer.toLowerCase().includes(q))
    : store.sold;

  const total = filteredBought.reduce((acc, f) => acc + f.profit, 0);
  const best = filteredBought.reduce((m, f) => Math.max(m, f.profit), 0);

  return (
    <Page>
      <PageHeader
        title="Flips"
        subtitle={mode === "bought"
          ? `${store.bought.length} tracked purchases`
          : `${store.sold.length} tracked sales`}
        trailing={<SearchField value={query} onChange={setQuery} />}
      />

      <div style={{ display: "flex", gap: 18, alignItems: "flex-start" }}>
        <SegmentedPicker
          value={mode}
          options={[{ value: "bought", label: "Bought" }, { value: "sold", label: "Sold" }]}
          onChange={setMode}
          width={240}
        />
        <div style={{ flex: 1 }} />
        {mode === "bought" && (
          <div style={{ display: "flex", alignItems: "flex-start", maxWidth: 420 }}>
            <Metric label="Shown Profit" value={Fmt.coins(total)} color="var(--profit)" />
            <VRule height={34} />
            <Metric label="Count" value={Fmt.int(filteredBought.length)} />
            <VRule height={34} />
            <Metric label="Best" value={Fmt.signedCoins(best)} color="var(--gold)" />
          </div>
        )}
      </div>

      <Hairline />

      {mode === "bought" ? (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <TableHeader columns={boughtCols} />
          <Hairline />
          {filteredBought.length === 0 ? (
            <EmptyState icon="cart" text="No purchases match." height={180} />
          ) : (
            filteredBought.map((flip, idx) => (
              <ListRow key={flip.auctionId} separator={idx < filteredBought.length - 1}>
                <BoughtRow flip={flip} />
              </ListRow>
            ))
          )}
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <TableHeader columns={soldCols} />
          <Hairline />
          {filteredSold.length === 0 ? (
            <EmptyState icon="seal" text="No sales match." height={180} />
          ) : (
            filteredSold.map((sale, idx) => (
              <ListRow key={`${sale.item}-${sale.ts}-${idx}`} separator={idx < filteredSold.length - 1}>
                <SoldRow sale={sale} />
              </ListRow>
            ))
          )}
        </div>
      )}
    </Page>
  );
}
