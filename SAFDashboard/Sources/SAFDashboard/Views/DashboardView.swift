import Charts
import SwiftUI

/// Shared page header — a clean title block with an optional trailing control.
struct PageHeader<Trailing: View>: View {
    let title: String
    var subtitle: String?
    var subtitleText: Text?
    @ViewBuilder var trailing: Trailing
    init(_ title: String, subtitle: String? = nil, subtitleText: Text? = nil, @ViewBuilder trailing: () -> Trailing) {
        self.title = title
        self.subtitle = subtitle
        self.subtitleText = subtitleText
        self.trailing = trailing()
    }
    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.rounded(26, .bold)).foregroundStyle(Theme.textPrimary)
                if let subtitleText {
                    subtitleText.font(.rounded(13, .medium)).foregroundStyle(Theme.textSecondary)
                } else if let subtitle {
                    Text(subtitle).font(.rounded(13, .medium)).foregroundStyle(Theme.textSecondary)
                }
            }
            Spacer()
            trailing.alignmentGuide(.firstTextBaseline) { $0[.bottom] }
        }
    }
}

extension PageHeader where Trailing == EmptyView {
    init(_ title: String, subtitle: String? = nil) { self.init(title, subtitle: subtitle) { EmptyView() } }
}

/// Standard scrollable page scaffold: consistent gutters and a capped content
/// width so nothing stretches awkwardly on wide displays.
struct Page<Content: View>: View {
    var spacing: CGFloat = 30
    @ViewBuilder var content: Content
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: spacing) { content }
                .padding(.horizontal, 40)
                .padding(.top, 30)
                .padding(.bottom, 40)
                .frame(maxWidth: 1280, alignment: .leading)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct DashboardView: View {
    @EnvironmentObject var store: AppStore

    private var profit: ProfitSummary? { store.profit }
    private var ppHour: Double {
        (store.accounts?.accounts ?? []).compactMap { $0.stats.profitPerHour }.reduce(0, +)
    }

    var body: some View {
        Page {
            PageHeader("Dashboard", subtitleText: subtitleText) { BotControlButtons() }

            metricStrip
            Hairline()

            chartSection

            HStack(alignment: .top, spacing: 40) {
                recentBuys.frame(maxWidth: .infinity, alignment: .leading)
                rightRail.frame(width: 320)
            }
        }
    }

    private var subtitleText: Text {
        let mode = store.status?.marketMode ?? "—"
        let live = store.status?.marketMode == "live"
        return Text("\(store.connectedCount)/\(store.configuredCount) accounts online   ·   market ")
            + Text(mode).foregroundColor(live ? Theme.warn : Theme.profit).fontWeight(.semibold)
    }

    /// Sum of tracked purchase profit over the last 7 days (the headline number).
    private var weekProfit: Double {
        let cutoff = UInt64(max(0, Date().timeIntervalSince1970 * 1000 - 7 * 86_400_000))
        return store.bought.filter { $0.ts >= cutoff }.reduce(0) { $0 + $1.profit }
    }

    private var metricStrip: some View {
        HStack(alignment: .top, spacing: 0) {
            weeklyHero
            VRule(height: 52)
            Metric(label: "Profit / hr", value: Fmt.coins(ppHour), sub: "this session", color: Theme.accent)
            VRule(height: 52)
            Metric(label: "Bought", value: Fmt.int(profit?.displayBought ?? 0), sub: "\(store.bought.count) tracked")
            VRule(height: 52)
            Metric(label: "Sold", value: Fmt.int(profit?.displaySold ?? 0), sub: "\(store.sold.count) tracked")
            VRule(height: 52)
            Metric(label: "Purse", value: Fmt.coins(profit?.purse ?? 0), sub: "liquid", color: Theme.gold)
            VRule(height: 52)
            Metric(label: "Ready", value: "\(store.readyCount)/\(store.configuredCount)",
                sub: store.readyCount > 0 ? "ready to flip" : "none ready",
                subColor: store.readyCount > 0 ? Theme.profit : Theme.warn)
        }
    }

    private var weeklyHero: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Net Profit · 7 Days".uppercased())
                .font(.rounded(11, .semibold)).tracking(0.8).foregroundStyle(Theme.textTertiary)
            HStack(spacing: 11) {
                Text(Fmt.coins(weekProfit))
                    .font(.numeric(36, .heavy)).foregroundStyle(Theme.profit)
                    .lineLimit(1).minimumScaleFactor(0.5)
                Coin(size: 26)
            }
            .padding(.top, 8)
            Text("lifetime \(Fmt.coins(profit?.displayProfit ?? 0)) · \(Fmt.int(profit?.displayBought ?? 0)) flips")
                .font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary)
                .padding(.top, 5)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var chartSection: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionLabel("Cumulative Profit", systemImage: "chart.line.uptrend.xyaxis") {
                Text(Fmt.signedCoins(store.series?.points.last?.cumulative ?? 0))
                    .font(.numeric(14, .bold)).foregroundStyle(Theme.profit)
            }
            Surface(padding: 18) {
                ProfitChart(points: store.series?.points ?? []).frame(height: 244)
            }
        }
    }

    private var recentBuys: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionLabel("Recent Buys", systemImage: "cart.fill") {
                Button("View all") { store.tab = .flips }
                    .buttonStyle(.plain).font(.rounded(12, .semibold)).foregroundStyle(Theme.accent)
            }
            if store.bought.isEmpty {
                EmptyState(icon: "cart", text: "No purchases recorded yet.").frame(height: 160)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(store.bought.prefix(8).enumerated()), id: \.element.id) { idx, flip in
                        ListRow(showsSeparator: idx < min(7, store.bought.count - 1)) { FlipRowContent(flip: flip) }
                    }
                }
            }
        }
    }

    private var rightRail: some View {
        VStack(alignment: .leading, spacing: 26) {
            VStack(alignment: .leading, spacing: 12) {
                SectionLabel("Live Activity", systemImage: "dot.radiowaves.left.and.right") {
                    if store.stream.connected { Pill(text: "LIVE", color: Theme.profit, filled: true) }
                }
                if store.events.isEmpty {
                    EmptyState(icon: "waveform.path.ecg", text: "Waiting for activity…").frame(height: 120)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(store.events.prefix(7).enumerated()), id: \.element.id) { idx, event in
                            ListRow(showsSeparator: idx < min(6, store.events.count - 1)) { ActivityRowContent(event: event) }
                        }
                    }
                }
            }
            VStack(alignment: .leading, spacing: 12) {
                SectionLabel("Accounts", systemImage: "person.2.fill") {
                    Button("Manage") { store.tab = .accounts }
                        .buttonStyle(.plain).font(.rounded(12, .semibold)).foregroundStyle(Theme.accent)
                }
                let connected = (store.accounts?.accounts ?? []).filter { !$0.isOffline }
                let notConnected = (store.accounts?.accounts.count ?? 0) - connected.count
                if !connected.isEmpty {
                    VStack(spacing: 0) {
                        ForEach(Array(connected.prefix(5).enumerated()), id: \.element.id) { idx, account in
                            ListRow(showsSeparator: idx < min(4, connected.count - 1)) { AccountRowContent(account: account) }
                        }
                    }
                } else {
                    EmptyState(icon: "person.crop.circle", text: notConnected > 0 ? "No accounts connected." : "No accounts.").frame(height: 90)
                }
                if notConnected > 0 {
                    Button("\(notConnected) not connected") { store.tab = .accounts }
                        .buttonStyle(.plain).font(.rounded(11.5, .medium)).foregroundStyle(Theme.textTertiary)
                }
            }
        }
    }
}

