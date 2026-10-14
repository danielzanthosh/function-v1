import Foundation
import Testing
@testable import FunctionCore

func temporaryStorage() throws -> JSONFileStore {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return JSONFileStore(directory: directory)
}

@Test func memoryPersistsAndRejectsSecrets() async throws {
    let storage = try temporaryStorage(); defer { try? FileManager.default.removeItem(at: storage.directory) }
    let memory = try MemoryStore(storage: storage)
    let item = MemoryItem(category: .userPreference, key: "Browser", value: "Use Safari")
    try await memory.remember(item)
    let restored = try MemoryStore(storage: storage)
    #expect(await restored.recall("Safari").first?.id == item.id)
    await #expect(throws: FunctionError.self) { try await restored.remember(MemoryItem(category: .userPreference, key: "API key", value: "sensitive")) }
    try await restored.forget(item.id)
    let empty = try MemoryStore(storage: storage)
    #expect(await empty.recall().isEmpty)
}

@Test func failedMemoryWriteDoesNotMutateCache() async throws {
    let storage = try temporaryStorage(); defer { try? FileManager.default.removeItem(at: storage.directory) }
    let memory = try MemoryStore(storage: storage)
    try FileManager.default.removeItem(at: storage.directory)
    try Data("block directory creation".utf8).write(to: storage.directory)
    await #expect(throws: (any Error).self) { try await memory.remember(MemoryItem(category: .userPreference, key: "Editor", value: "Xcode")) }
    #expect(await memory.recall().isEmpty)
}

@Test func malformedStorageIsNotSilentlyOverwritten() throws {
    let storage = try temporaryStorage(); defer { try? FileManager.default.removeItem(at: storage.directory) }
    let path = storage.directory.appendingPathComponent("config.json")
    try Data("bad json".utf8).write(to: path)
    #expect(throws: (any Error).self) { try storage.load(AppConfiguration.self, from: "config.json", default: AppConfiguration()) }
    #expect(try String(contentsOf: path, encoding: .utf8) == "bad json")
}

@Test func configurationHasNoCredentialFields() throws {
    let json = String(decoding: try JSONEncoder().encode(AppConfiguration()), as: UTF8.self)
    #expect(!json.lowercased().contains("api_key"))
    #expect(!json.lowercased().contains("apikey"))
    #expect(try JSONDecoder().decode(AppConfiguration.self, from: Data(json.utf8)) == AppConfiguration())
}

@Test func providerWirePreservesToolIDsAndGeminiSignature() throws {
    let call = ToolCall(id: "call-42", name: "open_app", arguments: .object(["name": .string("Safari")]), thoughtSignature: "signature")
    let wire = try OpenAIProvider.wireMessage(ChatMessage(role: .assistant, content: "", toolCalls: [call]))
    #expect(wire["tool_calls"].array.first?["id"].string == "call-42")
    #expect(wire["tool_calls"].array.first?["extra_content"]["google"]["thought_signature"].string == "signature")
    let toolWire = try OpenAIProvider.wireMessage(ChatMessage(role: .tool, content: "opened", toolCallID: call.id))
    #expect(toolWire["tool_call_id"].string == "call-42")
    let imageWire = try OpenAIProvider.wireMessage(ChatMessage(role: .user, content: "Screen", images: ["data:image/jpeg;base64,fixture"]))
    #expect(imageWire["content"].array.count == 2)
}

@Test func compactionKeepsToolGroupsTogether() throws {
    let call = ToolCall(id: "call", name: "take_screenshot", arguments: .object([:]))
    let messages = [ChatMessage(role: .system, content: "Rules"), ChatMessage(role: .user, content: String(repeating: "old", count: 5_000)),
                    ChatMessage(role: .assistant, content: "reply"), ChatMessage(role: .user, content: "Current"),
                    ChatMessage(role: .assistant, content: "", toolCalls: [call]), ChatMessage(role: .tool, content: "Done", toolCallID: call.id)]
    let result = try ContextBudget.compact(messages, tools: [], limit: 1_000)
    #expect(result.first?.role == .system)
    #expect(result.contains { $0.toolCalls.first?.id == "call" })
    #expect(result.contains { $0.toolCallID == "call" })
    #expect(!result.contains { $0.content.hasPrefix("oldold") })
}

