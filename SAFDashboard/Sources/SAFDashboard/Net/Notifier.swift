import AppKit
import Foundation
import UserNotifications

/// Native macOS notifications for buys / sells / errors. Off by default; the
/// user enables it (which requests authorization). Mirrors the web app's
/// `notify.ts`: only fires while the app is in the background so it doesn't
/// double up with the in-app toast.
@MainActor
final class Notifier {
    static let shared = Notifier()

    private let key = "notify.enabled"
    private var center: UNUserNotificationCenter? {
        // UNUserNotificationCenter traps if the process isn't a bundled,
        // signed .app. Guard so SPM/CLI runs degrade gracefully.
        guard Bundle.main.bundleIdentifier != nil else { return nil }
        return UNUserNotificationCenter.current()
    }

    var supported: Bool { center != nil }
    var enabled: Bool { UserDefaults.standard.bool(forKey: key) }

    /// Requests authorization and, on success, flips the stored preference on.
    func enable() async -> Bool {
        guard let center else { return false }
        do {
            let granted = try await center.requestAuthorization(options: [.alert, .sound])
            UserDefaults.standard.set(granted, forKey: key)
            return granted
        } catch {
            UserDefaults.standard.set(false, forKey: key)
            return false
        }
    }

    func disable() {
        UserDefaults.standard.set(false, forKey: key)
    }

    /// Posts a notification, but only when the app is not the foreground app —
    /// otherwise the in-app toast already covers it.
    func show(title: String, body: String, tag: String? = nil) {
        guard enabled, let center, !NSApplication.shared.isActive else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        let request = UNNotificationRequest(
            identifier: tag ?? UUID().uuidString, content: content, trigger: nil)
        center.add(request, withCompletionHandler: nil)
    }
}
