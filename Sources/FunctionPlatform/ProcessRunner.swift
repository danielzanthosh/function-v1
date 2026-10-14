import Foundation
#if os(macOS)
import Darwin
#else
import Glibc
#endif
import FunctionCore

public enum ProcessRunner {
    public static func run(executable: String, arguments: [String], timeout: Duration = .seconds(30)) async throws -> ToolResult {
        try Task.checkCancellation()
        let outputURL = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        guard FileManager.default.createFile(atPath: outputURL.path, contents: nil, attributes: [.posixPermissions: 0o600]) else {
            throw FunctionError.storage("Could not create command output file.")
        }
        let handle = try FileHandle(forWritingTo: outputURL)
        defer { try? handle.close(); try? FileManager.default.removeItem(at: outputURL) }
        let pid = try spawn(executable: executable, arguments: arguments, output: handle.fileDescriptor)
        let clock = ContinuousClock(); let deadline = clock.now.advanced(by: timeout)
        var status: Int32 = 0
        do {
            while true {
                let result = waitpid(pid, &status, WNOHANG)
                if result == pid { break }
                if result < 0 && errno != EINTR { throw FunctionError.invalidInput("Could not collect command status.") }
                try Task.checkCancellation()
                guard clock.now < deadline else { throw FunctionError.invalidInput("Command exceeded its time limit.") }
                var info = stat()
                if fstat(handle.fileDescriptor, &info) == 0 && info.st_size > 1_000_000 {
                    throw FunctionError.invalidInput("Command exceeded its output limit.")
                }
                try await Task.sleep(for: .milliseconds(25))
            }
            try Task.checkCancellation()
        } catch {
            // Each command owns a process group; kill descendants as well as the shell.
            kill(-pid, SIGKILL)
            while waitpid(pid, &status, 0) < 0 && errno == EINTR {}
            throw error
        }
        // A shell can exit while background children remain. Do not leave them running.
        kill(-pid, SIGKILL)
        let reader = try FileHandle(forReadingFrom: outputURL)
        defer { try? reader.close() }
        let data = try reader.read(upToCount: 64_000) ?? Data()
        let exitCode = (status & 0x7f) == 0 ? (status >> 8) & 0xff : 128 + (status & 0x7f)
        return ToolResult("Exit \(exitCode)\n" + String(decoding: data, as: UTF8.self), success: exitCode == 0)
    }
    private static func spawn(executable: String, arguments: [String], output: Int32) throws -> pid_t {
        #if os(macOS)
        var attributes: posix_spawnattr_t?
        var actions: posix_spawn_file_actions_t?
        #else
        var attributes = posix_spawnattr_t()
        var actions = posix_spawn_file_actions_t()
        #endif
        guard posix_spawnattr_init(&attributes) == 0 else { throw FunctionError.invalidInput("Could not initialize process attributes.") }
        defer { posix_spawnattr_destroy(&attributes) }
        guard posix_spawn_file_actions_init(&actions) == 0 else { throw FunctionError.invalidInput("Could not initialize process output.") }
        defer { posix_spawn_file_actions_destroy(&actions) }
        guard posix_spawnattr_setflags(&attributes, Int16(POSIX_SPAWN_SETPGROUP)) == 0,
              posix_spawnattr_setpgroup(&attributes, 0) == 0,
              posix_spawn_file_actions_adddup2(&actions, output, STDOUT_FILENO) == 0,
              posix_spawn_file_actions_adddup2(&actions, output, STDERR_FILENO) == 0,
              posix_spawn_file_actions_addopen(&actions, STDIN_FILENO, "/dev/null", O_RDONLY, 0) == 0 else {
            throw FunctionError.invalidInput("Could not configure command process.")
        }
        let values = [executable] + arguments
        let environment = ProcessInfo.processInfo.environment.map { "\($0.key)=\($0.value)" }
        var argv = values.map { strdup($0) } + [nil]
        var envp = environment.map { strdup($0) } + [nil]
        defer { for value in argv { free(value) }; for value in envp { free(value) } }
        guard argv.dropLast().allSatisfy({ $0 != nil }), envp.dropLast().allSatisfy({ $0 != nil }) else { throw FunctionError.storage("Could not allocate process arguments.") }
        var pid: pid_t = 0
        let result = posix_spawn(&pid, executable, &actions, &attributes, &argv, &envp)
        guard result == 0 else { throw FunctionError.invalidInput("Could not start command (\(result)).") }
        return pid
    }
}
