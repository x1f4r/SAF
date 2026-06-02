import AppKit
import Foundation

/// Builds and runs the self-hosted web dashboard (the `web/` Docker context that
/// ships inside this app) via `docker compose`, so the user can spin up the
/// browser dashboard with one click. The container reaches the bot through this
/// Mac at host.docker.internal — piggybacking on the app's existing connection.
@MainActor
final class WebServerManager: ObservableObject {
    enum Status: Equatable {
        case checking
        case unavailable(String)
        case stopped
        case working          // building / starting / stopping
        case running
        case failed(String)
    }

    @Published private(set) var status: Status = .checking
    @Published private(set) var log: [String] = []
    @Published var port: Int { didSet { UserDefaults.standard.set(port, forKey: "webserver.port") } }
    @Published var password: String { didSet { UserDefaults.standard.set(password, forKey: "webserver.password") } }

    private var dockerURL: URL?
    private var busy = false

    var url: URL { URL(string: "http://localhost:\(port)")! }
    var isRunning: Bool { status == .running }

    init() {
        port = (UserDefaults.standard.object(forKey: "webserver.port") as? Int) ?? 8090
        let saved = UserDefaults.standard.string(forKey: "webserver.password")
        password = saved ?? Self.randomPassword()
        if saved == nil { UserDefaults.standard.set(password, forKey: "webserver.password") }
    }

    static func randomPassword() -> String {
        let chars = Array("abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789")
        return String((0..<16).map { _ in chars.randomElement()! })
    }

    private var workingDir: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("SAF Dashboard/web-server", isDirectory: true)
    }
    private var bundledWeb: URL? { Bundle.main.resourceURL?.appendingPathComponent("web") }

    // MARK: - Public actions

    func refresh() {
        Task { await refreshImpl() }
    }

    func start(botPort: Int, token: String) {
        guard !busy else { return }
        Task { await startImpl(botPort: botPort, token: token) }
    }

    func stop() {
        guard !busy else { return }
        Task { await stopImpl() }
    }

    func open() { NSWorkspace.shared.open(url) }

    // MARK: - Implementation

    private func refreshImpl() async {
        status = .checking
        locateDocker()
        guard let docker = dockerURL else {
            status = .unavailable("Docker not found. Install Docker Desktop to host the web dashboard.")
            return
        }
        let (code, _) = await capture(docker, ["version", "--format", "{{.Server.Version}}"])
        if code != 0 {
            status = .unavailable("Docker isn't running. Open Docker Desktop, then recheck.")
            return
        }
        let (_, out) = await capture(docker, ["ps", "--filter", "name=saf-dashboard", "--format", "{{.Names}}"])
        status = out.contains("saf-dashboard") ? .running : .stopped
    }

    private func startImpl(botPort: Int, token: String) async {
        guard let docker = dockerURL else { await refreshImpl(); return }
        busy = true
        defer { busy = false }
        log.removeAll()
        status = .working
        appendLog("Preparing the web build context…")
        do { try prepareContext() } catch {
            status = .failed("Could not stage web files: \(error.localizedDescription)")
            return
        }
        let env = [
            "WEB_PORT": "\(port)",
            "DASHBOARD_PASSWORD": password,
            "BOT_API_TOKEN": token,
            "BOT_API_URL": "http://host.docker.internal:\(botPort)",
        ]
        appendLog("Building and starting the container on port \(port) (first run can take a minute)…")
        let code = await stream(docker, ["compose", "-p", "saf-dashboard", "up", "-d", "--build"], env: env)
        if code != 0 {
            status = .failed("docker compose exited with code \(code). See the log below.")
            return
        }
        appendLog("✓ Web dashboard is running at \(url.absoluteString)")
        status = .running
    }

    private func stopImpl() async {
        guard let docker = dockerURL else { return }
        busy = true
        defer { busy = false }
        status = .working
        appendLog("Stopping the web dashboard…")
        _ = await stream(docker, ["compose", "-p", "saf-dashboard", "down"], env: [:])
        appendLog("✓ Stopped.")
        status = .stopped
    }

    private func prepareContext() throws {
        guard let src = bundledWeb, FileManager.default.fileExists(atPath: src.path) else {
            throw NSError(domain: "saf", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Bundled web context not found in the app."])
        }
        let fm = FileManager.default
        try fm.createDirectory(at: workingDir.deletingLastPathComponent(), withIntermediateDirectories: true)
        if fm.fileExists(atPath: workingDir.path) { try fm.removeItem(at: workingDir) }
        try fm.copyItem(at: src, to: workingDir)
    }

    private func locateDocker() {
        let candidates = [
            "/usr/local/bin/docker",
            "/opt/homebrew/bin/docker",
            "/Applications/Docker.app/Contents/Resources/bin/docker",
        ]
        for c in candidates where FileManager.default.isExecutableFile(atPath: c) {
            dockerURL = URL(fileURLWithPath: c)
            return
        }
        // Fall back to a login shell so we pick up a custom install location.
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/zsh")
        proc.arguments = ["-lc", "command -v docker"]
        let pipe = Pipe()
        proc.standardOutput = pipe
        if (try? proc.run()) != nil {
            proc.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let path = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !path.isEmpty, FileManager.default.isExecutableFile(atPath: path) {
                dockerURL = URL(fileURLWithPath: path)
            }
        }
    }

    /// Small-output command; reads after exit. Use only for short outputs.
    private func capture(_ exe: URL, _ args: [String]) async -> (Int32, String) {
        await withCheckedContinuation { cont in
            let proc = makeProcess(exe, args, env: [:])
            let pipe = Pipe()
            proc.standardOutput = pipe
            proc.standardError = pipe
            proc.terminationHandler = { p in
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                cont.resume(returning: (p.terminationStatus, String(data: data, encoding: .utf8) ?? ""))
            }
            do { try proc.run() } catch { cont.resume(returning: (-1, error.localizedDescription)) }
        }
    }

    /// Long-running command; streams lines into `log` live.
    private func stream(_ exe: URL, _ args: [String], env: [String: String]) async -> Int32 {
        await withCheckedContinuation { cont in
            let proc = makeProcess(exe, args, env: env)
            let pipe = Pipe()
            proc.standardOutput = pipe
            proc.standardError = pipe
            pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
                let data = handle.availableData
                guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
                let lines = text.split(whereSeparator: \.isNewline).map(String.init)
                Task { @MainActor in lines.forEach { self?.appendLog($0) } }
            }
            proc.terminationHandler = { p in
                pipe.fileHandleForReading.readabilityHandler = nil
                cont.resume(returning: p.terminationStatus)
            }
            do { try proc.run() } catch {
                Task { @MainActor in self.appendLog("launch error: \(error.localizedDescription)") }
                cont.resume(returning: -1)
            }
        }
    }

    private func makeProcess(_ exe: URL, _ args: [String], env: [String: String]) -> Process {
        let proc = Process()
        proc.executableURL = exe
        proc.arguments = args
        // `compose` needs the staged context as cwd; status checks run before it
        // exists, and launching with a missing cwd would fail.
        if FileManager.default.fileExists(atPath: workingDir.path) {
            proc.currentDirectoryURL = workingDir
        }
        var environment = ProcessInfo.processInfo.environment
        environment["PATH"] = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:" + (environment["PATH"] ?? "")
        for (k, v) in env { environment[k] = v }
        proc.environment = environment
        return proc
    }

    private func appendLog(_ line: String) {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        log.append(trimmed)
        if log.count > 400 { log.removeFirst(log.count - 400) }
    }
}
