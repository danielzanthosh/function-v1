# Build and Release

## Development

Use Cargo for Rust development.

## Release

Produce native installers/packages for:

- Windows
- macOS

## Configuration

Production builds must not contain development API keys.

## Signing

Evaluate platform signing requirements before distribution.

## Versioning

Use semantic versioning.

Example:

MAJOR.MINOR.PATCH

## Release Checklist

- Build succeeds
- Tests pass
- No secrets included
- Windows tested
- macOS tested
- Permissions tested
- Hotkeys tested
- AI provider tested
- Browser automation tested
- Installer tested
