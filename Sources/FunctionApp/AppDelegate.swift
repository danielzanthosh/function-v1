#if os(macOS)
import AppKit
import SwiftUI
import FunctionPlatform

final class AssistantPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()
    private let hotkey = HotkeyController()
    private var panel: AssistantPanel?
    private var previousApplication: NSRunningApplication?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        let panel = AssistantPanel(contentRect: NSRect(x: 0, y: 0, width: 560, height: 110),
                                   styleMask: [.titled, .fullSizeContentView, .utilityWindow], backing: .buffered, defer: false)
        panel.title = "Function"; panel.titleVisibility = .hidden; panel.titlebarAppearsTransparent = true
        panel.isMovableByWindowBackground = true; panel.isReleasedWhenClosed = false
        panel.level = .floating; panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.contentView = NSHostingView(rootView: AssistantView(model: model))
        self.panel = panel
        model.onLayoutChanged = { [weak self] in self?.resizePanel() }
        model.hidePanel = { [weak self] in self?.panel?.orderOut(nil) }
        model.releaseFocus = { [weak self] in
            self?.panel?.orderOut(nil)
            self?.previousApplication?.activate()
        }
        model.onNeedsAttention = { [weak self] in self?.showPanel(captureApplication: true) }
        model.onConfigurationSaved = { [weak self] in self?.configureHotkey() }
        configureHotkey()
        if !model.configuration.startHidden || model.errorMessage != nil { showPanel() }
    }
    func applicationWillTerminate(_ notification: Notification) { model.cancel(); hotkey.stop() }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func configureHotkey() {
        hotkey.start(shortcut: model.configuration.hotkey) { [weak self] in
            guard let self else { return }
            if self.panel?.isVisible == true { self.panel?.orderOut(nil) } else { self.showPanel() }
        }
    }
    private func resizePanel() {
        guard let panel else { return }
        let height = 102 + (model.expanded ? 341 : 0) + model.suggestions.count * 36
            + (model.pendingApproval == nil ? 0 : 190) + (model.errorMessage == nil ? 0 : 90)
            + (model.calculation == nil ? 0 : 50)
        let top = panel.frame.maxY
        panel.setContentSize(NSSize(width: 560, height: height))
        panel.setFrameOrigin(NSPoint(x: panel.frame.minX, y: top - panel.frame.height))
    }
    func showPanel(captureApplication: Bool = true) {
        if captureApplication, let frontmost = NSWorkspace.shared.frontmostApplication,
           frontmost.processIdentifier != ProcessInfo.processInfo.processIdentifier { previousApplication = frontmost }
        guard let panel, let screen = NSScreen.main else { return }
        resizePanel()
        let frame = screen.visibleFrame
        let fraction = model.configuration.windowPosition == .upperThird ? 0.68 : 0.5
        panel.setFrameOrigin(NSPoint(x: frame.midX - panel.frame.width / 2, y: frame.minY + frame.height * fraction - panel.frame.height / 2))
        NSApp.activate(); panel.makeKeyAndOrderFront(nil)
    }
}
#endif
