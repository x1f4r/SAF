import { useStore } from "../store";
import { Icon } from "./Icon";

export function Toast() {
  const { toast } = useStore();
  if (!toast) return null;
  return (
    <div style={{
      position: "fixed", left: "50%", bottom: 22, transform: "translateX(-50%)", zIndex: 100,
      display: "flex", alignItems: "center", gap: 11, padding: "12px 16px", borderRadius: 14,
      background: "rgba(20,24,33,0.85)", backdropFilter: "blur(20px)",
      border: `1px solid color-mix(in srgb, ${toast.tint} 40%, transparent)`,
      boxShadow: "0 8px 24px rgba(0,0,0,0.45)", animation: "toastIn 0.35s ease",
    }}>
      <Icon name={toast.icon} size={15} style={{ color: toast.tint }} />
      <div>
        <div style={{ fontSize: 13, fontWeight: 700, color: "var(--text-1)" }}>{toast.title}</div>
        {toast.detail && <div style={{ fontSize: 11.5, fontWeight: 600, color: "var(--text-2)" }}>{toast.detail}</div>}
      </div>
    </div>
  );
}

export const TOAST_CSS = `@keyframes toastIn { from { opacity: 0; transform: translate(-50%, 12px); } to { opacity: 1; transform: translate(-50%, 0); } }`;
