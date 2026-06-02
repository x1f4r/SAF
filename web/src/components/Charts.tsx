// Hand-rolled SVG charts matching the macOS app's Swift Charts look:
// a smooth gradient area + line, and gradient bars.
import { useEffect, useRef, useState } from "react";
import { Fmt } from "../format";
import type { ProfitPoint } from "../types";
import { EmptyState } from "./UI";

function useMeasure() {
  const ref = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(600);
  useEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver((entries) => setW(entries[0].contentRect.width));
    ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);
  return { ref, width: w };
}

// Catmull-Rom → cubic bezier smooth path.
function smoothPath(pts: { x: number; y: number }[]): string {
  if (pts.length < 2) return pts.length ? `M${pts[0].x},${pts[0].y}` : "";
  let d = `M${pts[0].x},${pts[0].y}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] ?? pts[i];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[i + 2] ?? p2;
    const c1x = p1.x + (p2.x - p0.x) / 6;
    const c1y = p1.y + (p2.y - p0.y) / 6;
    const c2x = p2.x - (p3.x - p1.x) / 6;
    const c2y = p2.y - (p3.y - p1.y) / 6;
    d += ` C${c1x},${c1y} ${c2x},${c2y} ${p2.x},${p2.y}`;
  }
  return d;
}

const PAD = { l: 54, r: 12, t: 12, b: 22 };

function axisTicks(min: number, max: number, count = 4): number[] {
  if (max <= min) return [min];
  const step = (max - min) / count;
  return Array.from({ length: count + 1 }, (_, i) => min + step * i);
}

export function ProfitArea({ points, height = 244 }: { points: ProfitPoint[]; height?: number }) {
  const { ref, width } = useMeasure();
  if (!points.length) {
    return <div ref={ref}><EmptyState icon="line" text="Profit history appears here as flips complete." height={height} /></div>;
  }
  const xs = points.map((p) => p.ts);
  const ys = points.map((p) => p.cumulative);
  const xMin = Math.min(...xs), xMax = Math.max(...xs);
  const yMin = Math.min(0, ...ys), yMax = Math.max(...ys, 1);
  const innerW = Math.max(1, width - PAD.l - PAD.r);
  const innerH = height - PAD.t - PAD.b;
  const sx = (x: number) => PAD.l + ((x - xMin) / Math.max(1, xMax - xMin)) * innerW;
  const sy = (y: number) => PAD.t + (1 - (y - yMin) / Math.max(1, yMax - yMin)) * innerH;
  const pix = points.map((p) => ({ x: sx(p.ts), y: sy(p.cumulative) }));
  const line = smoothPath(pix);
  const area = `${line} L${pix[pix.length - 1].x},${PAD.t + innerH} L${pix[0].x},${PAD.t + innerH} Z`;
  const yt = axisTicks(yMin, yMax);
  const xt = axisTicks(xMin, xMax, 4);

  return (
    <div ref={ref} style={{ width: "100%" }}>
      <svg width={width} height={height} style={{ display: "block" }}>
        <defs>
          <linearGradient id="areaFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.30" />
            <stop offset="100%" stopColor="var(--accent)" stopOpacity="0.01" />
          </linearGradient>
          <linearGradient id="lineStroke" x1="0" y1="0" x2="1" y2="0">
            <stop offset="0%" stopColor="var(--accent)" />
            <stop offset="100%" stopColor="var(--accent2)" />
          </linearGradient>
        </defs>
        {yt.map((v, i) => (
          <g key={i}>
            <line x1={PAD.l} x2={width - PAD.r} y1={sy(v)} y2={sy(v)} stroke="var(--stroke)" strokeOpacity={0.6} />
            <text x={PAD.l - 8} y={sy(v) + 3} textAnchor="end" fontSize="10" fill="var(--text-3)">{Fmt.coins(v)}</text>
          </g>
        ))}
        {xt.map((v, i) => (
          <text key={i} x={sx(v)} y={height - 6} textAnchor="middle" fontSize="10" fill="var(--text-3)">
            {new Date(v).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </text>
        ))}
        <path d={area} fill="url(#areaFill)" />
        <path d={line} fill="none" stroke="url(#lineStroke)" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </div>
  );
}

export function ProfitBars({ points, height = 220 }: { points: ProfitPoint[]; height?: number }) {
  const { ref, width } = useMeasure();
  if (!points.length) {
    return <div ref={ref}><EmptyState icon="bars" text="No data yet." height={height} /></div>;
  }
  const ys = points.map((p) => p.profit);
  const yMax = Math.max(...ys, 1);
  const innerW = Math.max(1, width - PAD.l - PAD.r);
  const innerH = height - PAD.t - PAD.b;
  const bw = Math.max(2, Math.min(16, (innerW / points.length) * 0.55));
  const yt = axisTicks(0, yMax);
  return (
    <div ref={ref} style={{ width: "100%" }}>
      <svg width={width} height={height} style={{ display: "block" }}>
        <defs>
          <linearGradient id="barFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" />
            <stop offset="100%" stopColor="var(--accent2)" />
          </linearGradient>
        </defs>
        {yt.map((v, i) => (
          <g key={i}>
            <line x1={PAD.l} x2={width - PAD.r} y1={PAD.t + (1 - v / yMax) * innerH} y2={PAD.t + (1 - v / yMax) * innerH} stroke="var(--stroke)" strokeOpacity={0.6} />
            <text x={PAD.l - 8} y={PAD.t + (1 - v / yMax) * innerH + 3} textAnchor="end" fontSize="10" fill="var(--text-3)">{Fmt.coins(v)}</text>
          </g>
        ))}
        {points.map((p, i) => {
          const x = PAD.l + (i / Math.max(1, points.length - 1)) * innerW - bw / 2;
          const h = Math.max(0, (p.profit / yMax) * innerH);
          return <rect key={i} x={x} y={PAD.t + innerH - h} width={bw} height={h} rx={3} fill="url(#barFill)" />;
        })}
      </svg>
    </div>
  );
}
