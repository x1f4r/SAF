import { useMemo, useState } from "react";
import { useStore } from "../store";
import { api, APIError } from "../api";
import {
  Page, PageHeader, SectionLabel, SearchField, ActionChip, FormField,
  PrimaryButton, GhostButton,
} from "../components/UI";
import { Icon } from "../components/Icon";
import type { CommandDefinition, CommandOption, ConfirmAction } from "../types";

const QUICK: [string, string, string][] = [
  ["global_stats", "Global Stats", "chart.bar.doc.horizontal"],
  ["users", "Users", "person.2"],
  ["connections", "Connections", "link"],
  ["stats", "Stats", "gauge"],
  ["profit", "Profit", "dollarsign.circle"],
  ["ping", "Ping", "antenna.radiowaves.left.and.right"],
  ["reconcile", "Reconcile", "arrow.triangle.2.circlepath"],
  ["claim_sold", "Claim Sold", "tray.and.arrow.down"],
  ["bids", "Bids", "hand.raised"],
];

export function CommandsView() {
  const store = useStore();
  const [rawLine, setRawLine] = useState("");
  const [query, setQuery] = useState("");
  const [sheetCommand, setSheetCommand] = useState<CommandDefinition | null>(null);
  const [pendingConfirm, setPendingConfirm] = useState<ConfirmAction | null>(null);

  const filtered = useMemo(() => {
    if (query.trim() === "") return store.commands;
    const q = query.toLowerCase();
    return store.commands.filter(
      (c) => c.name.toLowerCase().includes(q) || c.description.toLowerCase().includes(q),
    );
  }, [store.commands, query]);

  function sendRaw() {
    const line = rawLine.trim();
    if (!line) return;
    store.runLine(line);
    setRawLine("");
  }

  async function run(name: string, options: Record<string, unknown> = {}) {
    try {
      const result = await api.execute(name, options);
      if (result.requiresConfirmation && result.confirm) {
        setPendingConfirm(result.confirm);
      } else {
        store.showToast({ icon: "checkmark.circle.fill", tint: "var(--accent)", title: `/${name}`, detail: "Sent" });
        await store.refresh();
      }
    } catch (e) {
      store.showToast({
        icon: "xmark.octagon.fill", tint: "var(--loss)", title: `/${name} failed`,
        detail: e instanceof APIError ? e.message : String(e),
      });
    }
  }

  return (
    <Page>
      <PageHeader title="Commands" subtitle="Run any SAF command — the same surface as Discord" />

      {/* Terminal bar */}
      <div
        style={{
          display: "flex", alignItems: "center", gap: 11,
          padding: "12px 16px", borderRadius: 13,
          background: "rgba(0,0,0,0.25)", border: "1px solid var(--stroke)",
        }}
      >
        <Icon name="chevron.right" size={13} strokeWidth={2.8} style={{ color: "var(--accent)", flexShrink: 0 }} />
        <input
          className="field mono"
          value={rawLine}
          placeholder="Type a command, e.g. /cofl switchregion EU"
          onChange={(e) => setRawLine(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") sendRaw(); }}
          style={{ flex: 1, background: "transparent", border: 0, padding: 0, fontSize: 13.5, fontWeight: 500 }}
        />
        <PrimaryButton title="Send" icon="paperplane.fill" onClick={sendRaw} />
      </div>

      {/* Quick Actions */}
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <SectionLabel title="Quick Actions" icon="bolt.fill" />
        <div className="flow">
          {QUICK.map(([name, title, icon]) => (
            <ActionChip key={name} title={title} icon={icon} onClick={() => run(name)} />
          ))}
        </div>
      </div>

      {/* All Commands */}
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <SectionLabel title="All Commands" icon="command" trailing={<SearchField value={query} onChange={setQuery} />} />
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(250px, 1fr))",
            gap: 12,
          }}
        >
          {filtered.map((command) => (
            <CommandCell
              key={command.name}
              command={command}
              onClick={() => {
                if (command.options.length > 0) setSheetCommand(command);
                else run(command.name);
              }}
            />
          ))}
        </div>
      </div>

      {sheetCommand && (
        <CommandOptionSheet
          command={sheetCommand}
          onRun={(options) => { run(sheetCommand.name, options); setSheetCommand(null); }}
          onCancel={() => setSheetCommand(null)}
        />
      )}

      {pendingConfirm && (
        <ConfirmDialog
          confirm={pendingConfirm}
          onCancel={() => setPendingConfirm(null)}
          onConfirm={() => { store.runButton(pendingConfirm.button, pendingConfirm.title); setPendingConfirm(null); }}
        />
      )}
    </Page>
  );
}

