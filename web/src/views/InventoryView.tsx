import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../api";
import { Fmt } from "../format";
import {
  Page,
  PageHeader,
  GhostButton,
  EmptyState,
  Hairline,
  ListRow,
  Pill,
} from "../components/UI";
import { ItemIcon } from "../components/ItemIcon";
import type { InventoryItem, InventoryResponse } from "../types";

function InventoryRow({ item }: { item: InventoryItem }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1, padding: "9px 6px" }}>
      <ItemIcon tag={item.tag} size={30} />
      <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
        <div style={{
          fontSize: 12.5, fontWeight: 600, color: "var(--text-1)",
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        }}>{item.itemName}</div>
        {item.tag && (
          <div className="mono" style={{
            fontSize: 10.5, fontWeight: 500, color: "var(--text-3)",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>{item.tag}</div>
        )}
      </div>
      {item.price != null && (
        <div className="num" style={{ fontSize: 12.5, fontWeight: 700, color: "var(--gold)" }}>
          {Fmt.coins(item.price)}
        </div>
      )}
      <div style={{ display: "flex", alignItems: "center", gap: 8, width: 150, justifyContent: "flex-end" }}>
        {item.inHotbar && <Pill text="Hotbar" color="var(--accent)" />}
        {item.slot != null && (
          <span className="num" style={{ fontSize: 11, fontWeight: 600, color: "var(--text-3)" }}>
            slot {item.slot}
          </span>
        )}
      </div>
    </div>
  );
}

export function InventoryView() {
  const store = useStore();
  const [selected, setSelected] = useState("");
  const [inventory, setInventory] = useState<InventoryResponse | undefined>();
  const [loading, setLoading] = useState(false);

  const accounts: string[] =
    store.accounts?.accounts.map((a) => a.ign) ?? store.status?.configured ?? [];
  const currentIgn: string | undefined = selected === "" ? accounts[0] : selected;

  useEffect(() => {
    let cancelled = false;
    if (!currentIgn) {
      setInventory(undefined);
      return;
    }
    setLoading(true);
    api
      .getInventory(currentIgn)
      .then((res) => {
        if (!cancelled) setInventory(res);
      })
      .catch(() => {
        if (!cancelled) setInventory(undefined);
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
      .getInventory(currentIgn)
      .then(setInventory)
      .catch(() => setInventory(undefined))
      .finally(() => setLoading(false));
  };

  const items = inventory?.items
    ? [...inventory.items].sort((a, b) => {
        const sa = a.slot ?? Number.MAX_SAFE_INTEGER;
        const sb = b.slot ?? Number.MAX_SAFE_INTEGER;
        return sa - sb;
      })
    : undefined;

  return (
    <Page>
      <PageHeader
        title="Inventory"
        subtitle="Items held by the selected account"
        trailing={<GhostButton title="Refresh" icon="arrow.clockwise" onClick={reload} />}
      />

      {accounts.length === 0 ? (
        <EmptyState icon="cube" text="No accounts." height={220} />
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

          {loading && !inventory ? (
            <EmptyState icon="hourglass" text="Loading…" height={200} />
          ) : items && items.length > 0 ? (
            <div style={{ display: "flex", flexDirection: "column" }}>
              <div style={{
                fontSize: 12, fontWeight: 600, color: "var(--text-2)", padding: "0 6px 8px",
              }}>
                {items.length} {items.length === 1 ? "item" : "items"}
              </div>
              <Hairline />
              {items.map((item, idx) => (
                <ListRow key={item.uuid ?? `${item.itemName}-${idx}`} separator={idx < items.length - 1}>
                  <InventoryRow item={item} />
                </ListRow>
              ))}
            </div>
          ) : (
            <EmptyState icon="cube" text="Inventory is empty." height={200} />
          )}
        </>
      )}
    </Page>
  );
}
