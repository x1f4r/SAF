import SwiftUI

/// Central design system. Dark, glassy, with a cyan→violet brand accent and
/// finance-grade semantic colors. Everything visual derives from here so the
/// app feels cohesive.
enum Theme {
    // MARK: Brand + accent
    static let accent = Color(hex: 0x5EC8FF)
    static let accent2 = Color(hex: 0x8B7BFF)
    static let brandGradient = LinearGradient(
        colors: [accent, accent2], startPoint: .topLeading, endPoint: .bottomTrailing)
    static let brandGradientH = LinearGradient(
        colors: [accent, accent2], startPoint: .leading, endPoint: .trailing)

    // MARK: Semantic
    static let profit = Color(hex: 0x37E0A6)
    static let loss = Color(hex: 0xFF6B6B)
    static let gold = Color(hex: 0xFFC861)
    static let warn = Color(hex: 0xFFB23E)

    static func pnl(_ value: Double) -> Color { value >= 0 ? profit : loss }

    // MARK: Surfaces
    static let bgTop = Color(hex: 0x0B0E14)
    static let bgBottom = Color(hex: 0x06070B)
    static var appBackground: some View {
        ZStack {
            LinearGradient(colors: [bgTop, bgBottom], startPoint: .top, endPoint: .bottom)
            RadialGradient(
                colors: [accent.opacity(0.10), .clear], center: .topLeading,
                startRadius: 1, endRadius: 760)
            RadialGradient(
                colors: [accent2.opacity(0.10), .clear], center: .bottomTrailing,
                startRadius: 1, endRadius: 820)
        }
        .ignoresSafeArea()
    }

    static let card = Color.white.opacity(0.04)
    static let cardStroke = Color.white.opacity(0.08)
    static let cardStrokeStrong = Color.white.opacity(0.14)
    static let sidebar = Color.black.opacity(0.18)

    static let textPrimary = Color.white.opacity(0.96)
    static let textSecondary = Color.white.opacity(0.62)
    static let textTertiary = Color.white.opacity(0.40)

    // MARK: Radii / spacing
    static let radius: CGFloat = 18
    static let radiusSm: CGFloat = 12
}

extension Color {
    init(hex: UInt32, alpha: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: alpha)
    }
}

extension Font {
    static func rounded(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .rounded)
    }
    static func numeric(_ size: CGFloat, _ weight: Font.Weight = .semibold) -> Font {
        .system(size: size, weight: weight, design: .rounded).monospacedDigit()
    }
}
