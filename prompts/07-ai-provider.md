# AI Provider

Read:

- docs/14-AI-PROVIDERS.md
- docs/07-ARCHITECTURE.md

Implement a provider-independent AI client.

Configuration:

- Base URL
- API key
- Model

Requirements:

- Chat
- Streaming
- Tool calls
- Error handling
- Timeouts

Use an abstraction so the rest of the application does not depend on a specific provider.

Never log API keys.

Never hard-code credentials.

Add configuration validation.

Test with a simple request before connecting the agent.
