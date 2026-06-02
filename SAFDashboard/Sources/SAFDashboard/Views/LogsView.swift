import SwiftUI

struct LogsView: View {
    @EnvironmentObject var store: AppStore
    @State private var autoScroll = true
    @State private var filter = ""

    private var lines: [LogLine] {
        store.logLines.enumerated().map { LogLine(id: $0.offset, text: $0.element) }
            .filter { filter.isEmpty || $0.text.localizedCaseInsensitiveContains(filter) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            PageHeader("Console", subtitle: "Live tail of the bot log") {
                HStack(spacing: 10) {
                    SearchField(text: $filter)
                    Toggle("", isOn: $autoScroll).labelsHidden().toggleStyle(.switch).tint(Theme.accent)
                    Text("Auto-scroll").font(.rounded(11.5, .medium)).foregroundStyle(Theme.textSecondary)
                }
            }
            .padding(.horizontal, 40).padding(.top, 30)

            Card(padding: 0) {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(lines) { line in
                                LogRow(line: line.text)
                                    .id(line.id)
                            }
                            Color.clear.frame(height: 1).id(-1)
                        }
                        .padding(.vertical, 8)
                    }
                    .onChange(of: store.logLines.count) {
                        if autoScroll { withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(-1, anchor: .bottom) } }
                    }
                    .onAppear { proxy.scrollTo(-1, anchor: .bottom) }
                }
            }
            .padding(.horizontal, 40).padding(.bottom, 24)
        }
    }
}

private struct LogLine: Identifiable { let id: Int; let text: String }

struct LogRow: View {
    let line: String
    private var tint: Color {
        let l = line.lowercased()
        if l.contains("error") || l.contains("fail") { return Theme.loss }
        if l.contains("warn") { return Theme.warn }
        if l.contains("bought") || l.contains("profit") || l.contains("sold") { return Theme.profit }
        if l.contains("info") { return Theme.textSecondary }
        return Theme.textTertiary
    }
    var body: some View {
        Text(line)
            .font(.system(size: 11.5, weight: .regular, design: .monospaced))
            .foregroundStyle(tint)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16).padding(.vertical, 1.5)
    }
}
