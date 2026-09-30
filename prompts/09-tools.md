# Tools

Read:

- docs/10-TOOLS.md
- docs/11-COMPUTER-CONTROL.md
- docs/16-SECURITY.md

Create the tool framework.

Every tool should have:

- Name
- Description
- Input schema
- Output schema
- Permission level
- Execution method

Implement a registry.

Initial tool categories:

- Screen
- Mouse
- Keyboard
- Browser
- Files
- Terminal
- Applications

Do not implement every tool immediately.

Build the framework so additional tools can be added cleanly.

Add permission enforcement before execution.
