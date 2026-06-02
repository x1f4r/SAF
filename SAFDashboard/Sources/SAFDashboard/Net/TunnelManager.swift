import Foundation

/// Manages a persistent `ssh -L` tunnel to the bot's loopback API. Auto-restarts
/// with backoff if ssh drops (autossh-style) so the desktop app survives flaky
/// networks and VPS reboots without manual intervention.
@MainActor
final class TunnelManager: ObservableObject {
    enum State: Equatable {
        case idle
        case connecting
        case up
        case retrying(String)
        case failed(String)
    }

    @Published private(set) var state: State = .idle

    private var process: Process?
    private var profile: ConnectionProfile?
    private var shouldRun = false
    private var retryTask: Task<Void, Never>?
    private var attempt = 0

    var isActive: Bool { shouldRun }

    func start(_ profile: ConnectionProfile) {
        stop()
        self.profile = profile
        shouldRun = true
        attempt = 0
        // Direct mode: no SSH tunnel, the API is already reachable on loopback.
        guard profile.usesTunnel else {
            state = .up
            return
        }
        launch()
    }

    func stop() {
        shouldRun = false
        retryTask?.cancel()
        retryTask = nil
        if let process, process.isRunning {
            process.terminationHandler = nil
            process.terminate()
        }
        process = nil
        state = .idle
    }

    private func launch() {
        guard shouldRun, let profile else { return }
        state = attempt == 0 ? .connecting : .retrying("reconnecting…")

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
        proc.arguments = sshArguments(for: profile)
        // ssh should never read a password prompt; fail fast instead of hanging.
        var env = ProcessInfo.processInfo.environment
        env["SSH_ASKPASS"] = "/usr/bin/false"
        proc.environment = env
        let pipe = Pipe()
        proc.standardError = pipe
        proc.standardOutput = pipe

        proc.terminationHandler = { [weak self] process in
            Task { @MainActor in
                self?.handleExit(status: process.terminationStatus)
            }
        }

        do {
            try proc.run()
            process = proc
            // ExitOnForwardFailure makes ssh exit quickly if the forward fails;
            // if it is still alive shortly after launch the tunnel is up.
            Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: 1_200_000_000)
                guard let self, self.shouldRun, self.process === proc, proc.isRunning else { return }
                self.attempt = 0
                self.state = .up
            }
        } catch {
            handleExit(status: -1, error: error.localizedDescription)
        }
    }

    private func sshArguments(for profile: ConnectionProfile) -> [String] {
        var args = [
            "-N", "-T",
            "-o", "ExitOnForwardFailure=yes",
            "-o", "ServerAliveInterval=15",
            "-o", "ServerAliveCountMax=3",
            "-o", "ConnectTimeout=12",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "BatchMode=yes",
        ]
        if profile.sshPort > 0 { args += ["-p", "\(profile.sshPort)"] }
        let identity = profile.identityFile.trimmingCharacters(in: .whitespaces)
        if !identity.isEmpty {
            args += ["-i", (identity as NSString).expandingTildeInPath, "-o", "IdentitiesOnly=yes"]
        }
        args += [
            "-L", "127.0.0.1:\(profile.localPort):127.0.0.1:\(profile.remotePort)",
            profile.sshDestination,
        ]
        return args
    }

    private func handleExit(status: Int32, error: String? = nil) {
        process = nil
        guard shouldRun else { return }
        attempt += 1
        let reason = error ?? "ssh exited (\(status))"
        let delay = min(2.0 * Double(attempt), 20.0)
        state = .retrying(reason)
        retryTask?.cancel()
        retryTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard let self, self.shouldRun, !Task.isCancelled else { return }
            self.launch()
        }
    }
}
