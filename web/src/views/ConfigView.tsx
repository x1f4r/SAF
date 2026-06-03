// Config editor — loads the bot's config (secrets stripped server-side), exposes
// the common tuning knobs, and PATCHes only the top-level keys you changed. Most
// fields apply on the next restart; the blacklist/live rules live on their own tab.
import { useEffect, useState } from "react";
import { api } from "../api";
import { useStore } from "../store";
import {
  Page,
  PageHeader,
  Surface,
  SectionLabel,
  PrimaryButton,
  GhostButton,
  FormField,
  EmptyState,
  Hairline,
} from "../components/UI";
import { Icon } from "../components/Icon";

// A flat pill toggle matching the cyan/violet design used across the app.
function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button onClick={() => onChange(!on)} type="button"
      style={{
        width: 44, height: 26, borderRadius: 999, border: "none", cursor: "pointer", padding: 3,
        background: on ? "var(--brand-grad-h)" : "rgba(255,255,255,0.1)",
        display: "flex", justifyContent: on ? "flex-end" : "flex-start", transition: "all 0.15s",
      }}>
      <span style={{ width: 20, height: 20, borderRadius: "50%", background: "#fff" }} />
    </button>
  );
}

function ToggleRow({ label, hint, on, onChange }: {
  label: string; hint?: string; on: boolean; onChange: (v: boolean) => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 700, color: "var(--text-1)" }}>{label}</div>
        {hint && <div style={{ fontSize: 11.5, fontWeight: 600, color: "var(--text-3)" }}>{hint}</div>}
      </div>
      <Toggle on={on} onChange={onChange} />
    </div>
  );
}

// The SAFE-to-edit knobs we surface, grouped by kind. Nested keys use dot paths.
type Toggles = "useCookie" | "relist" | "pingOnUpdate" | "bedSpam" | "blockUselessMessages";
type Numbers = "delay" | "buyingReadyDelay" | "waittime" | "clickDelay";
type Texts = "autoCookie";
type SkipTexts = "skip.minProfit" | "skip.minPrice" | "skip.profitPercentage";

const TOGGLE_FIELDS: { key: Toggles; label: string; hint: string }[] = [
  { key: "useCookie", label: "Use booster cookie", hint: "Keep a booster cookie active while flipping." },
  { key: "relist", label: "Relist", hint: "Re-list items that don't sell." },
  { key: "pingOnUpdate", label: "Ping on update", hint: "Mention you in Discord when the bot updates." },
  { key: "bedSpam", label: "Bed spam", hint: "Aggressively retry contested purchases." },
  { key: "blockUselessMessages", label: "Block useless messages", hint: "Suppress noisy chat output." },
];

const NUMBER_FIELDS: { key: Numbers; label: string; hint: string }[] = [
  { key: "delay", label: "Delay (ms)", hint: "Base delay between actions." },
  { key: "buyingReadyDelay", label: "Buying ready delay (ms)", hint: "Wait before the buy window opens." },
  { key: "waittime", label: "Wait time (ms)", hint: "Pause between flip cycles." },
  { key: "clickDelay", label: "Click delay (ms)", hint: "Delay between simulated clicks." },
];

const SKIP_FIELDS: { key: SkipTexts; label: string; hint: string; placeholder: string }[] = [
  { key: "skip.minProfit", label: "Min profit", hint: "Skip flips below this profit.", placeholder: "100k" },
  { key: "skip.minPrice", label: "Min price", hint: "Skip items cheaper than this.", placeholder: "50k" },
  { key: "skip.profitPercentage", label: "Profit %", hint: "Skip flips below this margin.", placeholder: "5" },
];

// Normalize a possibly-missing config value to a string for text inputs.
function asText(v: unknown): string {
  if (v == null) return "";
  return String(v);
}

