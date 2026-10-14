import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import FunctionCore

public struct NativeTools: ToolExecutor {
    public init() {}
    public func execute(_ call: ToolCall) async throws -> ToolResult {
        try Task.checkCancellation()
        let args = call.arguments
        switch call.name {
        case "execute_command", "terminal":
            return try await ProcessRunner.run(executable: "/bin/sh", arguments: ["-c", args.requiredString("command")])
        case "computer_files": return try files(args)
        case "browser_control":
            let url = try validatedHTTPURL(args.requiredString("url"))
            if try args.requiredString("action") == "fetch" { return try await fetch(url) }
            guard args["action"].string == "open" else { throw FunctionError.invalidInput("Unknown browser action.") }
            #if os(macOS)
            return try await Desktop.openURL(url)
            #else
            throw FunctionError.permission("Desktop tools require macOS.")
            #endif
        case "web_search":
            var components = URLComponents(string: "https://api.duckduckgo.com")!
            components.queryItems = [URLQueryItem(name: "q", value: try args.requiredString("query")), URLQueryItem(name: "format", value: "json"), URLQueryItem(name: "no_html", value: "1")]
            guard let url = components.url else { throw FunctionError.invalidInput("Invalid search query.") }
            return try await fetch(url)
        default:
            #if os(macOS)
            return try await desktop(call)
            #else
            throw FunctionError.permission("Desktop tools require macOS.")
            #endif
        }
    }
    private func fetch(_ url: URL) async throws -> ToolResult {
        var request = URLRequest(url: url); request.timeoutInterval = 20
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else { throw FunctionError.provider("Page could not be fetched.") }
        let raw = String(decoding: data.prefix(1_000_000), as: UTF8.self)
        let text = raw.replacingOccurrences(of: "(?is)<(script|style)[^>]*>.*?</\\1>", with: " ", options: .regularExpression)
            .replacingOccurrences(of: "<[^>]+>", with: " ", options: .regularExpression)
            .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        return ToolResult(String(text.prefix(12_000)))
    }
    private func allowedPath(_ value: String) throws -> URL {
        let expanded = NSString(string: value).expandingTildeInPath
        guard expanded.hasPrefix("/") else { throw FunctionError.invalidInput("Use an absolute file path.") }
        let url = URL(fileURLWithPath: expanded).standardizedFileURL.resolvingSymlinksInPath()
        let path = url.path
        let blocked = ["/System", "/etc", "/private/etc", "/usr", "/bin", "/sbin", NSHomeDirectory() + "/.ssh", NSHomeDirectory() + "/.aws",
                       NSHomeDirectory() + "/.codex", NSHomeDirectory() + "/.function", NSHomeDirectory() + "/Library/Keychains"]
        guard !blocked.contains(where: { path == $0 || path.hasPrefix($0 + "/") }) else { throw FunctionError.permission("This protected path is not available to file tools.") }
        return url
    }
    private func files(_ args: JSONValue) throws -> ToolResult {
        let url = try allowedPath(args.requiredString("path"))
        let manager = FileManager.default
        switch try args.requiredString("action") {
        case "exists": return ToolResult(manager.fileExists(atPath: url.path) ? "Exists." : "Does not exist.")
        case "list": return ToolResult(try manager.contentsOfDirectory(atPath: url.path).sorted().prefix(500).joined(separator: "\n"))
        case "read":
            let handle = try FileHandle(forReadingFrom: url); defer { try? handle.close() }
            return ToolResult(String(decoding: try handle.read(upToCount: 64_000) ?? Data(), as: UTF8.self))
        case "write": try Data((args["content"].string ?? "").utf8).write(to: url, options: .atomic); return ToolResult("File written.")
        case "move": try manager.moveItem(at: url, to: allowedPath(args.requiredString("destination"))); return ToolResult("File moved.")
        case "delete": try manager.removeItem(at: url); return ToolResult("File deleted.")
        default: throw FunctionError.invalidInput("Unknown file action.")
        }
    }
    #if os(macOS)
    private func desktop(_ call: ToolCall) async throws -> ToolResult {
        let args = call.arguments
        switch call.name {
        case "open_app": return try await Desktop.openApp(args.requiredString("name"))
        case "close_app": return try await Desktop.closeApp(args.requiredString("name"))
        case "take_screenshot", "screen_capture": return try await Desktop.screenshot()
        case "click", "double_click":
            return try await Desktop.click(x: coordinate(args, "x"), y: coordinate(args, "y"), right: args["button"].string == "right", double: call.name == "double_click")
        case "type_text": return try await Desktop.type(args.requiredString("text"))
        case "press_key": return try await Desktop.press(args.requiredString("key"))
        case "scroll": return try await Desktop.scroll(vertical: args["vertical"].number ?? 0, horizontal: args["horizontal"].number ?? 0)
        case "application_control":
            switch try args.requiredString("action") {
            case "list": return await Desktop.listApps()
            case "open": return try await Desktop.openApp(args.requiredString("name"))
            case "close": return try await Desktop.closeApp(args.requiredString("name"))
            default: throw FunctionError.invalidInput("Unknown application action.")
            }
        case "keyboard_control":
            switch try args.requiredString("action") {
            case "type": return try await Desktop.type(args.requiredString("text"))
            case "press": return try await Desktop.press(args.requiredString("key"))
            default: throw FunctionError.invalidInput("Unknown keyboard action.")
            }
        case "mouse_control":
            let x = try coordinate(args, "x"), y = try coordinate(args, "y")
            switch try args.requiredString("action") {
            case "move": return try await Desktop.move(x: x, y: y)
            case "click", "double_click": return try await Desktop.click(x: x, y: y, double: args["action"].string == "double_click")
            default: throw FunctionError.invalidInput("Unknown mouse action.")
            }
        default: throw FunctionError.invalidInput("Unknown tool '\(call.name)'.")
        }
    }
    private func coordinate(_ args: JSONValue, _ key: String) throws -> Double {
        guard let value = args[key].number, value.isFinite else { throw FunctionError.invalidInput("Provide numeric '\(key)'.") }
        return value
    }
    #endif
}
