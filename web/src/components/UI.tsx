// Shared UI primitives — the web port of the macOS Components.swift.
import { useState, type CSSProperties, type ReactNode } from "react";
import { Icon } from "./Icon";

export const Hairline = ({ style }: { style?: CSSProperties }) => (
  <div className="hairline" style={style} />
);
export const VRule = ({ height = 34 }: { height?: number }) => (
  <div className="vrule" style={{ height }} />
);

export function Surface({ children, padding = 20, style }: { children: ReactNode; padding?: number; style?: CSSProperties }) {
  return <div className="surface" style={{ padding, ...style }}>{children}</div>;
}

export function SectionLabel({ title, icon, trailing }: { title: string; icon?: string; trailing?: ReactNode }) {
  return (
    <div className="section-label">
      {icon && <span className="sl-icon"><Icon name={icon} size={13} /></span>}
      <span>{title}</span>
      <span className="sl-spacer" />
      {trailing && <span className="sl-trailing">{trailing}</span>}
    </div>
  );
}

export function Metric({ label, value, sub, subColor, color = "var(--text-1)", hero = false }: {
  label: string; value: string; sub?: string; subColor?: string; color?: string; hero?: boolean;
}) {
  return (
    <div className={`metric${hero ? " hero" : ""}`}>
      <div className="m-label">{label}</div>
      <div className="m-value num" style={{ color }}>{value}</div>
      {sub && <div className="m-sub" style={subColor ? { color: subColor } : undefined}>{sub}</div>}
    </div>
  );
}

export function Pill({ text, color = "var(--accent)", filled = false }: { text: string; color?: string; filled?: boolean }) {
  return (
    <span className="pill" style={filled
      ? { background: color, color: "rgba(0,0,0,0.85)" }
      : { background: `color-mix(in srgb, ${color} 14%, transparent)`, color }}>
      {text}
    </span>
  );
}

export function StatusDot({ color, pulse = false }: { color: string; pulse?: boolean }) {
  return <span className={`dot${pulse ? " pulse" : ""}`} style={{ background: color, boxShadow: `0 0 6px ${color}` }} />;
}

export function PrimaryButton({ title, icon, onClick, disabled, gradient }: {
  title: string; icon?: string; onClick?: () => void; disabled?: boolean; gradient?: string;
}) {
  return (
    <button className="btn btn-primary" onClick={onClick} disabled={disabled}
      style={gradient ? { background: gradient } : undefined}>
      {icon && <Icon name={icon} size={13} />}{title}
    </button>
  );
}

export function GhostButton({ title, icon, onClick, danger, disabled }: { title: string; icon?: string; onClick?: () => void; danger?: boolean; disabled?: boolean }) {
  return (
    <button className={`btn btn-ghost${danger ? " danger" : ""}`} onClick={onClick} disabled={disabled}
      style={disabled ? { opacity: 0.45, cursor: "default" } : undefined}>
      {icon && <Icon name={icon} size={13} />}{title}
    </button>
  );
}

export function ActionChip({ title, icon, onClick, variant }: {
  title: string; icon: string; onClick?: () => void; variant?: "danger" | "warn" | "good";
}) {
  return (
    <button className={`chip${variant ? " " + variant : ""}`} onClick={onClick}>
      <Icon name={icon} size={12} />{title}
    </button>
  );
}

export function MetaChip({ icon, text }: { icon: string; text: string }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 5, fontSize: 11, fontWeight: 600, color: "var(--text-2)" }}>
      <Icon name={icon} size={11} style={{ color: "var(--text-3)" }} />{text}
    </span>
  );
}

export function ListRow({ children, separator = true }: { children: ReactNode; separator?: boolean }) {
  return (
    <div>
      <div className="list-row" style={{ display: "flex", alignItems: "center" }}>{children}</div>
      {separator && <Hairline style={{ marginLeft: 4, opacity: 0.5 }} />}
    </div>
  );
}

export function EmptyState({ icon, text, height }: { icon: string; text: string; height?: number }) {
  return (
    <div className="empty-state" style={{ minHeight: height, height }}>
      <Icon name={icon} size={26} strokeWidth={1.4} style={{ color: "var(--text-3)" }} />
      <span>{text}</span>
    </div>
  );
}

