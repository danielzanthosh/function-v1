# AI Providers

The AI system must support configurable OpenAI-compatible endpoints.

## Configuration

The user provides:

- Base URL
- API key
- Model

Example conceptual configuration:

base_url = "..."
api_key = "..."
model = "..."

## Provider Abstraction

The rest of the application should not depend directly on one provider.

Use an abstraction such as:

AIProvider

with operations for:

- Chat
- Streaming
- Tool calls
- Vision where supported

## API Keys

API keys must be stored securely.

Do not place keys in:

- Source code
- Git
- Logs
- Screenshots
- Plaintext application logs

## Models

The model should be configurable.

Do not hard-code model names throughout the codebase.
