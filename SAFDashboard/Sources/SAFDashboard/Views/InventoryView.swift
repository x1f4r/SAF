import SwiftUI

struct InventoryView: View {
    @EnvironmentObject var store: AppStore
    @State private var selected: String = ""
    @State private var inventory: InventoryResponse?
    @State private var loading = false

    private var accounts: [String] { store.accounts?.accounts.map(\.ign) ?? store.status?.configured ?? [] }
    private var currentIgn: String? { selected.isEmpty ? accounts.first : selected }

    private var items: [InventoryItem] {
        (inventory?.items ?? []).sorted { ($0.slot ?? Int.max) < ($1.slot ?? Int.max) }
    }

    var body: some View {
        Page {
            PageHeader("Inventory", subtitle: "Items held by the bot account") {
                GhostButton(title: "Refresh", systemImage: "arrow.clockwise") { Task { await load() } }
            }

            if accounts.isEmpty {
                EmptyState(icon: "shippingbox", text: "No accounts.").frame(height: 220)
            } else {
                accountPicker
                Hairline()
                inventoryBody
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

    @ViewBuilder private var inventoryBody: some View {
        if loading && inventory == nil {
            EmptyState(icon: "hourglass", text: "Loading…").frame(height: 200)
        } else if !items.isEmpty {
            Card {
                VStack(alignment: .leading, spacing: 0) {
                    SectionHeader("Items", systemImage: "shippingbox.fill") {
                        Text("\(items.count)").font(.numeric(12, .bold)).foregroundStyle(Theme.textSecondary)
                    }
                    .padding(.bottom, 12)
                    Hairline()
                    ForEach(Array(items.enumerated()), id: \.element.id) { idx, item in
                        ListRow(showsSeparator: idx < items.count - 1, insets: EdgeInsets(top: 11, leading: 6, bottom: 11, trailing: 6)) {
                            InventoryRow(item: item)
                        }
                    }
                }
            }
        } else {
            EmptyState(icon: "shippingbox", text: "Inventory is empty.").frame(height: 200)
        }
    }

    private func load() async {
        guard let ign = currentIgn, let api = store.api else { return }
        loading = true
        defer { loading = false }
        inventory = try? await api.inventory(ign: ign)
    }
}

struct InventoryRow: View {
    let item: InventoryItem
    var body: some View {
        HStack(spacing: 12) {
            ItemIcon(tag: item.tag, size: 34)
            VStack(alignment: .leading, spacing: 3) {
                Text(item.itemName).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
                if let tag = item.tag, !tag.isEmpty {
                    Text(tag).font(.system(size: 10.5, weight: .medium, design: .monospaced)).foregroundStyle(Theme.textTertiary).lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let price = item.price {
                Text(Fmt.coins(price)).font(.numeric(12.5, .bold)).foregroundStyle(Theme.gold)
            }
            if let slot = item.slot {
                Text("slot \(slot)").font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary).frame(width: 56, alignment: .trailing)
            }
            if item.inHotbar {
                Pill(text: "Hotbar", color: Theme.accent)
            }
        }
    }
}
