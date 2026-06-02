import SwiftUI

struct OnboardingView: View {
    @EnvironmentObject var store: AppStore
    @State private var profile = ProfileStore.load() ?? ConnectionProfile()
    @State private var token = Keychain.token() ?? ""
    @State private var showAdvanced = false
    @State private var testing = false
    @State private var testResult: (ok: Bool, message: String)?

    var body: some View {
        ScrollView {
            VStack(spacing: 22) {
                header
                Card(padding: 24) {
                    VStack(alignment: .leading, spacing: 18) {
                        FormField(
                            label: "SSH destination", systemImage: "server.rack",
                            hint: "Alias (e.g. saf-vps) or user@host · leave blank for a direct local connection") {
                            TextField("saf-vps", text: $profile.sshDestination)
                        }
                        FormField(
                            label: "API token", systemImage: "key.fill",
                            hint: "Set on the VPS by scripts/setup-dashboard-api.sh") {
                            SecureField("paste token", text: $token)
                        }

                        DisclosureGroup(isExpanded: $showAdvanced) {
                            VStack(alignment: .leading, spacing: 16) {
                                FormField(label: "Identity file", systemImage: "lock.fill",
                                    hint: "Optional — overrides ssh config") {
                                    TextField("~/.ssh/id_ed25519", text: $profile.identityFile)
                                }
                                HStack(spacing: 14) {
                                    FormField(label: "SSH port", systemImage: "number") {
                                        TextField("22", value: $profile.sshPort, format: .number)
                                    }
                                    FormField(label: "Remote API port", systemImage: "arrow.down.to.line") {
                                        TextField("8787", value: $profile.remotePort, format: .number)
                                    }
                                    FormField(label: "Local port", systemImage: "arrow.up.to.line") {
                                        TextField("8787", value: $profile.localPort, format: .number)
                                    }
                                }
                            }
                            .padding(.top, 12)
                        } label: {
                            Text("Advanced")
                                .font(.rounded(12.5, .semibold))
                                .foregroundStyle(Theme.textSecondary)
                        }
                        .tint(Theme.accent)

                        if let result = testResult {
                            HStack(spacing: 8) {
                                Image(systemName: result.ok ? "checkmark.circle.fill" : "xmark.octagon.fill")
                                    .foregroundStyle(result.ok ? Theme.profit : Theme.loss)
                                Text(result.message)
                                    .font(.rounded(12, .medium))
                                    .foregroundStyle(result.ok ? Theme.profit : Theme.loss)
                                Spacer()
                            }
                            .padding(11)
                            .background(RoundedRectangle(cornerRadius: 10).fill((result.ok ? Theme.profit : Theme.loss).opacity(0.1)))
                        }

                        HStack(spacing: 12) {
                            GhostButton(title: testing ? "Testing…" : "Test connection", systemImage: "bolt.horizontal.fill") {
                                runTest()
                            }
                            Spacer()
                            PrimaryButton(title: "Connect", systemImage: "arrow.right") {
                                store.saveAndConnect(profile: profile, token: token)
                            }
                            .disabled(!profile.isComplete || token.isEmpty)
                            .opacity(!profile.isComplete || token.isEmpty ? 0.5 : 1)
                        }
                    }
                }
                .frame(maxWidth: 560)

                securityNote.frame(maxWidth: 560)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 50)
            .padding(.horizontal, 24)
        }
    }

    private var header: some View {
        VStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(Theme.brandGradient)
                    .frame(width: 70, height: 70)
                    .shadow(color: Theme.accent.opacity(0.5), radius: 18, y: 6)
                Image(systemName: "bolt.fill")
                    .font(.system(size: 32, weight: .black))
                    .foregroundStyle(.black.opacity(0.85))
            }
            Text("Connect to SAF")
                .font(.rounded(28, .bold))
                .foregroundStyle(Theme.textPrimary)
            Text("Securely link this dashboard to your bot over SSH.\nNo ports are exposed to the internet.")
                .multilineTextAlignment(.center)
                .font(.rounded(13.5, .medium))
                .foregroundStyle(Theme.textSecondary)
        }
    }

    private var securityNote: some View {
        Card(padding: 16) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "lock.shield.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(Theme.accent)
                VStack(alignment: .leading, spacing: 5) {
                    Text("How the connection works")
                        .font(.rounded(13, .semibold))
                        .foregroundStyle(Theme.textPrimary)
                    Text("The dashboard opens an SSH tunnel with your key and talks to the bot's API on the server's loopback interface. The tunnel reconnects automatically and survives VPS reboots. Switching servers later only means changing the destination above.")
                        .font(.rounded(12, .medium))
                        .foregroundStyle(Theme.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func runTest() {
        testing = true
        testResult = nil
        Task {
            let result = await store.test(profile: profile, token: token)
            await MainActor.run {
                testing = false
                testResult = result
            }
        }
    }
}

struct FormField<Content: View>: View {
    let label: String
    var systemImage: String? = nil
    var hint: String? = nil
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                if let systemImage {
                    Image(systemName: systemImage).font(.system(size: 11, weight: .semibold)).foregroundStyle(Theme.accent)
                }
                Text(label).font(.rounded(12, .semibold)).foregroundStyle(Theme.textSecondary)
            }
            content
                .textFieldStyle(.plain)
                .font(.rounded(13.5, .medium))
                .foregroundStyle(Theme.textPrimary)
                .padding(.horizontal, 12).padding(.vertical, 10)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.black.opacity(0.25)))
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Theme.cardStroke, lineWidth: 1))
            if let hint {
                Text(hint).font(.rounded(10.5, .medium)).foregroundStyle(Theme.textTertiary)
            }
        }
    }
}
