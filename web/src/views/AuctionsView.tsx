import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../api";
import { Fmt } from "../format";
import {
  Page,
  PageHeader,
  GhostButton,
  SectionLabel,
  EmptyState,
  Hairline,
  ListRow,
} from "../components/UI";
import type { AuctionEntry, AuctionsResponse } from "../types";

function AuctionRow({ entry, accent }: { entry: AuctionEntry; accent: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1, padding: "9px 6px" }}>
      <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
        <div style={{
          fontSize: 12.5, fontWeight: 600, color: "var(--text-1)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        }}>{entry.name ?? "Unknown item"}</div>
        {entry.auctionId && (
          <div className="mono" style={{
            fontSize: 10.5, fontWeight: 500, color: "var(--text-3)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>{entry.auctionId}</div>
        )}
        {entry.buyer && (
          <div style={{ fontSize: 11, fontWeight: 500, color: "var(--text-2)" }}>
            Buyer: {entry.buyer}
          </div>
        )}
      </div>
      {entry.endsIn && (
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text-3)", whiteSpace: "nowrap" }}>
          {entry.endsIn}
        </div>
      )}
      {entry.price != null && (
        <div className="num" style={{ width: 90, textAlign: "right", fontSize: 12.5, fontWeight: 700, color: accent }}>
          {Fmt.coins(entry.price)}
        </div>
      )}
    </div>
  );
}

function AuctionSection({
  title,
  icon,
  accent,
  entries,
  emptyText,
}: {
  title: string;
  icon: string;
  accent: string;
  entries: AuctionEntry[];
  emptyText: string;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <SectionLabel
        title={title}
        icon={icon}
        trailing={
          <span className="num" style={{ fontSize: 12, fontWeight: 700, color: accent }}>
            {entries.length}
          </span>
        }
      />
      {entries.length === 0 ? (
        <EmptyState icon={icon} text={emptyText} height={100} />
      ) : (
        <div style={{ display: "flex", flexDirection: "column" }}>
          {entries.map((entry, idx) => (
            <ListRow key={entry.auctionId ?? entry.itemUuid ?? idx} separator={idx < entries.length - 1}>
              <AuctionRow entry={entry} accent={accent} />
            </ListRow>
          ))}
        </div>
      )}
    </div>
  );
}

export function AuctionsView() {
  const store = useStore();
  const [selected, setSelected] = useState("");
  const [auctions, setAuctions] = useState<AuctionsResponse | undefined>();
  const [loading, setLoading] = useState(false);

  const accounts: string[] =
    store.accounts?.accounts.map((a) => a.ign) ?? store.status?.configured ?? [];
  const currentIgn: string | undefined = selected === "" ? accounts[0] : selected;

  useEffect(() => {
    let cancelled = false;
    if (!currentIgn) {
      setAuctions(undefined);
      return;
    }
    setLoading(true);
    api
      .getAuctions(currentIgn)
      .then((res) => {
        if (!cancelled) setAuctions(res);
      })
      .catch(() => {
        if (!cancelled) setAuctions(undefined);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentIgn, store.accounts]);

  const reload = () => {
    if (!currentIgn) return;
    setLoading(true);
    api
      .getAuctions(currentIgn)
      .then(setAuctions)
      .catch(() => setAuctions(undefined))
      .finally(() => setLoading(false));
  };

  const entries = auctions?.entries ?? [];
  const active = entries.filter((e) => e.status === "active");
  const sold = entries.filter((e) => e.status === "sold");
  const expired = entries.filter((e) => e.status === "expired");

  const freshness =
    auctions?.observedAtMs != null
      ? `Last scanned ${Fmt.relative(auctions.observedAtMs)}`
      : "Not scanned yet — open Manage Auctions";

  return (
    <Page>
      <PageHeader
        title="Auctions"
        subtitle="Listings and items waiting to collect"
        trailing={<GhostButton title="Refresh" icon="arrow.clockwise" onClick={reload} />}
      />

      {accounts.length === 0 ? (
        <EmptyState icon="tag" text="No accounts." height={220} />
      ) : (
        <>
          <div className="flow">
            {accounts.map((ign) => {
              const sel = ign === currentIgn;
              return (
                <button
                  key={ign}
                  onClick={() => setSelected(ign)}
                  style={{
                    padding: "8px 14px",
                    borderRadius: 999,
                    fontSize: 12.5,
                    fontWeight: 600,
                    cursor: "pointer",
                    color: sel ? "rgba(0,0,0,0.85)" : "var(--text-2)",
                    background: sel ? "var(--brand-grad-h)" : "rgba(255,255,255,0.04)",
                    border: sel ? "0" : "1px solid var(--stroke)",
                  }}
                >
                  {ign}
                </button>
              );
            })}
          </div>

          <div style={{ fontSize: 11.5, fontWeight: 600, color: "var(--text-3)" }}>
            {freshness}
          </div>

          <Hairline />

          {loading && !auctions ? (
            <EmptyState icon="hourglass" text="Loading…" height={200} />
          ) : (
            <>
              <AuctionSection
                title="Active"
                icon="tag"
                accent="var(--accent)"
                entries={active}
                emptyText="No active listings."
              />
              <AuctionSection
                title="Sold — to collect"
                icon="seal"
                accent="var(--profit)"
                entries={sold}
                emptyText="Nothing sold to collect."
              />
              <AuctionSection
                title="Expired — to collect"
                icon="hourglass"
                accent="var(--loss)"
                entries={expired}
                emptyText="Nothing expired to collect."
              />
            </>
          )}
        </>
      )}
    </Page>
  );
}
