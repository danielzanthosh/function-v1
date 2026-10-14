#if os(macOS)
import AppKit
import FunctionCore

public struct LauncherItem: Identifiable, Sendable, Equatable {
    public let url: URL
    public init(url: URL) { self.url = url }
    public var id: String { url.path }
    public var title: String { url.deletingPathExtension().lastPathComponent }
    public var isApplication: Bool { url.pathExtension == "app" }
}

@MainActor
public final class LauncherIndex {
    private var applications: [LauncherItem] = []
    private var indexed = false
    public init() {}
    public func search(_ queryText: String) async -> [LauncherItem] {
        if !indexed {
            indexed = true
            applications = ["/Applications", "/System/Applications", "/System/Applications/Utilities", NSHomeDirectory() + "/Applications"].flatMap {
                ((try? FileManager.default.contentsOfDirectory(at: URL(fileURLWithPath: $0), includingPropertiesForKeys: nil)) ?? [])
                    .filter { $0.pathExtension == "app" }.map { LauncherItem(url: $0) }
            }
        }
        let text = queryText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !text.hasPrefix("/"), !text.hasPrefix(">") else { return [] }
        let apps = applications.filter { $0.title.localizedCaseInsensitiveContains(text) }.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        let query = NSMetadataQuery()
        query.searchScopes = [NSMetadataQueryUserHomeScope]
        query.predicate = NSPredicate(format: "%K CONTAINS[cd] %@", "kMDItemDisplayName", text)
        guard query.start() else { return Array(apps.prefix(6)) }
        defer { query.stop() }
        let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(1))
        while query.isGathering && clock.now < deadline {
            do { try await Task.sleep(for: .milliseconds(50)) } catch { return [] }
        }
        guard !Task.isCancelled else { return [] }
        query.disableUpdates()
        let files = (0..<min(query.resultCount, 20)).compactMap { index -> LauncherItem? in
            guard let result = query.result(at: index) as? NSMetadataItem,
                  let path = result.value(forAttribute: NSMetadataItemPathKey) as? String,
                  !path.contains("/Library/Keychains/"), !path.contains("/.ssh/"), !path.contains("/.function/") else { return nil }
            return LauncherItem(url: URL(fileURLWithPath: path))
        }
        var seen: Set<String> = []
        return Array((apps + files).filter { seen.insert($0.id).inserted }.prefix(6))
    }
}
#endif
