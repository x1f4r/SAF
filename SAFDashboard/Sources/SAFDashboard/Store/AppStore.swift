import Combine
import Foundation
import SwiftUI

@MainActor
final class AppStore: ObservableObject {
    enum Phase: Equatable {
        case onboarding
        case live
    }

    // Navigation
    enum Tab: String, CaseIterable, Identifiable {
        case dashboard, accounts, flips, profit, queue, logs, commands, settings
        var id: String { rawValue }
        var title: String {
            switch self {
            case .dashboard: return "Dashboard"
            case .accounts: return "Accounts"
            case .flips: return "Flips"
            case .profit: return "Profit"
            case .queue: return "Queue"
            case .logs: return "Console"
            case .commands: return "Commands"
            case .settings: return "Connection"
            }
        }
        var icon: String {
            switch self {
            case .dashboard: return "square.grid.2x2.fill"
            case .accounts: return "person.2.fill"
            case .flips: return "arrow.left.arrow.right"
            case .profit: return "chart.line.uptrend.xyaxis"
            case .queue: return "list.bullet.rectangle.fill"
            case .logs: return "terminal.fill"
            case .commands: return "command"
            case .settings: return "antenna.radiowaves.left.and.right"
            }
        }
    }

    @Published var phase: Phase = .onboarding
    @Published var tab: Tab = .dashboard

    @Published var profile = ConnectionProfile()
    @Published var token = ""

    // Live data
    @Published var status: BotStatus?
    @Published var accounts: AccountsResponse?
    @Published var profit: ProfitSummary?
    @Published var series: ProfitSeries?
    @Published var bought: [FlipRecord] = []
    @Published var sold: [SaleRecord] = []
    @Published var logLines: [String] = []
    @Published var events: [LiveEvent] = []
    @Published var commands: [CommandDefinition] = []

    @Published var reachable = false
    @Published var lastError: String?
    @Published var lastRefresh: Date?
    @Published var toast: ToastMessage?

    let tunnel = TunnelManager()
    let stream = EventStream()
    let webServer = WebServerManager()

    private var pollTask: Task<Void, Never>?
    private var tick = 0
    private var bag = Set<AnyCancellable>()
    private var startedStream = false

    var api: APIClient? {
        guard !token.isEmpty else { return nil }
        return APIClient(baseURL: profile.apiBaseURL, token: token)
    }

    var connectionState: TunnelManager.State { tunnel.state }

    init() {
        if let saved = ProfileStore.load(), let savedToken = Keychain.token(),
            saved.isComplete, !savedToken.isEmpty
        {
            profile = saved
            token = savedToken
            phase = .live
            connect()
        }
        // Optional deterministic initial tab (used for previews/screenshots).
        if let raw = ProcessInfo.processInfo.environment["SAF_INITIAL_TAB"],
            let initial = Tab(rawValue: raw)
        {
            tab = initial
        }
        // Re-publish nested object changes so views observing AppStore update
        // when the tunnel/stream state changes.
        tunnel.objectWillChange.sink { [weak self] _ in self?.objectWillChange.send() }
            .store(in: &bag)
        stream.objectWillChange.sink { [weak self] _ in self?.objectWillChange.send() }
            .store(in: &bag)
        webServer.objectWillChange.sink { [weak self] _ in self?.objectWillChange.send() }
            .store(in: &bag)

        stream.onEvent = { [weak self] event in self?.apply(event) }
    }

    // MARK: Connection lifecycle

    func saveAndConnect(profile: ConnectionProfile, token: String) {
        self.profile = profile
        self.token = token
        ProfileStore.save(profile)
        Keychain.setToken(token)
        phase = .live
        connect()
    }

    func connect() {
        disconnectRuntime()
        startedStream = false
        tunnel.start(profile)
        pollTask = Task { [weak self] in await self?.pollLoop() }
    }

    func disconnect() {
        disconnectRuntime()
        phase = .onboarding
        reachable = false
        status = nil
        accounts = nil
        profit = nil
        ProfileStore.clear()
        Keychain.clear()
    }

    private func disconnectRuntime() {
        pollTask?.cancel()
        pollTask = nil
        stream.stop()
        tunnel.stop()
    }

    /// One-shot connectivity probe used by onboarding.
    func test(profile: ConnectionProfile, token: String) async -> (ok: Bool, message: String) {
        let probe = TunnelManager()
        probe.start(profile)
        // Wait up to ~8s for the tunnel to come up.
        for _ in 0..<16 {
            try? await Task.sleep(nanoseconds: 500_000_000)
            if case .up = probe.state { break }
            if case .failed(let msg) = probe.state { probe.stop(); return (false, msg) }
        }
        let client = APIClient(baseURL: profile.apiBaseURL, token: token)
        defer { probe.stop() }
        do {
            let status = try await client.status()
            let name = status.name ?? "SAF"
            let accts = status.configured.count
            return (true, "Connected to \(name) · \(accts) account\(accts == 1 ? "" : "s") · \(status.running.count) running")
        } catch {
            return (false, (error as? APIError)?.errorDescription ?? error.localizedDescription)
        }
    }

    // MARK: Polling

