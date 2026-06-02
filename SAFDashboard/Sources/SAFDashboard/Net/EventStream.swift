import Foundation

/// Live WebSocket feed of buys/sells/notifications/state from the bot. Emits
/// decoded events to a callback on the main actor and reconnects on drop.
@MainActor
final class EventStream: ObservableObject {
    @Published private(set) var connected = false

    private var task: URLSessionWebSocketTask?
    private var session: URLSession?
    private var shouldRun = false
    private var url: URL?
    private var token: String = ""
    var onEvent: ((LiveEvent) -> Void)?

    func start(url: URL, token: String) {
        stop()
        self.url = url
        self.token = token
        shouldRun = true
        connect()
    }

    func stop() {
        shouldRun = false
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        session?.invalidateAndCancel()
        session = nil
        connected = false
    }

    private func connect() {
        guard shouldRun, let url else { return }
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 0
        let session = URLSession(configuration: config)
        self.session = session
        var req = URLRequest(url: url)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let task = session.webSocketTask(with: req)
        self.task = task
        task.resume()
        connected = true
        receive()
    }

    private func receive() {
        task?.receive { [weak self] result in
            Task { @MainActor in
                guard let self else { return }
                switch result {
                case .success(let message):
                    self.handle(message)
                    self.receive()
                case .failure:
                    self.connected = false
                    guard self.shouldRun else { return }
                    try? await Task.sleep(nanoseconds: 1_500_000_000)
                    if self.shouldRun { self.connect() }
                }
            }
        }
    }

    private func handle(_ message: URLSessionWebSocketTask.Message) {
        let text: String?
        switch message {
        case .string(let s): text = s
        case .data(let d): text = String(data: d, encoding: .utf8)
        @unknown default: text = nil
        }
        guard let text, let data = text.data(using: .utf8),
            let value = try? JSONDecoder().decode(JSONValue.self, from: data)
        else { return }
        let type = value.stringField("type") ?? "unknown"
        let ts = UInt64(value.numberField("ts") ?? 0)
        onEvent?(LiveEvent(type: type, ts: ts, raw: value))
    }
}