@Test func oversizedCurrentTaskFailsWithoutDiscardingPrompt() {
    #expect(throws: FunctionError.self) { try ContextBudget.compact([ChatMessage(role: .user, content: String(repeating: "x", count: 20_000))], tools: [], limit: 100) }
}

@Test func permissionsCoverAliasesAndUnknownTools() {
    for name in ["click", "type_text", "keyboard_control", "mouse_control", "terminal", "execute_command", "unknown"] {
        #expect(ToolPolicy.requiresConfirmation(ToolCall(name: name, arguments: .object([:]))))
    }
    #expect(!ToolPolicy.requiresConfirmation(ToolCall(name: "computer_files", arguments: .object(["action": .string("read")]))))
    #expect(ToolPolicy.requiresConfirmation(ToolCall(name: "computer_files", arguments: .object(["action": .string("write")]))))
    #expect(ToolPolicy.requiresConfirmation(ToolCall(name: "memory", arguments: .object(["action": .string("remember")]))))
}

@Test func localCommandsDoNotHijackNaturalLanguage() {
    #expect(LocalCommand.parse("settings for wifi") == nil)
    #expect(LocalCommand.parse(" Settings ") == .settings)
    #expect(LocalCommand.parse("/model gemini-2.5-flash") == .model("gemini-2.5-flash"))
    #expect(LocalCommand.parse("> pwd") == .shell("pwd"))
}

@Test func endpointValidationRejectsCredentialsAndSchemes() throws {
    #expect(throws: FunctionError.self) { try validatedHTTPURL("file:///etc/passwd") }
    #expect(throws: FunctionError.self) { try validatedHTTPURL("https://user:password@example.com") }
    #expect(try validatedHTTPURL("http://127.0.0.1:9000/v1").host == "127.0.0.1")
}

@Test func compactionDoesNotTreatScreenshotAsNewUserRequest() throws {
    let prompt = ChatMessage(role: .user, content: "Keep my actual task")
    let messages = [ChatMessage(role: .system, content: "Rules"), prompt,
                    ChatMessage(role: .tool, content: "Large result " + String(repeating: "x", count: 6_000), toolCallID: "id"),
                    ChatMessage(role: .user, content: "Observation", images: ["image"], isObservation: true)]
    #expect(throws: FunctionError.self) { try ContextBudget.compact(messages, tools: [], limit: 1_000) }
}

@Test func calculatorUsesPrecedenceAndRejectsInvalidInput() {
    #expect(Calculator.evaluate("2 + 3 * 4") == 14)
    #expect(Calculator.evaluate("(2 + 3) * 4") == 20)
    #expect(Calculator.evaluate("2^3^2") == 512)
    #expect(Calculator.evaluate("-2^2") == -4)
    #expect(Calculator.evaluate("1/0") == nil)
    #expect(Calculator.evaluate("open Safari") == nil)
    #expect(Calculator.evaluate("123") == nil)
}

@Test func memoryRejectsUnrepresentableLimits() async throws {
    let storage = try temporaryStorage(); defer { try? FileManager.default.removeItem(at: storage.directory) }
    let memory = try MemoryStore(storage: storage)
    await #expect(throws: FunctionError.self) { try await memory.execute(.object(["action": .string("recall"), "limit": .number(1e99)])) }
}

