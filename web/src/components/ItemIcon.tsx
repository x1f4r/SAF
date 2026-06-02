// Official SkyBlock item icons by tag (Coflnet CDN) + player-head avatars.
import { useState } from "react";
import { Icon } from "./Icon";

export function ItemIcon({ tag, size = 34 }: { tag?: string | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  const url = tag ? `https://sky.coflnet.com/static/icon/${encodeURIComponent(tag)}` : null;
  return (
    <div style={{
      width: size, height: size, flex: "none", borderRadius: size * 0.26,
      background: "rgba(255,255,255,0.05)", border: "1px solid var(--stroke)",
      display: "flex", alignItems: "center", justifyContent: "center", overflow: "hidden",
    }}>
      {url && !failed ? (
        <img src={url} alt="" width={size * 0.78} height={size * 0.78} style={{ imageRendering: "auto", objectFit: "contain" }} onError={() => setFailed(true)} />
      ) : (
        <Icon name="cube" size={size * 0.38} style={{ color: "var(--text-3)" }} />
      )}
    </div>
  );
}

export function AccountAvatar({ url, size = 34 }: { url?: string | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  return (
    <div style={{
      width: size, height: size, flex: "none", borderRadius: size * 0.28,
      background: "rgba(255,255,255,0.06)", border: "1px solid var(--stroke)",
      display: "flex", alignItems: "center", justifyContent: "center", overflow: "hidden",
    }}>
      {url && !failed ? (
        <img src={url} alt="" width={size * 0.8} height={size * 0.8} style={{ imageRendering: "pixelated", objectFit: "contain" }} onError={() => setFailed(true)} />
      ) : (
        <Icon name="person" size={size * 0.4} style={{ color: "var(--text-3)" }} />
      )}
    </div>
  );
}
