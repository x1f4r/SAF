import AppKit
import SwiftUI

@main
struct SAFDashboardApp: App {
    @StateObject private var store = AppStore()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .frame(minWidth: 1140, minHeight: 740)
                .background(WindowConfigurator())
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .defaultSize(width: 1360, height: 880)
        .commands { CommandGroup(replacing: .newItem) {} }
    }
}

struct RootView: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        ZStack {
            Theme.appBackground
            switch store.phase {
            case .onboarding:
                OnboardingView()
                    .transition(.opacity.combined(with: .scale(scale: 0.98)))
            case .live:
                MainShell()
                    .transition(.opacity)
            }
        }
        .overlay(alignment: .bottom) { ToastView() }
        .animation(.spring(response: 0.4, dampingFraction: 0.85), value: store.phase)
        .preferredColorScheme(.dark)
        .tint(Theme.accent)
    }
}

struct MainShell: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        HStack(spacing: 0) {
            Sidebar()
            Divider().overlay(Theme.cardStroke)
            ZStack {
                switch store.tab {
                case .dashboard: DashboardView()
                case .accounts: AccountsView()
                case .flips: FlipsView()
                case .profit: ProfitView()
                case .queue: QueueView()
                case .logs: LogsView()
                case .commands: CommandsView()
                case .blacklist: BlacklistView()
                case .diagnostics: DiagnosticsView()
                case .settings: ConnectionView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

struct Sidebar: View {
    @EnvironmentObject var store: AppStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Brand
            HStack(spacing: 11) {
                ZStack {
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .fill(Theme.brandGradient)
                        .frame(width: 38, height: 38)
                        .shadow(color: Theme.accent.opacity(0.5), radius: 10, y: 3)
                    Image(systemName: "bolt.fill")
                        .font(.system(size: 18, weight: .black))
                        .foregroundStyle(.black.opacity(0.85))
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text("SAF")
                        .font(.rounded(20, .black))
                        .foregroundStyle(Theme.textPrimary)
                    Text("Auction Flipper")
                        .font(.rounded(10.5, .medium))
                        .foregroundStyle(Theme.textTertiary)
                }
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.top, 26)
            .padding(.bottom, 22)

            // Nav
            VStack(spacing: 4) {
                ForEach(Array(AppStore.Tab.allCases.enumerated()), id: \.element) { index, tab in
                    SidebarRow(tab: tab, selected: store.tab == tab, index: index) {
                        withAnimation(.easeOut(duration: 0.18)) { store.tab = tab }
                    }
                }
            }
            .padding(.horizontal, 12)

            Spacer()

            ConnectionBadge()
                .padding(.horizontal, 12)
                .padding(.bottom, 14)
        }
        .frame(width: 232)
        .background(Theme.sidebar)
    }
}

struct SidebarRow: View {
    let tab: AppStore.Tab
    let selected: Bool
    var index: Int = 0
    let action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 11) {
                Image(systemName: tab.icon)
                    .font(.system(size: 14, weight: .semibold))
                    .frame(width: 22)
                    .foregroundStyle(selected ? Color.black.opacity(0.85) : (hover ? Theme.textPrimary : Theme.textSecondary))
                Text(tab.title)
                    .font(.rounded(13.5, selected ? .semibold : .medium))
                    .foregroundStyle(selected ? Color.black.opacity(0.85) : (hover ? Theme.textPrimary : Theme.textSecondary))
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .fill(selected ? AnyShapeStyle(Theme.brandGradientH) : AnyShapeStyle(hover ? Color.white.opacity(0.06) : Color.clear))
            )
            .shadow(color: selected ? Theme.accent.opacity(0.35) : .clear, radius: 8, y: 2)
        }
        .buttonStyle(.plain)
        .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: .command)
        .onHover { hover = $0 }
        .animation(.easeOut(duration: 0.15), value: hover)
    }
}

struct ConnectionBadge: View {
    @EnvironmentObject var store: AppStore

    private var dotColor: Color {
        if case .up = store.connectionState { return store.reachable ? Theme.profit : Theme.warn }
        if case .failed = store.connectionState { return Theme.loss }
        return Theme.warn
    }
    private var label: String {
        switch store.connectionState {
        case .idle: return "Disconnected"
        case .connecting: return "Connecting…"
        case .up: return store.reachable ? "Connected" : "Tunnel up · waiting"
        case .retrying: return "Reconnecting…"
        case .failed: return "Connection failed"
        }
    }

    var body: some View {
        Card(padding: 12) {
            HStack(spacing: 10) {
                StatusDot(color: dotColor, pulse: true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(label)
                        .font(.rounded(12, .semibold))
                        .foregroundStyle(Theme.textPrimary)
                    Text(store.profile.sshDestination.isEmpty ? store.profile.label : store.profile.sshDestination)
                        .font(.rounded(10.5, .medium))
                        .foregroundStyle(Theme.textTertiary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if store.stream.connected {
                    Image(systemName: "dot.radiowaves.left.and.right")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(Theme.profit)
                }
            }
        }
    }
}

// MARK: - Toast

struct ToastView: View {
    @EnvironmentObject var store: AppStore
    @State private var work: DispatchWorkItem?

    var body: some View {
        Group {
            if let toast = store.toast {
                HStack(spacing: 11) {
                    Image(systemName: toast.icon)
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(toast.tint)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(toast.title).font(.rounded(13, .semibold)).foregroundStyle(Theme.textPrimary)
                        if let detail = toast.detail {
                            Text(detail).font(.rounded(11.5, .medium)).foregroundStyle(Theme.textSecondary).lineLimit(1)
                        }
                    }
                }
                .padding(.horizontal, 16).padding(.vertical, 12)
                .background(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(.ultraThinMaterial)
                        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(toast.tint.opacity(0.4), lineWidth: 1))
                )
                .shadow(color: .black.opacity(0.4), radius: 18, y: 8)
                .padding(.bottom, 22)
                .transition(.move(edge: .bottom).combined(with: .opacity))
                .id(toast.id)
                .onAppear {
                    work?.cancel()
                    let item = DispatchWorkItem { withAnimation { store.toast = nil } }
                    work = item
                    DispatchQueue.main.asyncAfter(deadline: .now() + 3.4, execute: item)
                }
            }
        }
        .animation(.spring(response: 0.4, dampingFraction: 0.8), value: store.toast)
    }
}

// MARK: - Window configuration + window-id export (for clean screenshots)

struct WindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.titlebarAppearsTransparent = true
            window.backgroundColor = NSColor(red: 0.024, green: 0.027, blue: 0.043, alpha: 1)
            window.isMovableByWindowBackground = true
            // Export window number so the screenshot helper can target just this
            // window without Screen-Recording TCC on a separate helper binary.
            let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("SAF Dashboard", isDirectory: true)
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try? "\(window.windowNumber)".write(
                to: dir.appendingPathComponent("window.id"), atomically: true, encoding: .utf8)
        }
        return view
    }
    func updateNSView(_ nsView: NSView, context: Context) {}
}
