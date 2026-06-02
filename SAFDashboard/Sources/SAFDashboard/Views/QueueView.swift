import SwiftUI

struct QueueView: View {
    @EnvironmentObject var store: AppStore
    @State private var selected: String = ""
    @State private var queue: QueueResponse?
    @State private var loading = false

    private var accounts: [String] { store.accounts?.accounts.map(\.ign) ?? store.status?.configured ?? [] }
    private var currentIgn: String? { selected.isEmpty ? accounts.first : selected }

    var body: some View {
        Page {
            PageHeader("Queue", subtitle: "Pending market actions per account") {
                HStack(spacing: 10) {
                    GhostButton(title: "Refresh", systemImage: "arrow.clockwise") { Task { await load() } }
                    if let ign = currentIgn {
                        GhostButton(title: "Clear", systemImage: "trash", role: .destructive) {
                            store.runCommand("clear_queue", options: ["username": .string(ign)], label: "Clear queue")
                            Task { try? await Task.sleep(nanoseconds: 600_000_000); await load() }
                        }
                    }
                }
            }

            if accounts.isEmpty {
                EmptyState(icon: "list.bullet", text: "No accounts.").frame(height: 220)
            } else {
                accountPicker
                Hairline()
                queueTable
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

    @ViewBuilder private var queueTable: some View {
        if loading && queue == nil {
            EmptyState(icon: "hourglass", text: "Loading…").frame(height: 200)
        } else if let entries = queue?.queue, !entries.isEmpty {
            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    Text("STATE").font(.rounded(9.5, .semibold)).tracking(0.7).foregroundStyle(Theme.textTertiary).frame(width: 140, alignment: .leading)
                    Text("DETAIL").font(.rounded(9.5, .semibold)).tracking(0.7).foregroundStyle(Theme.textTertiary).frame(maxWidth: .infinity, alignment: .leading)
                    Text("PRIORITY").font(.rounded(9.5, .semibold)).tracking(0.7).foregroundStyle(Theme.textTertiary).frame(width: 70, alignment: .trailing)
                }.padding(.horizontal, 6).padding(.bottom, 8)
                Hairline()
                ForEach(Array(entries.enumerated()), id: \.element.id) { idx, entry in
                    ListRow(showsSeparator: idx < entries.count - 1, insets: EdgeInsets(top: 11, leading: 6, bottom: 11, trailing: 6)) {
                        QueueRow(entry: entry)
                    }
                }
            }
        } else {
            EmptyState(icon: "checkmark.circle", text: "Queue is empty.").frame(height: 200)
        }
    }

    private func load() async {
        guard let ign = currentIgn, let api = store.api else { return }
        loading = true
        defer { loading = false }
        queue = try? await api.queue(ign: ign)
    }
}

struct QueueRow: View {
    let entry: QueueEntry
    private var stateColor: Color {
        switch entry.state.lowercased() {
        case "buying": return Theme.profit
        case "listing", "listingnoname": return Theme.gold
        case "delisting": return Theme.loss
        case "claiming", "claimsold": return Theme.accent
        default: return Theme.accent2
        }
    }
    var body: some View {
        HStack(spacing: 12) {
            Pill(text: entry.state, color: stateColor).frame(width: 140, alignment: .leading)
            VStack(alignment: .leading, spacing: 2) {
                if let item = entry.itemName {
                    Text(item).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
                }
                if let auction = entry.auctionId {
                    Text(auction).font(.system(size: 10.5, weight: .medium, design: .monospaced)).foregroundStyle(Theme.textTertiary).lineLimit(1)
                }
                if let price = entry.price {
                    Text(Fmt.coins(price)).font(.numeric(11, .medium)).foregroundStyle(Theme.gold)
                }
                if entry.itemName == nil && entry.auctionId == nil {
                    Text("—").foregroundStyle(Theme.textTertiary)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
            Text("\(entry.priority)").font(.numeric(13, .bold)).foregroundStyle(Theme.textSecondary).frame(width: 70, alignment: .trailing)
        }
    }
}
