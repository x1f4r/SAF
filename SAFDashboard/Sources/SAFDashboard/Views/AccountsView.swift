import SwiftUI

enum AccountFilter: String, CaseIterable { case connected = "Connected", all = "All" }

struct AccountsView: View {
    @EnvironmentObject var store: AppStore
    @State private var filter: AccountFilter = .connected

    var body: some View {
        let all = store.accounts?.accounts ?? []
        let connected = all.filter { !$0.isOffline }
        let shown = filter == .connected ? connected : all
        let hidden = all.count - connected.count

        return Page {
            PageHeader("Accounts",
                subtitle: "\(store.connectedCount) connected · \(store.readyCount) ready of \(store.configuredCount) configured") {
                SegmentedPicker(selection: $filter, options: AccountFilter.allCases, label: \.rawValue)
                    .frame(width: 220)
            }

            if shown.isEmpty {
                EmptyState(icon: "person.2",
                    text: all.isEmpty ? "No accounts reported by the bot." : "No accounts are connected right now. Check Diagnostics for why.")
                    .frame(height: 260)
            } else {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 420), spacing: 20)], spacing: 20) {
                    ForEach(shown) { account in AccountCard(account: account) }
                }
            }

            if filter == .connected && hidden > 0 {
                Button("\(hidden) not connected — show all") { filter = .all }
                    .buttonStyle(.plain).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.accent)
            }
        }
    }
}

func accountStatusBadge(_ a: AccountInfo) -> (text: String, color: Color, filled: Bool) {
    switch a.effectiveStatus {
    case "offline": return ("Offline", Theme.textTertiary, false)
    case "connecting": return ("Connecting", Theme.warn, false)
    default:
        if a.ready == true { return ("Ready", Theme.profit, true) }
        return (a.reason ?? "Not ready", Theme.warn, false)
    }
}

struct AccountCard: View {
    @EnvironmentObject var store: AppStore
    let account: AccountInfo
    private var s: AccountStats { account.stats }

    var body: some View {
        Surface(padding: 20) {
            VStack(alignment: .leading, spacing: 18) {
                let badge = accountStatusBadge(account)
                HStack(spacing: 13) {
                    AccountAvatar(url: account.headUrl, size: 44)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(account.ign).font(.rounded(17, .bold)).foregroundStyle(Theme.textPrimary)
                        HStack(spacing: 6) {
                            Pill(text: badge.text, color: badge.color, filled: badge.filled)
                            if let tier = s.coflTier { Pill(text: tier, color: Theme.gold) }
                        }
                    }
                    Spacer()
                    StatusDot(color: account.ready == true ? Theme.profit : (account.isOffline ? Theme.textTertiary : Theme.warn),
                        pulse: account.ready == true)
                }

                // Why this account can't flip.
                if account.ready != true, !account.isOffline, let reason = account.reason {
                    HStack(spacing: 8) {
                        Circle().fill(Theme.warn).frame(width: 7, height: 7)
                        Text(reason).font(.rounded(12, .semibold)).foregroundStyle(Theme.warn)
                        Spacer()
                    }
                    .padding(.horizontal, 11).padding(.vertical, 9)
                    .background(RoundedRectangle(cornerRadius: 10).fill(Theme.warn.opacity(0.09)))
                    .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.warn.opacity(0.25), lineWidth: 1))
                }

                HStack(alignment: .top, spacing: 0) {
                    Metric(label: "Profit", value: Fmt.coins(s.totalProfit), color: Theme.profit)
                    VRule(height: 30)
                    Metric(label: "Profit/hr", value: Fmt.coins(s.profitPerHour ?? 0), color: Theme.accent)
                    VRule(height: 30)
                    Metric(label: "Purse", value: s.purse.map(Fmt.coins) ?? "—", color: Theme.gold)
                    VRule(height: 30)
                    Metric(label: "Bought", value: Fmt.int(s.bought))
                    VRule(height: 30)
                    Metric(label: "Sold", value: Fmt.int(s.sold))
                }

                HStack(spacing: 16) {
                    MetaChip(icon: "antenna.radiowaves.left.and.right",
                        text: account.coflConnected == true ? "SkyCofl connected" : "SkyCofl offline")
                    MetaChip(icon: "checkmark.seal.fill", text: account.hasCookie == true ? "Cookie active" : "No cookie")
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
        .opacity(account.isOffline ? 0.62 : 1)
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
