import Foundation

public struct ProviderConfiguration: Codable, Sendable, Equatable {
    public var name = "openai-compatible"
    public var baseURL = "https://api.openai.com/v1"
    public var model = "gpt-4o"
    public init() {}
    public var resolvedBaseURL: String {
        name == "gemini" && baseURL == "https://api.openai.com/v1"
            ? "https://generativelanguage.googleapis.com/v1beta/openai" : baseURL
    }
}

public struct SpeechConfiguration: Codable, Sendable, Equatable {
    public var enabled = false
    public var baseURL = "https://api.openai.com/v1"
    public var model = "whisper-1"
    public var voice = "alloy"
    public var ttsModel = "gpt-4o-mini-tts"
    public var ttsBaseURL = "https://api.openai.com/v1"
    public var readReplies = false
    public var useSystemVoice = true
    public init() {}
}

public enum ThemeStyle: String, Codable, CaseIterable, Sendable { case carbonDark, obsidianOLED, slateMidnight, studioLight }
public enum AccentColor: String, Codable, CaseIterable, Sendable { case white, cyan, emerald, violet, amber }
public enum WindowPosition: String, Codable, CaseIterable, Sendable { case center, upperThird }

public struct AppConfiguration: Codable, Sendable, Equatable {
    public var provider = ProviderConfiguration()
    public var speech = SpeechConfiguration()
    public var theme: ThemeStyle = .carbonDark
    public var accent: AccentColor = .white
    public var windowPosition: WindowPosition = .upperThird
    public var hotkey = "Double Command"
    public var startHidden = true
    public var soundEnabled = true
    public var memoryEnabled = true
    public var searchEnabled = false
    public var requestDelayMilliseconds = 0
    public var inputTokenLimit = 32_000
    public var contextOptimizationEnabled = true
    public var maxSteps = 40
    public init() {}
    public func validate() throws {
        _ = try validatedHTTPURL(provider.resolvedBaseURL)
        _ = try validatedHTTPURL(speech.baseURL)
        _ = try validatedHTTPURL(speech.ttsBaseURL)
        guard !provider.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              (1...100).contains(maxSteps), (2_000...1_000_000).contains(inputTokenLimit),
              (0...10_000).contains(requestDelayMilliseconds),
              ["Double Command", "Command+Space", "Control+Space", "Option+Space"].contains(hotkey) else {
            throw FunctionError.invalidInput("Check the model, shortcut, action limit, delay, and context budget.")
        }
    }
}

public func validatedHTTPURL(_ text: String) throws -> URL {
    guard let url = URL(string: text), let scheme = url.scheme?.lowercased(),
          ["https", "http"].contains(scheme), url.host != nil, url.user == nil, url.password == nil else {
        throw FunctionError.invalidInput("Use an HTTP or HTTPS URL without embedded credentials.")
    }
    return url
}

public enum FunctionPaths {
    // Separate native storage preserves the Rust files for explicit migration.
    public static var directory: URL { FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".function/swift", isDirectory: true) }
}

public struct JSONFileStore: Sendable {
    public let directory: URL
    public init(directory: URL = FunctionPaths.directory) { self.directory = directory }
    public func load<T: Decodable>(_ type: T.Type, from name: String, default fallback: T) throws -> T {
        let path = directory.appendingPathComponent(name)
        guard FileManager.default.fileExists(atPath: path.path) else { return fallback }
        return try JSONDecoder().decode(type, from: Data(contentsOf: path))
    }
    public func save<T: Encodable>(_ value: T, to name: String) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let destination = directory.appendingPathComponent(name)
        try encoder.encode(value).write(to: destination, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: destination.path)
    }
}
