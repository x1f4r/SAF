import Foundation

/// A saved connection to a SAF bot host. Non-secret fields persist to
/// Application Support; the API token lives in the Keychain. Nothing here is
/// ever committed — it is per-user runtime state.
struct ConnectionProfile: Codable, Equatable {
    /// SSH destination: an alias from ~/.ssh/config (e.g. "saf-vps") or
    /// "user@host". Using an alias means the user's existing SSH config
    /// supplies the key, user, and port automatically.
    var sshDestination: String = ""
    /// Optional explicit identity file (overrides ssh config).
    var identityFile: String = ""
    /// Optional explicit SSH port (0 = use ssh config / default 22).
    var sshPort: Int = 0
    /// Port the bot's API listens on (loopback) on the remote host.
    var remotePort: Int = 8787
    /// Local port the tunnel forwards to. The app talks to 127.0.0.1:localPort.
    var localPort: Int = 8787
    /// Working directory on the remote host where the bot lives (the restart
    /// command runs `cd <restartDir> && <restartCommand>`).
    var restartDir: String = "/opt/saf"
    /// Shell command that restarts the bot on the remote host.
    var restartCommand: String = "./saf.sh restart"
    /// Friendly label shown in the UI.
    var label: String = "My SAF VPS"

    init() {}

    /// Tolerant decoder: every field falls back to its default when absent, so a
    /// `connection.json` written by an older build (one without `restartDir` /
    /// `restartCommand`) still loads instead of dropping the user back to
    /// onboarding. Swift's synthesized `Codable` would throw on the missing keys.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sshDestination = try c.decodeIfPresent(String.self, forKey: .sshDestination) ?? ""
        identityFile = try c.decodeIfPresent(String.self, forKey: .identityFile) ?? ""
        sshPort = try c.decodeIfPresent(Int.self, forKey: .sshPort) ?? 0
        remotePort = try c.decodeIfPresent(Int.self, forKey: .remotePort) ?? 8787
        localPort = try c.decodeIfPresent(Int.self, forKey: .localPort) ?? 8787
        restartDir = try c.decodeIfPresent(String.self, forKey: .restartDir) ?? "/opt/saf"
        restartCommand = try c.decodeIfPresent(String.self, forKey: .restartCommand) ?? "./saf.sh restart"
        label = try c.decodeIfPresent(String.self, forKey: .label) ?? "My SAF VPS"
    }

    /// When the SSH destination is blank, connect straight to 127.0.0.1:remotePort
    /// (the bot runs locally, or the user manages their own tunnel/VPN).
    var usesTunnel: Bool { !sshDestination.trimmingCharacters(in: .whitespaces).isEmpty }

    /// Always satisfiable: an empty destination means a direct local connection.
    var isComplete: Bool { remotePort > 0 }

    /// The port on this Mac's loopback where the bot API is reachable (the
    /// tunnel's local port, or the remote port in direct mode). A Docker
    /// container can reach it at host.docker.internal:<this>.
    var botLoopbackPort: Int { usesTunnel ? localPort : remotePort }
    private var effectivePort: Int { botLoopbackPort }
    var apiBaseURL: URL { URL(string: "http://127.0.0.1:\(effectivePort)")! }
    var wsURL: URL { URL(string: "ws://127.0.0.1:\(effectivePort)/v1/events")! }
}

/// Runs the bot's restart command on the remote host over a one-shot SSH
/// session, reusing the same `ssh` binary + hardening flags as the tunnel.
/// On macOS the restart goes over SSH (not the gateway) so it works the same
/// whether or not a web gateway is deployed; a blank SSH destination means
/// there is no remote to restart, which is a clear error.
enum RemoteRestart {
    struct Error: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    /// Execute `cd <restartDir> && <restartCommand>` on the remote host. Throws
    /// a descriptive error if SSH is unavailable or the command fails.
    @discardableResult
    static func run(_ profile: ConnectionProfile) async throws -> String {
        let destination = profile.sshDestination.trimmingCharacters(in: .whitespaces)
        guard !destination.isEmpty else {
            throw Error(message: "Restart requires an SSH connection. Set an SSH destination in Connection first.")
        }
        let dir = profile.restartDir.trimmingCharacters(in: .whitespaces)
        let command = profile.restartCommand.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty else {
            throw Error(message: "No restart command configured.")
        }
        let remote = dir.isEmpty ? command : "cd \(shellQuote(dir)) && \(command)"

        var args = [
            "-T",
            "-o", "ConnectTimeout=12",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "BatchMode=yes",
        ]
        if profile.sshPort > 0 { args += ["-p", "\(profile.sshPort)"] }
        let identity = profile.identityFile.trimmingCharacters(in: .whitespaces)
        if !identity.isEmpty {
            args += ["-i", (identity as NSString).expandingTildeInPath, "-o", "IdentitiesOnly=yes"]
        }
        args += [destination, remote]

        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let proc = Process()
                proc.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
                proc.arguments = args
                var env = ProcessInfo.processInfo.environment
                env["SSH_ASKPASS"] = "/usr/bin/false"
                proc.environment = env
                let pipe = Pipe()
                proc.standardError = pipe
                proc.standardOutput = pipe
                do {
                    try proc.run()
                    proc.waitUntilExit()
                    let data = pipe.fileHandleForReading.readDataToEndOfFile()
                    let output = String(data: data, encoding: .utf8)?
                        .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    if proc.terminationStatus == 0 {
                        continuation.resume(returning: output)
                    } else {
                        let reason = output.isEmpty ? "ssh exited (\(proc.terminationStatus))" : output
                        continuation.resume(throwing: Error(message: "Restart failed: \(reason)"))
                    }
                } catch {
                    continuation.resume(throwing: Error(message: "Could not run ssh: \(error.localizedDescription)"))
                }
            }
        }
    }

    /// Minimal single-quote shell escaping for the remote `cd` path.
    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}

enum ProfileStore {
    private static var fileURL: URL {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("SAF Dashboard", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("connection.json")
    }

    static func load() -> ConnectionProfile? {
        guard let data = try? Data(contentsOf: fileURL) else { return nil }
        return try? JSONDecoder().decode(ConnectionProfile.self, from: data)
    }

    static func save(_ profile: ConnectionProfile) {
        if let data = try? JSONEncoder().encode(profile) {
            try? data.write(to: fileURL, options: .atomic)
        }
    }

    static func clear() {
        try? FileManager.default.removeItem(at: fileURL)
    }
}

/// Minimal Keychain wrapper for the API bearer token (one item, generic
/// password). Keeps the token out of plists and the repo.
enum Keychain {
    private static let service = "com.x1f4r.safdashboard.token"
    private static let account = "api-token"

    static func setToken(_ token: String) {
        let data = Data(token.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
        var add = query
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        SecItemAdd(add as CFDictionary, nil)
    }

    static func token() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
            let data = result as? Data, let token = String(data: data, encoding: .utf8)
        else { return nil }
        return token
    }

    static func clear() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}