@Test func responsesWireRetainsCallIdentityAndImages() throws {
    let call = ToolCall(id: "call-123", name: "take_screenshot", arguments: .object([:]))
    let input = try ResponsesProvider.input([ChatMessage(role: .system, content: "Rules"),
        ChatMessage(role: .assistant, content: "", toolCalls: [call]), ChatMessage(role: .tool, content: "Screen", toolCallID: call.id),
        ChatMessage(role: .user, content: "Observe", images: ["image"], isObservation: true)])
    #expect(input[0]["call_id"].string == call.id)
    #expect(input[1]["type"].string == "function_call_output")
    #expect(input[2]["content"].array.last?["image_url"].string == "image")
}

@Test func responsesDecodeCompletedSSEAndRejectIncomplete() throws {
    let sse = #"data: {"type":"response.completed","response":{"output":[{"type":"function_call","call_id":"c1","name":"open_app","arguments":"{\"name\":\"Safari\"}"}]}}"#
    let reply = try ResponsesProvider.decodeEvents(Data(sse.utf8))
    #expect(reply.toolCalls.first?.arguments["name"].string == "Safari")
    #expect(throws: FunctionError.self) { try ResponsesProvider.decodeEvents(Data(#"data: {"type":"response.incomplete"}"#.utf8)) }
}

@Test func legacyImportConvertsDataWithoutWritingSecrets() throws {
    let storage = try temporaryStorage(); defer { try? FileManager.default.removeItem(at: storage.directory) }
    try Data(#"{"ai_provider":{"provider_name":"gemini","model":"gemini-2.5-flash","api_key":"legacy-fixture"},"theme_style":"studiolight"}"#.utf8).write(to: storage.directory.appendingPathComponent("config.json"))
    try Data(#"[{"id":"m1","category":"user_preference","key":"Browser","value":"Safari","created_at":123}]"#.utf8).write(to: storage.directory.appendingPathComponent("memory.json"))
    try Data(#"[{"title":"Earlier task","created_at":123,"updated_at":124,"display_messages":[{"is_user":true,"text":"Hello"},{"is_user":false,"text":"Hi"}]}]"#.utf8).write(to: storage.directory.appendingPathComponent("conversations.json"))
    let imported = try LegacyImport(directory: storage.directory)
    #expect(imported.configuration.provider.name == "gemini")
    #expect(imported.configuration.theme == .studioLight)
    #expect(imported.aiKey == "legacy-fixture")
    #expect(imported.memories.first?.id == "m1")
    #expect(imported.conversations.first?.messages.count == 2)
    #expect(!String(decoding: try JSONEncoder().encode(imported.configuration), as: UTF8.self).contains("legacy-fixture"))
}

@Test func unicodeKeyboardChunksDoNotSplitSurrogatePairs() {
    let text = String(repeating: "a", count: 19) + "🦊" + String(repeating: "é", count: 40)
    let chunks = KeyboardText.chunks(text)
    #expect(chunks.allSatisfy { $0.count <= 20 })
    #expect(chunks.map { String(decoding: $0, as: UTF16.self) }.joined() == text)
    #expect(KeyboardText.chunks("").isEmpty)
}

@Test func missingLegacyDataDoesNotProduceAnEmptyImport() throws {
    let storage = try temporaryStorage(); defer { try? FileManager.default.removeItem(at: storage.directory) }
    #expect(throws: FunctionError.self) { try LegacyImport(directory: storage.directory) }
}

@Test func responsesRoundTripsReasoningAlongsideToolCalls() throws {
    let sse = #"data: {"type":"response.completed","response":{"output":[{"type":"reasoning","id":"r1","encrypted_content":"fixture-reasoning","summary":[]},{"type":"function_call","id":"f1","call_id":"c1","name":"open_app","arguments":"{\"name\":\"Safari\"}"}]}}"#
    let reply = try ResponsesProvider.decodeEvents(Data(sse.utf8))
    let input = try ResponsesProvider.input([reply, ChatMessage(role: .tool, content: "Opened", toolCallID: "c1")])
    #expect(input[0]["encrypted_content"].string == "fixture-reasoning")
    #expect(input[1]["call_id"].string == "c1")
    #expect(input[2]["type"].string == "function_call_output")
}
