// Ported from the macOS app's Formatting.swift.

function trim(v: number): string {
  const r = Math.round(v * 10) / 10;
  return r === Math.round(r) ? String(Math.round(r)) : r.toFixed(1);
}

export const Fmt = {
  coins(value: number): string {
    if (!isFinite(value)) return "0";
    const sign = value < 0 ? "-" : "";
    const v = Math.abs(value);
    if (v >= 1e12) return `${sign}${trim(v / 1e12)}T`;
    if (v >= 1e9) return `${sign}${trim(v / 1e9)}B`;
    if (v >= 1e6) return `${sign}${trim(v / 1e6)}M`;
    if (v >= 1e3) return `${sign}${trim(v / 1e3)}k`;
    return `${sign}${Math.round(v)}`;
  },
  signedCoins(value: number): string {
    return (value >= 0 ? "+" : "") + Fmt.coins(value);
  },
  int(value: number): string {
    return new Intl.NumberFormat("en-US").format(Math.round(value));
  },
  percent(value: number | null | undefined): string {
    if (value == null) return "—";
    return `${Math.round(value)}%`;
  },
  relative(ms: number): string {
    const delta = (Date.now() - ms) / 1000;
    if (delta < 5) return "just now";
    if (delta < 60) return `${Math.floor(delta)}s ago`;
    if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
    if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
    return `${Math.floor(delta / 86400)}d ago`;
  },
  clock(ms: number): string {
    const d = new Date(ms);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  },
};

export function prettyFinder(finder: string): string {
  switch (finder.toUpperCase()) {
    case "USER": return "User";
    case "SNIPER_MEDIAN": return "Median";
    case "SNIPER": return "Sniper";
    case "TFM": return "TFM";
    case "AI": return "AI";
    case "CRAFTCOST":
    case "CRAFT_COST": return "Craft";
    case "STONKS": return "Stonks";
    case "FLIPPER": return "Flipper";
    default: return finder ? finder[0].toUpperCase() + finder.slice(1).toLowerCase() : finder;
  }
}
