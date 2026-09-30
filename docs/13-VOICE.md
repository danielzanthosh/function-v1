# Voice

Voice is a primary interaction mode.

## Input

The assistant should support speech-to-text.

The speech pipeline should use a Whisper-based service.

## Interaction

macOS:

Option + Space

Windows:

Alt + Space

Pressing the shortcut opens the assistant.

Push-to-talk behavior may be supported.

## States

Listening
Processing
Acting
Speaking
Idle

Each state should have a distinct visual indication.

## Output

Text-to-speech may be used for responses.

The system should support replacing the TTS provider.

## Errors

If speech recognition fails, allow the user to type the request instead.
