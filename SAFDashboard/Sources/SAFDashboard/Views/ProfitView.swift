import Charts
import SwiftUI

struct ProfitView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        Page {
            PageHeader("Profit", subtitle: "Performance across all accounts") {
                Text(Fmt.coins(store.profit?.displayProfit ?? 0))
                    .font(.numeric(20, .bold)).foregroundStyle(Theme.profit)
            }

            VStack(alignment: .leading, spacing: 14) {
                SectionLabel("Cumulative Profit", systemImage: "chart.line.uptrend.xyaxis")
                Surface(padding: 18) { ProfitChart(points: store.series?.points ?? []).frame(height: 300) }
            }

            HStack(alignment: .top, spacing: 28) {
                VStack(alignment: .leading, spacing: 14) {
                    SectionLabel("Profit per Window", systemImage: "chart.bar.fill")
                    Surface(padding: 18) { perWindowChart.frame(height: 220) }
                }
                .frame(maxWidth: .infinity)

                VStack(alignment: .leading, spacing: 14) {
                    SectionLabel("By Finder", systemImage: "scope")
                    finderBreakdown
                }
                .frame(width: 360)
            }

            VStack(alignment: .leading, spacing: 12) {
                SectionLabel("By Account", systemImage: "person.2.fill")
                accountBreakdown
            }
        }
    }

    @ViewBuilder private var perWindowChart: some View {
        if let points = store.series?.points, !points.isEmpty {
            Chart(points) { point in
                BarMark(
                    x: .value("Time", Date(timeIntervalSince1970: Double(point.ts) / 1000)),
                    y: .value("Profit", point.profit))
                .foregroundStyle(Theme.brandGradient).cornerRadius(4)
            }
            .chartXAxis { AxisMarks(values: .automatic(desiredCount: 5)) { _ in
                AxisGridLine().foregroundStyle(Theme.cardStroke.opacity(0.6))
                AxisValueLabel().font(.rounded(10)).foregroundStyle(Theme.textTertiary)
            } }
            .chartYAxis { AxisMarks { value in
                AxisGridLine().foregroundStyle(Theme.cardStroke.opacity(0.6))
                AxisValueLabel { if let v = value.as(Double.self) { Text(Fmt.coins(v)).font(.rounded(10)).foregroundStyle(Theme.textTertiary) } }
            } }
        } else {
            EmptyState(icon: "chart.bar", text: "No data yet.")
        }
    }

    private var finderBreakdown: some View {
        var totals: [String: Double] = [:]
        for flip in store.bought { totals[prettyFinder(flip.finder), default: 0] += flip.profit }
        let sorted = totals.sorted { $0.value > $1.value }
        let maxVal = sorted.first?.value ?? 1
        return Group {
            if sorted.isEmpty {
                EmptyState(icon: "scope", text: "No flips yet.").frame(height: 200)
            } else {
                VStack(spacing: 13) {
                    ForEach(sorted.prefix(8), id: \.key) { entry in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(entry.key).font(.rounded(12, .semibold)).foregroundStyle(Theme.textPrimary)
                                Spacer()
                                Text(Fmt.coins(entry.value)).font(.numeric(12, .bold)).foregroundStyle(Theme.profit)
                            }
                            GeometryReader { geo in
                                RoundedRectangle(cornerRadius: 3).fill(Theme.brandGradientH)
                                    .frame(width: geo.size.width * CGFloat(max(0.03, entry.value / maxVal)))
                            }
                            .frame(height: 5)
                            .background(RoundedRectangle(cornerRadius: 3).fill(Color.white.opacity(0.05)))
                        }
                    }
                }
            }
        }
    }

    private var accountBreakdown: some View {
        let accounts = store.profit?.accounts ?? []
        return Group {
            if accounts.isEmpty {
                EmptyState(icon: "person.2", text: "No accounts.").frame(height: 100)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(accounts.enumerated()), id: \.element.id) { idx, account in
                        ListRow(showsSeparator: idx < accounts.count - 1) {
                            HStack(spacing: 12) {
                                Text(account.ign).font(.rounded(13, .semibold)).foregroundStyle(Theme.textPrimary).frame(width: 150, alignment: .leading)
                                HStack(alignment: .top, spacing: 0) {
                                    LabeledValue("Profit", Fmt.coins(account.summary.totalProfit), Theme.profit)
                                    LabeledValue("Bought", Fmt.int(account.summary.bought), Theme.textSecondary)
                                    LabeledValue("Sold", Fmt.int(account.summary.sold), Theme.textSecondary)
                                    LabeledValue("Profit/hr", Fmt.coins(account.summary.profitPerHour ?? 0), Theme.accent)
                                }
                                Spacer()
                            }
                        }
                    }
                }
            }
        }
    }
}

struct LabeledValue: View {
    let label: String
    let value: String
    let color: Color
    init(_ label: String, _ value: String, _ color: Color) { self.label = label; self.value = value; self.color = color }
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label.uppercased()).font(.rounded(9, .semibold)).tracking(0.4).foregroundStyle(Theme.textTertiary)
            Text(value).font(.numeric(13, .bold)).foregroundStyle(color)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
