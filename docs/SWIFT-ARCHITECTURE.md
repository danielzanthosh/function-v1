# Swift architecture

## Modules

| Target | Responsibility |
| --- | --- |
| FunctionCore | Sendable data models, JSON values, provider HTTP and wire formats, agent loop, tool catalog/policy, context budgets, atomic JSON storage, memory actor, legacy import, calculator, local commands |
| FunctionPlatform | Keychain, native ChatGPT authentication, AppKit/ScreenCaptureKit desktop tools, subprocess lifecycle, Spotlight launcher, AVFoundation voice, global hotkey monitors |
| FunctionApp | SwiftUI floating assistant, settings, activity, history and memory views; observable main-actor app state; NSPanel lifecycle and focus restoration |
| FunctionCoreTests | Deterministic Swift Testing checks with scripted providers/executors and URLProtocol HTTP fixtures |

The package uses only Apple and Swift standard libraries. No Rust FFI, embedded web UI, Codex CLI, Playwright, Node, or third-party Swift packages are required to run the app.

## Request lifecycle

One owned Task runs each request. The model snapshots configuration and history, retrieves relevant memory, requests a completion, validates enabled tool names, asks for approval when required, executes tools sequentially, and returns results/observations to the provider. The loop terminates on completion, cancellation, repeated actions, an error, or the configured step limit.

The main actor owns all view state. Run IDs prevent late callbacks from cancelled work from altering a newer run. Approval continuations are resumed once, cleared before resumption, and cancelled when a run ends. Approving an action hides the panel and restores the previously focused application. The next approval or final reply shows the panel again.

Context compaction removes complete earlier user turns and older screenshot payloads. Screenshots are marked as observations so they cannot be mistaken for a new user request. Tool calls and corresponding results remain grouped. The active request is rejected if it cannot fit the chosen budget; it is never silently discarded. Token estimation is approximate and providers may still reject context near their limits.

## Providers

OpenAI and Gemini use compatible chat completions. Tool call identifiers and Gemini thought signatures are round-tripped. ChatGPT uses native PKCE authentication, Keychain refresh tokens, and a Responses SSE adapter. OAuth callback transfer is manual, with validation of scheme, host, port, path, and random state. Subscription backend compatibility and live access require account validation; no claim is made that every model/account combination works.

Speech uses independent Whisper compatible transcription and speech-generation endpoints. Text-to-speech can instead use macOS voices. HTTP retries apply only to transient provider statuses, with bounded backoff; tool actions are not automatically replayed by the transport. HTTP errors report status without printing server response bodies that may echo sensitive data.

## Native platform

SwiftUI renders every screen. An NSPanel supplies floating/Spaces behavior unavailable in macOS 14 SwiftUI scenes, and NSEvent monitors supply global shortcuts. Accessibility controls use CGEvent; screenshots use ScreenCaptureKit and exclude Function's windows. Capture is one pixel per global macOS point to avoid Retina click scaling errors. The screenshot tool reports the image dimensions and origin. The main display is captured; additional monitors can still receive global-point mouse actions.

Spotlight queries run on the main run loop and are cancelled when input changes. Application paths are cached, with a file-picker fallback. Voice recordings use temporary PCM WAV files and are capped at two minutes.

## Security and storage

Settings contain no credential fields. Keys are stored in Keychain or supplied through the process environment. Memory rejects common secret patterns and requires explicit save requests/approval; this is a safeguard, not a secret detector. Configuration, history, and memory use atomic local JSON writes with private file permissions. Memory mutations update cache only after successful persistence.

All desktop mutations, shell commands, file mutations, and memory mutations require approval. Public page fetch, screenshot observation, read/list/exists file operations, and memory recall can run without approval. Protected system and common credential paths are blocked from file tools after resolving symlinks. Shell and UI control can access broader data after approval; they are privileged, not sandboxed capabilities. Inspect displayed arguments before approving.

## Migration

Legacy import reads the old configuration, array of memory records, and display-message history. It validates all data before import, moves API keys to Keychain, preserves old files, and backs up current native JSON. A completion marker prevents repeated imports. Import writes are atomic per file, not a multi-file transaction; failures are reported and backups remain available. Legacy OAuth sessions must be reauthenticated.

## macOS validation checklist

1. Build with Swift 6.2+, run `swift test`, and package the `.app`.
2. Open via menu bar before permissions; grant Accessibility and verify the global shortcut.
3. Verify compact/expanded sizing, settings, history restore/deletion, memory recall/deletion, and calculator.
4. Approve app launch, keyboard input, click, scroll, and screenshot; verify focus returns to the target app and Retina coordinates match.
5. Deny an action; cancel during provider waits, approval, tool execution, and recording; start a new conversation and confirm no stale updates.
6. Record/transcribe speech, review input, and test system/provider playback; deny microphone permission and confirm a useful error.
7. Exercise OpenAI compatible/Gemini/local endpoints and authenticated ChatGPT renewal with synthetic prompts.
8. Import synthetic legacy files and verify backups, Keychain keys, memory, history, and unchanged originals.
9. Verify packaged resources, microphone usage text, signing, and (for distribution) notarization.

The Linux sandbox validates the portable modules and parses macOS source, but does not type-check Apple SDK code or run these interactive checks. The macOS workflow is the build gate for native code.
