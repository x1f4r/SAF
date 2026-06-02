import Foundation

enum Fmt {
    /// Compact SkyBlock-style coin formatting: 1.2B, 340.5M, 12.3k.
    static func coins(_ value: Double) -> String {
        let sign = value < 0 ? "-" : ""
        let v = abs(value)
        switch v {
        case 1_000_000_000_000...: return "\(sign)\(trim(v / 1_000_000_000_000))T"
        case 1_000_000_000...: return "\(sign)\(trim(v / 1_000_000_000))B"
        case 1_000_000...: return "\(sign)\(trim(v / 1_000_000))M"
        case 1_000...: return "\(sign)\(trim(v / 1_000))k"
        default: return "\(sign)\(Int(v.rounded()))"
        }
    }

    static func coins(_ value: Int) -> String { coins(Double(value)) }

    /// Coins with a leading glyph for display next to the gold accent.
    static func coinsGlyph(_ value: Double) -> String { coins(value) }

    private static func trim(_ v: Double) -> String {
        let r = (v * 10).rounded() / 10
        return r == r.rounded() ? String(Int(r)) : String(format: "%.1f", r)
    }

    static func int(_ value: Int) -> String {
        let f = NumberFormatter()
        f.numberStyle = .decimal
        return f.string(from: NSNumber(value: value)) ?? "\(value)"
    }

    static func signedCoins(_ value: Double) -> String {
        (value >= 0 ? "+" : "") + coins(value)
    }

    static func percent(_ value: Double?) -> String {
        guard let value else { return "—" }
        return String(format: "%.0f%%", value)
    }

    static func ms(_ value: Int?) -> String {
        guard let value else { return "—" }
        return value >= 1000 ? String(format: "%.1fs", Double(value) / 1000) : "\(value)ms"
    }

    /// "12s ago", "4m ago", "2h ago", "3d ago".
    static func relative(fromMs ms: UInt64) -> String {
        let then = Date(timeIntervalSince1970: Double(ms) / 1000)
        let delta = Date().timeIntervalSince(then)
        if delta < 5 { return "just now" }
        if delta < 60 { return "\(Int(delta))s ago" }
        if delta < 3600 { return "\(Int(delta / 60))m ago" }
        if delta < 86400 { return "\(Int(delta / 3600))h ago" }
        return "\(Int(delta / 86400))d ago"
    }

    static func clock(fromMs ms: UInt64) -> String {
        let date = Date(timeIntervalSince1970: Double(ms) / 1000)
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss"
        return f.string(from: date)
    }

    static func duration(ms: UInt64) -> String {
        let s = Int(ms / 1000)
        let h = s / 3600, m = (s % 3600) / 60
        if h > 24 { return "\(h / 24)d \(h % 24)h" }
        if h > 0 { return "\(h)h \(m)m" }
        return "\(m)m"
    }
}
