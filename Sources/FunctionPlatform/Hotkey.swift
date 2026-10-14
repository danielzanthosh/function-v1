#if os(macOS)
import AppKit
import FunctionCore

@MainActor
public final class HotkeyController {
    private var globalMonitor: Any?
    private var localMonitor: Any?
    private var lastCommandTap: TimeInterval = 0
    private var commandDown = false
    private var commandUsed = false
    public init() {}
    public func start(shortcut: String, action: @escaping @MainActor () -> Void) {
        stop()
        let receive: @MainActor (NSEvent) -> Void = { [weak self] event in
            guard let self else { return }
            let modifiers = event.modifierFlags.intersection([.command, .control, .option, .shift])
            if shortcut == "Double Command" {
                if event.type == .keyDown { self.commandUsed = true; self.lastCommandTap = 0; return }
                let down = modifiers.contains(.command)
                if down && !self.commandDown { self.commandUsed = !modifiers.subtracting(.command).isEmpty }
                if !down && self.commandDown && !self.commandUsed {
                    if self.lastCommandTap > 0 && event.timestamp - self.lastCommandTap < 0.35 {
                        self.lastCommandTap = 0; action()
                    } else { self.lastCommandTap = event.timestamp }
                }
                self.commandDown = down
            } else if event.type == .keyDown && event.keyCode == 49 && !event.isARepeat {
                let desired: NSEvent.ModifierFlags = shortcut == "Command+Space" ? .command : shortcut == "Option+Space" ? .option : .control
                if modifiers == desired { action() }
            }
        }
        globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.flagsChanged, .keyDown]) { event in
            MainActor.assumeIsolated { receive(event) }
        }
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: [.flagsChanged, .keyDown]) { event in
            MainActor.assumeIsolated { receive(event) }; return event
        }
    }
    public func stop() {
        if let globalMonitor { NSEvent.removeMonitor(globalMonitor) }
        if let localMonitor { NSEvent.removeMonitor(localMonitor) }
        globalMonitor = nil; localMonitor = nil; lastCommandTap = 0; commandDown = false; commandUsed = false
    }
}
#endif
