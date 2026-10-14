#if os(macOS)
import SwiftUI
import AppKit
import FunctionCore
import FunctionPlatform

struct AssistantView: View {
    @Bindable var model: AppModel
    @Environment(\.openSettings) private var openSettings
    @FocusState private var inputFocused: Bool
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                FunctionLogo().frame(width: 24, height: 24)
                TextField("Ask Function or type a command…", text: $model.input)
                    .textFieldStyle(.plain).font(.system(size: 16)).focused($inputFocused)
                    .onSubmit(submit).disabled(model.busy)
                    .onChange(of: model.input) { _, _ in model.search() }
                if model.busy && model.state != .listening {
                    Button("Stop", systemImage: "stop.fill", action: model.cancel).labelStyle(.iconOnly).help("Stop task")
                } else {
                    Button(model.state == .listening ? "Stop recording" : "Record voice", systemImage: model.state == .listening ? "stop.circle.fill" : "mic", action: model.toggleRecording)
                        .labelStyle(.iconOnly).disabled(!model.configuration.speech.enabled)
                }
                Button("Expand", systemImage: model.expanded ? "chevron.up" : "chevron.down") { model.expanded.toggle() }.labelStyle(.iconOnly)
            }
            .padding(18)
            if let value = model.calculation, !model.busy {
                Button { model.submit() } label: {
                    HStack { Image(systemName: "equal"); Text(value.formatted()); Spacer(); Text("↵ Copy result").foregroundStyle(.secondary) }.padding()
                }.buttonStyle(.plain)
            }
            if !model.busy && !model.suggestions.isEmpty {
                VStack(spacing: 0) {
                    ForEach(model.suggestions) { item in
                        Button { model.openSuggestion(item) } label: {
                            HStack {
                                Image(systemName: "app").frame(width: 24)
                                Text(item.title); Spacer(); Text("Open").foregroundStyle(.secondary)
                            }.padding(.horizontal, 20).padding(.vertical, 8).contentShape(Rectangle())
                        }.buttonStyle(.plain)
                    }
                }
            }
            if let approval = model.pendingApproval {
                ApprovalView(call: approval, approve: model.approve).padding(16)
            }
            if model.expanded {
                Divider()
                ConversationView(messages: model.conversation.messages, activities: model.activities)
                    .frame(height: 340)
            }
            if let error = model.errorMessage {
                HStack(alignment: .top) {
                    Image(systemName: "exclamationmark.triangle")
                    Text(error).textSelection(.enabled).font(.callout)
                    Spacer()
                    Button("Dismiss", systemImage: "xmark") { model.errorMessage = nil }.labelStyle(.iconOnly)
                }.foregroundStyle(.red).padding(14)
            }
            Divider()
            HStack(spacing: 14) {
                Circle().fill(statusColor).frame(width: 5, height: 5)
                Text(model.state.label).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                Spacer()
                Button("New conversation", systemImage: "plus", action: model.newConversation).labelStyle(.iconOnly).help("New conversation")
                Button("History", systemImage: "clock") { model.showHistory = true }.labelStyle(.iconOnly)
                Button("Browse files", systemImage: "folder", action: model.browseFile).labelStyle(.iconOnly)
                Button("Memory", systemImage: "brain") { model.showMemory = true; model.loadMemories() }.labelStyle(.iconOnly)
                SettingsLink { Image(systemName: "gearshape").accessibilityLabel("Settings") }
            }.padding(.horizontal, 18).padding(.vertical, 10)
        }
        .buttonStyle(.borderless)
        .frame(width: 560)
        .background(Palette.background(model.configuration.theme))
        .tint(Palette.accent(model.configuration.accent, light: model.configuration.theme == .studioLight))
        .preferredColorScheme(model.configuration.theme == .studioLight ? .light : .dark)
        .onAppear { inputFocused = true; model.onLayoutChanged?() }
        .onChange(of: model.expanded) { _, _ in model.onLayoutChanged?() }
        .onChange(of: model.suggestions.count) { _, _ in model.onLayoutChanged?() }
        .onChange(of: model.pendingApproval) { _, _ in model.onLayoutChanged?() }
        .onChange(of: model.errorMessage) { _, _ in model.onLayoutChanged?() }
        .onChange(of: model.calculation) { _, _ in model.onLayoutChanged?() }
        .onExitCommand { if model.busy { model.cancel() } else { model.hidePanel?() } }
        .sheet(isPresented: $model.showHistory) { HistoryView(model: model) }
        .sheet(isPresented: $model.showMemory) { MemoryView(model: model) }
    }
    private var statusColor: Color {
        switch model.state {
        case .failed: .red
        case .completed: .green
        case .listening: .orange
        default: .secondary
        }
    }
    private func submit() {
        if LocalCommand.parse(model.input) == .settings { model.input = ""; openSettings() }
        else { model.submit() }
    }
}

