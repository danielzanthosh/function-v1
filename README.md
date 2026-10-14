# Function

A native macOS desktop assistant, written entirely in Swift with a SwiftUI interface. Press **Command twice** to bring up the compact floating launcher, type or record a request, and follow the assistant's actions as it works.

## Requirements

- macOS 14 or later.
- Swift 6.2 or later (Xcode 26 or a compatible Swift toolchain).
- An OpenAI compatible endpoint, Gemini API key, local model endpoint, or ChatGPT account with access to the selected model.

There are no third-party package dependencies, Rust libraries, web views, or external automation runtimes. AppKit, ScreenCaptureKit, CoreGraphics, AVFoundation, Security, and CryptoKit provide native platform integration; every application screen is SwiftUI.

## Build and run

```sh
swift test
bash scripts/package-macos.sh
open dist/Function.app
```

Open `Package.swift` in Xcode to edit or debug the package. `swift run Function` also runs the app during development, but the packaged `.app` is recommended for a stable identity when granting macOS permissions. The packaging script creates an ad-hoc signed local build; public distribution requires Developer ID signing and notarization. The macOS workflow supports both using the existing Apple signing secrets.

A Linux host can build and test the portable core with `swift test`; the Linux executable prints the macOS requirement and does not provide a desktop interface.

## Features

- Floating assistant, menu bar entry, configurable global shortcut, compact and expanded modes.
- Application and Spotlight file search, file picker, and instant arithmetic with Enter to copy the result.
- Multi-step agent with structured tools, screenshots, bounded context, retries, cancellation, and action history.
- Native application launching/quitting, screen capture, mouse clicks/movement, scrolling, Unicode typing, and keyboard shortcuts.
- File read/write/list/move/delete, bounded shell commands, public web-page fetch, and optional DuckDuckGo instant answers.
- Local conversations and explicitly saved memory with history and deletion controls.
- Whisper compatible transcription, provider speech generation, and macOS speech playback.
- OpenAI compatible providers, Gemini's compatible endpoint, and a native ChatGPT subscription adapter.
- Four themes, five accents, request delay, context budget, memory/search switches, and voice settings.
- Keychain credentials and explicit approval for every action that changes the desktop, files, or memory.

## Configure

Open **Settings** from the menu bar or the assistant's gear button. Enter a provider endpoint, model, and API key, then save. Gemini resolves the default endpoint to its OpenAI compatible API. Local model servers may leave the API key empty. `OPENAI_API_KEY` or `GEMINI_API_KEY` is a fallback when a Keychain key is absent.

Transcription and text-to-speech each have independent endpoints, models, and Keychain keys. Enable the microphone and click it to record; click again to transcribe. Review the transcription and press Enter to submit. Recordings are limited to two minutes and temporary audio is removed after transcription or cancellation.

For **ChatGPT plan**, click Sign in, complete browser authentication, then paste the complete `http://localhost:1455/auth/callback?...` URL from the browser address bar into the callback field and complete sign-in. The browser may show a connection error because Function uses manual callback transfer rather than binding a local port. Credentials and refresh tokens are stored in Keychain. Select a model available to that account. This subscription endpoint is not the public API and may change; live account access has not been verified in the Linux sandbox.

Grant **Accessibility** for keyboard/mouse controls and global shortcuts, **Screen Recording** for screenshots, and **Microphone** for voice. Function can still be opened from the menu bar before permissions are granted. Avoid shortcuts already used by macOS.

## Commands

`/settings`, `/conversations`, `/new`, `/theme`, `/voice`, `/sound`, `/clear`, `/memory`, `/about`, `/help`, and `/model <name>`. Prefix a shell command with `>` to run it after approval. **Escape** cancels an active task or hides the launcher.

## Data and migration

Native configuration, memory, and conversations live in `~/.function/swift/`. API keys and OAuth tokens live in macOS Keychain, never in configuration JSON. Existing Rust files remain untouched in `~/.function/`.

Use **Settings → Privacy → Import Rust version settings, memory, and history** for a one-time import. This replaces native settings and saved data after confirmation, backs up existing native JSON, and copies legacy plaintext keys into Keychain. The original files are preserved, including any plaintext keys already present there; remove those manually after confirming the import. Legacy ChatGPT tokens are not imported: sign in again. A malformed import reports an error; a write failure can leave a partial import, with backups retained for recovery.

## Validation and scope

The app targets macOS. SwiftUI does not provide a Windows desktop runtime, so the former Windows build is retired. Historical Rust design documents remain in `docs/legacy/`; active build and runtime documentation is [Swift architecture](docs/SWIFT-ARCHITECTURE.md).

Portable core tests cover tool execution/denial, cancellation, provider HTTP/wire formats, retry behavior, context compaction, memory storage, migration, and arithmetic. macOS CI builds/tests/packages the app. Accessibility, screen recording, microphone, global shortcut, and authenticated provider flows require manual verification on a Mac. DuckDuckGo uses instant answers, which are not a full ranked search index. Browser fetch reads public pages and does not access logged-in sessions; the agent can use native browser UI tools for those.

## Project policies

[Contributing](CONTRIBUTING.md) · [Privacy](docs/PRIVACY.md) · [Memory](docs/MEMORY.md) · [Security](SECURITY.md) · [Terms](docs/TERMS.md) · [License](LICENSE)
