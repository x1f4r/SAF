// Login screen — web port of the macOS OnboardingView visual style, adapted to
// the gateway's password sign-in flow.
import { useState } from "react";
import { useStore } from "../store";
import { Surface, FormField, PrimaryButton } from "../components/UI";
import { Icon } from "../components/Icon";

export function LoginView() {
  const store = useStore();
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const noPasswordRequired = !!store.session && !store.session.needsPassword;

  async function submit() {
    if (busy) return;
    setBusy(true);
    setError(null);
    const ok = await store.login(password);
    setBusy(false);
    if (!ok) setError("Incorrect password. Please try again.");
  }

  async function continueOpen() {
    if (busy) return;
    setBusy(true);
    setError(null);
    const ok = await store.login("");
    setBusy(false);
    if (!ok) setError("Could not connect. Please try again.");
  }

  return (
    <div style={{ overflowY: "auto", height: "100%" }}>
      <div style={{ minHeight: "100%", display: "flex", alignItems: "center", justifyContent: "center", padding: "50px 24px" }}>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 22, width: "100%", maxWidth: 440 }}>
          {/* Header / brand mark */}
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 14 }}>
            <div style={{
              width: 70, height: 70, borderRadius: 20, background: "var(--brand-grad)",
              display: "flex", alignItems: "center", justifyContent: "center",
              boxShadow: "0 6px 18px color-mix(in srgb, var(--accent) 50%, transparent)",
            }}>
              <Icon name="bolt.fill" size={32} strokeWidth={2.6} style={{ color: "#fff" }} />
            </div>
            <div style={{ fontSize: 28, fontWeight: 800, color: "var(--text-1)", textAlign: "center" }}>SAF Dashboard</div>
            <div style={{ fontSize: 13.5, fontWeight: 600, color: "var(--text-2)", textAlign: "center" }}>
              Sign in to monitor and control your bot.
            </div>
          </div>

          {/* Sign-in card */}
          <Surface padding={24} style={{ width: "100%" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
              {noPasswordRequired ? (
                <>
                  <div style={{
                    display: "flex", alignItems: "center", gap: 8, padding: 11, borderRadius: 10,
                    background: "color-mix(in srgb, var(--profit) 10%, transparent)",
                  }}>
                    <Icon name="checkmark.circle.fill" size={15} style={{ color: "var(--profit)" }} />
                    <span style={{ fontSize: 12, fontWeight: 600, color: "var(--profit)" }}>No password required</span>
                  </div>
                  <PrimaryButton title="Continue" icon="chevron.right" onClick={continueOpen} disabled={busy} />
                </>
              ) : (
                <>
                  <FormField label="Password" icon="key.fill">
                    <input
                      className="field"
                      type="password"
                      placeholder="Enter password"
                      value={password}
                      autoFocus
                      onChange={(e) => { setPassword(e.target.value); if (error) setError(null); }}
                      onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
                    />
                  </FormField>

                  {error && (
                    <div style={{
                      display: "flex", alignItems: "center", gap: 8, padding: 11, borderRadius: 10,
                      background: "color-mix(in srgb, var(--loss) 10%, transparent)",
                    }}>
                      <Icon name="xmark.octagon.fill" size={15} style={{ color: "var(--loss)" }} />
                      <span style={{ fontSize: 12, fontWeight: 600, color: "var(--loss)" }}>{error}</span>
                    </div>
                  )}

                  <PrimaryButton title={busy ? "Signing in…" : "Sign in"} icon="chevron.right" onClick={submit} disabled={busy} />
                </>
              )}
            </div>
          </Surface>

          {/* Security note */}
          <Surface padding={16} style={{ width: "100%" }}>
            <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
              <Icon name="lock.shield.fill" size={18} strokeWidth={2.2} style={{ color: "var(--accent)", flexShrink: 0, marginTop: 1 }} />
              <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
                <div style={{ fontSize: 13, fontWeight: 700, color: "var(--text-1)" }}>How the connection works</div>
                <div style={{ fontSize: 12, fontWeight: 500, color: "var(--text-2)", lineHeight: 1.5 }}>
                  The gateway proxies your requests to the bot over a private connection. Your browser never sees the bot
                  token — it only talks to the gateway, which holds the credentials and forwards commands on your behalf.
                </div>
              </div>
            </div>
          </Surface>
        </div>
      </div>
    </div>
  );
}
