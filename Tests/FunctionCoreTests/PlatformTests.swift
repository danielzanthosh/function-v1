import Foundation
import Testing
import FunctionCore
import FunctionPlatform
#if os(macOS)
import Darwin
#else
import Glibc
#endif

@Test func shellReportsExitAndOutput() async throws {
    let result = try await ProcessRunner.run(executable: "/bin/sh", arguments: ["-c", "printf 'fixture output'; exit 7"])
    #expect(!result.success)
    #expect(result.text.contains("Exit 7"))
    #expect(result.text.contains("fixture output"))
}

@Test func cancellationKillsShellDescendants() async throws {
    let pidFile = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: pidFile) }
    // Controlled fixture path contains only ASCII UUID characters.
    let task = Task { try await ProcessRunner.run(executable: "/bin/sh", arguments: ["-c", "sleep 20 & echo $! > '\(pidFile.path)'; wait"]) }
    let clock = ContinuousClock(); let deadline = clock.now.advanced(by: .seconds(5))
    while !FileManager.default.fileExists(atPath: pidFile.path), clock.now < deadline { try await Task.sleep(for: .milliseconds(10)) }
    task.cancel()
    await #expect(throws: CancellationError.self) { try await task.value }
    let pidText = try String(contentsOf: pidFile, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines)
    let pid = try #require(Int32(pidText))
    // A terminated descendant can briefly be a zombie until the OS reaps it.
    #if os(Linux)
    if let state = try? String(contentsOfFile: "/proc/\(pid)/stat", encoding: .utf8) {
        #expect(state.split(separator: " ").dropFirst(2).first == "Z")
    } else { #expect(kill(pid, 0) != 0) }
    #else
    let reapingDeadline = clock.now.advanced(by: .seconds(2))
    while kill(pid, 0) == 0 && clock.now < reapingDeadline { try await Task.sleep(for: .milliseconds(20)) }
    #expect(kill(pid, 0) != 0)
    #endif
}

@Test func commandTimeoutStopsExecution() async throws {
    await #expect(throws: FunctionError.self) { try await ProcessRunner.run(executable: "/bin/sh", arguments: ["-c", "sleep 20"], timeout: .milliseconds(50)) }
}

@Test func fileToolsRejectResolvedCredentialPaths() async {
    let tools = NativeTools()
    for path in ["/etc/passwd", NSHomeDirectory() + "/.ssh/id_rsa", NSHomeDirectory() + "/.function/swift/config.json"] {
        await #expect(throws: FunctionError.self) { try await tools.execute(ToolCall(name: "computer_files", arguments: .object(["action": .string("read"), "path": .string(path)]))) }
    }
}
