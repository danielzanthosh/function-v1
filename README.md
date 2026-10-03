# Function

A native AI computer assistant for Windows and macOS.

## Concept

Press:

macOS: Double Command (⌘ ⌘)

Windows: Ctrl + Space

Speak or type a task.

Function understands the request and interacts with the computer through controlled tools.

## Features

- Native GPUI interface (`function-ui`)
- AI agent Observe-Think-Act cognitive loop (`function-agent`)
- Voice interaction & Whisper transcription (`function-platform`, `function-providers`)
- Browser automation & computer control (`function-tools`, `function-platform`)
- Screen observation & action execution (`function-tools`)
- Persistent memory store (`function-memory`)
- Configurable AI providers (`function-providers`, `function-config`)
- Windows and macOS support

## Architecture

Rust Cargo Workspace:
- `function-app`: Application binary entry point (`function.exe`) and orchestration
- `function-ui`: GPUI presentation layer, views (Compact, Spotlight, Expanded, Settings), and themes
- `function-agent`: Autonomous agent loop and state management
- `function-tools`: Native computer tools (apps, browser, keyboard, mouse, screen)
- `function-memory`: Persistent JSON and in-memory context store
- `function-providers`: OpenAI, Whisper, and Mock LLM/STT provider integrations
- `function-platform`: Native operating system integration (audio capture, window centering, hotkeys)
- `function-config`: Application configuration, secret redaction, and serialization

## Build & Run

```powershell
# Run debug build
cargo run --bin function

# Build optimized release binary
cargo build --release --bin function
```

## Development

Read PRODUCT.md first.

Then follow the documentation in docs/.

Run workspace test suite:
```powershell
cargo test --workspace
```

## Philosophy

Function feels like a native computer tool rather than a chatbot.

It is fast, transparent, controllable, and useful.
