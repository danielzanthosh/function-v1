# Global Hotkey

Read:

- docs/17-PLATFORM.md
- docs/20-UX-FLOWS.md

Implement global assistant activation.

macOS:

Command + ;

Windows:

Alt + Space

The hotkey should:

1. Open the assistant.
2. Focus input.
3. Prepare voice interaction when appropriate.

Support:

- Open
- Close
- Cancel

Keep the hotkey configurable internally so settings can be added later.

Implement platform-specific code behind a common abstraction.

Test on the target platform before considering the feature complete.
