import Foundation

public indirect enum JSONValue: Codable, Sendable, Equatable {
    case string(String), number(Double), bool(Bool), object([String: JSONValue]), array([JSONValue]), null

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .bool(value) }
        else if let value = try? container.decode(Double.self) { self = .number(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode([String: JSONValue].self) { self = .object(value) }
        else { self = .array(try container.decode([JSONValue].self)) }
    }
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .string(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .bool(let value): try container.encode(value)
        case .object(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .null: try container.encodeNil()
        }
    }
    public subscript(_ key: String) -> JSONValue {
        guard case .object(let values) = self else { return .null }
        return values[key] ?? .null
    }
    public var string: String? { if case .string(let value) = self { value } else { nil } }
    public var number: Double? { if case .number(let value) = self { value } else { nil } }
    public var array: [JSONValue] { if case .array(let values) = self { values } else { [] } }
    public func requiredString(_ key: String) throws -> String {
        guard let value = self[key].string, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw FunctionError.invalidInput("Provide '\(key)'.")
        }
        return value
    }
    public func encodedString() throws -> String { String(decoding: try JSONEncoder().encode(self), as: UTF8.self) }
}

public enum FunctionError: LocalizedError, Sendable {
    case invalidInput(String), provider(String), permission(String), storage(String), stepLimit
    public var errorDescription: String? {
        switch self {
        case .invalidInput(let text), .provider(let text), .permission(let text), .storage(let text): text
        case .stepLimit: "Stopped after reaching the action limit. Review the activity before continuing."
        }
    }
}

public struct ToolCall: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var name: String
    public var arguments: JSONValue
    public var thoughtSignature: String?
    public init(id: String = UUID().uuidString, name: String, arguments: JSONValue, thoughtSignature: String? = nil) {
        self.id = id; self.name = name; self.arguments = arguments; self.thoughtSignature = thoughtSignature
    }
}

public struct ChatMessage: Codable, Sendable, Equatable, Identifiable {
    public enum Role: String, Codable, Sendable { case system, user, assistant, tool }
    public var id: UUID = UUID()
    public var role: Role
    public var content: String
    public var images: [String] = []
    public var isObservation = false
    public var toolCallID: String?
    public var toolCalls: [ToolCall] = []
    public var providerItems: [JSONValue] = []
    public init(role: Role, content: String, images: [String] = [], toolCallID: String? = nil, toolCalls: [ToolCall] = [], isObservation: Bool = false) {
        self.role = role; self.content = content; self.images = images; self.toolCallID = toolCallID; self.toolCalls = toolCalls; self.isObservation = isObservation
    }
}

public struct ToolDefinition: Sendable, Equatable {
    public var name: String
    public var description: String
    public var parameters: JSONValue
    public init(_ name: String, _ description: String, properties: [String: JSONValue], required: [String] = []) {
        self.name = name; self.description = description
        self.parameters = .object(["type": .string("object"), "properties": .object(properties),
                                   "required": .array(required.map(JSONValue.string)), "additionalProperties": .bool(false)])
    }
}

public struct ToolResult: Sendable, Equatable {
    public var text: String
    public var images: [String]
    public var success: Bool
    public init(_ text: String, images: [String] = [], success: Bool = true) {
        self.text = text; self.images = images; self.success = success
    }
}

public enum AgentState: Sendable, Equatable {
    case idle, listening, processing, acting(String), waitingForConfirmation, completed, failed(String)
    public var label: String {
        switch self {
        case .idle: "Ready"
        case .listening: "Listening"
        case .processing: "Thinking"
        case .acting(let action): action.replacingOccurrences(of: "_", with: " ")
        case .waitingForConfirmation: "Waiting for approval"
        case .completed: "Completed"
        case .failed: "Error"
        }
    }
}

public struct Activity: Identifiable, Sendable, Equatable {
    public let id: UUID
    public let name: String
    public let success: Bool
    public let detail: String
    public init(name: String, result: ToolResult) {
        id = UUID(); self.name = name; success = result.success; detail = result.text
    }
}

public struct Conversation: Codable, Identifiable, Sendable, Equatable {
    public var id = UUID()
    public var title: String = "New conversation"
    public var createdAt = Date()
    public var updatedAt = Date()
    public var messages: [ChatMessage] = []
    public init() {}
    public mutating func append(_ message: ChatMessage) {
        if messages.isEmpty && message.role == .user { title = String(message.content.prefix(42)) }
        messages.append(message); updatedAt = Date()
    }
}
