import SwiftUI

struct AuctionsView: View {
    @EnvironmentObject var store: AppStore
    @State private var selected: String = ""
    @State private var auctions: AuctionsResponse?
    @State private var loading = false

    private var accounts: [String] { store.accounts?.accounts.map(\.ign) ?? store.status?.configured ?? [] }
    private var currentIgn: String? { selected.isEmpty ? accounts.first : selected }

    private var entries: [AuctionEntry] { auctions?.entries ?? [] }
    private var active: [AuctionEntry] { entries.filter { $0.status.lowercased() == "active" } }
    private var sold: [AuctionEntry] { entries.filter { $0.status.lowercased() == "sold" } }
    private var expired: [AuctionEntry] { entries.filter { $0.status.lowercased() == "expired" } }

    private var freshness: String {
        guard let ms = auctions?.observedAtMs else { return "Not scanned yet — open Manage Auctions" }
        return "Last scanned \(Fmt.relative(fromMs: UInt64(max(0, ms))))"
    }

    var body: some View {
        Page {
            PageHeader("Auctions", subtitle: "Listings on the bot's auction house") {
                GhostButton(title: "Refresh", systemImage: "arrow.clockwise") { Task { await load() } }
            }

            if accounts.isEmpty {
                EmptyState(icon: "tag", text: "No accounts.").frame(height: 220)
            } else {
                accountPicker

                HStack(spacing: 7) {
                    Image(systemName: auctions?.observedAtMs == nil ? "eye.slash" : "clock")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Theme.textTertiary)
                    Text(freshness).font(.rounded(11.5, .medium)).foregroundStyle(Theme.textTertiary)
                }

                Hairline()

                if loading && auctions == nil {
                    EmptyState(icon: "hourglass", text: "Loading…").frame(height: 200)
                } else {
                    section(title: "Active", icon: "tag.fill", accent: Theme.accent,
                            entries: active, emptyIcon: "tag", emptyText: "No active listings.") { entry in
                        AuctionRow(entry: entry, accent: Theme.accent, showsEndsIn: true)
                    }
                    section(title: "Sold — to collect", icon: "checkmark.seal.fill", accent: Theme.profit,
                            entries: sold, emptyIcon: "checkmark.seal", emptyText: "Nothing sold to collect.") { entry in
                        AuctionRow(entry: entry, accent: Theme.profit, showsBuyer: true)
                    }
                    section(title: "Expired — to collect", icon: "clock.badge.xmark.fill", accent: Theme.loss,
                            entries: expired, emptyIcon: "clock.badge.xmark", emptyText: "Nothing expired to collect.") { entry in
                        AuctionRow(entry: entry, accent: Theme.loss)
                    }
                }
            }
        }
        .task(id: store.accounts?.accounts.count) {
            if selected.isEmpty { selected = accounts.first ?? "" }
            await load()
        }
    }

    private var accountPicker: some View {
        FlowLayout(spacing: 8) {
            ForEach(accounts, id: \.self) { ign in
                Button { selected = ign; Task { await load() } } label: {
                    Text(ign)
                        .font(.rounded(12.5, .semibold))
                        .foregroundStyle(selected == ign ? Color.black.opacity(0.85) : Theme.textSecondary)
                        .padding(.horizontal, 14).padding(.vertical, 8)
                        .background(Capsule().fill(selected == ign ? AnyShapeStyle(Theme.brandGradientH) : AnyShapeStyle(Color.white.opacity(0.04))))
                        .overlay(Capsule().strokeBorder(Theme.cardStroke, lineWidth: selected == ign ? 0 : 1))
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder
    private func section(
        title: String, icon: String, accent: Color, entries: [AuctionEntry],
        emptyIcon: String, emptyText: String,
        @ViewBuilder row: @escaping (AuctionEntry) -> AuctionRow
    ) -> some View {
        Card {
            VStack(alignment: .leading, spacing: 0) {
                SectionHeader(title, systemImage: icon) {
                    Pill(text: "\(entries.count)", color: accent)
                }
                .padding(.bottom, 12)
                Hairline()
                if entries.isEmpty {
                    EmptyState(icon: emptyIcon, text: emptyText).frame(height: 96)
                } else {
                    ForEach(Array(entries.enumerated()), id: \.element.id) { idx, entry in
                        ListRow(showsSeparator: idx < entries.count - 1, insets: EdgeInsets(top: 11, leading: 6, bottom: 11, trailing: 6)) {
                            row(entry)
                        }
                    }
                }
            }
        }
    }

    private func load() async {
        guard let ign = currentIgn, let api = store.api else { return }
        loading = true
        defer { loading = false }
        auctions = try? await api.auctions(ign: ign)
    }
}

struct AuctionRow: View {
    let entry: AuctionEntry
    var accent: Color = Theme.accent
    var showsEndsIn: Bool = false
    var showsBuyer: Bool = false

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(entry.name ?? "Unknown item").font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
                if let auctionId = entry.auctionId, !auctionId.isEmpty {
                    Text(auctionId).font(.system(size: 10.5, weight: .medium, design: .monospaced)).foregroundStyle(Theme.textTertiary).lineLimit(1)
                }
                if showsBuyer, let buyer = entry.buyer, !buyer.isEmpty {
                    Text("to \(buyer)").font(.rounded(11, .medium)).foregroundStyle(Theme.textSecondary).lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if showsEndsIn, let endsIn = entry.endsIn, !endsIn.isEmpty {
                Pill(text: endsIn, color: accent)
            }
            if let price = entry.price {
                Text(Fmt.coins(price)).font(.numeric(12.5, .bold)).foregroundStyle(Theme.gold)
            }
        }
    }
}
