import SwiftUI

// MARK: - Layout primitives

/// A hairline rule used to separate content instead of boxing it.
struct Hairline: View {
    var opacity: Double = 1
    var body: some View {
        Rectangle().fill(Theme.cardStroke.opacity(opacity)).frame(height: 1)
    }
}

/// A thin vertical rule for metric strips.
struct VRule: View {
    var height: CGFloat = 34
    var body: some View {
        Rectangle().fill(Theme.cardStroke).frame(width: 1, height: height)
    }
}

/// An intentional grouping surface — quiet, borderless, used sparingly for a
/// genuinely distinct functional zone (a chart, a feed). Not a default wrapper.
struct Surface<Content: View>: View {
    var padding: CGFloat = 20
    var tint: Color = .white
    @ViewBuilder var content: Content
    var body: some View {
        content
            .padding(padding)
            .background(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(tint.opacity(0.022))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .strokeBorder(Theme.cardStroke.opacity(0.7), lineWidth: 1)
            )
    }
}

/// Compatibility aliases so the whole app shares the quiet surface + editorial
/// label styling without per-call-site churn.
typealias Card = Surface
typealias SectionHeader = SectionLabel

/// Small editorial section label (uppercase, tracked).
struct SectionLabel<Trailing: View>: View {
    let title: String
    var systemImage: String?
    @ViewBuilder var trailing: Trailing
    init(_ title: String, systemImage: String? = nil, @ViewBuilder trailing: () -> Trailing) {
        self.title = title; self.systemImage = systemImage; self.trailing = trailing()
    }
    var body: some View {
        HStack(spacing: 7) {
            if let systemImage {
                Image(systemName: systemImage).font(.system(size: 11, weight: .bold)).foregroundStyle(Theme.accent)
            }
            Text(title.uppercased())
                .font(.rounded(11, .semibold)).tracking(1.1)
                .foregroundStyle(Theme.textTertiary)
            Spacer(minLength: 8)
            trailing
        }
    }
}

extension SectionLabel where Trailing == EmptyView {
    init(_ title: String, systemImage: String? = nil) {
        self.init(title, systemImage: systemImage) { EmptyView() }
    }
}

// MARK: - Metric (flat KPI, no box)

struct Metric: View {
    let label: String
    let value: String
    var sub: String? = nil
    var subColor: Color = Theme.textTertiary
    var color: Color = Theme.textPrimary
    var hero: Bool = false
    var body: some View {
        VStack(alignment: .leading, spacing: hero ? 8 : 6) {
            Text(label.uppercased())
                .font(.rounded(hero ? 11 : 10, .semibold)).tracking(0.8)
                .foregroundStyle(Theme.textTertiary)
            Text(value)
                .font(.numeric(hero ? 33 : 22, .bold))
                .foregroundStyle(color)
                .contentTransition(.numericText())
                .lineLimit(1).minimumScaleFactor(0.55)
            if let sub {
                Text(sub).font(.rounded(11, .medium)).foregroundStyle(subColor).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - Pills / status

struct Pill: View {
    let text: String
    var color: Color = Theme.accent
    var filled: Bool = false
    var body: some View {
        Text(text)
            .font(.rounded(11, .semibold))
            .foregroundStyle(filled ? Color.black.opacity(0.85) : color)
            .padding(.horizontal, 9).padding(.vertical, 3.5)
            .background(Capsule().fill(filled ? AnyShapeStyle(color) : AnyShapeStyle(color.opacity(0.14))))
    }
}

struct StatusDot: View {
    let color: Color
    var pulse: Bool = false
    @State private var on = false
    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 8, height: 8)
            .shadow(color: color.opacity(0.9), radius: on && pulse ? 6 : 3)
            .scaleEffect(on && pulse ? 1.18 : 1)
            .onAppear {
                guard pulse else { return }
                withAnimation(.easeInOut(duration: 1.1).repeatForever(autoreverses: true)) { on = true }
            }
    }
}

// MARK: - Buttons

struct PrimaryButton: View {
    let title: String
    var systemImage: String? = nil
    var tint: LinearGradient = Theme.brandGradientH
    var action: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                if let systemImage { Image(systemName: systemImage).font(.system(size: 12, weight: .bold)) }
                Text(title).font(.rounded(13, .semibold))
            }
            .foregroundStyle(Color.black.opacity(0.88))
            .padding(.horizontal, 15).padding(.vertical, 9)
            .background(Capsule().fill(tint))
            .shadow(color: Theme.accent.opacity(hover ? 0.4 : 0.22), radius: hover ? 12 : 6, y: 3)
        }
        .buttonStyle(.plain)
        .scaleEffect(hover ? 1.03 : 1)
        .onHover { hover = $0 }
        .animation(.spring(response: 0.25, dampingFraction: 0.7), value: hover)
    }
}

struct GhostButton: View {
    let title: String
    var systemImage: String? = nil
    var role: ButtonRole? = nil
    var action: () -> Void
    @State private var hover = false
    private var tint: Color { role == .destructive ? Theme.loss : Theme.textSecondary }
    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if let systemImage { Image(systemName: systemImage).font(.system(size: 12, weight: .semibold)) }
                Text(title).font(.rounded(13, .medium))
            }
            .foregroundStyle(hover ? (role == .destructive ? Theme.loss : Theme.textPrimary) : tint)
            .padding(.horizontal, 13).padding(.vertical, 8)
            .background(Capsule().fill(Color.white.opacity(hover ? 0.07 : 0.0)))
            .overlay(Capsule().strokeBorder(Theme.cardStroke, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .animation(.easeOut(duration: 0.15), value: hover)
    }
}

// MARK: - Flat list row chrome

/// A list row that highlights on hover and draws a bottom hairline — flat, no box.
struct ListRow<Content: View>: View {
    var showsSeparator: Bool = true
    var insets: EdgeInsets = EdgeInsets(top: 9, leading: 4, bottom: 9, trailing: 4)
    @ViewBuilder var content: Content
    @State private var hover = false
    var body: some View {
        VStack(spacing: 0) {
            content
                .padding(insets)
                .background(
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(Color.white.opacity(hover ? 0.045 : 0)))
                .contentShape(Rectangle())
                .onHover { hover = $0 }
            if showsSeparator { Hairline(opacity: 0.5).padding(.leading, insets.leading) }
        }
        .animation(.easeOut(duration: 0.12), value: hover)
    }
}

struct EmptyState: View {
    let icon: String
    let text: String
    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: icon).font(.system(size: 26, weight: .light)).foregroundStyle(Theme.textTertiary)
            Text(text).font(.rounded(12, .medium)).foregroundStyle(Theme.textTertiary).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
