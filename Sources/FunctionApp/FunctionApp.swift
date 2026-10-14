#if os(macOS)
import SwiftUI
import AppKit
import FunctionCore
import FunctionPlatform

@main
struct FunctionApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    var body: some Scene {
        Settings { SettingsView(model: delegate.model) }
        MenuBarExtra("Function", systemImage: "asterisk") {
            Button("Show Function") { delegate.showPanel() }
            Button("New conversation") { delegate.model.newConversation(); delegate.showPanel() }
            SettingsLink { Text("Settings…") }
            Divider()
            Button("Quit Function") { NSApp.terminate(nil) }
        }
    }
}
#else
import Foundation
@main
struct FunctionApp {
    static func main() {
        print("Function's SwiftUI desktop app requires macOS 14 or later. The Swift core supports Linux for testing.")
    }
}
#endif
