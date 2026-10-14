import Foundation

public enum ToolPolicy {
    // UI actions may send messages, purchase, or delete. Every mutating action needs approval.
    public static func requiresConfirmation(_ call: ToolCall) -> Bool {
        switch call.name {
        case "take_screenshot", "web_search": false
        case "computer_files": !["read", "list", "exists"].contains(call.arguments["action"].string ?? "")
        case "memory": call.arguments["action"].string != "recall"
        case "browser_control": call.arguments["action"].string != "fetch"
        case "application_control": call.arguments["action"].string != "list"
        default: true
        }
    }
}

public enum ToolCatalog {
    public static let string: JSONValue = .object(["type": .string("string")])
    public static let number: JSONValue = .object(["type": .string("number")])
    public static func options(_ values: [String]) -> JSONValue { .object(["type": .string("string"), "enum": .array(values.map(JSONValue.string))]) }
    public static func definitions(memory: Bool = true, search: Bool = true) -> [ToolDefinition] {
        var tools = [
            ToolDefinition("open_app", "Launch or focus a macOS application by name.", properties: ["name": string], required: ["name"]),
            ToolDefinition("close_app", "Gracefully quit an application; never force quit.", properties: ["name": string], required: ["name"]),
            ToolDefinition("take_screenshot", "Observe the screen. Coordinates are pixels in the returned image; screenshot reports its scale.", properties: [:]),
            ToolDefinition("click", "Click at a screen coordinate in global macOS points (top-left origin).", properties: ["x": number, "y": number, "button": options(["left", "right"])], required: ["x", "y"]),
            ToolDefinition("double_click", "Double click at a screen coordinate in macOS points.", properties: ["x": number, "y": number], required: ["x", "y"]),
            ToolDefinition("type_text", "Type Unicode text into the focused application.", properties: ["text": string], required: ["text"]),
            ToolDefinition("press_key", "Press a key or shortcut, e.g. cmd+t, return, ctrl+c, escape.", properties: ["key": string], required: ["key"]),
            ToolDefinition("scroll", "Scroll by signed pixel amounts. Positive vertical moves up.", properties: ["vertical": number, "horizontal": number]),
            ToolDefinition("execute_command", "Run a shell command only when no native tool applies. Always requires approval.", properties: ["command": string], required: ["command"]),
            ToolDefinition("computer_files", "Read, write, list, move, delete, or test local files. Never read credential stores.", properties: ["action": options(["read", "write", "list", "exists", "move", "delete"]), "path": string, "content": string, "destination": string], required: ["action", "path"]),
            ToolDefinition("browser_control", "Open a URL in the default browser or fetch a public page; fetch does not access browser sessions.", properties: ["action": options(["open", "fetch"]), "url": string], required: ["action", "url"]),
            ToolDefinition("application_control", "List running applications, open, or close an application.", properties: ["action": options(["list", "open", "close"]), "name": string], required: ["action"]),
            ToolDefinition("mouse_control", "Move or click the mouse.", properties: ["action": options(["move", "click", "double_click"]), "x": number, "y": number], required: ["action", "x", "y"]),
            ToolDefinition("keyboard_control", "Type text or press a keyboard shortcut.", properties: ["action": options(["type", "press"]), "text": string, "key": string], required: ["action"]),
            ToolDefinition("screen_capture", "Capture the main display.", properties: [:]),
            ToolDefinition("terminal", "Run a shell command with approval.", properties: ["command": string], required: ["command"])
        ]
        if memory {
            tools.append(ToolDefinition("memory", "Save concise notes only when explicitly asked. Never save secrets. Recall relevant notes or forget by id.", properties: [
                "action": options(["remember", "recall", "forget", "clear_category"]), "category": options(MemoryCategory.allCases.map(\.rawValue)),
                "key": string, "value": string, "query": string, "id": string, "limit": number], required: ["action"]))
        }
        if search { tools.append(ToolDefinition("web_search", "Search public web results with DuckDuckGo.", properties: ["query": string], required: ["query"])) }
        return tools
    }
}

public protocol ToolExecutor: Sendable {
    func execute(_ call: ToolCall) async throws -> ToolResult
}

public enum LocalCommand: Equatable, Sendable {
    case settings, history, newConversation, theme, voice, sound, clear, memory, about, help, model(String), shell(String)
    public static func parse(_ text: String) -> Self? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let lower = trimmed.lowercased()
        switch lower {
        case "settings", "/settings", "configure": return .settings
        case "conversation", "/conversation", "/conversations", "history": return .history
        case "/new": return .newConversation
        case "/theme", "/themes": return .theme
        case "/voice": return .voice
        case "/sound": return .sound
        case "/clear": return .clear
        case "/memory": return .memory
        case "/about": return .about
        case "/help": return .help
        case "/model": return .model("")
        default:
            if lower.hasPrefix("/model ") { return .model(String(trimmed.dropFirst(7)).trimmingCharacters(in: .whitespaces)) }
            if trimmed.hasPrefix(">"), !trimmed.dropFirst().trimmingCharacters(in: .whitespaces).isEmpty {
                return .shell(trimmed.dropFirst().trimmingCharacters(in: .whitespaces))
            }
            return nil
        }
    }
}
