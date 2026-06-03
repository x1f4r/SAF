import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../api";
import { Fmt } from "../format";
import {
  Page,
  PageHeader,
  GhostButton,
  ActionChip,
  EmptyState,
  Hairline,
  ListRow,
  Pill,
} from "../components/UI";
import {
  entryAuctionId,
  entryItemName,
  entryPrice,
  type QueueEntry,
  type QueueResponse,
} from "../types";

function stateColor(state: string): string {
  switch (state.toLowerCase()) {
    case "buying":
      return "var(--profit)";
    case "listing":
    case "listingnoname":
      return "var(--gold)";
    case "delisting":
      return "var(--loss)";
    case "claiming":
    case "claimsold":
      return "var(--accent)";
    default:
      return "var(--accent2)";
  }
}

function QueueRow({ entry, onCancel }: { entry: QueueEntry; onCancel: () => void }) {
  const item = entryItemName(entry);
  const auction = entryAuctionId(entry);
  const price = entryPrice(entry);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1 }}>
      <div style={{ width: 140, display: "flex", justifyContent: "flex-start" }}>
        <Pill text={entry.state} color={stateColor(entry.state)} />
      </div>
      <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
        {item && (
          <div style={{
            fontSize: 12.5, fontWeight: 600, color: "var(--text-1)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>{item}</div>
        )}
        {auction && (
          <div className="mono num" style={{
            fontSize: 10.5, fontWeight: 500, color: "var(--text-3)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>{auction}</div>
        )}
        {price != null && (
          <div className="num" style={{ fontSize: 11, fontWeight: 500, color: "var(--gold)" }}>
            {Fmt.coins(price)}
          </div>
        )}
        {!item && !auction && (
          <div style={{ color: "var(--text-3)" }}>—</div>
        )}
      </div>
      <div className="num" style={{
        width: 70, textAlign: "right", fontSize: 13, fontWeight: 700, color: "var(--text-2)",
      }}>{entry.priority}</div>
      <ActionChip title="Cancel" icon="trash.fill" variant="danger" onClick={onCancel} />
    </div>
  );
}

export function QueueView() {
  const store = useStore();
  const [selected, setSelected] = useState("");
  const [queue, setQueue] = useState<QueueResponse | undefined>();
  const [loading, setLoading] = useState(false);

  const accounts: string[] =
    store.accounts?.accounts.map((a) => a.ign) ?? store.status?.configured ?? [];
  const currentIgn: string | undefined = selected === "" ? accounts[0] : selected;

  useEffect(() => {
    let cancelled = false;
    if (!currentIgn) {
      setQueue(undefined);
      return;
    }
    setLoading(true);
    api
      .queue(currentIgn)
      .then((res) => {
        if (!cancelled) setQueue(res);
      })
      .catch(() => {
        if (!cancelled) setQueue(undefined);
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
      .queue(currentIgn)
      .then(setQueue)
      .catch(() => setQueue(undefined))
      .finally(() => setLoading(false));
  };

  const entries = queue?.queue;

  return (
    <Page>
      <PageHeader
        title="Queue"
        subtitle="Pending market actions per account"
        trailing={
          <div style={{ display: "flex", gap: 10 }}>
            <GhostButton title="Refresh" icon="arrow.clockwise" onClick={reload} />
            {currentIgn && (
              <GhostButton
                title="Clear"
                icon="trash.fill"
                danger
                onClick={() => {
                  store.runCommand("clear_queue", { username: currentIgn }, "Clear queue");
                  setTimeout(reload, 600);
                }}
              />
            )}
          </div>
        }
      />

      {accounts.length === 0 ? (
        <EmptyState icon="list.bullet" text="No accounts." height={220} />
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

          <Hairline />

          {loading && !queue ? (
            <EmptyState icon="hourglass" text="Loading…" height={200} />
          ) : entries && entries.length > 0 ? (
            <div style={{ display: "flex", flexDirection: "column" }}>
              <div style={{ display: "flex", gap: 12, padding: "0 6px 8px" }}>
                <div style={{ width: 140, ...headerCell }}>STATE</div>
                <div style={{ flex: 1, ...headerCell }}>DETAIL</div>
                <div style={{ width: 70, textAlign: "right", ...headerCell }}>PRIORITY</div>
                <div style={{ width: 86, ...headerCell }} />
              </div>
              <Hairline />
              {entries.map((entry, idx) => (
                <ListRow key={idx} separator={idx < entries.length - 1}>
                  <QueueRow
                    entry={entry}
                    onCancel={() => {
                      if (!currentIgn) return;
                      store.runCommand("cancel_queue", { username: currentIgn, index: idx }, "Cancel queue entry");
                      setTimeout(reload, 600);
                    }}
                  />
                </ListRow>
              ))}
            </div>
          ) : (
            <EmptyState icon="checkmark.circle" text="Queue is empty." height={200} />
          )}
        </>
      )}
    </Page>
  );
}

const headerCell = {
  fontSize: 9.5,
  fontWeight: 600,
  letterSpacing: 0.7,
  color: "var(--text-3)",
} as const;
