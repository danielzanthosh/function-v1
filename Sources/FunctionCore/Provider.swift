import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public protocol CompletionProvider: Sendable {
    func complete(messages: [ChatMessage], tools: [ToolDefinition]) async throws -> ChatMessage
}

public struct HTTPClient: Sendable {
    private let session: URLSession
    public init(session: URLSession = .shared) { self.session = session }
    public func send(_ request: URLRequest) async throws -> Data {
        // Only retry provider requests on transient HTTP responses; tool actions are never retried here.
        for attempt in 0..<4 {
            try Task.checkCancellation()
            let (data, response) = try await session.data(for: request)
            guard let http = response as? HTTPURLResponse else { throw FunctionError.provider("Invalid server response.") }
            if (200..<300).contains(http.statusCode) { return data }
            if [429, 500, 502, 503, 504].contains(http.statusCode), attempt < 3 {
                let delay = min(30, max(1, Double(http.value(forHTTPHeaderField: "Retry-After") ?? "") ?? pow(2, Double(attempt))))
                try await Task.sleep(for: .seconds(delay)); continue
            }
            // Provider bodies can echo prompts or credentials; report status without exposing them.
            throw FunctionError.provider("Provider returned HTTP \(http.statusCode). Check the endpoint, key, and model.")
        }
        throw FunctionError.provider("Provider retry limit reached.")
    }
}

public struct OpenAIProvider: CompletionProvider {
    private let configuration: ProviderConfiguration
    private let apiKey: String
    private let client: HTTPClient
    public init(configuration: ProviderConfiguration, apiKey: String, client: HTTPClient = HTTPClient()) {
        self.configuration = configuration; self.apiKey = apiKey; self.client = client
    }
    public static func wireMessage(_ message: ChatMessage) throws -> JSONValue {
        var result: [String: JSONValue] = ["role": .string(message.role.rawValue)]
        if message.images.isEmpty { result["content"] = .string(message.content) }
        else {
            result["content"] = .array([.object(["type": .string("text"), "text": .string(message.content)])] + message.images.map {
                .object(["type": .string("image_url"), "image_url": .object(["url": .string($0)])])
            })
        }
        if let id = message.toolCallID { result["tool_call_id"] = .string(id) }
        if !message.toolCalls.isEmpty {
            result["tool_calls"] = .array(try message.toolCalls.map { call in
                var wire: [String: JSONValue] = ["id": .string(call.id), "type": .string("function"),
                    "function": .object(["name": .string(call.name), "arguments": .string(try call.arguments.encodedString())])]
                if let signature = call.thoughtSignature {
                    wire["extra_content"] = .object(["google": .object(["thought_signature": .string(signature)])])
                }
                return .object(wire)
            })
        }
        return .object(result)
    }
    public func complete(messages: [ChatMessage], tools: [ToolDefinition]) async throws -> ChatMessage {
        var body: [String: JSONValue] = ["model": .string(configuration.model),
                                        "messages": .array(try messages.map(Self.wireMessage))]
        if !tools.isEmpty {
            body["tools"] = .array(tools.map { .object(["type": .string("function"), "function": .object([
                "name": .string($0.name), "description": .string($0.description), "parameters": $0.parameters])]) })
        }
        let request = try makeRequest(baseURL: configuration.resolvedBaseURL, path: "chat/completions", apiKey: apiKey,
                                      body: JSONEncoder().encode(JSONValue.object(body)))
        let response = try JSONDecoder().decode(JSONValue.self, from: await client.send(request))
        guard let choice = response["choices"].array.first, choice["message"] != .null else {
            throw FunctionError.provider("The provider returned no assistant message.")
        }
        let wire = choice["message"]
        let calls = try wire["tool_calls"].array.map { call -> ToolCall in
            let arguments = try call["function"].requiredString("arguments")
            return ToolCall(id: try call.requiredString("id"), name: try call["function"].requiredString("name"),
                            arguments: try JSONDecoder().decode(JSONValue.self, from: Data(arguments.utf8)),
                            thoughtSignature: call["extra_content"]["google"]["thought_signature"].string)
        }
        if choice["finish_reason"].string == "length", calls.isEmpty {
            throw FunctionError.provider("The model reached its output limit. Try a shorter task or a different model.")
        }
        return ChatMessage(role: .assistant, content: wire["content"].string ?? "", toolCalls: calls)
    }
}

public func makeRequest(baseURL: String, path: String, apiKey: String, body: Data, contentType: String = "application/json") throws -> URLRequest {
    let base = try validatedHTTPURL(baseURL)
    var request = URLRequest(url: base.appendingPathComponent(path))
    request.httpMethod = "POST"; request.timeoutInterval = 120
    if !apiKey.isEmpty { request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization") }
    request.setValue(contentType, forHTTPHeaderField: "Content-Type"); request.httpBody = body
    return request
}

public struct SpeechProvider: Sendable {
    public let configuration: SpeechConfiguration
    private let apiKey: String
    private let client: HTTPClient
    public init(configuration: SpeechConfiguration, apiKey: String, client: HTTPClient = HTTPClient()) {
        self.configuration = configuration; self.apiKey = apiKey; self.client = client
    }
    public func transcribe(wav: Data) async throws -> String {
        let boundary = "Function-\(UUID().uuidString)"
        var body = Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n\(configuration.model)\r\n".utf8)
        body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"file\"; filename=\"recording.wav\"\r\nContent-Type: audio/wav\r\n\r\n".utf8))
        body.append(wav); body.append(Data("\r\n--\(boundary)--\r\n".utf8))
        let request = try makeRequest(baseURL: configuration.baseURL, path: "audio/transcriptions", apiKey: apiKey, body: body,
                                      contentType: "multipart/form-data; boundary=\(boundary)")
        let response = try JSONDecoder().decode(JSONValue.self, from: await client.send(request))
        return try response.requiredString("text")
    }
    public func synthesize(_ text: String) async throws -> Data {
        let body = JSONValue.object(["model": .string(configuration.ttsModel), "voice": .string(configuration.voice),
                                     "input": .string(text), "response_format": .string("wav")])
        return try await client.send(makeRequest(baseURL: configuration.ttsBaseURL, path: "audio/speech", apiKey: apiKey,
                                                body: JSONEncoder().encode(body)))
    }
}