export function PageHeader({ title, subtitle, trailing }: { title: string; subtitle?: ReactNode; trailing?: ReactNode }) {
  return (
    <div style={{ display: "flex", alignItems: "flex-end", justifyContent: "space-between", gap: 16 }}>
      <div>
        <div style={{ fontSize: 26, fontWeight: 800, color: "var(--text-1)" }}>{title}</div>
        {subtitle && <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text-2)", marginTop: 4 }}>{subtitle}</div>}
      </div>
      {trailing}
    </div>
  );
}

export function Page({ children, gap = 30 }: { children: ReactNode; gap?: number }) {
  return (
    <div style={{ overflowY: "auto", height: "100%" }}>
      <div style={{
        display: "flex", flexDirection: "column", gap,
        padding: "30px 40px 40px", maxWidth: 1280,
      }}>
        {children}
      </div>
    </div>
  );
}

export function SearchField({ value, onChange, width = 170 }: { value: string; onChange: (v: string) => void; width?: number }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 7, padding: "8px 12px", borderRadius: 999,
      background: "rgba(255,255,255,0.04)", border: "1px solid var(--stroke)",
    }}>
      <Icon name="search" size={12} style={{ color: "var(--text-3)" }} />
      <input className="field" value={value} placeholder="Search…" onChange={(e) => onChange(e.target.value)}
        style={{ background: "transparent", border: 0, padding: 0, width, fontSize: 12.5 }} />
    </div>
  );
}

export function SegmentedPicker<T extends string>({ value, options, onChange, width }: {
  value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; width?: number;
}) {
  return (
    <div style={{
      display: "flex", gap: 4, padding: 4, borderRadius: 11, width,
      background: "rgba(255,255,255,0.04)", border: "1px solid var(--stroke)",
    }}>
      {options.map((o) => {
        const sel = o.value === value;
        return (
          <button key={o.value} onClick={() => onChange(o.value)} style={{
            flex: 1, padding: "7px 0", borderRadius: 8, border: 0, fontSize: 12.5, fontWeight: 700,
            color: sel ? "rgba(0,0,0,0.85)" : "var(--text-2)",
            background: sel ? "var(--brand-grad-h)" : "transparent",
          }}>{o.label}</button>
        );
      })}
    </div>
  );
}

export function FormField({ label, icon, hint, children }: { label: string; icon?: string; hint?: string; children: ReactNode }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, flex: 1 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        {icon && <Icon name={icon} size={11} style={{ color: "var(--accent)" }} />}
        <span style={{ fontSize: 12, fontWeight: 700, color: "var(--text-2)" }}>{label}</span>
      </div>
      {children}
      {hint && <span style={{ fontSize: 10.5, fontWeight: 600, color: "var(--text-3)" }}>{hint}</span>}
    </div>
  );
}

// Gold coin glyph next to the hero number.
export function Coin({ size = 26 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" style={{ display: "block", flexShrink: 0 }}>
      <defs>
        <radialGradient id="safCoin" cx="38%" cy="32%" r="75%">
          <stop offset="0%" stopColor="#ffe9b8" />
          <stop offset="55%" stopColor="#FFC861" />
          <stop offset="100%" stopColor="#d79a2e" />
        </radialGradient>
      </defs>
      <circle cx="8" cy="8" r="7" fill="url(#safCoin)" stroke="#9c6f1f" strokeWidth="0.6" />
      <circle cx="8" cy="8" r="4.6" fill="none" stroke="#fff3d4" strokeWidth="0.8" opacity="0.5" />
      <path d="M8 4.4v7.2M6 6.1h3a1.3 1.3 0 010 2.6H6.4M6.6 8.7H9.2a1.3 1.3 0 010 2.6H6"
        stroke="#825a14" strokeWidth="0.9" fill="none" strokeLinecap="round" opacity="0.5" />
    </svg>
  );
}

export function CodeBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="mono" onClick={() => { navigator.clipboard?.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 1200); }}
      style={{
        display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, cursor: "pointer",
        padding: "11px 14px", borderRadius: 10, background: "rgba(0,0,0,0.3)", border: "1px solid var(--stroke)",
        fontSize: 12, color: "var(--accent)",
      }}>
      <span>{text}</span>
      <Icon name={copied ? "check" : "copy"} size={11} style={{ color: copied ? "var(--profit)" : "var(--text-3)" }} />
    </div>
  );
}