// MARK: - Bot control

struct BotControlButtons: View {
    @EnvironmentObject var store: AppStore
    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 7) {
                StatusDot(color: store.isHalted ? Theme.warn : Theme.profit, pulse: !store.isHalted)
                Text(store.isHalted ? "Halted" : "Running")
                    .font(.rounded(12.5, .semibold))
                    .foregroundStyle(store.isHalted ? Theme.warn : Theme.profit)
            }
            if store.isHalted {
                PrimaryButton(title: "Start Bot", systemImage: "play.fill",
                    tint: LinearGradient(colors: [Theme.profit, Color(hex: 0x2BB98A)], startPoint: .leading, endPoint: .trailing)) {
                    store.runControl("start")
                }
            } else {
                PrimaryButton(title: "Stop Bot", systemImage: "pause.fill",
                    tint: LinearGradient(colors: [Theme.warn, Color(hex: 0xE0892B)], startPoint: .leading, endPoint: .trailing)) {
                    store.runControl("stop")
                }
            }
        }
    }
}

// MARK: - Profit chart

struct ProfitChart: View {
    let points: [ProfitPoint]
    var body: some View {
        if points.isEmpty {
            EmptyState(icon: "chart.xyaxis.line", text: "Profit history appears here as flips complete.")
        } else {
            Chart(points) { point in
                AreaMark(
                    x: .value("Time", Date(timeIntervalSince1970: Double(point.ts) / 1000)),
                    y: .value("Profit", point.cumulative))
                .interpolationMethod(.catmullRom)
                .foregroundStyle(LinearGradient(colors: [Theme.accent.opacity(0.30), Theme.accent.opacity(0.01)], startPoint: .top, endPoint: .bottom))
                LineMark(
                    x: .value("Time", Date(timeIntervalSince1970: Double(point.ts) / 1000)),
                    y: .value("Profit", point.cumulative))
                .interpolationMethod(.catmullRom)
                .foregroundStyle(Theme.brandGradientH)
                .lineStyle(StrokeStyle(lineWidth: 2.4, lineCap: .round))
            }
            .chartXAxis {
                AxisMarks(values: .automatic(desiredCount: 5)) { _ in
                    AxisGridLine().foregroundStyle(Theme.cardStroke.opacity(0.6))
                    AxisValueLabel().font(.rounded(10)).foregroundStyle(Theme.textTertiary)
                }
            }
            .chartYAxis {
                AxisMarks { value in
                    AxisGridLine().foregroundStyle(Theme.cardStroke.opacity(0.6))
                    AxisValueLabel {
                        if let v = value.as(Double.self) { Text(Fmt.coins(v)).font(.rounded(10)).foregroundStyle(Theme.textTertiary) }
                    }
                }
            }
        }
    }
}

