import AppKit
import SwiftUI

/// Loads official SkyBlock item icons keyed by item tag, from Coflnet's static
/// icon CDN (the same data source the bot already uses). Two-level cache:
/// in-memory NSCache + on-disk so icons render instantly after first fetch and
/// keep working offline.
@MainActor
final class IconCache {
    static let shared = IconCache()

    private let memory = NSCache<NSString, NSImage>()
    private var inFlight: Set<String> = []
    private let diskDir: URL

    init() {
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("SAF Dashboard/icons", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        diskDir = base
        memory.countLimit = 600
    }

    private func diskURL(for tag: String) -> URL {
        diskDir.appendingPathComponent(tag.replacingOccurrences(of: "/", with: "_") + ".png")
    }

    static func iconURL(tag: String) -> URL? {
        URL(string: "https://sky.coflnet.com/static/icon/\(tag)")
    }

    func cached(_ tag: String) -> NSImage? {
        if let image = memory.object(forKey: tag as NSString) { return image }
        let url = diskURL(for: tag)
        if let data = try? Data(contentsOf: url), let image = NSImage(data: data) {
            memory.setObject(image, forKey: tag as NSString)
            return image
        }
        return nil
    }

    func load(_ tag: String) async -> NSImage? {
        if let image = cached(tag) { return image }
        guard !inFlight.contains(tag), let url = Self.iconURL(tag: tag) else { return nil }
        inFlight.insert(tag)
        defer { inFlight.remove(tag) }
        guard let (data, response) = try? await URLSession.shared.data(from: url),
            let http = response as? HTTPURLResponse, http.statusCode == 200,
            let image = NSImage(data: data)
        else { return nil }
        memory.setObject(image, forKey: tag as NSString)
        try? data.write(to: diskURL(for: tag))
        return image
    }
}

/// SwiftUI view that renders an item's icon by tag with a tasteful placeholder.
struct ItemIcon: View {
    let tag: String?
    var size: CGFloat = 34
    @State private var image: NSImage?

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.26, style: .continuous)
                .fill(Color.white.opacity(0.05))
                .overlay(
                    RoundedRectangle(cornerRadius: size * 0.26, style: .continuous)
                        .strokeBorder(Theme.cardStroke, lineWidth: 1))
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.medium)
                    .scaledToFit()
                    .padding(size * 0.12)
            } else {
                Image(systemName: "cube.fill")
                    .font(.system(size: size * 0.38))
                    .foregroundStyle(Theme.textTertiary)
            }
        }
        .frame(width: size, height: size)
        .task(id: tag) {
            image = nil
            guard let tag, !tag.isEmpty else { return }
            if let cached = IconCache.shared.cached(tag) {
                image = cached
            } else {
                image = await IconCache.shared.load(tag)
            }
        }
    }
}
