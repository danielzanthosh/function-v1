import Foundation
import Testing
@testable import FunctionCore

actor ScriptedProvider: CompletionProvider {
    var replies: [ChatMessage]
    var requests: [[ChatMessage]] = []
    init(_ replies: [ChatMessage]) { self.replies = replies }
    func complete(messages: [ChatMessage], tools: [ToolDefinition]) async throws -> ChatMessage {
        requests.append(messages)
        guard !replies.isEmpty else { throw FunctionError.provider("No scripted response") }
        return replies.removeFirst()
    }
}
actor RecordingExecutor: ToolExecutor {
    var calls: [ToolCall] = []
    func execute(_ call: ToolCall) async throws -> ToolResult {
        calls.append(call)
        return ToolResult("Observed", images: call.name == "take_screenshot" ? ["data:image/jpeg;base64,fixture"] : [])
    }
}
actor EventLog {
    var states: [AgentState] = []
    var activities: [Activity] = []
    func state(_ value: AgentState) { states.append(value) }
    func activity(_ value: Activity) { activities.append(value) }
}

@Test func observeActLoopReturnsToolResultsAndImages() async throws {
    let call = ToolCall(id: "shot-1", name: "take_screenshot", arguments: .object([:]))
    let provider = ScriptedProvider([ChatMessage(role: .assistant, content: "", toolCalls: [call]), ChatMessage(role: .assistant, content: "Done")])
    let executor = RecordingExecutor(), events = EventLog()
    let agent = Agent(provider: provider, executor: executor, memory: nil, configuration: AppConfiguration())
    let reply = try await agent.run(prompt: "Read the screen", history: [], state: { await events.state($0) }, activity: { await events.activity($0) }, confirm: { _ in false })
    #expect(reply == "Done")
    #expect(await executor.calls.count == 1)
    let requests = await provider.requests
    #expect(requests[1].contains { $0.role == .tool && $0.toolCallID == "shot-1" })
    #expect(requests[1].contains { !$0.images.isEmpty })
    #expect(await events.states.last == .completed)
}

@Test func deniedActionsAreNeverExecuted() async throws {
    let call = ToolCall(name: "execute_command", arguments: .object(["command": .string("rm important")]))
    let provider = ScriptedProvider([ChatMessage(role: .assistant, content: "", toolCalls: [call]), ChatMessage(role: .assistant, content: "Cancelled action")])
    let executor = RecordingExecutor()
    let agent = Agent(provider: provider, executor: executor, memory: nil, configuration: AppConfiguration())
    _ = try await agent.run(prompt: "Remove it", history: [], state: { _ in }, activity: { _ in }, confirm: { _ in false })
    #expect(await executor.calls.isEmpty)
    let requests = await provider.requests
    #expect(requests[1].contains { $0.content.contains("User denied") })
}

@Test func disabledToolsCannotRun() async throws {
    var config = AppConfiguration(); config.memoryEnabled = false; config.searchEnabled = false
    let provider = ScriptedProvider([ChatMessage(role: .assistant, content: "", toolCalls: [ToolCall(name: "web_search", arguments: .object(["query": .string("test")]))]), ChatMessage(role: .assistant, content: "Disabled")])
    let executor = RecordingExecutor()
    _ = try await Agent(provider: provider, executor: executor, memory: nil, configuration: config).run(prompt: "Search", history: [], state: { _ in }, activity: { _ in }, confirm: { _ in true })
    #expect(await executor.calls.isEmpty)
}

@Test func repeatedActionsStop() async throws {
    let call = ToolCall(name: "take_screenshot", arguments: .object([:]))
    let response = ChatMessage(role: .assistant, content: "", toolCalls: [call])
    let provider = ScriptedProvider(Array(repeating: response, count: 8))
    let executor = RecordingExecutor()
    await #expect(throws: FunctionError.self) {
        try await Agent(provider: provider, executor: executor, memory: nil, configuration: AppConfiguration()).run(prompt: "Look", history: [], state: { _ in }, activity: { _ in }, confirm: { _ in true })
    }
    #expect(await executor.calls.count == 3)
}

@Test func cancellationPreventsToolExecution() async throws {
    let executor = RecordingExecutor()
    let provider = ScriptedProvider([ChatMessage(role: .assistant, content: "", toolCalls: [ToolCall(name: "click", arguments: .object(["x": .number(1), "y": .number(2)]))])])
    let gate = AsyncStream<Void>.makeStream()
    let task = Task {
        try await Agent(provider: provider, executor: executor, memory: nil, configuration: AppConfiguration()).run(prompt: "Click", history: [], state: { _ in }, activity: { _ in }, confirm: { _ in
            gate.continuation.yield(())
            try await Task.sleep(for: .seconds(30))
            return true
        })
    }
    for await _ in gate.stream { break }
    task.cancel()
    await #expect(throws: CancellationError.self) { try await task.value }
    #expect(await executor.calls.isEmpty)
}

@Test func stepLimitIsAnError() async {
    var config = AppConfiguration(); config.maxSteps = 1
    let provider = ScriptedProvider([ChatMessage(role: .assistant, content: "", toolCalls: [ToolCall(name: "take_screenshot", arguments: .object([:]))])])
    await #expect(throws: FunctionError.self) {
        try await Agent(provider: provider, executor: RecordingExecutor(), memory: nil, configuration: config).run(prompt: "Look", history: [], state: { _ in }, activity: { _ in }, confirm: { _ in true })
    }
}