// MARK: - Row contents (flat, used inside ListRow)

struct FlipRowContent: View {
    let flip: FlipRecord
    var body: some View {
        HStack(spacing: 12) {
            ItemIcon(tag: flip.tag, size: 34)
            VStack(alignment: .leading, spacing: 2) {
                Text(flip.item).font(.rounded(13, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
                HStack(spacing: 7) {
                    Text(prettyFinder(flip.finder)).font(.rounded(11, .medium)).foregroundStyle(Theme.accent2)
                    Text("·").foregroundStyle(Theme.textTertiary)
                    Text(Fmt.relative(fromMs: flip.ts)).font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary)
                }
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text(Fmt.signedCoins(flip.profit)).font(.numeric(13.5, .bold)).foregroundStyle(Theme.pnl(flip.profit))
                Text(Fmt.coins(Double(flip.price))).font(.numeric(11, .medium)).foregroundStyle(Theme.textTertiary)
            }
        }
    }
}

struct ActivityRowContent: View {
    let event: LiveEvent
    private var info: (icon: String, tint: Color, title: String, detail: String) {
        switch event.type {
        case "purchase":
            return ("cart.fill", Theme.profit, "Bought \(event.raw.stringField("item") ?? "item")", Fmt.signedCoins(event.raw.numberField("profit") ?? 0))
        case "sold":
            return ("checkmark.seal.fill", Theme.gold, "Sold \(event.raw.stringField("item") ?? "item")", Fmt.coins(event.raw.numberField("price") ?? 0))
        case "claim":
            return ("tray.and.arrow.down.fill", Theme.accent, "Claimed sale", Fmt.coins(event.raw.numberField("coins") ?? 0))
        case "state":
            return ("bolt.horizontal.fill", Theme.warn, "State changed", "")
        default:
            return ("bell.fill", Theme.accent2, event.raw["notification"]?.stringField("title") ?? "Notification", "")
        }
    }
    var body: some View {
        let i = info
        HStack(spacing: 10) {
            Image(systemName: i.icon).font(.system(size: 11, weight: .bold)).foregroundStyle(i.tint)
                .frame(width: 24, height: 24).background(Circle().fill(i.tint.opacity(0.14)))
            VStack(alignment: .leading, spacing: 1) {
                Text(i.title).font(.rounded(12, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
                Text(Fmt.clock(fromMs: event.ts)).font(.rounded(10, .medium)).foregroundStyle(Theme.textTertiary)
            }
            Spacer(minLength: 4)
            if !i.detail.isEmpty { Text(i.detail).font(.numeric(12, .bold)).foregroundStyle(i.tint) }
        }
    }
}

struct AccountRowContent: View {
    let account: AccountInfo
    var body: some View {
        HStack(spacing: 10) {
            AccountAvatar(url: account.headUrl, size: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(account.ign).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary).lineLimit(1)
                Text(account.ready == true ? "\(account.stats.bought) bought · \(account.stats.sold) sold" : (account.reason ?? "Not ready"))
                    .font(.rounded(10.5, .medium))
                    .foregroundStyle(account.ready == true ? Theme.textTertiary : Theme.warn)
                    .lineLimit(1)
            }
            Spacer(minLength: 4)
            StatusDot(color: account.ready == true ? Theme.profit : (account.isOffline ? Theme.textTertiary : Theme.warn),
                pulse: account.ready == true)
        }
    }
}

struct AccountAvatar: View {
    let url: String?
    var size: CGFloat = 34
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.28).fill(Color.white.opacity(0.06))
            if let url, let u = URL(string: url) {
                AsyncImage(url: u) { image in
                    image.resizable().interpolation(.none).scaledToFit()
                } placeholder: {
                    Image(systemName: "person.fill").font(.system(size: size * 0.4)).foregroundStyle(Theme.textTertiary)
                }
                .padding(size * 0.1)
            } else {
                Image(systemName: "person.fill").font(.system(size: size * 0.4)).foregroundStyle(Theme.textTertiary)
            }
        }
        .frame(width: size, height: size)
        .overlay(RoundedRectangle(cornerRadius: size * 0.28).strokeBorder(Theme.cardStroke, lineWidth: 1))
    }
}

func prettyFinder(_ finder: String) -> String {
    switch finder.uppercased() {
    case "USER": return "User"
    case "SNIPER_MEDIAN": return "Median"
    case "SNIPER": return "Sniper"
    case "TFM": return "TFM"
    case "AI": return "AI"
    case "CRAFTCOST", "CRAFT_COST": return "Craft"
    case "STONKS": return "Stonks"
    case "FLIPPER": return "Flipper"
    default: return finder.capitalized
    }
}
