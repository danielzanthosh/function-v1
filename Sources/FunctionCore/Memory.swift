import Foundation

public enum MemoryCategory: String, Codable, CaseIterable, Sendable {
    case userPreference = "user_preference", frequentApplication = "frequent_application"
    case frequentWebsite = "frequent_website", taskSummary = "task_summary", customInstruction = "custom_instruction"
}
public struct MemoryItem: Codable, Sendable, Identifiable, Equatable {
    public var id = UUID().uuidString
    public var category: MemoryCategory
    public var key: String
    public var value: String
    public var createdAt = Date()
    public init(category: MemoryCategory, key: String, value: String) { self.category = category; self.key = key; self.value = value }
}

public actor MemoryStore {
    private var items: [MemoryItem]
    private let storage: JSONFileStore
    public init(storage: JSONFileStore = JSONFileStore()) throws {
        self.storage = storage
        items = try storage.load([MemoryItem].self, from: "memory.json", default: [])
    }
    public func recall(_ query: String = "", category: MemoryCategory? = nil, limit: Int = 20) -> [MemoryItem] {
        let terms = query.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
        return items.filter { category == nil || $0.category == category }.compactMap { item -> (MemoryItem, Int)? in
            let text = "\(item.key) \(item.value)".lowercased()
            let score = terms.filter { text.contains($0) }.count
            return terms.isEmpty || score > 0 ? (item, score) : nil
        }.sorted { $0.1 == $1.1 ? $0.0.createdAt > $1.0.createdAt : $0.1 > $1.1 }.prefix(max(0, min(100, limit))).map(\.0)
    }
    public static func validate(_ item: MemoryItem) throws {
        guard !item.key.isEmpty, item.key.count <= 120, !item.value.isEmpty, item.value.count <= 1_000 else {
            throw FunctionError.invalidInput("Memory needs a label up to 120 characters and a note up to 1,000 characters.")
        }
        let text = "\(item.key) \(item.value)".lowercased()
        guard !["password", "api_key", "api key", "access token", "refresh token", "private key", "recovery phrase", "sk-"].contains(where: text.contains) else {
            throw FunctionError.permission("Do not store secrets in memory.")
        }
    }
    public func remember(_ item: MemoryItem) throws {
        try Self.validate(item)
        var updated = items.filter { !($0.category == item.category && $0.key == item.key) }
        updated.append(item)
        try persist(updated)
    }
    public func forget(_ id: String) throws { try persist(items.filter { $0.id != id }) }
    public func clear(category: MemoryCategory? = nil) throws { try persist(category.map { category in items.filter { $0.category != category } } ?? []) }
    private func persist(_ updated: [MemoryItem]) throws {
        try storage.save(updated, to: "memory.json")
        items = updated // Publish only after the atomic write succeeds.
    }
    public func execute(_ arguments: JSONValue) throws -> ToolResult {
        let action = try arguments.requiredString("action")
        let category = arguments["category"].string.flatMap(MemoryCategory.init(rawValue:))
        switch action {
        case "remember":
            guard let category else { throw FunctionError.invalidInput("Choose a valid memory category.") }
            let item = MemoryItem(category: category, key: try arguments.requiredString("key"), value: try arguments.requiredString("value"))
            try remember(item); return ToolResult("Remembered \(item.key) (id: \(item.id)).")
        case "recall":
            let requestedLimit = arguments["limit"].number ?? 20
            guard requestedLimit.isFinite, requestedLimit >= 1, requestedLimit <= 100 else {
                throw FunctionError.invalidInput("Memory limit must be between 1 and 100.")
            }
            let result = recall(arguments["query"].string ?? "", category: category, limit: Int(requestedLimit))
            return ToolResult(String(decoding: try JSONEncoder().encode(result), as: UTF8.self))
        case "forget": try forget(arguments.requiredString("id")); return ToolResult("Memory deleted.")
        case "clear_category":
            guard let category else { throw FunctionError.invalidInput("Choose a valid memory category.") }
            try clear(category: category); return ToolResult("Category cleared.")
        default: throw FunctionError.invalidInput("Unknown memory action.")
        }
    }
}
