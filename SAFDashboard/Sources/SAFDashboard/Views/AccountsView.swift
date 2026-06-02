import SwiftUI

struct AccountsView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        Page {
            PageHeader("Accounts", subtitle: "\(store.runningCount) running of \(store.configuredCount) configured")

            let accounts = store.accounts?.accounts ?? []
            if accounts.isEmpty {
                EmptyState(icon: "person.2", text: "No accounts reported by the bot.").frame(height: 280)
            } else {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 420), spacing: 20)], spacing: 20) {
                    ForEach(accounts) { account in AccountCard(account: account) }
                }
            }
        }
    }
}

struct AccountCard: View {
    @EnvironmentObject var store: AppStore
    let account: AccountInfo
    private var s: AccountStats { account.stats }

    var body: some View {
        Surface(padding: 20) {
            VStack(alignment: .leading, spacing: 18) {
                HStack(spacing: 13) {
                    AccountAvatar(url: account.headUrl, size: 44)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(account.ign).font(.rounded(17, .bold)).foregroundStyle(Theme.textPrimary)
                        HStack(spacing: 6) {
                            Pill(text: account.running ? "Running" : "Idle",
                                color: account.running ? Theme.profit : Theme.textTertiary, filled: account.running)
                            if let tier = s.coflTier { Pill(text: tier, color: Theme.gold) }
                        }
                    }
                    Spacer()
                    StatusDot(color: account.running ? Theme.profit : Theme.textTertiary, pulse: account.running)
                }

                HStack(alignment: .top, spacing: 0) {
                    Metric(label: "Profit", value: Fmt.coins(s.totalProfit), color: Theme.profit)
                    VRule(height: 30)
                    Metric(label: "Profit/hr", value: Fmt.coins(s.profitPerHour ?? 0), color: Theme.accent)
                    VRule(height: 30)
                    Metric(label: "Purse", value: Fmt.coins(s.purse ?? 0), color: Theme.gold)
                    VRule(height: 30)
                    Metric(label: "Bought", value: Fmt.int(s.bought))
                    VRule(height: 30)
                    Metric(label: "Sold", value: Fmt.int(s.sold))
                }

                HStack(spacing: 16) {
                    MetaChip(icon: "antenna.radiowaves.left.and.right", text: s.coflPingMs.map { "\($0)ms cofl" } ?? "—")
                    MetaChip(icon: "wifi", text: s.hypixelPingMs.map { "\($0)ms mc" } ?? "—")
                    if let used = s.auctionSlotsUsed, let max = s.auctionSlotsMax {
                        MetaChip(icon: "tag.fill", text: "\(used)/\(max) slots")
                    }
                    MetaChip(icon: "list.bullet", text: "\(account.queueSize) queued")
                }

                Hairline()

                FlowLayout(spacing: 8) {
                    ActionChip(title: "Reconcile", icon: "arrow.triangle.2.circlepath") {
                        store.runCommand("reconcile", options: ["username": .string(account.ign)], label: "Reconcile \(account.ign)")
                    }
                    ActionChip(title: "Claim Sold", icon: "tray.and.arrow.down") {
                        store.runCommand("claim_sold", options: ["username": .string(account.ign)], label: "Claim sold")
                    }
                    ActionChip(title: "Bids", icon: "hand.raised.fill") {
                        store.runCommand("bids", options: ["username": .string(account.ign)], label: "Collect bids")
                    }
                    ActionChip(title: "Sell Inv", icon: "shippingbox.fill") {
                        store.runButton("saf:confirmSellInventory:\(account.ign):0", title: "Queue inventory listings")
                    }
                    ActionChip(title: "Delist All", icon: "trash.fill", tint: Theme.loss) {
                        store.runButton("saf:confirmDelistAll:\(account.ign)", title: "Delist everything")
                    }
                    if account.running {
                        ActionChip(title: "Stop", icon: "pause.fill", tint: Theme.warn) {
                            store.runCommand("stop_bot", options: ["username": .string(account.ign)], label: "Stop \(account.ign)")
                        }
                    } else {
                        ActionChip(title: "Start", icon: "play.fill", tint: Theme.profit) {
                            store.runCommand("start_bot", options: ["username": .string(account.ign)], label: "Start \(account.ign)")
                        }
                    }
                }
            }
        }
    }
}

struct MetaChip: View {
    let icon: String
    let text: String
    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: icon).font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.textTertiary)
            Text(text).font(.rounded(11, .medium)).foregroundStyle(Theme.textSecondary)
        }
    }
}

struct ActionChip: View {
    let title: String
    let icon: String
    var tint: Color = Theme.accent
    var action: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: icon).font(.system(size: 11, weight: .semibold))
                Text(title).font(.rounded(12, .medium))
            }
            .foregroundStyle(hover ? tint : Theme.textSecondary)
            .padding(.horizontal, 11).padding(.vertical, 7)
            .background(Capsule().fill(hover ? tint.opacity(0.14) : Color.white.opacity(0.04)))
            .overlay(Capsule().strokeBorder(hover ? tint.opacity(0.4) : Theme.cardStroke, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .animation(.easeOut(duration: 0.12), value: hover)
    }
}

struct FlowLayout: Layout {
    var spacing: CGFloat = 8
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? 600
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x + size.width > maxWidth { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
        return CGSize(width: maxWidth, height: y + rowHeight)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, rowHeight: CGFloat = 0
        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX { x = bounds.minX; y += rowHeight + spacing; rowHeight = 0 }
            view.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
    }
}
