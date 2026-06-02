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
    /// Friendly label shown in the UI.
    var label: String = "My SAF VPS"

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
