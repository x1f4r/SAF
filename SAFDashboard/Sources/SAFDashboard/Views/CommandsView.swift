import SwiftUI

struct CommandsView: View {
    @EnvironmentObject var store: AppStore
    @State private var rawLine = ""
    @State private var query = ""
    @State private var sheetCommand: CommandDefinition?
    @State private var pendingConfirm: ConfirmAction?

    private let quick: [(String, String, String)] = [
        ("global_stats", "Global Stats", "chart.bar.doc.horizontal"),
        ("users", "Users", "person.2"),
        ("connections", "Connections", "link"),
        ("stats", "Stats", "gauge"),
        ("profit", "Profit", "dollarsign.circle"),
        ("ping", "Ping", "antenna.radiowaves.left.and.right"),
        ("reconcile", "Reconcile", "arrow.triangle.2.circlepath"),
        ("claim_sold", "Claim Sold", "tray.and.arrow.down"),
        ("bids", "Bids", "hand.raised"),
    ]

    private var filtered: [CommandDefinition] {
        guard !query.isEmpty else { return store.commands }
        return store.commands.filter { $0.name.localizedCaseInsensitiveContains(query) || $0.description.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        Page {
            PageHeader("Commands", subtitle: "Run any SAF command — the same surface as Discord")

            terminalBar

            VStack(alignment: .leading, spacing: 12) {
                SectionLabel("Quick Actions", systemImage: "bolt.fill")
                FlowLayout(spacing: 8) {
                    ForEach(quick, id: \.0) { item in
                        ActionChip(title: item.1, icon: item.2) { run(name: item.0) }
                    }
                }
            }

            VStack(alignment: .leading, spacing: 12) {
                SectionLabel("All Commands", systemImage: "command") { SearchField(text: $query) }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 250), spacing: 12)], spacing: 12) {
                    ForEach(filtered) { command in
                        CommandCell(command: command) {
                            if !command.options.isEmpty { sheetCommand = command } else { run(name: command.name) }
                        }
                    }
                }
            }
        }
        .sheet(item: $sheetCommand) { command in
            CommandOptionSheet(command: command) { options in
                run(name: command.name, options: options); sheetCommand = nil
            } cancel: { sheetCommand = nil }
        }
        .alert("Confirm action", isPresented: Binding(get: { pendingConfirm != nil }, set: { if !$0 { pendingConfirm = nil } })) {
            Button("Cancel", role: .cancel) { pendingConfirm = nil }
            Button("Confirm", role: .destructive) {
                if let confirm = pendingConfirm { store.runButton(confirm.button, title: confirm.title) }
                pendingConfirm = nil
            }
        } message: { Text(pendingConfirm?.message ?? "") }
    }

    private var terminalBar: some View {
        HStack(spacing: 11) {
            Image(systemName: "chevron.right").font(.system(size: 13, weight: .bold)).foregroundStyle(Theme.accent)
            TextField("Type a command, e.g. /cofl switchregion EU", text: $rawLine)
                .textFieldStyle(.plain)
                .font(.system(size: 13.5, weight: .medium, design: .monospaced))
                .foregroundStyle(Theme.textPrimary)
                .onSubmit(sendRaw)
            PrimaryButton(title: "Send", systemImage: "paperplane.fill") { sendRaw() }
        }
        .padding(.horizontal, 16).padding(.vertical, 12)
        .background(RoundedRectangle(cornerRadius: 13, style: .continuous).fill(Color.black.opacity(0.25)))
        .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(Theme.cardStroke, lineWidth: 1))
    }

    private func sendRaw() {
        let line = rawLine.trimmingCharacters(in: .whitespaces)
        guard !line.isEmpty else { return }
        store.runLine(line); rawLine = ""
    }

    private func run(name: String, options: [String: JSONValue] = [:]) {
        Task {
            guard let api = store.api else { return }
            do {
                let result = try await api.execute(command: name, options: options)
                if result.requiresConfirmation == true, let confirm = result.confirm {
                    pendingConfirm = confirm
                } else {
                    store.toast = ToastMessage(icon: "checkmark.circle.fill", tint: Theme.accent, title: "/\(name)", detail: "Sent")
                    await store.refresh()
                }
            } catch {
                store.toast = ToastMessage(icon: "xmark.octagon.fill", tint: Theme.loss, title: "/\(name) failed",
                    detail: (error as? APIError)?.errorDescription ?? error.localizedDescription)
            }
        }
    }
}

struct CommandCell: View {
    let command: CommandDefinition
    let action: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("/\(command.name)").font(.rounded(13, .semibold)).foregroundStyle(hover ? Theme.accent : Theme.textPrimary)
                    Spacer()
                    if !command.options.isEmpty {
                        Image(systemName: "slider.horizontal.3").font(.system(size: 10)).foregroundStyle(Theme.textTertiary)
                    }
                }
                Text(command.description).font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary).lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(13)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 12).fill(Color.white.opacity(hover ? 0.05 : 0.022)))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(hover ? Theme.accent.opacity(0.35) : Theme.cardStroke, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .animation(.easeOut(duration: 0.12), value: hover)
    }
}

struct CommandOptionSheet: View {
    let command: CommandDefinition
    let run: ([String: JSONValue]) -> Void
    let cancel: () -> Void
    @State private var values: [String: String] = [:]
    @State private var bools: [String: Bool] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 4) {
                Text("/\(command.name)").font(.rounded(20, .bold)).foregroundStyle(Theme.textPrimary)
                Text(command.description).font(.rounded(12.5, .medium)).foregroundStyle(Theme.textSecondary)
            }
            VStack(alignment: .leading, spacing: 14) {
                ForEach(command.options) { option in optionField(option) }
            }
            HStack {
                GhostButton(title: "Cancel") { cancel() }
                Spacer()
                PrimaryButton(title: "Run", systemImage: "play.fill") { run(build()) }
            }
        }
        .padding(26)
        .frame(width: 460)
        .background(Theme.bgBottom)
    }

    @ViewBuilder private func optionField(_ option: CommandOption) -> some View {
        if option.kind == "boolean" {
            Toggle(isOn: Binding(get: { bools[option.name] ?? false }, set: { bools[option.name] = $0 })) {
                Text(option.name).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary)
            }.tint(Theme.accent)
        } else if !option.choices.isEmpty {
            FormField(label: option.name + (option.required ? " *" : ""), hint: option.description) {
                Picker("", selection: Binding(get: { values[option.name] ?? "" }, set: { values[option.name] = $0 })) {
                    Text("—").tag("")
                    ForEach(option.choices) { choice in Text(choice.name).tag(choice.value) }
                }.labelsHidden().pickerStyle(.menu).tint(Theme.accent)
            }
        } else {
            FormField(label: option.name + (option.required ? " *" : ""), hint: option.description) {
                TextField(option.kind, text: Binding(get: { values[option.name] ?? "" }, set: { values[option.name] = $0 }))
            }
        }
    }

    private func build() -> [String: JSONValue] {
        var out: [String: JSONValue] = [:]
        for option in command.options {
            if option.kind == "boolean" {
                if let b = bools[option.name], b { out[option.name] = .bool(true) }
            } else if let v = values[option.name], !v.isEmpty {
                if option.kind == "integer", let n = Double(v) { out[option.name] = .number(n) }
                else { out[option.name] = .string(v) }
            }
        }
        return out
    }
}