export function ConfigView() {
  const store = useStore();
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [saving, setSaving] = useState(false);

  // The config exactly as the server last gave it (the baseline we diff against).
  const [original, setOriginal] = useState<Record<string, any>>({});
  // Working copy of the editable fields.
  const [toggles, setToggles] = useState<Record<Toggles | string, boolean>>({});
  const [numbers, setNumbers] = useState<Record<Numbers | string, string>>({});
  const [autoCookie, setAutoCookie] = useState("");
  // Initialize with empty strings: buildPatch() runs on the first render (before
  // the GET resolves) and calls .trim() on these, which would throw on undefined.
  const [skip, setSkip] = useState<Record<string, string>>({ minProfit: "", minPrice: "", profitPercentage: "" });

  const load = () => {
    setLoading(true);
    setFailed(false);
    api.getConfig()
      .then((res) => {
        const cfg = res.config ?? {};
        setOriginal(cfg);
        const t: Record<string, boolean> = {};
        for (const f of TOGGLE_FIELDS) t[f.key] = !!cfg[f.key];
        setToggles(t);
        const n: Record<string, string> = {};
        for (const f of NUMBER_FIELDS) n[f.key] = asText(cfg[f.key]);
        setNumbers(n);
        setAutoCookie(asText(cfg.autoCookie));
        const sk = cfg.skip ?? {};
        setSkip({
          minProfit: asText(sk.minProfit),
          minPrice: asText(sk.minPrice),
          profitPercentage: asText(sk.profitPercentage),
        });
      })
      .catch((e) => {
        setFailed(true);
        store.showToast({ icon: "x", tint: "var(--loss)", title: "Couldn't load config", detail: String(e?.message ?? e) });
      })
      .finally(() => setLoading(false));
  };

  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { load(); }, []);

  // Build a patch of only the top-level keys that changed. The `skip` object is
  // merged wholesale (per the API contract) whenever any of its fields differ.
  function buildPatch(): Record<string, unknown> {
    const patch: Record<string, unknown> = {};

    for (const f of TOGGLE_FIELDS) {
      if (toggles[f.key] !== !!original[f.key]) patch[f.key] = toggles[f.key];
    }
    for (const f of NUMBER_FIELDS) {
      const raw = numbers[f.key]?.trim() ?? "";
      if (raw === "") continue;
      const num = Number(raw);
      if (!Number.isFinite(num)) continue;
      if (num !== Number(original[f.key])) patch[f.key] = num;
    }
    if (autoCookie.trim() !== asText(original.autoCookie)) patch.autoCookie = autoCookie.trim();

    const origSkip = original.skip ?? {};
    const sMinProfit = skip.minProfit ?? "";
    const sMinPrice = skip.minPrice ?? "";
    const sProfitPct = skip.profitPercentage ?? "";
    const skipChanged =
      sMinProfit !== asText(origSkip.minProfit) ||
      sMinPrice !== asText(origSkip.minPrice) ||
      sProfitPct !== asText(origSkip.profitPercentage);
    if (skipChanged) {
      patch.skip = {
        ...origSkip,
        minProfit: sMinProfit.trim(),
        minPrice: sMinPrice.trim(),
        profitPercentage: sProfitPct.trim(),
      };
    }
    return patch;
  }

  const patch = buildPatch();
  const dirtyCount = Object.keys(patch).length;

  function save() {
    if (dirtyCount === 0 || saving) return;
    setSaving(true);
    api.patchConfig(patch)
      .then((res) => {
        const updated = res.updated ?? Object.keys(patch);
        store.showToast({
          icon: "check", tint: "var(--profit)", title: "Config saved",
          detail: `${updated.length} field${updated.length === 1 ? "" : "s"} updated — restart to apply.`,
        });
        store.logDiag("info", `Config updated: ${updated.join(", ")}`);
        // Re-read so the baseline matches what was persisted.
        load();
      })
      .catch((e: any) => {
        // The gateway surfaces the bot's structured errors via APIError.message,
        // but forbidden_fields / parse_error carry a clearer body when present.
        const forbidden: string[] | undefined = e?.forbidden;
        const detail = forbidden?.length
          ? `Forbidden: ${forbidden.join(", ")}`
          : String(e?.message ?? e);
        store.showToast({ icon: "x", tint: "var(--loss)", title: "Save failed", detail });
        store.logDiag("error", "Config save failed: " + detail);
      })
      .finally(() => setSaving(false));
  }

  return (
    <Page gap={18}>
      <PageHeader
        title="Config"
        subtitle="The common tuning knobs — most changes apply on the next restart."
        trailing={
          <div style={{ display: "flex", gap: 10 }}>
            <GhostButton title="Reload" icon="arrow.clockwise" onClick={load} />
            <PrimaryButton
              title={dirtyCount > 0 ? `Save ${dirtyCount} change${dirtyCount === 1 ? "" : "s"}` : "Save"}
              icon="checkmark.circle.fill"
              onClick={save}
              disabled={dirtyCount === 0 || saving || loading || failed}
            />
          </div>
        }
      />

      {loading ? (
        <EmptyState icon="hourglass" text="Loading config…" height={220} />
      ) : failed ? (
        <EmptyState icon="exclamationmark.triangle.fill" text="Couldn't reach the bot config." height={220} />
      ) : (
        <>
          {/* Applies-on-restart note */}
          <div style={{
            display: "flex", alignItems: "center", gap: 9,
            fontSize: 12, fontWeight: 600, color: "var(--text-3)", lineHeight: 1.5,
            padding: "11px 14px", borderRadius: 10,
            background: "color-mix(in srgb, var(--warn) 9%, transparent)",
            border: "1px solid color-mix(in srgb, var(--warn) 28%, transparent)",
          }}>
            <Icon name="exclamationmark.triangle.fill" size={13} style={{ color: "var(--warn)" }} />
            These settings apply on the next restart. Save here, then restart the bot from the Connection tab.
          </div>

          {/* Toggles */}
          <Surface padding={22}>
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <SectionLabel title="Behavior" icon="slider.horizontal.3" />
              {TOGGLE_FIELDS.map((f, i) => (
                <div key={f.key} style={{ display: "flex", flexDirection: "column", gap: 16 }}>
                  <ToggleRow
                    label={f.label}
                    hint={f.hint}
                    on={!!toggles[f.key]}
                    onChange={(v) => setToggles((cur) => ({ ...cur, [f.key]: v }))}
                  />
                  {i < TOGGLE_FIELDS.length - 1 && <Hairline />}
                </div>
              ))}
            </div>
          </Surface>

          {/* Timing */}
          <Surface padding={22}>
            <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
              <SectionLabel title="Timing" icon="gauge" />
              <FormField label="Auto cookie" icon="cookie" hint="When to auto-buy a booster cookie, e.g. 2h. Blank disables it.">
                <input className="field" value={autoCookie} placeholder="2h"
                  onChange={(e) => setAutoCookie(e.target.value)} />
              </FormField>
              <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
                {NUMBER_FIELDS.map((f) => (
                  <FormField key={f.key} label={f.label} icon="speedometer" hint={f.hint}>
                    <input
                      className="field"
                      type="number"
                      value={numbers[f.key] ?? ""}
                      onChange={(e) => setNumbers((cur) => ({ ...cur, [f.key]: e.target.value }))}
                    />
                  </FormField>
                ))}
              </div>
            </div>
          </Surface>

          {/* Skip thresholds */}
          <Surface padding={22}>
            <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
              <SectionLabel title="Skip thresholds" icon="scope" />
              <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
                {SKIP_FIELDS.map((f) => {
                  const key = f.key.split(".")[1];
                  return (
                    <FormField key={f.key} label={f.label} icon="dollar" hint={f.hint}>
                      <input
                        className="field"
                        value={skip[key] ?? ""}
                        placeholder={f.placeholder}
                        onChange={(e) => setSkip((cur) => ({ ...cur, [key]: e.target.value }))}
                      />
                    </FormField>
                  );
                })}
              </div>
            </div>
          </Surface>

          <div style={{ fontSize: 11.5, fontWeight: 600, color: "var(--text-3)", lineHeight: 1.5 }}>
            Secrets (the Discord token, session, API keys, webhook URL) are never exposed here and can't be
            edited from the browser. Saving an unrecognized field is rejected by the bot.
          </div>
        </>
      )}
    </Page>
  );
}
