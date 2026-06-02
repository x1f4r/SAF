import SwiftUI

struct FlipsView: View {
    @EnvironmentObject var store: AppStore
    @State private var mode: Mode = .bought
    @State private var query = ""

    enum Mode: String, CaseIterable { case bought = "Bought", sold = "Sold" }

    private var filteredBought: [FlipRecord] {
        guard !query.isEmpty else { return store.bought }
        return store.bought.filter { $0.item.localizedCaseInsensitiveContains(query) || $0.finder.localizedCaseInsensitiveContains(query) }
    }
    private var filteredSold: [SaleRecord] {
        guard !query.isEmpty else { return store.sold }
        return store.sold.filter { $0.item.localizedCaseInsensitiveContains(query) || $0.buyer.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        Page {
            PageHeader("Flips", subtitle: mode == .bought ? "\(store.bought.count) tracked purchases" : "\(store.sold.count) tracked sales") {
                SearchField(text: $query)
            }

            HStack(spacing: 18) {
                SegmentedPicker(selection: $mode, options: Mode.allCases, label: \.rawValue).frame(width: 240)
                Spacer()
                if mode == .bought { summaryStrip(bought: filteredBought).frame(maxWidth: 420) }
            }

            Hairline()

            if mode == .bought {
                boughtTable
            } else {
                soldTable
            }
        }
    }

    private func summaryStrip(bought: [FlipRecord]) -> some View {
        let total = bought.reduce(0.0) { $0 + $1.profit }
        let best = bought.max(by: { $0.profit < $1.profit })
        return HStack(alignment: .top, spacing: 0) {
            Metric(label: "Shown Profit", value: Fmt.coins(total), color: Theme.profit)
            VRule(height: 34)
            Metric(label: "Count", value: Fmt.int(bought.count))
            VRule(height: 34)
            Metric(label: "Best", value: Fmt.signedCoins(best?.profit ?? 0), color: Theme.gold)
        }
    }

    // Column widths (0 = flexible). Shared by header + rows so columns align.
    private static let boughtCols: [(String, CGFloat)] =
        [("Item", 0), ("Finder", 86), ("Bought", 92), ("Target", 92), ("Profit", 100), ("When", 84)]
    private static let soldCols: [(String, CGFloat)] =
        [("Item", 0), ("Buyer", 160), ("Price", 120), ("When", 84)]

    @ViewBuilder private var boughtTable: some View {
        VStack(spacing: 0) {
            FlipsTableHeader(columns: Self.boughtCols)
            Hairline()
            if filteredBought.isEmpty {
                EmptyState(icon: "cart", text: "No purchases match.").frame(height: 180)
            } else {
                ForEach(Array(filteredBought.enumerated()), id: \.element.id) { idx, flip in
                    ListRow(showsSeparator: idx < filteredBought.count - 1, insets: EdgeInsets(top: 9, leading: 6, bottom: 9, trailing: 6)) {
                        BoughtFlipRow(flip: flip, widths: Self.boughtCols.map(\.1))
                    }
                }
            }
        }
    }

    @ViewBuilder private var soldTable: some View {
        VStack(spacing: 0) {
            FlipsTableHeader(columns: Self.soldCols)
            Hairline()
            if filteredSold.isEmpty {
                EmptyState(icon: "checkmark.seal", text: "No sales match.").frame(height: 180)
            } else {
                ForEach(Array(filteredSold.enumerated()), id: \.element.id) { idx, sale in
                    ListRow(showsSeparator: idx < filteredSold.count - 1, insets: EdgeInsets(top: 10, leading: 6, bottom: 10, trailing: 6)) {
                        SoldFlipRow(sale: sale, widths: Self.soldCols.map(\.1))
                    }
                }
            }
        }
    }
}

/// Constrains a table cell to a fixed width, or flexes when width == 0.
struct Col: ViewModifier {
    let width: CGFloat
    var trailing: Bool = false
    func body(content: Content) -> some View {
        if width == 0 { content.frame(maxWidth: .infinity, alignment: .leading) }
        else { content.frame(width: width, alignment: trailing ? .trailing : .leading) }
    }
}

struct FlipsTableHeader: View {
    let columns: [(String, CGFloat)]
    var body: some View {
        HStack(spacing: 12) {
            ForEach(Array(columns.enumerated()), id: \.offset) { idx, col in
                Text(col.0.uppercased())
                    .font(.rounded(9.5, .semibold)).tracking(0.7).foregroundStyle(Theme.textTertiary)
                    .modifier(Col(width: col.1, trailing: idx == columns.count - 1))
            }
        }
        .padding(.horizontal, 6).padding(.bottom, 8)
    }
}

struct BoughtFlipRow: View {
    let flip: FlipRecord
    let widths: [CGFloat]
    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 11) {
                ItemIcon(tag: flip.tag, size: 30)
                Text(flip.item).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
            }.modifier(Col(width: widths[0]))
            Text(prettyFinder(flip.finder)).font(.rounded(11.5, .medium)).foregroundStyle(Theme.accent2).modifier(Col(width: widths[1]))
            Text(Fmt.coins(Double(flip.price))).font(.numeric(12, .medium)).foregroundStyle(Theme.textSecondary).modifier(Col(width: widths[2]))
            Text(Fmt.coins(flip.targetPrice)).font(.numeric(12, .medium)).foregroundStyle(Theme.textSecondary).modifier(Col(width: widths[3]))
            Text(Fmt.signedCoins(flip.profit)).font(.numeric(12.5, .bold)).foregroundStyle(Theme.pnl(flip.profit)).modifier(Col(width: widths[4]))
            Text(Fmt.relative(fromMs: flip.ts)).font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary).modifier(Col(width: widths[5], trailing: true))
        }
    }
}

