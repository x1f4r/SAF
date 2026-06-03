import Foundation

enum APIError: LocalizedError {
    case notConfigured
    case http(Int, String)
    case transport(String)
    case decoding(String)

    var errorDescription: String? {
        switch self {
        case .notConfigured: return "Not connected."
        case .http(let code, let msg): return "Server error \(code): \(msg)"
        case .transport(let msg): return msg
        case .decoding(let msg): return "Bad response: \(msg)"
        }
    }
}

/// Thin REST client against the bot's loopback API (reached through the SSH
/// tunnel). All calls carry the bearer token.
struct APIClient {
    let baseURL: URL
    let token: String

    private var session: URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 12
        config.waitsForConnectivity = false
        return URLSession(configuration: config)
    }

    private func request(_ path: String, method: String = "GET", body: Data? = nil) -> URLRequest {
        // Build by string so query strings in `path` are preserved (appendingPathComponent
        // would percent-encode the `?`).
        let url = URL(string: baseURL.absoluteString + "/" + path) ?? baseURL
        var req = URLRequest(url: url)
        req.httpMethod = method
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        if let body {
            req.httpBody = body
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        return req
    }

    private func send<T: Decodable>(_ req: URLRequest, as type: T.Type) async throws -> T {
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: req)
        } catch {
            throw APIError.transport(error.localizedDescription)
        }
        guard let http = response as? HTTPURLResponse else {
            throw APIError.transport("No HTTP response")
        }
        guard (200..<300).contains(http.statusCode) else {
            let msg = (try? JSONDecoder().decode([String: JSONValue].self, from: data))?["message"]?
                .asString ?? String(data: data, encoding: .utf8) ?? ""
            throw APIError.http(http.statusCode, msg)
        }
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw APIError.decoding("\(error)")
        }
    }

    // MARK: Reads

    func health() async throws -> JSONValue { try await send(request("healthz"), as: JSONValue.self) }
    func status() async throws -> BotStatus { try await send(request("v1/status"), as: BotStatus.self) }
    func accounts() async throws -> AccountsResponse {
        try await send(request("v1/accounts"), as: AccountsResponse.self)
    }
    func profit() async throws -> ProfitSummary {
        try await send(request("v1/profit"), as: ProfitSummary.self)
    }
    func profitSeries(ign: String = "all", sinceMs: UInt64 = 0, bucketSec: UInt64 = 3600)
        async throws -> ProfitSeries
    {
        try await send(
            request("v1/profit/\(ign)/series?since=\(sinceMs)&bucket=\(bucketSec)"),
            as: ProfitSeries.self)
    }
    func boughtFlips(account: String? = nil, limit: Int = 150) async throws -> [FlipRecord] {
        var path = "v1/flips?kind=bought&limit=\(limit)"
        if let account { path += "&account=\(account)" }
        return try await send(request(path), as: FlipsResponse.self).flips
    }
    func soldFlips(account: String? = nil, limit: Int = 150) async throws -> [SaleRecord] {
        var path = "v1/flips?kind=sold&limit=\(limit)"
        if let account { path += "&account=\(account)" }
        return try await send(request(path), as: SalesResponse.self).flips
    }
    func queue(ign: String) async throws -> QueueResponse {
        try await send(request("v1/accounts/\(ign)/queue"), as: QueueResponse.self)
    }
    func inventory(ign: String) async throws -> InventoryResponse {
        try await send(request("v1/accounts/\(ign)/inventory"), as: InventoryResponse.self)
    }
    func auctions(ign: String) async throws -> AuctionsResponse {
        try await send(request("v1/accounts/\(ign)/auctions"), as: AuctionsResponse.self)
    }
    func logs(lines: Int = 250) async throws -> [String] {
        struct R: Codable { var lines: [String] }
        return try await send(request("v1/logs?lines=\(lines)"), as: R.self).lines
    }
    func alerts(lines: Int = 120) async throws -> [Alert] {
        struct R: Codable { var alerts: [Alert] }
        return try await send(request("v1/alerts?lines=\(lines)"), as: R.self).alerts
    }
    func commands() async throws -> [CommandDefinition] {
        try await send(request("v1/commands"), as: CommandsResponse.self).commands
    }

    /// The bot's editable config (secrets stripped server-side). Returns the
    /// `config` object from `{ ok, config }`.
    func getConfig() async throws -> [String: JSONValue] {
        struct R: Codable { var ok: Bool?; var config: [String: JSONValue] }
        return try await send(request("v1/config"), as: R.self).config
    }

    // MARK: Writes

    /// PATCH only the changed top-level keys. The server validates against its
    /// SAFE allowlist; forbidden/unknown keys come back as an HTTP 400 whose
    /// message is surfaced to the user.
    @discardableResult
    func patchConfig(_ patch: [String: JSONValue]) async throws -> (message: String, updated: [String]) {
        struct R: Codable { var ok: Bool?; var message: String?; var updated: [String]? }
        let body = try JSONEncoder().encode(patch)
        let result = try await send(request("v1/config", method: "PATCH", body: body), as: R.self)
        return (result.message ?? "Config updated", result.updated ?? [])
    }

    @discardableResult
    func execute(command: String, options: [String: JSONValue] = [:]) async throws -> CommandResult {
        var payload: [String: JSONValue] = ["command": .string(command)]
        if !options.isEmpty { payload["options"] = .object(options) }
        let body = try JSONEncoder().encode(payload)
        return try await send(request("v1/command", method: "POST", body: body), as: CommandResult.self)
    }

    @discardableResult
    func executeLine(_ line: String) async throws -> CommandResult {
        let body = try JSONEncoder().encode(["line": line])
        return try await send(request("v1/command", method: "POST", body: body), as: CommandResult.self)
    }

    @discardableResult
    func executeTransfer(from: String, to: String, amount: String, stopSource: Bool) async throws -> CommandResult {
        let body = try JSONSerialization.data(withJSONObject: [
            "transfer": ["from": from, "to": to, "amount": amount, "stop_source": stopSource],
        ])
        return try await send(request("v1/command", method: "POST", body: body), as: CommandResult.self)
    }

    @discardableResult
    func executeButton(_ button: String) async throws -> CommandResult {
        let body = try JSONEncoder().encode(["button": button])
        return try await send(request("v1/command", method: "POST", body: body), as: CommandResult.self)
    }

    @discardableResult
    func control(_ action: String) async throws -> CommandResult {
        try await send(request("v1/control/\(action)", method: "POST"), as: CommandResult.self)
    }
}
