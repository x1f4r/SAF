import { useStore } from "../store";
import { displayProfit } from "../types";
import { Fmt, prettyFinder } from "../format";
import {
  Page,
  PageHeader,
  SectionLabel,
  Surface,
  ListRow,
  EmptyState,
} from "../components/UI";
import { ProfitArea, ProfitBars } from "../components/Charts";

function LabeledValue({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2, flex: 1, alignItems: "flex-start" }}>
      <div style={{ fontSize: 9, fontWeight: 600, letterSpacing: 0.4, textTransform: "uppercase", color: "var(--text-3)" }}>
        {label}
      </div>
      <div className="num" style={{ fontSize: 13, fontWeight: 700, color }}>{value}</div>
    </div>
  );
}

export function ProfitView() {
  const store = useStore();

  // By Finder: aggregate bought profit by pretty finder name, sorted desc.
  const totals: Record<string, number> = {};
  for (const flip of store.bought) {
    const key = prettyFinder(flip.finder);
    totals[key] = (totals[key] ?? 0) + flip.profit;
  }
  const finderSorted = Object.entries(totals).sort((a, b) => b[1] - a[1]);
  const finderMax = finderSorted[0]?.[1] ?? 1;

  const accounts = store.profit?.accounts ?? [];

  return (
    <Page>
      <PageHeader
        title="Profit"
        subtitle="Performance across all accounts"
        trailing={
          <span className="num" style={{ fontSize: 20, fontWeight: 700, color: "var(--profit)" }}>
            {Fmt.coins(displayProfit(store.profit))}
          </span>
        }
      />

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <SectionLabel title="Cumulative Profit" icon="chart.line.uptrend.xyaxis" />
        <Surface padding={18}>
          <ProfitArea points={store.series?.points ?? []} height={300} />
        </Surface>
      </div>

      <div style={{ display: "flex", alignItems: "flex-start", gap: 28 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14, flex: 1 }}>
          <SectionLabel title="Profit per Window" icon="chart.bar.fill" />
          <Surface padding={18}>
            <ProfitBars points={store.series?.points ?? []} height={220} />
          </Surface>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 14, width: 360 }}>
          <SectionLabel title="By Finder" icon="scope" />
          {finderSorted.length === 0 ? (
            <EmptyState icon="scope" text="No flips yet." height={200} />
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
              {finderSorted.slice(0, 8).map(([key, value]) => (
                <div key={key} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <div style={{ display: "flex", alignItems: "center" }}>
                    <span style={{ fontSize: 12, fontWeight: 600, color: "var(--text-1)" }}>{key}</span>
                    <span style={{ flex: 1 }} />
                    <span className="num" style={{ fontSize: 12, fontWeight: 700, color: "var(--profit)" }}>
                      {Fmt.coins(value)}
                    </span>
                  </div>
                  <div style={{ height: 5, borderRadius: 3, background: "rgba(255,255,255,0.05)", overflow: "hidden" }}>
                    <div
                      style={{
                        height: 5,
                        borderRadius: 3,
                        width: `${Math.max(0.03, value / finderMax) * 100}%`,
                        background: "var(--brand-grad-h)",
                      }}
                    />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <SectionLabel title="By Account" icon="person.2.fill" />
        {accounts.length === 0 ? (
          <EmptyState icon="person.2" text="No accounts." height={100} />
        ) : (
          <div>
            {accounts.map((account, idx) => (
              <ListRow key={account.ign} separator={idx < accounts.length - 1}>
                <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1 }}>
                  <span style={{ fontSize: 13, fontWeight: 600, color: "var(--text-1)", width: 150 }}>
                    {account.ign}
                  </span>
                  <div style={{ display: "flex", alignItems: "flex-start", flex: 1 }}>
                    <LabeledValue label="Profit" value={Fmt.coins(account.summary.totalProfit)} color="var(--profit)" />
                    <LabeledValue label="Bought" value={Fmt.int(account.summary.bought)} color="var(--text-2)" />
                    <LabeledValue label="Sold" value={Fmt.int(account.summary.sold)} color="var(--text-2)" />
                    <LabeledValue label="Profit/hr" value={Fmt.coins(account.summary.profitPerHour ?? 0)} color="var(--accent)" />
                  </div>
                </div>
              </ListRow>
            ))}
          </div>
        )}
      </div>
    </Page>
  );
}