struct FunctionLogo: View {
    var body: some View {
        if let url = (Bundle.main.url(forResource: "icon", withExtension: "png") ?? Bundle.module.url(forResource: "icon", withExtension: "png")), let image = NSImage(contentsOf: url) {
            Image(nsImage: image).resizable().scaledToFit().accessibilityLabel("Function")
        } else { Image(systemName: "asterisk").accessibilityLabel("Function") }
    }
}

enum Palette {
    static func background(_ theme: ThemeStyle) -> Color {
        switch theme {
        case .studioLight: Color(red: 0.969, green: 0.965, blue: 0.957)
        case .obsidianOLED: .black
        case .slateMidnight: Color(red: 0.065, green: 0.07, blue: 0.085)
        case .carbonDark: Color(red: 0.078, green: 0.071, blue: 0.039)
        }
    }
    static func accent(_ accent: AccentColor, light: Bool) -> Color {
        switch accent { case .white: light ? .black : .white; case .cyan: .cyan; case .emerald: .green; case .violet: .purple; case .amber: .orange }
    }
}

struct ApprovalView: View {
    let call: ToolCall
    let approve: (Bool) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Approve \(call.name.replacingOccurrences(of: "_", with: " "))?", systemImage: "hand.raised")
                .font(.headline)
            ScrollView {
                Text((try? call.arguments.encodedString()) ?? "Invalid arguments")
                    .font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
            }.frame(maxHeight: 100)
            HStack {
                Button("Deny") { approve(false) }.keyboardShortcut(.escape, modifiers: [])
                Spacer()
                Button("Approve once") { approve(true) }.buttonStyle(.borderedProminent)
            }
        }
    }
}

struct ConversationView: View {
    let messages: [ChatMessage]
    let activities: [Activity]
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(messages) { message in
                        MessageRow(message: message)
                    }
                    if !activities.isEmpty {
                        DisclosureGroup("Activity · \(activities.count) actions") {
                            ForEach(activities) { activity in
                                VStack(alignment: .leading, spacing: 4) {
                                    Label(activity.name.replacingOccurrences(of: "_", with: " "), systemImage: activity.success ? "checkmark" : "exclamationmark.triangle")
                                    Text(activity.detail).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                                }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 4)
                            }
                        }
                    }
                    Color.clear.frame(height: 1).id("end")
                }.padding(20)
            }.onChange(of: messages.count) { _, _ in proxy.scrollTo("end", anchor: .bottom) }
        }
    }
}

struct MessageRow: View {
    let message: ChatMessage
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(message.role == .user ? "YOU" : "FUNCTION").font(.system(size: 10, weight: .semibold, design: .monospaced)).foregroundStyle(.secondary)
            if message.role == .user { Text(message.content).textSelection(.enabled) }
            else { Text(.init(message.content)).textSelection(.enabled) }
        }.frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct HistoryView: View {
    @Bindable var model: AppModel
    var body: some View {
        VStack(alignment: .leading) {
            HStack { Text("Conversations").font(.title2); Spacer(); Button("Done") { model.showHistory = false } }.padding()
            List(model.conversations) { conversation in
                HStack {
                    Button { model.restore(conversation) } label: {
                        VStack(alignment: .leading) { Text(conversation.title); Text(conversation.updatedAt, style: .date).font(.caption).foregroundStyle(.secondary) }
                    }.buttonStyle(.plain)
                    Spacer()
                    DeleteConversationButton(conversation: conversation, delete: model.deleteConversation)
                }
            }
            Button("New conversation", action: model.newConversation).padding()
        }.frame(width: 540, height: 400)
    }
}

struct DeleteConversationButton: View {
    let conversation: Conversation
    let delete: (Conversation) -> Void
    @State private var confirming = false
    var body: some View {
        Button("Delete", systemImage: "trash") { confirming = true }.labelStyle(.iconOnly)
            .confirmationDialog("Delete this conversation?", isPresented: $confirming) {
                Button("Delete conversation", role: .destructive) { delete(conversation) }
            }
    }
}

struct MemoryView: View {
    @Bindable var model: AppModel
    @State private var confirmingClear = false
    var body: some View {
        VStack {
            HStack { Text("Memory").font(.title2); Spacer(); Button("Done") { model.showMemory = false } }.padding()
            List(model.memories) { item in
                HStack {
                    VStack(alignment: .leading) { Text(item.key).bold(); Text(item.value).foregroundStyle(.secondary) }
                    Spacer(); ForgetMemoryButton(item: item, forget: model.forgetMemory)
                }
            }
            Button("Clear all memory", role: .destructive) { confirmingClear = true }.padding()
                .confirmationDialog("Permanently clear all saved notes?", isPresented: $confirmingClear) {
                    Button("Clear memory", role: .destructive, action: model.clearMemory)
                }
        }.frame(width: 540, height: 400)
    }
}

struct ForgetMemoryButton: View {
    let item: MemoryItem
    let forget: (MemoryItem) -> Void
    @State private var confirming = false
    var body: some View {
        Button("Forget", systemImage: "trash") { confirming = true }.labelStyle(.iconOnly)
            .confirmationDialog("Forget this note?", isPresented: $confirming) {
                Button("Forget", role: .destructive) { forget(item) }
            }
    }
}
#endif
