import Foundation

/// Appends diagnostics lines to a rotating on-disk log so a troubleshooting
/// trail survives app restarts. Writes happen on a utility background queue and
/// fail silently on any I/O or permission error — logging must never disrupt
/// the app. Mirrors the web dashboard's persisted diagnostics history.
final class DiagnosticsLogger {
    static let shared = DiagnosticsLogger()

    /// One line per diagnostic event: `ISO8601 [LEVEL] message`.
    struct Entry {
        let ts: Date
        let level: String
        let message: String
    }

    /// Rotate once the active log crosses this size (1 MB) into `diagnostics.log.1`.
    private let rotateThreshold: UInt64 = 1_048_576
    private let queue = DispatchQueue(label: "com.x1f4r.safdashboard.diaglog", qos: .utility)
    private let iso = ISO8601DateFormatter()

    private init() {}

    /// The folder holding the diagnostics logs, created on demand. Used by the
    /// "Open logs folder" action in Diagnostics.
    var logsDirectory: URL {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("SAF Dashboard", isDirectory: true)
            .appendingPathComponent("logs", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private var logURL: URL { logsDirectory.appendingPathComponent("diagnostics.log") }
    private var rotatedURL: URL { logsDirectory.appendingPathComponent("diagnostics.log.1") }

    /// Append one event. Returns immediately; the write is performed on the
    /// background queue and any failure is swallowed.
    func append(_ entry: Entry) {
        let line = "\(iso.string(from: entry.ts)) [\(entry.level.uppercased())] \(entry.message)\n"
        queue.async { [weak self] in
            self?.write(line)
        }
    }

    func append(level: String, message: String) {
        append(Entry(ts: Date(), level: level, message: message))
    }

    // MARK: Private I/O (background queue only)

    private func write(_ line: String) {
        let url = logURL
        rotateIfNeeded(at: url)
        guard let data = line.data(using: .utf8) else { return }
        let fm = FileManager.default
        if !fm.fileExists(atPath: url.path) {
            try? data.write(to: url, options: .atomic)
            return
        }
        guard let handle = try? FileHandle(forWritingTo: url) else { return }
        defer { try? handle.close() }
        do {
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
        } catch {
            // Silent fallback: logging must never surface an error to the user.
        }
    }

    private func rotateIfNeeded(at url: URL) {
        let fm = FileManager.default
        guard let attrs = try? fm.attributesOfItem(atPath: url.path),
            let size = attrs[.size] as? UInt64, size >= rotateThreshold
        else { return }
        let backup = rotatedURL
        try? fm.removeItem(at: backup)
        try? fm.moveItem(at: url, to: backup)
    }
}
