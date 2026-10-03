# Gemini, Context Management, Speech Providers, and Professional Themes

## Purpose

Extend Function V1 with reliable context-window management, Gemini support through Google's OpenAI-compatible API, independent STT/TTS provider configuration, and a more professional emoji-free settings experience.

## Requirements

1. Detect when an outbound model request would exceed the selected model's context limit.
2. Compact old conversation messages, tool outputs, and screenshots before sending an oversized request.
3. Preserve system instructions, the current task, recent conversation context, and the latest useful tool state.
4. Never retry an oversized request unchanged.
5. Add Gemini through `https://generativelanguage.googleapis.com/v1beta/openai/` using `GEMINI_API_KEY`.
6. Support configurable Gemini model names, streaming, tool calling, and image inputs.
7. Preserve existing OpenAI-compatible and mock provider behavior.
8. Give AI, STT, and TTS their own provider, API key, model, base URL, and provider-specific options.
9. Keep advanced provider settings collapsed until expanded.
10. Apply provider configuration changes without restarting the app.
11. Remove decorative emoji from settings and theme controls.
12. Improve theme selection with professional labels, swatches, selected states, contrast, and spacing.
13. Any input beginning with `> ` executes the remainder as a shell command locally instead of being sent to an AI provider.

## Current Architecture and Constraints

- `function-agent` owns the observe-think-act loop and builds the full message list before each model call.
- `function-providers` currently implements an OpenAI-compatible LLM and Whisper-compatible STT through curl, plus mock providers.
- `function-config` stores one AI provider config and a limited speech config. `provider_name` exists but is not currently used to construct providers.
- `function-ui` owns settings editing and holds the active `Agent` and STT provider handles.
- `function-app` constructs the initial providers and agent before opening the GPUI window.
- The existing native platform speech function remains a fallback path.
- The repository uses Rust, Tokio, GPUI, serde, and curl-based provider transport. No new network client dependency is required for this feature.

## Design

### 1. Provider configuration model

Replace the single-purpose speech settings with explicit serializable configuration objects:

- `AiProviderConfig`: provider name, base URL, model, API-key reference, optional inline key.
- `SttProviderConfig`: enabled, provider name, base URL, model, API-key reference, optional language.
- `TtsProviderConfig`: enabled, provider name, base URL, model, API-key reference, voice, output format.

Each config resolves its own environment variable and credential reference. AI keeps `OPENAI_API_KEY`; Gemini uses `GEMINI_API_KEY`; STT/TTS use provider-specific variables or their configured credential reference. Existing config files deserialize with defaults and retain backward compatibility.

Provider construction is centralized so startup and live settings updates use the same selection rules. Unknown or unconfigured providers fall back to the existing mock/native behavior with a clear status message.

### 2. Gemini provider

Add a `GeminiLlmProvider` backed by the existing OpenAI-compatible request transport. Its default base URL is:

`https://generativelanguage.googleapis.com/v1beta/openai/`

It sends the same chat-completions message format used by the current provider, including multimodal `content` arrays, function tools, tool calls, and streaming SSE responses. It adds provider metadata and model context-limit lookup without changing the existing OpenAI-compatible provider contract.

Model limits are centralized and conservative. Known Gemini models receive explicit limits; unknown configurable models use a documented safe fallback. The provider exposes the effective context limit to the agent.

### 3. Context management

Add a deterministic context manager in `function-agent` that runs immediately before each provider request.

The manager estimates tokens from text, JSON tool output, and image payloads, then reserves an output allowance. When the request is over budget it compacts in this order:

1. Drop or summarize the oldest tool observations.
2. Remove old screenshot image payloads while retaining a short observation marker.
3. Truncate oversized tool output to bounded excerpts.
4. Remove the oldest historical turns while preserving the system message, current task, and recent turns.
5. Truncate only non-critical historical text if necessary.

The resulting request must fit the model budget before dispatch. If a provider still reports a context-length error because the estimate was conservative, the agent may perform one additional compaction pass using a smaller budget; it must never resend the same oversized message list.

The stored conversation history remains useful for the UI, while each outbound request receives a compacted copy. System instructions and the current user task are never removed.

### 4. Independent STT/TTS

STT settings no longer inherit the AI endpoint or key. The existing Whisper-compatible implementation receives its own base URL, model, and API key. TTS gains a provider abstraction implementation for OpenAI-compatible speech endpoints, with native platform speech remaining the fallback when TTS is disabled or unavailable.

Speech requests run through the app's Tokio runtime and report actionable provider errors. Live settings changes replace the active STT/TTS handles just as AI provider changes replace the active LLM provider.

### 5. Explicit shell command prefix

The local command layer recognizes a leading `> ` after trimming leading whitespace. It removes the prefix, trims the command, and dispatches it through the existing cross-platform shell launcher. This path bypasses context management and all LLM providers, while retaining the existing native shell selection, launch-error reporting, and safety behavior. Empty `> ` input remains a visible shell-launcher prompt rather than executing an empty command.

### 6. Settings UX

Keep the primary settings surface concise. Provider configuration is grouped into expandable sections:

- AI Provider (open by default when settings are first visited).
- Speech-to-Text.
- Text-to-Speech.

Each section exposes only relevant fields when enabled. Model and endpoint fields remain configurable. Save and Enter use one persistence path and apply all provider changes atomically from the user's perspective.

### 7. Professional themes

Remove decorative emoji from theme names, API-key visibility, sound controls, and other settings labels. Use existing vector/icon components or plain text labels. Theme choices use compact swatches and clear selected borders instead of emoji prefixes. Improve surface contrast, typography hierarchy, spacing, and focus/hover states while preserving the existing theme enum and serialized values.

## Error Handling

- Context compaction is logged with provider, model, original estimate, final estimate, and discarded categories; prompt contents and secrets are not logged.
- Provider errors retain HTTP status and structured response details.
- Invalid or missing provider configuration is surfaced in settings and does not crash the app.
- A failed save does not replace active providers; a successful save updates disk and live providers together.
- STT/TTS failures fall back gracefully and leave the text input usable.

## Testing

- Unit tests for provider-specific key resolution and backward-compatible config deserialization.
- Unit tests for Gemini defaults, payload mapping, streaming/tool/image parsing, and model limits.
- Unit tests for context estimation, compaction ordering, preservation of system/current/recent messages, and no-unchanged-retry behavior.
- Agent integration test proving an oversized history is compacted before provider invocation.
- Tests for independent STT/TTS config round trips and live provider replacement.
- Existing workspace tests plus `cargo check` after implementation.

## Non-Goals

- No native Gemini SDK; the requested OpenAI-compatible endpoint is the transport boundary.
- No remote model discovery or automatic model list downloads.
- No redesign of the conversation view beyond settings and context-status feedback needed for this feature.
- No decorative emoji in the settings experience.
