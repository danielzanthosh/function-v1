# Build and Release

## Overview

Function is built as a native desktop application in Rust using Cargo workspaces, GPUI for hardware-accelerated 2D rendering, and native platform bindings for low-latency computer control.

## Workspace Architecture

```
Version 1/
├── crates/
│   ├── app/        - Binary entry point (function.exe) & runtime orchestration
│   ├── ui/         - GPUI views (Compact, Spotlight, Expanded, Settings), branding & themes
│   ├── agent/      - Core Observe-Think-Act cognitive loop & execution state machine
│   ├── memory/     - Local persistent memory store (~/.function/memory.json)
│   ├── tools/      - Native computer tools (file, terminal, browser, screen, keyboard/mouse)
│   ├── providers/  - OpenAI/Whisper/Mock LLM and STT provider integrations
│   ├── platform/   - Native OS services (Win32 window centering, icons, hotkeys, audio capture)
│   └── config/     - Configuration schema, secrets redaction, and persistence
```

## Release Build

To produce a production release build:

```powershell
cargo build --release --bin function
```

The resulting optimized executable will be located at:
`target/release/function.exe`

## Testing & Quality Assurance

Run all workspace unit tests and verification suites:

```powershell
cargo test --workspace
```

All 14 unit tests across all workspace crates must pass before tagging or packaging a release.

## Platform Status & Testing

- **Windows 10 / Windows 11 (Tested & Verified)**:
  - Frameless curved window with native DWM attributes (`DWMWA_WINDOW_CORNER_PREFERENCE`).
  - Native global hotkeys (`Alt+Space`) via Win32 `RegisterHotKey`.
  - Native display detection and centering (`GetSystemMetrics`, `SetWindowPos`).
  - Portable Win32 icon association (`WM_SETICON`) across taskbar and caption.
  - Safe dynamic library loading (`LoadLibraryA("dwmapi.dll")`) for portability across MinGW and MSVC runtimes.
- **macOS (Platform Architecture Prepared)**:
  - Core platform abstraction in `function-platform` supports macOS CG/CoreGraphics and Carbon hotkey APIs.
  - *Note: Only claimed as tested after executing on physical macOS runner.*

## Security & Secrets Management

- **Zero Baked-In Secrets**: Production release binaries contain no API keys, tokens, or environment credentials.
- **Log Sanitization**: All loggers and UI streams automatically sanitize API keys (`sk-...`) using `function_config::redact_secrets`.
- **Permission Boundaries**: Tools are categorized into `Safe`, `Confirm`, and `Restricted` levels. Destructive file operations (`delete`, `move`) and high-impact terminal commands require interactive user approval.

## Release Checklist

- [x] Clean workspace build succeeds (`cargo check --workspace`, `cargo build --release`)
- [x] All unit and integration tests pass (`cargo test --workspace`)
- [x] No credentials or secrets included in binaries or source control
- [x] Windows 10/11 functionality verified (frameless window, centering, audio, hotkeys)
- [x] Security confirmations and permission boundaries enforced
- [x] In-app preferences and theme customizations persist cleanly to `~/.function/config.json`
- [ ] Physical macOS distribution packaging (macOS CI pipeline)
- [ ] Code signing with platform certificate before external public distribution