    private func pollLoop() async {
        while !Task.isCancelled {
            if case .up = tunnel.state {
                await refresh()
            }
            try? await Task.sleep(nanoseconds: 3_000_000_000)
            tick += 1
        }
    }

    func refresh() async {
        guard let api else { return }
        do {
            async let status = api.status()
            async let accounts = api.accounts()
            async let profit = api.profit()
            let (s, a, p) = try await (status, accounts, profit)
            self.status = s
            self.accounts = a
            self.profit = p
            self.reachable = true
            self.lastError = nil
            self.lastRefresh = Date()

            if !startedStream {
                startedStream = true
                stream.start(url: profile.wsURL, token: token)
            }

            // Slower-cadence data.
            if tick % 3 == 0 || series == nil {
                if let series = try? await api.profitSeries(bucketSec: 1800) { self.series = series }
                if let bought = try? await api.boughtFlips(limit: 200) { self.bought = bought }
                if let sold = try? await api.soldFlips(limit: 200) { self.sold = sold }
            }
            if let logs = try? await api.logs(lines: 300) { self.logLines = logs }
            if commands.isEmpty, let commands = try? await api.commands() { self.commands = commands }
        } catch {
            self.reachable = false
            self.lastError = (error as? APIError)?.errorDescription ?? error.localizedDescription
        }
    }

    // MARK: Live event handling

    private func apply(_ event: LiveEvent) {
        events.insert(event, at: 0)
        if events.count > 250 { events.removeLast(events.count - 250) }

        switch event.type {
        case "purchase":
            if let data = try? JSONEncoder().encode(event.raw),
                let flip = try? JSONDecoder().decode(FlipRecord.self, from: data)
            {
                bought.insert(flip, at: 0)
                if bought.count > 400 { bought.removeLast() }
                toast = ToastMessage(
                    icon: "cart.fill", tint: Theme.profit,
                    title: "Bought \(flip.item)",
                    detail: "\(Fmt.coins(Double(flip.price))) → \(Fmt.signedCoins(flip.profit)) profit")
            }
        case "sold":
            if let data = try? JSONEncoder().encode(event.raw),
                let sale = try? JSONDecoder().decode(SaleRecord.self, from: data)
            {
                sold.insert(sale, at: 0)
                if sold.count > 400 { sold.removeLast() }
                toast = ToastMessage(
                    icon: "checkmark.seal.fill", tint: Theme.gold,
                    title: "Sold \(sale.item)", detail: Fmt.coins(Double(sale.price)))
            }
        case "state":
            if var status {
                status.halted = event.raw.numberField("halted").map { $0 != 0 }
                    ?? (event.raw["halted"].flatMap { if case .bool(let b) = $0 { return b }; return nil } ?? status.halted)
                self.status = status
            }
        default:
            break
        }
    }

    // MARK: Commands

    func runControl(_ action: String) {
        Task {
            guard let api else { return }
            do {
                _ = try await api.control(action)
                toast = ToastMessage(
                    icon: action == "start" ? "play.fill" : "pause.fill",
                    tint: action == "start" ? Theme.profit : Theme.warn,
                    title: action == "start" ? "Bot started" : "Bot stopped", detail: nil)
                await refresh()
            } catch {
                showError(error)
            }
        }
    }

    func runCommand(_ name: String, options: [String: JSONValue] = [:], label: String? = nil) {
        Task {
            guard let api else { return }
            do {
                let result = try await api.execute(command: name, options: options)
                if let confirm = result.confirm, result.requiresConfirmation == true {
                    toast = ToastMessage(
                        icon: "exclamationmark.triangle.fill", tint: Theme.warn,
                        title: confirm.title, detail: "Confirm in Commands")
                } else {
                    toast = ToastMessage(
                        icon: "checkmark.circle.fill", tint: Theme.accent,
                        title: label ?? "/\(name)", detail: "Sent")
                }
                await refresh()
            } catch {
                showError(error)
            }
        }
    }

    func runButton(_ button: String, title: String) {
        Task {
            guard let api else { return }
            do {
                _ = try await api.executeButton(button)
                toast = ToastMessage(icon: "checkmark.circle.fill", tint: Theme.accent, title: title, detail: "Sent")
                await refresh()
            } catch { showError(error) }
        }
    }

    func runLine(_ line: String) {
        Task {
            guard let api else { return }
            do {
                _ = try await api.executeLine(line)
                toast = ToastMessage(icon: "terminal.fill", tint: Theme.accent, title: line, detail: "Sent")
                await refresh()
            } catch { showError(error) }
        }
    }

    private func showError(_ error: Error) {
        let message = (error as? APIError)?.errorDescription ?? error.localizedDescription
        lastError = message
        toast = ToastMessage(icon: "xmark.octagon.fill", tint: Theme.loss, title: "Command failed", detail: message)
    }

    // MARK: Derived helpers

    var isHalted: Bool { status?.halted ?? true }
    var runningCount: Int { status?.running.count ?? 0 }
    var configuredCount: Int { status?.configured.count ?? accounts?.configured.count ?? 0 }
}

struct ToastMessage: Identifiable, Equatable {
    let id = UUID()
    var icon: String
    var tint: Color
    var title: String
    var detail: String?
}
