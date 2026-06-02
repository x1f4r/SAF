import AppKit
import SwiftUI

/// Connection health, bot alerts, and a troubleshooting log — the place to look
/// when something fails. Mirrors the web dashboard's Diagnostics view.
struct DiagnosticsView: View {
    @EnvironmentObject var store: AppStore

    private var stateColor: Color {
        switch store.healthState {
        case "connected": return Theme.profit
        case "degraded": return Theme.gold
        case "connecting": return Theme.warn
        default: return Theme.loss
        }
    }

    var body: some View {
        Page(spacing: 22) {
            PageHeader("Diagnostics", subtitle: "Connection health, bot alerts, and a troubleshooting log.") {
                GhostButton(title: "Copy report", systemImage: "doc.on.doc") { copyReport() }
            }

            healthStrip

            HStack(alignment: .top, spacing: 40) {
                botAlerts.frame(maxWidth: .infinity, alignment: .leading)
                connectionLog.frame(width: 360)
            }
        }
    }

    private var healthStrip: some View {
        HStack(spacing: 26) {
            HealthItem(icon: "link", label: "Connection", value: store.healthState.capitalized, color: stateColor)
            HealthItem(icon: "antenna.radiowaves.left.and.right", label: "API",
                value: store.reachable ? "Reachable" : "Unreachable", color: store.reachable ? Theme.profit : Theme.loss)
            HealthItem(icon: "dot.radiowaves.left.and.right", label: "Live feed",
                value: store.stream.connected ? "Streaming" : "Off", color: store.stream.connected ? Theme.profit : Theme.warn)
            HealthItem(icon: "bolt.fill", label: "Bot", value: store.status?.name ?? "—", color: Theme.accent)
            HealthItem(icon: "person.2.fill", label: "Accounts ready",
                value: "\(store.readyCount)/\(store.configuredCount)", color: store.readyCount > 0 ? Theme.profit : Theme.warn)
            Spacer(minLength: 0)
        }
    }

    private var botAlerts: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionLabel("Bot Alerts", systemImage: "exclamationmark.triangle.fill") {
                Text("\(store.alerts.count)").font(.rounded(11, .medium)).foregroundStyle(Theme.textTertiary)
            }
            if store.alerts.isEmpty {
                EmptyState(icon: "checkmark.seal.fill", text: "No warnings or errors from the bot.").frame(height: 150)
            } else {
                VStack(spacing: 0) {
                    ForEach(store.alerts.reversed().prefix(60)) { alert in
                        AlertRow(alert: alert)
                    }
                }
            }
        }
    }

    private var connectionLog: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionLabel("Connection Log", systemImage: "waveform.path.ecg")
            if store.diag.isEmpty {
                EmptyState(icon: "waveform.path.ecg", text: "No connection events yet.").frame(height: 150)
            } else {
                VStack(spacing: 0) {
                    ForEach(store.diag.prefix(80)) { event in
                        DiagRow(event: event)
                    }
                }
            }
        }
    }

    private func copyReport() {
        var lines = [
            "# SAF Dashboard diagnostics — \(Date().ISO8601Format())",
            "connection=\(store.healthState) reachable=\(store.reachable) liveFeed=\(store.stream.connected) bot=\(store.status?.name ?? "—") accountsReady=\(store.readyCount)/\(store.configuredCount)",
            "", "## Connection log",
        ]
        lines += store.diag.map { "\($0.ts.ISO8601Format()) \($0.level.uppercased()) \($0.message)" }
        lines += ["", "## Bot alerts"]
        lines += store.alerts.map { "\($0.ts) \($0.level.uppercased()) \($0.message)" }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(lines.joined(separator: "\n"), forType: .string)
        store.toast = ToastMessage(icon: "doc.on.doc", tint: Theme.accent, title: "Diagnostics copied", detail: "Paste it into a bug report")
    }
}

private struct HealthItem: View {
    let icon: String
    let label: String
    let value: String
    let color: Color
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon).font(.system(size: 15, weight: .semibold)).foregroundStyle(color)
                .frame(width: 32, height: 32).background(Circle().fill(color.opacity(0.15)))
            VStack(alignment: .leading, spacing: 1) {
                Text(label.uppercased()).font(.rounded(9, .semibold)).tracking(0.4).foregroundStyle(Theme.textTertiary)
                Text(value).font(.rounded(12.5, .semibold)).foregroundStyle(Theme.textPrimary)
            }
        }
    }
}

private struct AlertRow: View {
    let alert: Alert
    private var color: Color { alert.level == "error" ? Theme.loss : Theme.warn }
    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Circle().fill(color).frame(width: 7, height: 7).padding(.top, 5)
            VStack(alignment: .leading, spacing: 1) {
                Text(cleanMessage(alert.message))
                    .font(.system(size: 11.5, weight: .regular, design: .monospaced))
                    .foregroundStyle(Theme.textPrimary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(relativeTime(alert.ts)).font(.rounded(10, .medium)).foregroundStyle(Theme.textTertiary)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 8).padding(.horizontal, 4)
        .overlay(Hairline(opacity: 0.5), alignment: .bottom)
    }
}

private struct DiagRow: View {
    let event: AppStore.DiagEvent
    private var color: Color {
        switch event.level { case "error": return Theme.loss; case "warn": return Theme.warn; default: return Theme.accent }
    }
    var body: some View {
        HStack(alignment: .top, spacing: 9) {
            Circle().fill(color).frame(width: 7, height: 7).padding(.top, 4)
            VStack(alignment: .leading, spacing: 1) {
                Text(event.message).font(.rounded(12, .regular)).foregroundStyle(Theme.textPrimary)
                Text(clock(event.ts)).font(.numeric(10, .medium)).foregroundStyle(Theme.textTertiary)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 7).padding(.horizontal, 4)
        .overlay(Hairline(opacity: 0.5), alignment: .bottom)
    }
}

private func cleanMessage(_ message: String) -> String {
    // Drop the leading "<ts> LEVEL target:" prefix for readability.
    if let range = message.range(of: #"^\S+\s+(WARN|ERROR)\s+"#, options: .regularExpression) {
        return String(message[range.upperBound...])
    }
    return message
}

private func relativeTime(_ iso: String) -> String {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = f.date(from: iso) ?? ISO8601DateFormatter().date(from: iso) {
        return Fmt.relative(fromMs: UInt64(date.timeIntervalSince1970 * 1000))
    }
    return iso
}

private func clock(_ date: Date) -> String {
    let f = DateFormatter(); f.dateFormat = "HH:mm:ss"
    return f.string(from: date)
}
