# Contributing to Function

Function is a Swift 6 package with a native macOS SwiftUI app. Read [PRODUCT.md](PRODUCT.md) and [Swift architecture](docs/SWIFT-ARCHITECTURE.md) before changing behavior.

## Development

Use macOS 14+ and Swift 6.2+ (Xcode 26 or a compatible toolchain):

```sh
swift build
swift test
bash scripts/package-macos.sh
```

Linux can run the portable core tests. Native desktop, voice, and SwiftUI behavior require a Mac and the corresponding system permissions. Report any unavailable checks explicitly.

## Guidelines

- Prefer Swift value types for data and use Swift 6 concurrency checks.
- Keep SwiftUI state on the main actor. Separate provider and agent behavior from views.
- Keep changes focused and preserve the monochrome Function identity.
- Never log credentials, prompt bodies, screenshots, or OAuth tokens.
- Keep credentials in Keychain, not configuration JSON.
- Preserve approval boundaries, cancellation, tool/result pairing, and honest success reporting.
- Use meaningful tests for behavior changes; include macOS verification for platform changes.
- Keep generated build output, personal settings, and secrets out of commits.

Summarize the change, why it was needed, checks run, and any limitations in a pull request. Follow [SECURITY.md](SECURITY.md) for vulnerability reports.

Contributions are distributed under the project's MIT / Apache-2.0 dual license.
