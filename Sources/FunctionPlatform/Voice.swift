#if os(macOS)
import AVFoundation
import FunctionCore

@MainActor
public final class VoiceController {
    private var recorder: AVAudioRecorder?
    private var recordingURL: URL?
    private var player: AVAudioPlayer?
    private let synthesizer = AVSpeechSynthesizer()
    public init() {}
    public func start() async throws {
        let granted = await AVCaptureDevice.requestAccess(for: .audio)
        try Task.checkCancellation()
        guard granted else { throw FunctionError.permission("Enable microphone access for Function in System Settings.") }
        stopSpeaking()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("function-\(UUID().uuidString).wav")
        let settings: [String: Any] = [AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: 16_000,
            AVNumberOfChannelsKey: 1, AVLinearPCMBitDepthKey: 16, AVLinearPCMIsFloatKey: false, AVLinearPCMIsBigEndianKey: false]
        do {
            let recorder = try AVAudioRecorder(url: url, settings: settings)
            guard recorder.record(forDuration: 120) else { throw FunctionError.invalidInput("Could not start microphone recording.") }
            self.recorder = recorder; recordingURL = url
        } catch {
            try? FileManager.default.removeItem(at: url)
            throw error
        }
    }
    public func stop() throws -> Data {
        recorder?.stop(); recorder = nil
        guard let url = recordingURL else { throw FunctionError.invalidInput("No voice recording is active.") }
        recordingURL = nil; defer { try? FileManager.default.removeItem(at: url) }
        return try Data(contentsOf: url)
    }
    public func cancel() {
        recorder?.stop(); recorder = nil
        if let recordingURL { try? FileManager.default.removeItem(at: recordingURL) }
        recordingURL = nil; stopSpeaking()
    }
    public func speak(_ text: String) {
        let utterance = AVSpeechUtterance(string: text)
        synthesizer.speak(utterance)
    }
    public func play(_ data: Data) throws { player = try AVAudioPlayer(data: data); player?.play() }
    public func stopSpeaking() { synthesizer.stopSpeaking(at: .immediate); player?.stop(); player = nil }
}
#endif
