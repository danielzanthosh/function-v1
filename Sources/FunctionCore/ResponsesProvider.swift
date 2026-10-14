import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

// The native ChatGPT adapter uses the same structured tool runtime as API providers.
// Authentication and token renewal are supplied by the platform Keychain service.
public struct ResponsesProvider: CompletionProvider {
    public typealias Authorization = @Sendable () async throws -> (token: String, accountID: String?)
    private let model: String
    private let baseURL: String
    private let authorize: Authorization
    private let client: HTTPClient
    public init(model: String, baseURL: String = "https://chatgpt.com/backend-api/codex", client: HTTPClient = HTTPClient(), authorize: @escaping Authorization) {
        self.model = model; self.baseURL = baseURL; self.client = client; self.authorize = authorize
    }
    public static func input(_ messages: [ChatMessage]) throws -> [JSONValue] {
        var input: [JSONValue] = []
        for message in messages where message.role != .system {
            if message.role == .assistant && !message.providerItems.isEmpty {
                // Responses reasoning models require the complete output items with subsequent tool results.
                input.append(contentsOf: message.providerItems)
                continue
            }
            if message.role == .tool {
                guard let id = message.toolCallID else { throw FunctionError.invalidInput("Tool result lacks a call id.") }
                input.append(.object(["type": .string("function_call_output"), "call_id": .string(id), "output": .string(message.content)]))
            } else {
                if !message.content.isEmpty || !message.images.isEmpty {
                    var content: [JSONValue] = []
                    if !message.content.isEmpty { content.append(.object(["type": .string(message.role == .assistant ? "output_text" : "input_text"), "text": .string(message.content)])) }
                    content += message.images.map { .object(["type": .string("input_image"), "image_url": .string($0)]) }
                    input.append(.object(["role": .string(message.role.rawValue), "content": .array(content)]))
                }
                for call in message.toolCalls {
                    input.append(.object(["type": .string("function_call"), "call_id": .string(call.id), "name": .string(call.name), "arguments": .string(try call.arguments.encodedString())]))
                }
            }
        }
        return input
    }
    public static func decodeEvents(_ data: Data) throws -> ChatMessage {
        let text = String(decoding: data, as: UTF8.self)
        var output: JSONValue?
        for line in text.split(whereSeparator: \.isNewline) where line.hasPrefix("data: ") {
            let payload = line.dropFirst(6)
            guard payload != "[DONE]", let event = try? JSONDecoder().decode(JSONValue.self, from: Data(payload.utf8)) else { continue }
            if ["response.failed", "error", "response.incomplete"].contains(event["type"].string ?? "") {
                throw FunctionError.provider("The ChatGPT request failed or reached its output limit.")
            }
            if event["type"].string == "response.completed" { output = event["response"]["output"] }
        }
        guard let output else { throw FunctionError.provider("ChatGPT returned no completed response.") }
        var reply = ChatMessage(role: .assistant, content: "")
        reply.providerItems = output.array
        for item in output.array {
            if item["type"].string == "function_call" {
                let arguments = try item.requiredString("arguments")
                reply.toolCalls.append(ToolCall(id: try item.requiredString("call_id"), name: try item.requiredString("name"),
                                               arguments: try JSONDecoder().decode(JSONValue.self, from: Data(arguments.utf8))))
            } else if item["type"].string == "message" {
                let parts = item["content"].array.compactMap { $0["type"].string == "output_text" ? $0["text"].string : nil }
                reply.content += parts.joined(separator: "\n")
            }
        }
        return reply
    }
    public func complete(messages: [ChatMessage], tools: [ToolDefinition]) async throws -> ChatMessage {
        let authorization = try await authorize()
        let body: JSONValue = .object([
            "model": .string(model), "instructions": .string(messages.filter { $0.role == .system }.map(\.content).joined(separator: "\n")),
            "input": .array(try Self.input(messages)), "store": .bool(false), "stream": .bool(true),
            "include": .array([.string("reasoning.encrypted_content")]),
            "tools": .array(tools.map { .object(["type": .string("function"), "name": .string($0.name), "description": .string($0.description), "parameters": $0.parameters, "strict": .bool(false)]) })
        ])
        var request = try makeRequest(baseURL: baseURL, path: "responses", apiKey: authorization.token, body: JSONEncoder().encode(body))
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        if let account = authorization.accountID { request.setValue(account, forHTTPHeaderField: "ChatGPT-Account-Id") }
        return try Self.decodeEvents(await client.send(request))
    }
}
