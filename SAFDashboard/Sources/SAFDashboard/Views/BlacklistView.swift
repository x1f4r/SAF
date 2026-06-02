import SwiftUI

struct BlacklistView: View {
    @EnvironmentObject var store: AppStore

    @State private var scope: Scope = .buy
    @State private var field: Field = .tag
    @State private var value = ""
    @State private var duration = ""
    @State private var account = ""

    private enum Scope: String, CaseIterable { case buy, relist }

    private enum Field: String, CaseIterable {
        case tag, name, enchant
        case itemEnchant = "item-enchant"

        var placeholder: String {
            switch self {
            case .tag: return "HYPERION"
            case .name: return "Hyperion"
            case .enchant: return "THE_ONE:5"
            case .itemEnchant: return "LAVA_SHELL_NECKLACE THE_ONE:5"
            }
        }
    }

    private var accounts: [String] { store.accounts?.accounts.map(\.ign) ?? store.status?.configured ?? [] }
    private var trimmedValue: String { value.trimmingCharacters(in: .whitespaces) }
    private var trimmedDuration: String { duration.trimmingCharacters(in: .whitespaces) }
    private var canSubmit: Bool { !trimmedValue.isEmpty }

    var body: some View {
        Page {
            PageHeader("Blacklist", subtitle: "Live buy/relist rules — applied without a restart.")

            Surface(padding: 22) {
                VStack(alignment: .leading, spacing: 18) {
                    SectionLabel("Add rule", systemImage: "nosign")

                    VStack(alignment: .leading, spacing: 7) {
                        Text("SCOPE").font(.rounded(9.5, .semibold)).tracking(0.7).foregroundStyle(Theme.textTertiary)
                        SegmentedPicker(selection: $scope, options: Scope.allCases, label: \.rawValue).frame(width: 220)
                    }

                    HStack(alignment: .top, spacing: 14) {
                        FormField(label: "Field", systemImage: "tag") {
                            Picker("", selection: $field) {
                                ForEach(Field.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                            }
                            .labelsHidden().pickerStyle(.menu).tint(Theme.accent)
                        }
                        .frame(width: 190)

                        FormField(label: "Value", systemImage: "text.cursor") {
                            TextField(field.placeholder, text: $value)
                        }
                    }

                    HStack(alignment: .top, spacing: 14) {
                        FormField(label: "Duration", systemImage: "clock", hint: "Relative window — blank means forever.") {
                            TextField("7d / 12h / blank = forever", text: $duration)
                        }

                        FormField(label: "Account", systemImage: "person", hint: "Default applies to all accounts.") {
                            Picker("", selection: $account) {
                                Text("All accounts").tag("")
                                ForEach(accounts, id: \.self) { Text($0).tag($0) }
                            }
                            .labelsHidden().pickerStyle(.menu).tint(Theme.accent)
                        }
                        .frame(width: 220)
                    }

                    HStack(spacing: 12) {
                        PrimaryButton(title: "Add rule", systemImage: "plus.circle.fill") { submit(action: "add") }
                            .disabled(!canSubmit)
                            .opacity(canSubmit ? 1 : 0.5)
                        GhostButton(title: "Remove rule", systemImage: "minus.circle", role: .destructive) { submit(action: "remove") }
                            .disabled(!canSubmit)
                            .opacity(canSubmit ? 1 : 0.5)
                        Spacer()
                    }
                }
            }

            helper
        }
    }

    private var helper: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel("How fields match", systemImage: "info.circle")
            VStack(alignment: .leading, spacing: 6) {
                helperRow("tag", "Reforge/item tag, e.g. HYPERION.")
                helperRow("name", "Display name, e.g. Hyperion.")
                helperRow("enchant", "Enchant + level on any item, e.g. THE_ONE:5.")
                helperRow("item-enchant", "Enchant + level only on a specific item, e.g. LAVA_SHELL_NECKLACE THE_ONE:5.")
            }
            Text("For an absolute end-date, use the Commands terminal: blacklist add buy tag X --until 2026-07-01.")
                .font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary)
                .padding(.top, 4)
        }
    }

    private func helperRow(_ key: String, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(key)
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .foregroundStyle(Theme.accent)
                .frame(width: 104, alignment: .leading)
            Text(text).font(.rounded(11.5, .medium)).foregroundStyle(Theme.textSecondary)
        }
    }

    private func submit(action: String) {
        guard canSubmit else { return }
        var options: [String: JSONValue] = [
            "action": .string(action),
            "scope": .string(scope.rawValue),
            "field": .string(field.rawValue),
            "value": .string(trimmedValue),
        ]
        if !trimmedDuration.isEmpty { options["duration"] = .string(trimmedDuration) }
        if !account.isEmpty { options["username"] = .string(account) }
        store.runCommand("blacklist", options: options, label: action == "add" ? "Blacklist add" : "Blacklist remove")
    }
}
