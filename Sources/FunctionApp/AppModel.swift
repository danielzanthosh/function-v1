#if os(macOS)
import SwiftUI
import AppKit
import FunctionCore
import FunctionPlatform

@Observable @MainActor
final class AppModel {
    var configuration = AppConfiguration()
    var input = ""
    var state: AgentState = .idle
    var conversation = Conversation()
    var conversations: [Conversation] = []
    var activities: [Activity] = []
    var memories: [MemoryItem] = []
    var suggestions: [LauncherItem] = []
    var calculation: Double?
    var expanded = false
    var showHistory = false
    var showMemory = false
    var errorMessage: String?
    var pendingApproval: ToolCall?
    var apiKeyDraft = ""
    var speechKeyDraft = ""
    var ttsKeyDraft = ""
    var oauthCallback = ""
    var oauthLogin: ChatGPTAuthentication.Login?
    var signingIn = false
    var authenticationStatus = ""
    @ObservationIgnored private let chatGPT = ChatGPTAuthentication()
    private(set) var busy = false
    @ObservationIgnored private let storage = JSONFileStore()
    @ObservationIgnored private let credentials = KeychainStore()
    @ObservationIgnored private let voice = VoiceController()
    @ObservationIgnored private let launcher = LauncherIndex()
    @ObservationIgnored private var memory: MemoryStore?
    @ObservationIgnored private var runTask: Task<Void, Never>?
    @ObservationIgnored private var runID = UUID()
    @ObservationIgnored private var approvalContinuation: CheckedContinuation<Bool, any Error>?
    @ObservationIgnored var onLayoutChanged: (() -> Void)?
    @ObservationIgnored var onNeedsAttention: (() -> Void)?
    @ObservationIgnored var onConfigurationSaved: (() -> Void)?
    @ObservationIgnored var hidePanel: (() -> Void)?
    @ObservationIgnored var releaseFocus: (() -> Void)?
    @ObservationIgnored private var credentialsLoaded = true
    @ObservationIgnored private var historyWritable = true
    @ObservationIgnored private var searchTask: Task<Void, Never>?

