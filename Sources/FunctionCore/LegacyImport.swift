import Foundation

public struct LegacyImport: Sendable {
    public var configuration: AppConfiguration
    public var aiKey: String?
    public var speechKey: String?
    public var ttsKey: String?
    public var memories: [MemoryItem]
    public var conversations: [Conversation]
    public init(directory: URL) throws {
        configuration = AppConfiguration(); aiKey = nil; speechKey = nil; ttsKey = nil; memories = []; conversations = []
        guard ["config.json", "memory.json", "conversations.json"].contains(where: {
            FileManager.default.fileExists(atPath: directory.appendingPathComponent($0).path)
        }) else { throw FunctionError.storage("No legacy settings, memory, or history were found.") }
        let decoder = JSONDecoder()
        let configURL = directory.appendingPathComponent("config.json")
        if FileManager.default.fileExists(atPath: configURL.path) {
            let wire = try decoder.decode(JSONValue.self, from: Data(contentsOf: configURL))
            guard case .object = wire else { throw FunctionError.storage("Legacy settings have an unexpected format.") }
            let provider = wire["ai_provider"]
            configuration.provider.name = provider["provider_name"].string ?? configuration.provider.name
            configuration.provider.baseURL = provider["base_url"].string ?? configuration.provider.baseURL
            configuration.provider.model = provider["model"].string ?? configuration.provider.model
            aiKey = provider["api_key"].string
            let tts = wire["tts"]
            ttsKey = tts["api_key"].string
            configuration.speech.ttsBaseURL = tts["base_url"].string ?? configuration.speech.ttsBaseURL
            configuration.speech.ttsModel = tts["model"].string ?? configuration.speech.ttsModel
            configuration.speech.voice = tts["voice"].string ?? configuration.speech.voice
            if case .bool(let enabled) = tts["enabled"] { configuration.speech.readReplies = enabled; configuration.speech.useSystemVoice = false }
            let speech = wire["speech"]
            speechKey = speech["api_key"].string
            configuration.speech.baseURL = speech["base_url"].string ?? configuration.speech.baseURL
            configuration.speech.model = speech["model"].string ?? configuration.speech.model
            if case .bool(let enabled) = speech["enabled"] { configuration.speech.enabled = enabled }
            if case .bool(let enabled) = wire["memory"]["enabled"] { configuration.memoryEnabled = enabled }
            if case .bool(let enabled) = wire["search"]["enabled"] { configuration.searchEnabled = enabled }
            if case .bool(let enabled) = wire["sound_enabled"] { configuration.soundEnabled = enabled }
            if case .bool(let hidden) = wire["start_hidden"] { configuration.startHidden = hidden }
            switch wire["theme_style"].string {
            case "studiolight": configuration.theme = .studioLight
            case "obsidianoled": configuration.theme = .obsidianOLED
            case "slatemidnight": configuration.theme = .slateMidnight
            default: break
            }
            if let accent = wire["accent_color"].string.flatMap(AccentColor.init(rawValue:)) { configuration.accent = accent }
            if wire["window_position"].string == "center" { configuration.windowPosition = .center }
        }
        let memoryURL = directory.appendingPathComponent("memory.json")
        if FileManager.default.fileExists(atPath: memoryURL.path) {
            let wire = try decoder.decode(JSONValue.self, from: Data(contentsOf: memoryURL))
            guard case .array = wire else { throw FunctionError.storage("Legacy memory has an unexpected format.") }
            memories = try wire.array.map { item in
                guard let category = item["category"].string.flatMap(MemoryCategory.init(rawValue:)) else { throw FunctionError.storage("Unknown legacy memory category.") }
                var result = MemoryItem(category: category, key: try item.requiredString("key"), value: try item.requiredString("value"))
                result.id = try item.requiredString("id")
                result.createdAt = Date(timeIntervalSince1970: item["created_at"].number ?? 0)
                return result
            }
        }
        let conversationsURL = directory.appendingPathComponent("conversations.json")
        if FileManager.default.fileExists(atPath: conversationsURL.path) {
            let wire = try decoder.decode(JSONValue.self, from: Data(contentsOf: conversationsURL))
            guard case .array = wire else { throw FunctionError.storage("Legacy conversations have an unexpected format.") }
            conversations = try wire.array.map { saved in
                var result = Conversation()
                result.title = saved["title"].string ?? "Imported conversation"
                result.createdAt = Date(timeIntervalSince1970: saved["created_at"].number ?? 0)
                result.updatedAt = Date(timeIntervalSince1970: saved["updated_at"].number ?? 0)
                result.messages = try saved["display_messages"].array.map { entry in
                    let role: ChatMessage.Role = entry["is_user"] == .bool(true) ? .user : .assistant
                    return ChatMessage(role: role, content: try entry.requiredString("text"))
                }
                return result
            }
        }
        try configuration.validate()
    }
}
