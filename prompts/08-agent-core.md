# Agent Core

Read:

- docs/08-AGENT.md
- docs/07-ARCHITECTURE.md
- docs/10-TOOLS.md

Implement the agent runtime.

Core loop:

OBSERVE
THINK
ACT
OBSERVE
THINK
ACT
DONE

Implement:

- Task state
- Tool calls
- Tool results
- Iteration
- Completion
- Error recovery
- Cancellation

The LLM must not directly execute system operations.

Every computer operation must happen through a registered tool.

Do not expose private chain-of-thought in the UI.

Expose concise action/status information instead.

Make the agent runtime independent of GPUI.
