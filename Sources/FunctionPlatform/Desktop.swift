#if os(macOS)
import AppKit
import ApplicationServices
@preconcurrency import ScreenCaptureKit
import ImageIO
import UniformTypeIdentifiers
import FunctionCore

@MainActor
public enum Desktop {
    public static var accessibilityGranted: Bool { AXIsProcessTrusted() }
    public static func requestAccessibility() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
        AXIsProcessTrustedWithOptions(options as CFDictionary)
    }
    private static func checkAccessibility() throws {
        guard accessibilityGranted else { throw FunctionError.permission("Enable Function in System Settings → Privacy & Security → Accessibility.") }
    }
    public static func openApp(_ name: String) async throws -> ToolResult {
        let workspace = NSWorkspace.shared
        let folders = ["/Applications", "/System/Applications", "/System/Applications/Utilities", NSHomeDirectory() + "/Applications"]
        let name = name.lowercased().replacingOccurrences(of: ".app", with: "")
        let candidates = folders.flatMap { (try? FileManager.default.contentsOfDirectory(at: URL(fileURLWithPath: $0), includingPropertiesForKeys: nil)) ?? [] }
        guard let app = candidates.first(where: { $0.pathExtension == "app" && $0.deletingPathExtension().lastPathComponent.lowercased() == name }) else {
            throw FunctionError.invalidInput("Application '\(name)' was not found.")
        }
        let configuration = NSWorkspace.OpenConfiguration(); configuration.activates = true
        _ = try await workspace.openApplication(at: app, configuration: configuration)
        return ToolResult("Opened \(app.deletingPathExtension().lastPathComponent).")
    }
    public static func closeApp(_ name: String) throws -> ToolResult {
        guard let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName?.lowercased() == name.lowercased() }),
              app.processIdentifier != ProcessInfo.processInfo.processIdentifier else { throw FunctionError.invalidInput("Application is not running or is Function itself.") }
        guard app.terminate() else { throw FunctionError.permission("Application declined to quit.") }
        return ToolResult("Requested that \(name) quit. Check for unsaved-document prompts.")
    }
    public static func openURL(_ url: URL) throws -> ToolResult {
        guard NSWorkspace.shared.open(url) else { throw FunctionError.invalidInput("Could not open this URL.") }
        return ToolResult("Opened \(url.absoluteString).")
    }
    public static func listApps() -> ToolResult {
        ToolResult(NSWorkspace.shared.runningApplications.compactMap(\.localizedName).sorted().joined(separator: "\n"))
    }
    public static func screenshot() async throws -> ToolResult {
        guard CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() else {
            throw FunctionError.permission("Enable Function in System Settings → Privacy & Security → Screen Recording, then relaunch.")
        }
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == CGMainDisplayID() }) else { throw FunctionError.invalidInput("Main display not found.") }
        let ownApplications = content.applications.filter { $0.processID == ProcessInfo.processInfo.processIdentifier }
        let filter = SCContentFilter(display: display, excludingApplications: ownApplications, exceptingWindows: [])
        let bounds = CGDisplayBounds(display.displayID)
        let configuration = SCStreamConfiguration()
        // One pixel per global point removes Retina coordinate ambiguity.
        configuration.width = Int(bounds.width); configuration.height = Int(bounds.height)
        configuration.showsCursor = true
        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: configuration)
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data as CFMutableData, UTType.jpeg.identifier as CFString, 1, nil) else {
            throw FunctionError.invalidInput("Could not encode screenshot.")
        }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.65] as CFDictionary)
        guard CGImageDestinationFinalize(destination) else { throw FunctionError.invalidInput("Screenshot encoding failed.") }
        return ToolResult("Main display \(image.width)×\(image.height). Scale: 1 pixel per macOS point. Global origin: (\(bounds.minX), \(bounds.minY)). Add origin to image coordinates for clicks.", images: ["data:image/jpeg;base64," + (data as Data).base64EncodedString()])
    }
    public static func click(x: Double, y: Double, right: Bool = false, double: Bool = false) throws -> ToolResult {
        try checkAccessibility()
        guard x.isFinite, y.isFinite else { throw FunctionError.invalidInput("Coordinates must be finite.") }
        let position = CGPoint(x: x, y: y)
        let button: CGMouseButton = right ? .right : .left
        for count in 1...(double ? 2 : 1) {
            let downType: CGEventType = right ? .rightMouseDown : .leftMouseDown
            let upType: CGEventType = right ? .rightMouseUp : .leftMouseUp
            guard let down = CGEvent(mouseEventSource: nil, mouseType: downType, mouseCursorPosition: position, mouseButton: button),
                  let up = CGEvent(mouseEventSource: nil, mouseType: upType, mouseCursorPosition: position, mouseButton: button) else {
                throw FunctionError.invalidInput("Could not create mouse events.")
            }
            down.setIntegerValueField(.mouseEventClickState, value: Int64(count))
            up.setIntegerValueField(.mouseEventClickState, value: Int64(count))
            down.post(tap: .cghidEventTap); up.post(tap: .cghidEventTap)
        }
        return ToolResult("Clicked (\(x), \(y)).")
    }
    public static func move(x: Double, y: Double) throws -> ToolResult {
        try checkAccessibility()
        guard x.isFinite, y.isFinite, let event = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: CGPoint(x: x, y: y), mouseButton: .left) else {
            throw FunctionError.invalidInput("Invalid mouse coordinate.")
        }
        event.post(tap: .cghidEventTap); return ToolResult("Mouse moved.")
    }
    public static func type(_ text: String) throws -> ToolResult {
        try checkAccessibility()
        guard text.count <= 10_000 else { throw FunctionError.invalidInput("Text is too long for one action.") }
        for chunk in KeyboardText.chunks(text) {
            for down in [true, false] {
                guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down) else { throw FunctionError.invalidInput("Could not create keyboard event.") }
                chunk.withUnsafeBufferPointer { event.keyboardSetUnicodeString(stringLength: chunk.count, unicodeString: $0.baseAddress) }
                event.post(tap: .cghidEventTap)
            }
        }
        return ToolResult("Typed \(text.count) characters.")
    }
    public static func press(_ shortcut: String) throws -> ToolResult {
        try checkAccessibility()
        let parts = shortcut.lowercased().split(separator: "+").map(String.init)
        let codes: [String: CGKeyCode] = ["a":0, "s":1, "d":2, "f":3, "h":4, "g":5, "z":6, "x":7, "c":8, "v":9, "b":11,
            "q":12, "w":13, "e":14, "r":15, "y":16, "t":17, "1":18, "2":19, "3":20, "4":21, "6":22, "5":23, "9":25, "7":26, "8":28, "0":29,
            "o":31, "u":32, "i":34, "p":35, "return":36, "enter":36, "l":37, "j":38, "k":40, "n":45, "m":46, "tab":48, "space":49,
            "delete":51, "backspace":51, "escape":53, "esc":53, "left":123, "right":124, "down":125, "up":126, "home":115, "end":119, "pageup":116, "pagedown":121]
        guard let key = parts.last, let code = codes[key] else { throw FunctionError.invalidInput("Unsupported key '\(shortcut)'.") }
        var flags: CGEventFlags = []
        for modifier in parts.dropLast() {
            switch modifier {
            case "cmd", "command": flags.insert(.maskCommand)
            case "ctrl", "control": flags.insert(.maskControl)
            case "alt", "option": flags.insert(.maskAlternate)
            case "shift": flags.insert(.maskShift)
            default: throw FunctionError.invalidInput("Unsupported modifier '\(modifier)'.")
            }
        }
        for down in [true, false] {
            guard let event = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down) else { throw FunctionError.invalidInput("Could not create keyboard event.") }
            event.flags = flags; event.post(tap: .cghidEventTap)
        }
        return ToolResult("Pressed \(shortcut).")
    }
    public static func scroll(vertical: Double, horizontal: Double) throws -> ToolResult {
        try checkAccessibility()
        guard vertical.isFinite, horizontal.isFinite, abs(vertical) <= 10_000, abs(horizontal) <= 10_000,
              let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2, wheel1: Int32(vertical), wheel2: Int32(horizontal), wheel3: 0) else {
            throw FunctionError.invalidInput("Scroll amounts must be between -10,000 and 10,000.")
        }
        event.post(tap: .cghidEventTap); return ToolResult("Scrolled.")
    }
}
#endif
