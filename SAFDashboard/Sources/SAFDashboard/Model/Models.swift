import Foundation

// MARK: - Status

struct BotStatus: Codable, Equatable {
    var ok: Bool
    var version: String?
    var name: String?
    var startedAtMs: UInt64?
    var uptimeMs: UInt64?
    var marketMode: String?
    var halted: Bool
    var paused: Bool
    var configured: [String]
    var running: [String]
    var defaultIgn: String?
    var commandInbox: String?
    var stateBaseDir: String?
}

// MARK: - Accounts

struct AccountStats: Codable, Equatable, Hashable {
    var bought: Int
    var sold: Int
    var totalProfit: Double
    var userFinderFlips: Int
    var profitPerHour: Double?
    var purse: Double?
    var startedAtMs: UInt64?
    var coflDelayMs: UInt64?
    var coflPingMs: UInt64?
    var coflTier: String?
    var coflExpiresAt: UInt64?
    var cookieExpiresAt: UInt64?
    var hypixelPingMs: UInt64?
    var auctionSlotsUsed: Int?
    var auctionSlotsMax: Int?
}

struct AccountInfo: Codable, Equatable, Identifiable, Hashable {
    var ign: String
    var running: Bool
    var queueSize: Int
    var connectionId: String?
    var coflConnected: Bool?
    var hasCookie: Bool?
    var status: String?          // offline | connecting | online
    var ready: Bool?
    var reason: String?
    var stats: AccountStats
    var headUrl: String?
    var id: String { ign }

    var effectiveStatus: String { status ?? (running ? "online" : "offline") }
    var isOffline: Bool { effectiveStatus == "offline" }
}

struct AccountsResponse: Codable, Equatable {
    var configured: [String]
    var running: [String]
    var defaultIgn: String?
    var readyCount: Int?
    var connectedCount: Int?
    var accounts: [AccountInfo]
}

struct Alert: Codable, Equatable, Identifiable {
    var ts: String
    var level: String   // warn | error
    var message: String
    var id: String { ts + message }
}

// MARK: - Profit

struct ProfitSummary: Codable, Equatable {
    var totalProfit: Double
    var bought: Int
    var sold: Int
    var purse: Double
    var accounts: [ProfitAccount]
    var lifetime: LifetimeFigures?

    /// Prefer lifetime (ledger) totals, which persist across restarts.
    var displayProfit: Double { lifetime?.totalProfit ?? totalProfit }
    var displayBought: Int { max(lifetime?.bought ?? 0, bought) }
    var displaySold: Int { max(lifetime?.sold ?? 0, sold) }
}

struct LifetimeFigures: Codable, Equatable {
    var totalProfit: Double
    var bought: Int
    var sold: Int
}

struct ProfitAccount: Codable, Equatable, Identifiable {
    var ign: String
    var summary: ProfitFigures
    var id: String { ign }
}

struct ProfitFigures: Codable, Equatable {
    var totalProfit: Double
    var bought: Int
    var sold: Int
    var userFinderFlips: Int
    var profitPerHour: Double?
    var purse: Double?
}

struct ProfitSeries: Codable, Equatable {
    var bucketMs: UInt64
    var points: [ProfitPoint]
}

struct ProfitPoint: Codable, Equatable, Identifiable {
    var ts: UInt64
    var profit: Double
    var cumulative: Double
    var count: Int
    var id: UInt64 { ts }
}

// MARK: - Flips (ledger)

struct FlipRecord: Codable, Equatable, Identifiable {
    var ts: UInt64
    var account: String
    var item: String
    var weirdItemName: String?
    var tag: String?
    var price: UInt64
    var targetPrice: Double
    var profit: Double
    var finder: String
    var volume: Double?
    var profitPercentage: Double?
    var buyKind: String?
    var buySpeedMs: UInt64?
    var auctionId: String
    var id: String { "\(account)-\(auctionId)-\(ts)" }
}

struct SaleRecord: Codable, Equatable, Identifiable {
    var ts: UInt64
    var account: String
    var item: String
    var buyer: String
    var price: UInt64
    var id: String { "\(account)-\(item)-\(ts)" }
}

struct FlipsResponse: Codable { var kind: String; var flips: [FlipRecord] }
struct SalesResponse: Codable { var kind: String; var flips: [SaleRecord] }

// MARK: - Queue

struct QueueEntry: Codable, Equatable, Identifiable {
    var action: JSONValue
    var state: String
    var priority: Int
    var id: String { "\(priority)-\(state)-\(action.stringField("auctionID") ?? action.stringField("itemUuid") ?? "")" }

    var auctionId: String? { action.stringField("auctionID") ?? action.stringField("auctionId") }
    var itemName: String? { action.stringField("itemName") ?? action.stringField("item_name") }
    var price: Double? { action.numberField("price") }
}

struct QueueResponse: Codable { var ign: String; var queue: [QueueEntry]; var bidData: JSONValue? }

// MARK: - Commands

struct CommandDefinition: Codable, Equatable, Identifiable {
    var name: String
    var description: String
    var options: [CommandOption]
    var id: String { name }
}

struct CommandOption: Codable, Equatable, Identifiable {
    var name: String
    var description: String
    var kind: String
    var required: Bool
    var choices: [CommandChoice]
    var id: String { name }
}

struct CommandChoice: Codable, Equatable, Identifiable {
    var name: String
    var value: String
    var id: String { value }
}

struct CommandsResponse: Codable { var commands: [CommandDefinition] }

// MARK: - Command execution result

struct CommandResult: Codable {
    var ok: Bool?
    var outcome: JSONValue?
    var error: String?
    var message: String?
    var requiresConfirmation: Bool?
    var confirm: ConfirmAction?
    var action: String?
}

struct ConfirmAction: Codable {
    var title: String
    var message: String
    var button: String
}

// MARK: - Live events

struct LiveEvent: Identifiable {
    let id = UUID()
    var type: String
    var ts: UInt64
    var raw: JSONValue
}

// MARK: - JSONValue (arbitrary JSON)

enum JSONValue: Codable, Equatable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: JSONValue])
    case array([JSONValue])
    case null

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(Double.self) { self = .number(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([String: JSONValue].self) { self = .object(v) }
        else if let v = try? c.decode([JSONValue].self) { self = .array(v) }
        else { self = .null }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .string(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .bool(let v): try c.encode(v)
        case .object(let v): try c.encode(v)
        case .array(let v): try c.encode(v)
        case .null: try c.encodeNil()
        }
    }

    func stringField(_ key: String) -> String? {
        if case .object(let dict) = self, case .string(let v)? = dict[key] { return v }
        return nil
    }
    func numberField(_ key: String) -> Double? {
        if case .object(let dict) = self {
            if case .number(let v)? = dict[key] { return v }
            if case .string(let v)? = dict[key] { return Double(v) }
        }
        return nil
    }
    var asString: String? { if case .string(let v) = self { return v }; return nil }
    subscript(_ key: String) -> JSONValue? {
        if case .object(let dict) = self { return dict[key] }
        return nil
    }
}
