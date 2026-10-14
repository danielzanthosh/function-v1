# Frameworks and Technologies

## Core

Rust is the primary programming language.

## UI

GPUI is the primary UI framework.

The interface should use native GPUI views and elements.

## AI

Use an OpenAI-compatible API abstraction.

The application must not hard-code a single provider.

Configuration:

- Base URL
- API key
- Model

## Browser

Use Playwright where browser automation is appropriate.

The browser system should support:

- Chromium
- Chrome
- Edge

Safari support can be evaluated separately.

## Database

SQLite is the primary local database.

## Speech

Speech-to-text should use a Whisper-based service.

## Search

Search should be optional.

The search layer must be provider-independent.

## Platform

Use Rust abstractions over platform-specific APIs.

Windows and macOS implementations should be isolated where possible.

## Secrets

Use the operating system credential/keychain system.

Never commit API keys.

## Logging

Use structured application logging.

Logs must not expose secrets.