function CommandCell({ command, onClick }: { command: CommandDefinition; onClick: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 4,
        padding: 13, borderRadius: 12, textAlign: "left", cursor: "pointer", width: "100%",
        background: hover ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.022)",
        border: `1px solid ${hover ? "rgba(94,200,255,0.35)" : "var(--stroke)"}`,
        transition: "all 0.12s ease",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", width: "100%", gap: 8 }}>
        <span className="mono" style={{ fontSize: 13, fontWeight: 600, color: hover ? "var(--accent)" : "var(--text-1)" }}>
          /{command.name}
        </span>
        <span style={{ flex: 1 }} />
        {command.options.length > 0 && (
          <Icon name="slider.horizontal.3" size={10} style={{ color: "var(--text-3)" }} />
        )}
      </div>
      <span
        style={{
          fontSize: 11, fontWeight: 500, color: "var(--text-3)", width: "100%",
          display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden",
        }}
      >
        {command.description}
      </span>
    </button>
  );
}

function CommandOptionSheet({ command, onRun, onCancel }: {
  command: CommandDefinition;
  onRun: (options: Record<string, unknown>) => void;
  onCancel: () => void;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [bools, setBools] = useState<Record<string, boolean>>({});

  function build(): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const option of command.options) {
      if (option.kind === "boolean") {
        if (bools[option.name]) out[option.name] = true;
      } else {
        const v = values[option.name];
        if (v && v !== "") {
          if (option.kind === "integer" && !isNaN(Number(v))) out[option.name] = Number(v);
          else out[option.name] = v;
        }
      }
    }
    return out;
  }

  return (
    <Overlay onClose={onCancel}>
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          display: "flex", flexDirection: "column", gap: 18, width: 460, padding: 26,
          borderRadius: 16, background: "var(--bg-bottom)", border: "1px solid var(--stroke)",
          boxShadow: "0 24px 60px rgba(0,0,0,0.5)",
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="mono" style={{ fontSize: 20, fontWeight: 800, color: "var(--text-1)" }}>/{command.name}</span>
          <span style={{ fontSize: 12.5, fontWeight: 500, color: "var(--text-2)" }}>{command.description}</span>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          {command.options.map((option) => (
            <OptionField
              key={option.name}
              option={option}
              value={values[option.name] ?? ""}
              checked={bools[option.name] ?? false}
              onText={(v) => setValues((cur) => ({ ...cur, [option.name]: v }))}
              onBool={(v) => setBools((cur) => ({ ...cur, [option.name]: v }))}
            />
          ))}
        </div>

        <div style={{ display: "flex", alignItems: "center" }}>
          <GhostButton title="Cancel" onClick={onCancel} />
          <span style={{ flex: 1 }} />
          <PrimaryButton title="Run" icon="play.fill" onClick={() => onRun(build())} />
        </div>
      </div>
    </Overlay>
  );
}

function OptionField({ option, value, checked, onText, onBool }: {
  option: CommandOption;
  value: string;
  checked: boolean;
  onText: (v: string) => void;
  onBool: (v: boolean) => void;
}) {
  if (option.kind === "boolean") {
    return (
      <label style={{ display: "flex", alignItems: "center", gap: 10, cursor: "pointer" }}>
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => onBool(e.target.checked)}
          style={{ accentColor: "var(--accent)", width: 16, height: 16, cursor: "pointer" }}
        />
        <span style={{ fontSize: 12.5, fontWeight: 600, color: "var(--text-1)" }}>{option.name}</span>
      </label>
    );
  }
  const label = option.name + (option.required ? " *" : "");
  if (option.choices.length > 0) {
    return (
      <FormField label={label} hint={option.description}>
        <select className="field" value={value} onChange={(e) => onText(e.target.value)}>
          <option value="">—</option>
          {option.choices.map((choice) => (
            <option key={choice.value} value={choice.value}>{choice.name}</option>
          ))}
        </select>
      </FormField>
    );
  }
  return (
    <FormField label={label} hint={option.description}>
      <input
        className="field"
        value={value}
        placeholder={option.kind}
        onChange={(e) => onText(e.target.value)}
      />
    </FormField>
  );
}

function ConfirmDialog({ confirm, onConfirm, onCancel }: {
  confirm: ConfirmAction;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Overlay onClose={onCancel}>
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          display: "flex", flexDirection: "column", gap: 16, width: 400, padding: 26,
          borderRadius: 16, background: "var(--bg-bottom)", border: "1px solid var(--stroke)",
          boxShadow: "0 24px 60px rgba(0,0,0,0.5)",
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <span style={{ fontSize: 17, fontWeight: 800, color: "var(--text-1)" }}>{confirm.title}</span>
          <span style={{ fontSize: 12.5, fontWeight: 500, color: "var(--text-2)" }}>{confirm.message}</span>
        </div>
        <div style={{ display: "flex", alignItems: "center" }}>
          <GhostButton title="Cancel" onClick={onCancel} />
          <span style={{ flex: 1 }} />
          <GhostButton title="Confirm" icon="exclamationmark.triangle.fill" danger onClick={onConfirm} />
        </div>
      </div>
    </Overlay>
  );
}

function Overlay({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 100,
        display: "flex", alignItems: "center", justifyContent: "center",
        background: "rgba(0,0,0,0.5)", backdropFilter: "blur(2px)",
      }}
    >
      {children}
    </div>
  );
}
