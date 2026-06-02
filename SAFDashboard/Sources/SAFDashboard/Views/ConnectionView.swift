import SwiftUI

struct ConnectionView: View {
    @EnvironmentObject var store: AppStore
    @State private var profile = ConnectionProfile()
    @State private var token = ""
    @State private var loaded = false
    @State private var testing = false
    @State private var testResult: (ok: Bool, message: String)?

    var body: some View {
        Page(spacing: 18) {
                PageHeader("Connection", subtitle: "Manage the secure link to your bot host")

                statusCard

                Card(padding: 22) {
                    VStack(alignment: .leading, spacing: 16) {
                        SectionHeader("Settings", systemImage: "slider.horizontal.3")
                        FormField(label: "SSH destination", systemImage: "server.rack", hint: "Alias from ~/.ssh/config or user@host") {
                            TextField("saf-vps", text: $profile.sshDestination)
                        }
                        FormField(label: "API token", systemImage: "key.fill") {
                            SecureField("token", text: $token)
                        }
                        HStack(spacing: 14) {
                            FormField(label: "Identity file", systemImage: "lock.fill") {
                                TextField("optional", text: $profile.identityFile)
                            }
                            FormField(label: "Remote port", systemImage: "arrow.down.to.line") {
                                TextField("8787", value: $profile.remotePort, format: .number)
                            }
                            FormField(label: "Local port", systemImage: "arrow.up.to.line") {
                                TextField("8787", value: $profile.localPort, format: .number)
                            }
                        }
                        if let result = testResult {
                            HStack(spacing: 8) {
                                Image(systemName: result.ok ? "checkmark.circle.fill" : "xmark.octagon.fill")
                                    .foregroundStyle(result.ok ? Theme.profit : Theme.loss)
                                Text(result.message).font(.rounded(12, .medium)).foregroundStyle(result.ok ? Theme.profit : Theme.loss)
                                Spacer()
                            }.padding(11).background(RoundedRectangle(cornerRadius: 10).fill((result.ok ? Theme.profit : Theme.loss).opacity(0.1)))
                        }
                        HStack(spacing: 12) {
                            GhostButton(title: testing ? "Testing…" : "Test", systemImage: "bolt.horizontal.fill") { runTest() }
                            GhostButton(title: "Disconnect", systemImage: "xmark.circle", role: .destructive) { store.disconnect() }
                            Spacer()
                            PrimaryButton(title: "Save & Reconnect", systemImage: "arrow.clockwise") {
                                store.saveAndConnect(profile: profile, token: token)
                            }
                        }
                    }
                }

                WebServerSection()

                NotificationsSection()

                setupHelp
        }
        .onAppear {
            if !loaded { profile = store.profile; token = store.token; loaded = true }
        }
    }

    private var statusCard: some View {
        Card {
            HStack(spacing: 20) {
                statusItem("Tunnel", tunnelLabel, tunnelColor, "lock.shield.fill")
                Divider().frame(height: 38).overlay(Theme.cardStroke)
                statusItem("API", store.reachable ? "Reachable" : "Unreachable", store.reachable ? Theme.profit : Theme.warn, "antenna.radiowaves.left.and.right")
                Divider().frame(height: 38).overlay(Theme.cardStroke)
                statusItem("Live feed", store.stream.connected ? "Streaming" : "Off", store.stream.connected ? Theme.profit : Theme.textTertiary, "dot.radiowaves.left.and.right")
                Divider().frame(height: 38).overlay(Theme.cardStroke)
                statusItem("Bot", (store.status?.name ?? "—"), Theme.accent, "bolt.fill")
                Spacer()
                if let refresh = store.lastRefresh {
                    VStack(alignment: .trailing, spacing: 2) {
                        Text("LAST SYNC").font(.rounded(9, .semibold)).foregroundStyle(Theme.textTertiary)
                        Text(refresh.formatted(date: .omitted, time: .standard)).font(.numeric(12, .medium)).foregroundStyle(Theme.textSecondary)
                    }
                }
            }
        }
    }

    private func statusItem(_ label: String, _ value: String, _ color: Color, _ icon: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: icon).font(.system(size: 15, weight: .semibold)).foregroundStyle(color)
                .frame(width: 32, height: 32).background(Circle().fill(color.opacity(0.15)))
            VStack(alignment: .leading, spacing: 1) {
                Text(label.uppercased()).font(.rounded(9, .semibold)).tracking(0.4).foregroundStyle(Theme.textTertiary)
                Text(value).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary)
            }
        }
    }

    private var tunnelLabel: String {
        switch store.connectionState {
        case .idle: return "Idle"
        case .connecting: return "Connecting"
        case .up: return "Up"
        case .retrying: return "Retrying"
        case .failed: return "Failed"
        }
    }
    private var tunnelColor: Color {
        switch store.connectionState {
        case .up: return Theme.profit
        case .failed: return Theme.loss
        default: return Theme.warn
        }
    }

    private var setupHelp: some View {
        Card {
            VStack(alignment: .leading, spacing: 10) {
                SectionHeader("Set up a new server", systemImage: "wrench.and.screwdriver.fill")
                Text("On any machine with SSH access to the bot host, run the helper to enable the API, generate a token, and restart SAF:")
                    .font(.rounded(12, .medium)).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                CodeBlock("scripts/setup-dashboard-api.sh <ssh-destination>")
                Text("It prints a token — paste it above. Moving to a brand-new VPS only means re-running this and updating the destination here; your bot's saved state lives on the server.")
                    .font(.rounded(11.5, .medium)).foregroundStyle(Theme.textTertiary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

struct NotificationsSection: View {
    @State private var on = Notifier.shared.enabled
    @State private var busy = false

    var body: some View {
        if Notifier.shared.supported {
            Card {
                HStack(spacing: 14) {
                    Image(systemName: "bell.fill").font(.system(size: 15, weight: .semibold)).foregroundStyle(Theme.accent)
                        .frame(width: 32, height: 32).background(Circle().fill(Theme.accent.opacity(0.15)))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Desktop notifications").font(.rounded(13, .bold)).foregroundStyle(Theme.textPrimary)
                        Text("Buys, sells, and errors when the app is in the background.")
                            .font(.rounded(11.5, .medium)).foregroundStyle(Theme.textTertiary)
                    }
                    Spacer()
                    Toggle("", isOn: $on)
                        .labelsHidden().toggleStyle(.switch).tint(Theme.accent).disabled(busy)
                        .onChange(of: on) { _, want in
                            if want {
                                busy = true
                                Task {
                                    let granted = await Notifier.shared.enable()
                                    await MainActor.run { on = granted; busy = false }
                                }
                            } else {
                                Notifier.shared.disable()
                            }
                        }
                }
            }
        }
    }
}

struct CodeBlock: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        HStack {
            Text(text).font(.system(size: 12, weight: .medium, design: .monospaced)).foregroundStyle(Theme.accent).textSelection(.enabled)
            Spacer()
            Image(systemName: "doc.on.doc").font(.system(size: 11)).foregroundStyle(Theme.textTertiary)
        }
        .padding(.horizontal, 14).padding(.vertical, 11)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.black.opacity(0.3)))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.cardStroke, lineWidth: 1))
        .onTapGesture {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        }
    }
}

extension ConnectionView {
    private func runTest() {
        testing = true; testResult = nil
        Task {
            let result = await store.test(profile: profile, token: token)
            await MainActor.run {
                testing = false
                testResult = result
            }
        }
    }
}