struct SoldFlipRow: View {
    let sale: SaleRecord
    let widths: [CGFloat]
    var body: some View {
        HStack(spacing: 12) {
            Text(sale.item).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1).modifier(Col(width: widths[0]))
            Text(sale.buyer).font(.rounded(12, .medium)).foregroundStyle(Theme.textSecondary).lineLimit(1).modifier(Col(width: widths[1]))
            Text(Fmt.coins(Double(sale.price))).font(.numeric(12.5, .bold)).foregroundStyle(Theme.gold).modifier(Col(width: widths[2]))
            Text(Fmt.relative(fromMs: sale.ts)).font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary).modifier(Col(width: widths[3], trailing: true))
        }
    }
}

struct SearchField: View {
    @Binding var text: String
    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass").font(.system(size: 12, weight: .semibold)).foregroundStyle(Theme.textTertiary)
            TextField("Search…", text: $text)
                .textFieldStyle(.plain).font(.rounded(12.5, .medium)).foregroundStyle(Theme.textPrimary).frame(width: 170)
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .background(Capsule().fill(Color.white.opacity(0.04)))
        .overlay(Capsule().strokeBorder(Theme.cardStroke, lineWidth: 1))
    }
}

struct SegmentedPicker<T: Hashable>: View {
    @Binding var selection: T
    let options: [T]
    let label: (T) -> String
    var body: some View {
        HStack(spacing: 4) {
            ForEach(options, id: \.self) { option in
                let selected = option == selection
                Button { withAnimation(.easeOut(duration: 0.15)) { selection = option } } label: {
                    Text(label(option))
                        .font(.rounded(12.5, .semibold))
                        .foregroundStyle(selected ? Color.black.opacity(0.85) : Theme.textSecondary)
                        .frame(maxWidth: .infinity).padding(.vertical, 7)
                        .background(RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(selected ? AnyShapeStyle(Theme.brandGradientH) : AnyShapeStyle(Color.clear)))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .background(RoundedRectangle(cornerRadius: 11).fill(Color.white.opacity(0.04)))
        .overlay(RoundedRectangle(cornerRadius: 11).strokeBorder(Theme.cardStroke, lineWidth: 1))
    }
}
