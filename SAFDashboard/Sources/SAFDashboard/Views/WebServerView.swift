import SwiftUI

/// "Host a web dashboard" section for the Connection tab. Runs the bundled
/// `web/` Docker context with one click so the dashboard is reachable in a
/// browser (and from other devices).
struct WebServerSection: View {
    @EnvironmentObject var store: AppStore
    private var ws: WebServerManager { store.webServer }

    var body: some View {
        Card(padding: 22) {
            VStack(alignment: .leading, spacing: 16) {
                SectionHeader("Host a Web Dashboard", systemImage: "globe") {
                    statusBadge
                }
                Text("Run the same dashboard in a browser via Docker — reach it from this Mac or any device on your network. The container connects to the bot through this app.")
                    .font(.rounded(12, .medium)).foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)

                content
            }
        }
        .onAppear { ws.refresh() }
    }

    @ViewBuilder private var content: some View {
        switch ws.status {
        case .checking:
            row(spinner: true, "Checking for Docker…")
        case .unavailable(let message):
            VStack(alignment: .leading, spacing: 12) {
                infoLine(icon: "exclamationmark.triangle.fill", tint: Theme.warn, text: message)
                HStack(spacing: 10) {
                    GhostButton(title: "Recheck", systemImage: "arrow.clockwise") { ws.refresh() }
                    GhostButton(title: "Get Docker Desktop", systemImage: "arrow.up.right.square") {
                        if let url = URL(string: "https://www.docker.com/products/docker-desktop/") {
                            NSWorkspace.shared.open(url)
                        }
                    }
                }
            }
        case .stopped, .failed:
            VStack(alignment: .leading, spacing: 14) {
                if case .failed(let message) = ws.status {
                    infoLine(icon: "xmark.octagon.fill", tint: Theme.loss, text: message)
                }
                HStack(spacing: 14) {
                    FormField(label: "Port", systemImage: "number") {
                        TextField("8090", value: Binding(get: { ws.port }, set: { ws.port = $0 }), format: .number)
                    }
                    FormField(label: "Dashboard password", systemImage: "key.fill", hint: "Used to log in to the web dashboard") {
                        TextField("password", text: Binding(get: { ws.password }, set: { ws.password = $0 }))
                    }
                }
                HStack(spacing: 12) {
                    PrimaryButton(title: wasFailed ? "Try Again" : "Start Web Server",
                        systemImage: "play.fill",
                        tint: LinearGradient(colors: [Theme.profit, Color(hex: 0x2BB98A)], startPoint: .leading, endPoint: .trailing)) {
                        ws.start(botPort: store.profile.botLoopbackPort, token: store.token)
                    }
                    .disabled(!store.reachable)
                    .opacity(store.reachable ? 1 : 0.5)
                    if !store.reachable {
                        Text("Connect to the bot first").font(.rounded(11.5, .medium)).foregroundStyle(Theme.warn)
                    }
                    Spacer()
                }
                logBlock
            }
        case .working:
            VStack(alignment: .leading, spacing: 12) {
                row(spinner: true, "Building and starting the container…")
                logBlock
            }
        case .running:
            VStack(alignment: .leading, spacing: 14) {
                infoLine(icon: "checkmark.circle.fill", tint: Theme.profit, text: "Running at \(ws.url.absoluteString)")
                HStack(spacing: 12) {
                    PrimaryButton(title: "Open in Browser", systemImage: "safari") { ws.open() }
                    GhostButton(title: "Stop", systemImage: "stop.fill", role: .destructive) { ws.stop() }
                    GhostButton(title: "Recheck", systemImage: "arrow.clockwise") { ws.refresh() }
                    Spacer()
                }
                VStack(alignment: .leading, spacing: 6) {
                    Text("LOG IN WITH").font(.rounded(9.5, .semibold)).tracking(0.5).foregroundStyle(Theme.textTertiary)
                    CodeBlock(ws.password)
                }
                logBlock
            }
        }
    }

    private var wasFailed: Bool { if case .failed = ws.status { return true }; return false }

    private var statusBadge: some View {
        let (color, text): (Color, String) = {
            switch ws.status {
            case .running: return (Theme.profit, "Running")
            case .working, .checking: return (Theme.warn, "Working")
            case .stopped: return (Theme.textTertiary, "Stopped")
            case .failed: return (Theme.loss, "Failed")
            case .unavailable: return (Theme.warn, "Docker needed")
            }
        }()
        return HStack(spacing: 6) {
            StatusDot(color: color, pulse: ws.status == .running || ws.status == .working)
            Text(text).font(.rounded(11.5, .semibold)).foregroundStyle(color)
        }
    }

    private func infoLine(icon: String, tint: Color, text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon).font(.system(size: 13, weight: .semibold)).foregroundStyle(tint)
            Text(text).font(.rounded(12.5, .medium)).foregroundStyle(Theme.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
    }

    private func row(spinner: Bool, _ text: String) -> some View {
        HStack(spacing: 10) {
            if spinner { ProgressView().controlSize(.small).tint(Theme.accent) }
            Text(text).font(.rounded(12.5, .medium)).foregroundStyle(Theme.textSecondary)
        }
    }

    @ViewBuilder private var logBlock: some View {
        if !ws.log.isEmpty {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 1) {
                        ForEach(Array(ws.log.suffix(60).enumerated()), id: \.offset) { idx, line in
                            Text(line)
                                .font(.system(size: 10.5, weight: .regular, design: .monospaced))
                                .foregroundStyle(Theme.textTertiary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .id(idx)
                        }
                        Color.clear.frame(height: 1).id(-1)
                    }
                    .padding(10)
                }
                .frame(height: 130)
                .background(RoundedRectangle(cornerRadius: 10).fill(Color.black.opacity(0.3)))
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.cardStroke, lineWidth: 1))
                .onChange(of: ws.log.count) { proxy.scrollTo(-1, anchor: .bottom) }
            }
        }
    }
}