    init() {
        var errors: [String] = []
        do { configuration = try storage.load(AppConfiguration.self, from: "config.json", default: AppConfiguration()) }
        catch { errors.append("Settings: \(error.localizedDescription)") }
        do { conversations = try storage.load([Conversation].self, from: "conversations.json", default: []) }
        catch { errors.append("History: \(error.localizedDescription)"); historyWritable = false }
        do { memory = try MemoryStore(storage: storage) }
        catch { errors.append("Memory: \(error.localizedDescription)"); configuration.memoryEnabled = false }
        do {
            apiKeyDraft = try credentials.read("ai_api_key") ?? ""
            speechKeyDraft = try credentials.read("speech_api_key") ?? ""
            ttsKeyDraft = try credentials.read("tts_api_key") ?? ""
        } catch { errors.append("Keychain: \(error.localizedDescription)"); credentialsLoaded = false }
        if !errors.isEmpty { errorMessage = errors.joined(separator: "\n") }
    }
    func beginChatGPTSignIn() {
        guard !signingIn else { return }
        signingIn = true
        Task {
            defer { signingIn = false }
            do {
                let login = try await chatGPT.begin()
                oauthLogin = login; oauthCallback = ""
                guard NSWorkspace.shared.open(login.url) else { throw FunctionError.invalidInput("Could not open sign-in page.") }
                authenticationStatus = "After signing in, copy the localhost callback URL from the browser address bar and paste it below."
            } catch { errorMessage = error.localizedDescription }
        }
    }
    func finishChatGPTSignIn() {
        guard let login = oauthLogin, !signingIn else { return }
        signingIn = true
        let callback = oauthCallback
        Task {
            defer { signingIn = false }
            do {
                try await chatGPT.finish(login, callback: callback)
                oauthLogin = nil; oauthCallback = ""; authenticationStatus = "Signed in. Save settings to select this provider."
            } catch { errorMessage = error.localizedDescription }
        }
    }
    func signOutChatGPT() {
        Task {
            do { try await chatGPT.signOut(); oauthLogin = nil; oauthCallback = ""; authenticationStatus = "Signed out." }
            catch { errorMessage = error.localizedDescription }
        }
    }
    func importLegacy() {
        guard !busy else { return }
        do {
            let marker = storage.directory.appendingPathComponent("legacy-imported.json")
            guard !FileManager.default.fileExists(atPath: marker.path) else { throw FunctionError.storage("Legacy data was already imported. Original files remain in ~/.function.") }
            let imported = try LegacyImport(directory: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".function"))
            for item in imported.memories { try MemoryStore.validate(item) }
            let key = imported.aiKey ?? apiKeyDraft
            let speechKey = imported.speechKey ?? speechKeyDraft
            let ttsKey = imported.ttsKey ?? ttsKeyDraft
            // Backup existing native data before an explicit migration.
            let backup = storage.directory.appendingPathComponent("backup-" + UUID().uuidString, isDirectory: true)
            for name in ["config.json", "memory.json", "conversations.json"] {
                let source = storage.directory.appendingPathComponent(name)
                if FileManager.default.fileExists(atPath: source.path) {
                    try FileManager.default.createDirectory(at: backup, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
                    try FileManager.default.copyItem(at: source, to: backup.appendingPathComponent(name))
                }
            }
            try credentials.write(key, account: "ai_api_key"); try credentials.write(speechKey, account: "speech_api_key"); try credentials.write(ttsKey, account: "tts_api_key")
            try storage.save(imported.memories, to: "memory.json")
            try storage.save(imported.conversations, to: "conversations.json")
            try storage.save(imported.configuration, to: "config.json")
            try storage.save(true, to: "legacy-imported.json")
            configuration = imported.configuration; conversations = imported.conversations; historyWritable = true; credentialsLoaded = true
            apiKeyDraft = key; speechKeyDraft = speechKey; ttsKeyDraft = ttsKey; memory = try MemoryStore(storage: storage)
            conversation = Conversation(); onConfigurationSaved?(); loadMemories()
            authenticationStatus = "Imported legacy data. Original files were preserved."
        } catch { errorMessage = "Import failed: \(error.localizedDescription)" }
    }
    func saveSettings() {
        guard !busy else { errorMessage = "Stop the current task before changing settings."; return }
        do {
            guard credentialsLoaded else { throw FunctionError.storage("Keychain could not be read. Unlock it and relaunch before saving settings.") }
            if configuration.memoryEnabled && memory == nil { memory = try MemoryStore(storage: storage) }
            try configuration.validate()
            try credentials.write(apiKeyDraft.trimmingCharacters(in: .whitespacesAndNewlines), account: "ai_api_key")
            try credentials.write(speechKeyDraft.trimmingCharacters(in: .whitespacesAndNewlines), account: "speech_api_key")
            try credentials.write(ttsKeyDraft.trimmingCharacters(in: .whitespacesAndNewlines), account: "tts_api_key")
            try storage.save(configuration, to: "config.json")
            onConfigurationSaved?()
        } catch { errorMessage = error.localizedDescription }
    }
    func search() {
        searchTask?.cancel()
        let query = input
        calculation = Calculator.evaluate(query)
        searchTask = Task { [weak self, launcher] in
            try? await Task.sleep(for: .milliseconds(120))
            guard !Task.isCancelled else { return }
            let results = await launcher.search(query)
            guard !Task.isCancelled, self?.input == query else { return }
            self?.suggestions = results
        }
    }
    func openSuggestion(_ item: LauncherItem) {
        guard !busy else { return }
        if NSWorkspace.shared.open(item.url) { input = ""; suggestions = []; hidePanel?() }
        else { errorMessage = "Could not open \(item.title)." }
    }
    func browseFile() {
        let picker = NSOpenPanel(); picker.canChooseDirectories = true; picker.canChooseFiles = true
        picker.begin { [weak self] response in
            Task { @MainActor in
                guard response == .OK, let url = picker.url else { return }
                self?.openSuggestion(LauncherItem(url: url))
            }
        }
    }
    func submit() {
        guard !busy else { return }
        let prompt = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !prompt.isEmpty else { return }
        if let value = Calculator.evaluate(prompt) {
            NSPasteboard.general.clearContents(); NSPasteboard.general.setString(String(value), forType: .string)
            input = ""; calculation = nil; return
        }
        if let command = LocalCommand.parse(prompt), handle(command) { input = ""; return }
        input = ""; suggestions = []; expanded = true; errorMessage = nil; activities = []
        let history = conversation.messages
        conversation.append(ChatMessage(role: .user, content: prompt))
        persistConversation()
        let id = UUID(); runID = id; busy = true; state = .processing
        let config = configuration
        runTask = Task { [weak self] in
            guard let self else { return }
            defer { if self.runID == id { self.busy = false; self.runTask = nil } }
            do {
                let key = try self.credentials.read("ai_api_key") ?? ""
                let environmentKey = ProcessInfo.processInfo.environment[config.provider.name == "gemini" ? "GEMINI_API_KEY" : "OPENAI_API_KEY"] ?? ""
                let provider: any CompletionProvider
                if config.provider.name == "chatgpt-plan" {
                    let authentication = self.chatGPT
                    provider = ResponsesProvider(model: config.provider.model) { try await authentication.authorization() }
                } else {
                    provider = OpenAIProvider(configuration: config.provider, apiKey: key.isEmpty ? environmentKey : key)
                }
                let agent = Agent(provider: provider, executor: NativeTools(), memory: self.memory, configuration: config)
                let reply = try await agent.run(prompt: prompt, history: history, state: { [weak self] state in
                    await self?.setState(state, runID: id)
                }, activity: { [weak self] activity in
                    await self?.addActivity(activity, runID: id)
                }, confirm: { [weak self] call in
                    guard let self else { throw CancellationError() }
                    return try await self.confirm(call, runID: id)
                })
                try Task.checkCancellation()
                guard self.runID == id else { return }
                self.conversation.append(ChatMessage(role: .assistant, content: reply)); self.persistConversation()
                if config.soundEnabled { NSSound(named: "Glass")?.play() }
                if config.speech.readReplies {
                    if config.speech.useSystemVoice { self.voice.speak(reply) }
                    else {
                        let key = try self.credentials.read("tts_api_key") ?? ""
                        let audio = try await SpeechProvider(configuration: config.speech, apiKey: key).synthesize(reply)
                        try Task.checkCancellation(); try self.voice.play(audio)
                    }
                }
            } catch is CancellationError {
                if self.runID == id { self.state = .idle }
            } catch {
                if self.runID == id { self.state = .failed(error.localizedDescription); self.errorMessage = error.localizedDescription; self.onNeedsAttention?() }
            }
        }
    }
    private func setState(_ state: AgentState, runID: UUID) {
        if self.runID == runID {
            self.state = state
            if state == .waitingForConfirmation || state == .completed { onNeedsAttention?() }
        }
    }
    private func addActivity(_ activity: Activity, runID: UUID) { if self.runID == runID { activities.append(activity) } }
    private func confirm(_ call: ToolCall, runID: UUID) async throws -> Bool {
        try Task.checkCancellation()
        guard self.runID == runID else { throw CancellationError() }
        let allowed = try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                approvalContinuation = continuation; pendingApproval = call; onNeedsAttention?()
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                guard self?.runID == runID else { return }
                self?.resolveApproval(cancelled: true)
            }
        }
        if allowed { try await Task.sleep(for: .milliseconds(200)) }
        return allowed
    }
    func approve(_ allowed: Bool) { resolveApproval(allowed: allowed) }
    private func resolveApproval(allowed: Bool = false, cancelled: Bool = false) {
        let continuation = approvalContinuation
        approvalContinuation = nil; pendingApproval = nil
        if cancelled { continuation?.resume(throwing: CancellationError()) }
        else { continuation?.resume(returning: allowed); if allowed { releaseFocus?() } }
    }
    func cancel() {
        runTask?.cancel(); resolveApproval(cancelled: true); voice.cancel()
        // Invalidate callbacks before allowing another task to start.
        runID = UUID(); runTask = nil; busy = false; state = .idle
    }
    func newConversation() {
        cancel(); persistConversation(); conversation = Conversation(); activities = []; expanded = false; showHistory = false
    }
    func restore(_ saved: Conversation) { cancel(); conversation = saved; activities = []; showHistory = false; expanded = true }
    func deleteConversation(_ saved: Conversation) {
        guard historyWritable else { errorMessage = "History could not be loaded. Repair it before deleting conversations."; return }
        let updated = conversations.filter { $0.id != saved.id }
        do {
            try storage.save(updated, to: "conversations.json")
            conversations = updated
            if conversation.id == saved.id { cancel(); conversation = Conversation() }
        } catch { errorMessage = error.localizedDescription }
    }
    private func persistConversation() {
        guard !conversation.messages.isEmpty, historyWritable else { return }
        var updated = conversations.filter { $0.id != conversation.id }; updated.append(conversation)
        updated.sort { $0.updatedAt > $1.updatedAt }
        do { try storage.save(updated, to: "conversations.json"); conversations = updated }
        catch { errorMessage = "Could not save conversation: \(error.localizedDescription)" }
    }
    func loadMemories() { Task { memories = await memory?.recall(limit: 100) ?? [] } }
    func forgetMemory(_ item: MemoryItem) {
        Task { do { try await memory?.forget(item.id); loadMemories() } catch { errorMessage = error.localizedDescription } }
    }
    func clearMemory() { Task { do { try await memory?.clear(); loadMemories() } catch { errorMessage = error.localizedDescription } } }
    func toggleRecording() {
        if state == .listening {
            let id = runID
            state = .processing
            runTask = Task { [weak self] in
                guard let self else { return }
                defer { if self.runID == id { self.busy = false; self.runTask = nil } }
                do {
                    let data = try self.voice.stop()
                    let key = try self.credentials.read("speech_api_key") ?? ""
                    let text = try await SpeechProvider(configuration: self.configuration.speech, apiKey: key).transcribe(wav: data)
                    try Task.checkCancellation()
                    guard self.runID == id else { return }
                    self.input = text; self.state = .idle
                } catch { if self.runID == id { self.errorMessage = error.localizedDescription; self.state = .idle } }
            }
        } else {
            guard !busy, configuration.speech.enabled else { return }
            busy = true; let id = UUID(); runID = id
            runTask = Task { [weak self] in
                guard let self else { return }
                do { try await self.voice.start(); guard self.runID == id else { return }; self.state = .listening }
                catch { if self.runID == id { self.busy = false; self.state = .idle; self.errorMessage = error.localizedDescription } }
            }
        }
    }
    func handle(_ command: LocalCommand) -> Bool {
        switch command {
        case .settings: return false // The view opens the Settings scene.
        case .history: showHistory = true
        case .newConversation: newConversation()
        case .theme: configuration.theme = configuration.theme == .studioLight ? .carbonDark : .studioLight; saveSettings()
        case .voice: configuration.speech.enabled.toggle(); saveSettings()
        case .sound: configuration.soundEnabled.toggle(); saveSettings()
        case .clear: input = ""
        case .memory: showMemory = true; loadMemories()
        case .about: NSApplication.shared.orderFrontStandardAboutPanel(nil)
        case .help: errorMessage = "Commands: /settings, /conversations, /new, /theme, /voice, /sound, /clear, /memory, /model <name>. Prefix a shell command with >."
        case .model(let model): if model.isEmpty { errorMessage = "Current model: \(configuration.provider.model)" } else { configuration.provider.model = model; saveSettings() }
        case .shell(let command): runShell(command)
        }
        return true
    }
    private func runShell(_ command: String) {
        expanded = true; busy = true; let id = UUID(); runID = id
        let call = ToolCall(name: "execute_command", arguments: .object(["command": .string(command)]))
        runTask = Task {
            defer { if runID == id { busy = false; runTask = nil } }
            do {
                state = .waitingForConfirmation
                guard try await confirm(call, runID: id) else { state = .idle; return }
                let result = try await NativeTools().execute(call)
                try Task.checkCancellation(); guard runID == id else { return }
                activities.append(Activity(name: call.name, result: result))
                conversation.append(ChatMessage(role: .user, content: "> \(command)"))
                conversation.append(ChatMessage(role: .assistant, content: result.text)); persistConversation(); state = .completed
            } catch { if runID == id { state = .failed(error.localizedDescription) } }
        }
    }
}
#endif
