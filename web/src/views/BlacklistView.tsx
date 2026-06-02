import { useState } from "react";
import { useStore } from "../store";
import {
  Page, PageHeader, Surface, SectionLabel, SegmentedPicker,
  FormField, PrimaryButton, GhostButton, Hairline,
} from "../components/UI";

type Scope = "buy" | "relist";
type Field = "tag" | "name" | "enchant" | "item-enchant";

const FIELD_OPTIONS: { value: Field; label: string }[] = [
  { value: "tag", label: "Tag" },
  { value: "name", label: "Name" },
  { value: "enchant", label: "Enchant" },
  { value: "item-enchant", label: "Item + Enchant" },
];

const VALUE_PLACEHOLDER: Record<Field, string> = {
  tag: "HYPERION",
  name: "Hyperion",
  enchant: "THE_ONE:5",
  "item-enchant": "LAVA_SHELL_NECKLACE THE_ONE:5",
};

export function BlacklistView() {
  const store = useStore();
  const [scope, setScope] = useState<Scope>("buy");
  const [field, setField] = useState<Field>("tag");
  const [value, setValue] = useState("");
  const [duration, setDuration] = useState("");
  const [username, setUsername] = useState("");

  const accounts = store.accounts?.accounts ?? [];
  const trimmedValue = value.trim();
  const canSubmit = trimmedValue.length > 0;

  function buildOptions(action: "add" | "remove"): Record<string, unknown> {
    const options: Record<string, unknown> = { action, scope, field, value: trimmedValue };
    const dur = duration.trim();
    if (dur) options.duration = dur;
    if (username) options.username = username;
    return options;
  }

  function submit(action: "add" | "remove") {
    if (!canSubmit) return;
    store.runCommand("blacklist", buildOptions(action), action === "add" ? "Blacklist add" : "Blacklist remove");
  }

  return (
    <Page>
      <PageHeader title="Blacklist" subtitle="Live buy/relist rules — applied without a restart." />

      <Surface>
        <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
          <SectionLabel title="Add rule" icon="tag.fill" />

          <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
            <FormField label="Scope" icon="slider.horizontal.3">
              <SegmentedPicker<Scope>
                value={scope}
                onChange={setScope}
                options={[
                  { value: "buy", label: "Buy" },
                  { value: "relist", label: "Relist" },
                ]}
              />
            </FormField>

            <FormField label="Field" icon="tag.fill">
              <select className="field" value={field} onChange={(e) => setField(e.target.value as Field)}>
                {FIELD_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
            </FormField>
          </div>

          <FormField label="Value" icon="search" hint="What to match — see the note below for the format per field.">
            <input
              className="field"
              value={value}
              placeholder={VALUE_PLACEHOLDER[field]}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") submit("add"); }}
            />
          </FormField>

          <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
            <FormField label="Duration" icon="hourglass" hint="Optional — blank means forever.">
              <input
                className="field"
                value={duration}
                placeholder="7d / 12h / blank = forever"
                onChange={(e) => setDuration(e.target.value)}
              />
            </FormField>

            <FormField label="Account" icon="person.fill" hint="Optional — default applies to all accounts.">
              <select className="field" value={username} onChange={(e) => setUsername(e.target.value)}>
                <option value="">All accounts</option>
                {accounts.map((a) => (
                  <option key={a.ign} value={a.ign}>{a.ign}</option>
                ))}
              </select>
            </FormField>
          </div>

          <Hairline />

          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <PrimaryButton title="Add rule" icon="checkmark.circle.fill" onClick={() => submit("add")} disabled={!canSubmit} />
            <span style={{ flex: 1 }} />
            <GhostButton title="Remove rule" icon="trash.fill" danger onClick={() => submit("remove")} />
          </div>
        </div>
      </Surface>

      <Surface>
        <div style={{ display: "flex", flexDirection: "column", gap: 10, fontSize: 12, fontWeight: 500, color: "var(--text-2)", lineHeight: 1.55 }}>
          <SectionLabel title="How it works" icon="command" />
          <div>
            <b style={{ color: "var(--text-1)" }}>Scope</b> picks where the rule applies — <b>Buy</b> blocks purchasing the item, <b>Relist</b> stops it from being relisted.
          </div>
          <div>
            <b style={{ color: "var(--text-1)" }}>Field</b> selects how the value is matched:
            <ul style={{ margin: "6px 0 0", paddingLeft: 18, display: "flex", flexDirection: "column", gap: 3 }}>
              <li><b>Tag</b> — the item's internal id, e.g. <span className="mono">HYPERION</span>.</li>
              <li><b>Name</b> — the display name, e.g. <span className="mono">Hyperion</span>.</li>
              <li><b>Enchant</b> — an enchant and level, e.g. <span className="mono">THE_ONE:5</span>.</li>
              <li><b>Item + Enchant</b> — both together, e.g. <span className="mono">LAVA_SHELL_NECKLACE THE_ONE:5</span>.</li>
            </ul>
          </div>
          <div>
            <b style={{ color: "var(--text-1)" }}>Duration</b> auto-expires the rule (<span className="mono">7d</span>, <span className="mono">12h</span>); leave it blank to keep it forever.
          </div>
          <div>
            For an absolute end-date, use the Commands terminal:{" "}
            <span className="mono" style={{ color: "var(--accent)" }}>blacklist add buy tag X --until 2026-07-01</span>.
          </div>
        </div>
      </Surface>
    </Page>
  );
}
