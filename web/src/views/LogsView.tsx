import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../store";
import { PageHeader, SearchField, Surface } from "../components/UI";

function lineTint(line: string): string {
  const l = line.toLowerCase();
  if (l.includes("error") || l.includes("fail")) return "var(--loss)";
  if (l.includes("warn")) return "var(--warn)";
  if (l.includes("bought") || l.includes("profit") || l.includes("sold")) return "var(--profit)";
  if (l.includes("info")) return "var(--text-2)";
  return "var(--text-3)";
}

export function LogsView() {
  const store = useStore();
  const [autoScroll, setAutoScroll] = useState(true);
  const [filter, setFilter] = useState("");

  const lines = useMemo(
    () => store.logs.filter((text) => filter === "" || text.toLowerCase().includes(filter.toLowerCase())),
    [store.logs, filter],
  );

  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (autoScroll) bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [store.logs.length, autoScroll]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div style={{ padding: "30px 40px 0" }}>
        <PageHeader
          title="Console"
          subtitle="Live tail of the bot log"
          trailing={
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <SearchField value={filter} onChange={setFilter} />
              <input
                type="checkbox"
                checked={autoScroll}
                onChange={(e) => setAutoScroll(e.target.checked)}
                style={{ accentColor: "var(--accent)", width: 16, height: 16, cursor: "pointer" }}
              />
              <span style={{ fontSize: 11.5, fontWeight: 500, color: "var(--text-2)" }}>Auto-scroll</span>
            </div>
          }
        />
      </div>

      <div style={{ flex: 1, minHeight: 0, padding: "24px 40px" }}>
        <Surface padding={0} style={{ height: "100%", overflow: "hidden" }}>
          <div ref={scrollRef} style={{ height: "100%", overflowY: "auto", padding: "8px 0" }}>
            {lines.map((text, i) => (
              <div
                key={i}
                className="mono"
                style={{
                  fontSize: 11.5,
                  color: lineTint(text),
                  padding: "1.5px 16px",
                  whiteSpace: "pre-wrap",
                  userSelect: "text",
                }}
              >
                {text}
              </div>
            ))}
            <div ref={bottomRef} style={{ height: 1 }} />
          </div>
        </Surface>
      </div>
    </div>
  );
}
