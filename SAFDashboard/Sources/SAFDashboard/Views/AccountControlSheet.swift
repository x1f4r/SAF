import SwiftUI

/// Per-account control panel presented as a sheet. Groups the full lifecycle,
/// market, bank, and transfer actions for a single account into one flat,
/// hairline-separated surface. Every action routes through the store and
/// surfaces its result via the global toast; the sheet stays open.
struct AccountControlSheet: View {
    let account: AccountInfo
    @EnvironmentObject var store: AppStore
    @Environment(\.dismiss) private var dismiss

    @State private var bankAmount = "all"
    @State private var bankPersonal = false
    @State private var transferDest = ""
    @State private var transferAmount = "all"
    @State private var keepSourceRunning = false

    private var ign: String { account.ign }

    /// Other configured accounts (by IGN), excluding this one.
    private var otherAccounts: [String] {
        let all = store.accounts?.accounts.map(\.ign) ?? store.accounts?.configured ?? []
        return all.filter { $0 != ign }
    }

    private var withdrawAllBlocked: Bool {
        bankAmount.trimmingCharacters(in: .whitespaces).lowercased() == "all"
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                header
                Hairline()
                lifecycle
                Hairline()
                market
                Hairline()
                bank
                if otherAccounts.count >= 1, (store.accounts?.accounts.count ?? store.accounts?.configured.count ?? 0) >= 2 {
                    Hairline()
                    transfer
                }
                Hairline()
                HStack {
                    Spacer()
                    GhostButton(title: "Done", systemImage: "checkmark") { dismiss() }
                }
            }
            .padding(24)
        }
        .frame(width: 480)
        .frame(minHeight: 460)
        .background(Theme.appBackground)
        .onAppear {
            if transferDest.isEmpty { transferDest = otherAccounts.first ?? "" }
        }
    }

    // MARK: 1. Header

    private var header: some View {
        let badge = accountStatusBadge(account)
        return VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 13) {
                AccountAvatar(url: account.headUrl, size: 46)
                VStack(alignment: .leading, spacing: 5) {
                    Text(ign).font(.rounded(19, .bold)).foregroundStyle(Theme.textPrimary)
                    HStack(spacing: 6) {
                        Pill(text: badge.text, color: badge.color, filled: badge.filled)
                        if let tier = account.stats.coflTier { Pill(text: tier, color: Theme.gold) }
                    }
                }
                Spacer()
                StatusDot(color: account.ready == true ? Theme.profit : (account.isOffline ? Theme.textTertiary : Theme.warn),
                    pulse: account.ready == true)
            }
            HStack(spacing: 6) {
                Image(systemName: "checkmark.seal.fill")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(account.hasCookie == true ? Theme.profit : Theme.warn)
                Text("Booster cookie: ")
                    .font(.rounded(12, .medium)).foregroundStyle(Theme.textSecondary)
                + Text(account.hasCookie == true ? "active" : "none")
                    .font(.rounded(12, .semibold))
                    .foregroundColor(account.hasCookie == true ? Theme.profit : Theme.warn)
            }
        }
    }

    // MARK: 2. Lifecycle

    private var lifecycle: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel("Lifecycle", systemImage: "power")

            FlowLayout(spacing: 8) {
                if account.running {
                    ActionChip(title: "Stop", icon: "pause.fill", tint: Theme.warn) {
                        store.runCommand("stop_bot", options: ["username": .string(ign)], label: "Stop \(ign)")
                    }
                } else {
                    ActionChip(title: "Start", icon: "play.fill", tint: Theme.profit) {
                        store.runCommand("start_bot", options: ["username": .string(ign)], label: "Start \(ign)")
                    }
                }
                ActionChip(title: "Restart", icon: "arrow.clockwise") {
                    store.runCommand("stop_bot", options: ["username": .string(ign)], label: "Stop \(ign)")
                    store.runCommand("start_bot", options: ["username": .string(ign)], label: "Start \(ign)")
                }
            }

            chipRow(title: "Stop after", icon: "timer") {
                ForEach(["10m", "30m", "1h", "3h"], id: \.self) { d in
                    ActionChip(title: d, icon: "timer", tint: Theme.warn) {
                        store.runCommand("timeout", options: ["duration": .string(d), "username": .string(ign)],
                            label: "Stop \(ign) in \(d)")
                    }
                }
            }

            chipRow(title: "Start in", icon: "hourglass") {
                ForEach(["10m", "30m", "1h"], id: \.self) { d in
                    ActionChip(title: d, icon: "hourglass", tint: Theme.accent) {
                        store.runCommand("start_in", options: ["duration": .string(d), "username": .string(ign)],
                            label: "Start \(ign) in \(d)")
                    }
                }
            }

            chipRow(title: "Switch region", icon: "globe") {
                ForEach(["US", "EU"], id: \.self) { region in
                    ActionChip(title: region, icon: "globe") {
                        store.runCommand("cofl", options: ["command": .string("switchregion \(region)"), "username": .string(ign)],
                            label: "Switch \(ign) → \(region)")
                    }
                }
            }
        }
    }

    // MARK: 3. Market

    private var market: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel("Market", systemImage: "tag.fill")
            FlowLayout(spacing: 8) {
                ActionChip(title: "Reconcile", icon: "arrow.triangle.2.circlepath") {
                    store.runButton("saf:reconcile:\(ign)", title: "Reconcile")
                }
                ActionChip(title: "Claim Sold", icon: "tray.and.arrow.down") {
                    store.runCommand("claim_sold", options: ["username": .string(ign)], label: "Claim sold")
                }
                ActionChip(title: "Collect Bids", icon: "hand.raised.fill") {
                    store.runButton("saf:bids:\(ign)", title: "Bids")
                }
                ActionChip(title: "Sell Inventory", icon: "shippingbox.fill") {
                    store.runButton("saf:confirmSellInventory:\(ign):0", title: "Queue listings")
                }
                ActionChip(title: "Delist All", icon: "trash.fill", tint: Theme.loss) {
                    store.runButton("saf:confirmDelistAll:\(ign)", title: "Delist all")
                }
                ActionChip(title: "Clear Queue", icon: "xmark.bin.fill", tint: Theme.loss) {
                    store.runCommand("clear_queue", options: ["username": .string(ign)], label: "Clear queue")
                }
                ActionChip(title: "Clear Data", icon: "eraser.fill", tint: Theme.loss) {
                    store.runButton("saf:confirmClearData:\(ign)", title: "Clear data")
                }
            }
        }
    }

    // MARK: 4. Bank

    private var bank: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel("Bank", systemImage: "banknote.fill")

            HStack(alignment: .bottom, spacing: 12) {
                FormField(label: "Amount", systemImage: "number") {
                    TextField("all", text: $bankAmount)
                }
                Toggle(isOn: $bankPersonal) {
                    Text("Personal").font(.rounded(12, .semibold)).foregroundStyle(Theme.textSecondary)
                }
                .toggleStyle(.switch).tint(Theme.accent).fixedSize()
                .padding(.bottom, 4)
            }

            HStack(spacing: 10) {
                PrimaryButton(title: "Deposit", systemImage: "arrow.down.to.line") {
                    store.runCommand("bank", options: bankOptions(withdraw: false), label: "Deposit \(displayAmount)")
                }
                PrimaryButton(title: "Withdraw", systemImage: "arrow.up.from.line", tint: Theme.brandGradientH) {
                    store.runCommand("bank", options: bankOptions(withdraw: true), label: "Withdraw \(displayAmount)")
                }
                .disabled(withdrawAllBlocked)
                .opacity(withdrawAllBlocked ? 0.4 : 1)
                Spacer()
            }

            if withdrawAllBlocked {
                Text("Withdraw needs a specific amount — \u{201C}all\u{201D} isn’t accepted.")
                    .font(.rounded(10.5, .medium)).foregroundStyle(Theme.warn)
            }
        }
    }

    private var displayAmount: String {
        bankAmount.trimmingCharacters(in: .whitespaces).isEmpty ? "all" : bankAmount
    }

    private func bankOptions(withdraw: Bool) -> [String: JSONValue] {
        var opts: [String: JSONValue] = [
            "username": .string(ign),
            "amount": .string(displayAmount),
        ]
        if withdraw { opts["withdraw"] = .bool(true) }
        if bankPersonal { opts["personal"] = .bool(true) }
        return opts
    }

    // MARK: 5. Transfer

    private var transfer: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel("Transfer", systemImage: "arrow.left.arrow.right")

            HStack(alignment: .bottom, spacing: 12) {
                FormField(label: "To account", systemImage: "person.crop.circle") {
                    Picker("", selection: $transferDest) {
                        ForEach(otherAccounts, id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden()
                    .pickerStyle(.menu)
                    .tint(Theme.accent)
                }
                FormField(label: "Amount", systemImage: "number") {
                    TextField("all", text: $transferAmount)
                }
            }

            HStack(spacing: 12) {
                Toggle(isOn: $keepSourceRunning) {
                    Text("Keep source running").font(.rounded(12, .semibold)).foregroundStyle(Theme.textSecondary)
                }
                .toggleStyle(.switch).tint(Theme.accent).fixedSize()
                Spacer()
                PrimaryButton(title: "Transfer", systemImage: "arrow.left.arrow.right") {
                    let amount = transferAmount.trimmingCharacters(in: .whitespaces).isEmpty ? "all" : transferAmount
                    store.runTransfer(from: ign, to: transferDest, amount: amount, stopSource: !keepSourceRunning)
                }
                .disabled(transferDest.isEmpty)
                .opacity(transferDest.isEmpty ? 0.4 : 1)
            }
        }
    }

    // MARK: Helpers

    @ViewBuilder
    private func chipRow<Chips: View>(title: String, icon: String, @ViewBuilder chips: () -> Chips) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: icon).font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.textTertiary)
                Text(title).font(.rounded(11.5, .semibold)).foregroundStyle(Theme.textSecondary)
            }
            FlowLayout(spacing: 8) { chips() }
        }
    }
}
