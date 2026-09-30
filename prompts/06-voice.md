# Voice

Read:

- docs/13-VOICE.md
- docs/20-UX-FLOWS.md

Implement the voice input pipeline.

Architecture:

Microphone
→ Speech-to-text
→ Transcript
→ Agent

Create a provider abstraction for speech recognition.

Do not hard-code the rest of the application to one STT provider.

Implement:

- Listening state
- Recording
- Transcription
- Error handling
- Transcript insertion into the assistant UI

Keep the provider credentials outside source code.

Do not implement advanced TTS until the input pipeline works.
