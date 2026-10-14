#if os(macOS)
import SwiftUI
import FunctionCore
import FunctionPlatform

struct SettingsView: View {
    @Bindable var model: AppModel
    @State private var saved = false
    @State private var confirmingImport = false
    var body: some View {
        TabView {
            Form {
                Section("AI provider") {
                    Picker("Provider", selection: $model.configuration.provider.name) {
                        Text("OpenAI compatible").tag("openai-compatible")
                        Text("Gemini").tag("gemini")
                        Text("ChatGPT plan").tag("chatgpt-plan")
                    }
                    if model.configuration.provider.name == "chatgpt-plan" {
                        Button("Sign in to ChatGPT", action: model.beginChatGPTSignIn).disabled(model.signingIn)
                        if model.oauthLogin != nil {
                            SecureField("Browser callback URL", text: $model.oauthCallback)
                            Button("Complete sign-in", action: model.finishChatGPTSignIn).disabled(model.signingIn)
                        }
                        Button("Sign out", action: model.signOutChatGPT).disabled(model.signingIn)
                        Text(model.authenticationStatus).font(.caption).foregroundStyle(.secondary)
                        Text("Use a model available to your ChatGPT account. The subscription endpoint can change independently of Function.").font(.caption).foregroundStyle(.secondary)
                    }
                    TextField("Base URL", text: $model.configuration.provider.baseURL)
                    TextField("Model", text: $model.configuration.provider.model)
                    SecureField("API key (Keychain)", text: $model.apiKeyDraft)
                    Text("For local models, use their OpenAI compatible endpoint. A key may be left empty.").font(.caption).foregroundStyle(.secondary)
                }
                Section("Agent") {
                    Stepper("Action limit: \(model.configuration.maxSteps)", value: $model.configuration.maxSteps, in: 1...100)
                    Picker("Context budget", selection: $model.configuration.inputTokenLimit) {
                        ForEach([2_000, 4_000, 8_000, 16_000, 32_000, 64_000, 128_000], id: \.self) { limit in Text("\(limit / 1_000)K tokens").tag(limit) }
                    }
                    Toggle("Optimize context", isOn: $model.configuration.contextOptimizationEnabled)
                    Picker("Delay between requests", selection: $model.configuration.requestDelayMilliseconds) {
                        ForEach([0, 250, 500, 1_000, 2_000, 5_000, 10_000], id: \.self) { delay in Text(delay == 0 ? "None" : "\(delay) ms").tag(delay) }
                    }
                    Toggle("Persistent memory", isOn: $model.configuration.memoryEnabled)
                    Toggle("Web search (DuckDuckGo)", isOn: $model.configuration.searchEnabled)
                }
            }.tabItem { Label("AI", systemImage: "sparkle") }
            Form {
                Section("Speech to text") {
                    Toggle("Enable microphone", isOn: $model.configuration.speech.enabled)
                    TextField("Speech base URL", text: $model.configuration.speech.baseURL)
                    TextField("Transcription model", text: $model.configuration.speech.model)
                    SecureField("Speech API key (Keychain)", text: $model.speechKeyDraft)
                }
                Section("Text to speech") {
                    Toggle("Read replies aloud", isOn: $model.configuration.speech.readReplies)
                    Toggle("Use macOS voice", isOn: $model.configuration.speech.useSystemVoice)
                    TextField("TTS base URL", text: $model.configuration.speech.ttsBaseURL)
                    SecureField("TTS API key (Keychain)", text: $model.ttsKeyDraft)
                    TextField("Speech model", text: $model.configuration.speech.ttsModel)
                    TextField("Voice", text: $model.configuration.speech.voice)
                }
            }.tabItem { Label("Voice", systemImage: "mic") }
            Form {
                Picker("Theme", selection: $model.configuration.theme) {
                    ForEach(ThemeStyle.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }
                Picker("Accent", selection: $model.configuration.accent) {
                    ForEach(AccentColor.allCases, id: \.self) { Text($0.rawValue.capitalized).tag($0) }
                }
                Picker("Position", selection: $model.configuration.windowPosition) {
                    ForEach(WindowPosition.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }
                Picker("Global shortcut", selection: $model.configuration.hotkey) {
                    ForEach(["Double Command", "Control+Space", "Option+Space", "Command+Space"], id: \.self) { Text($0).tag($0) }
                }
                Toggle("Start hidden", isOn: $model.configuration.startHidden)
                Toggle("Completion sound", isOn: $model.configuration.soundEnabled)
                Text("Global shortcuts require Accessibility permission. Choose a shortcut that does not conflict with macOS.").font(.caption).foregroundStyle(.secondary)
            }.tabItem { Label("Appearance", systemImage: "paintbrush") }
            Form {
                Section("Permissions") {
                    Button("Request Accessibility permission", action: Desktop.requestAccessibility)
                    Text("Enable Screen Recording when prompted by the first screenshot, and Microphone when prompted by the first recording.")
                }
                Section("Migration") {
                    Button("Import Rust version settings, memory, and history") { confirmingImport = true }
                        .confirmationDialog("Replace native settings, memory, and history with the legacy data? Native data is backed up and the original files are preserved.", isPresented: $confirmingImport) {
                            Button("Import legacy data", action: model.importLegacy)
                        }
                }
                Section("Privacy") {
                    Text("Prompts, tool results, and screenshots go to your selected provider. API keys stay in macOS Keychain. Memory and conversation history stay in ~/.function/swift.")
                    Text("Every action that changes the desktop, files, or memory requires your approval.")
                }
            }.tabItem { Label("Privacy", systemImage: "lock") }
        }
        .formStyle(.grouped)
        .padding()
        .frame(width: 620, height: 520)
        .disabled(model.busy)
        .safeAreaInset(edge: .bottom) {
            HStack {
                if saved { Text("Saved").foregroundStyle(.secondary) }
                if let error = model.errorMessage { Text(error).font(.caption).foregroundStyle(.red) }
                Spacer()
                Button("Save settings") { model.errorMessage = nil; model.saveSettings(); saved = model.errorMessage == nil }
                    .buttonStyle(.borderedProminent).disabled(model.busy)
            }.padding()
        }
    }
}
#endif
