# Gemini, Context Management, Speech Providers, and Professional Themes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe automatic context compaction, Gemini support, independent AI/STT/TTS providers, explicit shell-prefix execution, expandable professional settings, and subtle macOS Liquid Glass styling without breaking current providers.

**Architecture:** Keep the existing curl/OpenAI-compatible transport as the wire boundary, add provider metadata and configuration-driven construction around it, and place deterministic context compaction immediately before agent provider calls. Separate serializable AI/STT/TTS configuration, centralize live provider construction, and keep UI-only styling changes in the GPUI layer with macOS-specific material behavior isolated behind platform cfg gates.

**Tech Stack:** Rust 2021, Tokio, GPUI 0.2.2, serde/serde_json, curl subprocess transport, existing platform AppKit integration.

**Spec:** `docs/superpowers/specs/2026-10-03-gemini-context-speech-theme-design.md`

## Global Constraints

- “Never retry an oversized request unchanged.”
- “Add Gemini through `https://generativelanguage.googleapis.com/v1beta/openai/` using `GEMINI_API_KEY`.”
- “Keep advanced provider settings collapsed until expanded.”
- “Any input beginning with `> ` executes the remainder as a shell command locally instead of being sent to an AI provider.”
- “No full-window blur, glass applied to every card, or glass stacked over other glass surfaces.”
- Preserve existing OpenAI-compatible, mock, native speech fallback, serialized theme values, and backward-compatible config files.
- Do not add a native Gemini SDK or a new network client dependency.

## Review Focus

- A huge screenshot/tool result must be compacted before dispatch, with system instructions and the current task intact — covered by Task 3 context tests.
- An unknown Gemini model must use a safe context limit rather than an unbounded request — covered by Task 2 model-limit tests.
- A legacy config without STT/TTS sections must deserialize with usable defaults — covered by Task 1 config tests.
- A leading `> ` command must bypass the AI provider and an empty prefix must not execute a blank command — covered by Task 6 local-command tests.
- Reduced-transparency/reduced-motion macOS settings must receive a readable non-glass fallback — covered by Task 7 platform/style tests or a deterministic fallback helper test.

---

### Task 1: Separate provider configuration and key resolution

**Files:**
- Modify: `crates/config/src/lib.rs`
- Modify: `crates/config/src/lib.rs` tests
- Modify: `crates/ui/src/views/function_view.rs` only where configuration fields are copied or persisted

**Interfaces:**
- Produces `SttProviderConfig`, `TtsProviderConfig`, provider-aware `AiProviderConfig::is_configured`, and provider-aware `resolve_api_key` behavior.
- Existing `AppConfig` JSON remains readable when `stt`/`tts` fields are absent.

- [ ] **Step 1: Write failing config tests** for Gemini environment-key selection, independent STT/TTS round trips, and legacy config deserialization without speech-provider sections.
- [ ] **Step 2: Run `cargo test -p function-config`** and verify the new tests fail for missing fields/behavior.
- [ ] **Step 3: Implement the config structs, serde defaults, provider-specific environment variable selection, and backward-compatible migration/defaults.** Keep inline keys optional and do not log secret values.
- [ ] **Step 4: Run `cargo test -p function-config`** and verify all config tests pass.
- [ ] **Step 5: Commit** with `feat(config): add independent AI speech provider settings`.

### Task 2: Provider metadata, Gemini transport, and configurable speech providers

**Files:**
- Create: `crates/providers/src/context_limits.rs`
- Modify: `crates/providers/src/lib.rs`
- Modify: `crates/providers/src/openai.rs`
- Create or modify: `crates/providers/src/gemini.rs`
- Modify: `crates/providers/src` tests

**Interfaces:**
- `LlmProvider::context_limit(&self, model: &str) -> usize` with a conservative default.
- `GeminiLlmProvider::new(base_url, api_key, model)`, implementing streaming, tool calls, images, and `LlmProvider`.
- `model_context_limit(provider, model) -> usize` in `context_limits.rs`.
- Configurable OpenAI-compatible STT/TTS constructors with independent endpoint, model, and key values.

- [ ] **Step 1: Write failing provider tests** for Gemini defaults, endpoint normalization, Gemini key usage, tool/image JSON mapping, streaming delta parsing, known/unknown model limits, and configurable STT/TTS endpoints.
- [ ] **Step 2: Run `cargo test -p function-providers`** and verify the tests fail because Gemini and model-limit interfaces do not exist.
- [ ] **Step 3: Extract reusable OpenAI-compatible request/response helpers without changing current OpenAI behavior.** Add Gemini as a provider descriptor/wrapper using the exact Google base URL default and provider metadata.
- [ ] **Step 4: Add provider-specific context-limit lookup and independent OpenAI-compatible STT/TTS request construction.** Preserve structured HTTP errors and runtime-safe async behavior.
- [ ] **Step 5: Run `cargo test -p function-providers`** and verify all provider tests pass.
- [ ] **Step 6: Commit** with `feat(providers): add Gemini and configurable speech transports`.

### Task 3: Deterministic context estimation and compaction

**Files:**
- Create: `crates/agent/src/context.rs`
- Modify: `crates/agent/src/lib.rs`
- Modify: `crates/agent/src` tests

**Interfaces:**
- `ContextBudget { context_limit: usize, output_reserve: usize }`.
- `ContextCompactionReport { before_tokens, after_tokens, removed_messages, removed_images, truncated_outputs }`.
- `compact_messages(messages: &[ChatMessage], budget: ContextBudget) -> (Vec<ChatMessage>, ContextCompactionReport)`.
- `messages_fit(messages: &[ChatMessage], budget: ContextBudget) -> bool`.

- [ ] **Step 1: Write failing tests** proving system/current/recent messages survive, old screenshots are removed first, large tool output is truncated, old turns are removed in order, and the compacted result fits the reserved budget.
- [ ] **Step 2: Run `cargo test -p function-agent context`** and verify the tests fail because the context module does not exist.
- [ ] **Step 3: Implement token estimation and deterministic compaction.** Count text/JSON conservatively, assign bounded image cost, preserve system and current task, and ensure the final result is smaller than the dispatch budget.
- [ ] **Step 4: Run the context tests** and verify all preservation and ordering assertions pass.
- [ ] **Step 5: Commit** with `feat(agent): add deterministic context compaction`.

### Task 4: Integrate context management into the agent loop

**Files:**
- Modify: `crates/agent/src/lib.rs`
- Modify: `crates/agent/src` tests

**Interfaces:**
- Agent reads `provider.context_limit(model)` and compacts a request copy before every completion call.
- Agent may perform one stricter compaction pass only after a recognized context-length provider error; it never resends the same message vector.

- [ ] **Step 1: Write a failing agent integration test** using a recording provider that rejects oversized input and asserts the first dispatched request is already compacted; add a test that a context error causes a changed second request, not an identical retry.
- [ ] **Step 2: Run the targeted agent tests** and verify the recording provider observes the current unbounded behavior.
- [ ] **Step 3: Integrate `compact_messages` before provider calls, keep the un-compacted history for UI persistence, and log only non-sensitive compaction metadata.** Detect context-length errors without retrying unchanged content.
- [ ] **Step 4: Run targeted agent tests plus existing agent tests** and verify all pass.
- [ ] **Step 5: Commit** with `feat(agent): compact requests before provider dispatch`.

### Task 5: Centralize provider construction and live speech/provider replacement

**Files:**
- Create: `crates/app/src/providers.rs` or equivalent provider-factory module
- Modify: `crates/app/src/main.rs`
- Modify: `crates/ui/src/views/function_view.rs`
- Modify: `crates/agent/src/lib.rs` if provider replacement interfaces need adjustment

**Interfaces:**
- `build_llm_provider(config: &AppConfig) -> Arc<dyn LlmProvider>`.
- `build_stt_provider(config: &AppConfig) -> Arc<dyn SpeechToTextProvider>`.
- `build_tts_provider(config: &AppConfig) -> Option<Arc<dyn TextToSpeechProvider>>`.
- Successful settings save updates all active provider handles atomically from the UI perspective.

- [ ] **Step 1: Write failing factory tests** for OpenAI, Gemini, missing-key mock fallback, independent STT, and disabled TTS selection.
- [ ] **Step 2: Run the factory tests** and verify provider selection is currently hard-coded in `main.rs`.
- [ ] **Step 3: Move startup construction into the factory, add live replacement for LLM/STT/TTS, and preserve agent tools, memory, subscriptions, and conversation history.** Route TTS through the configured provider before native fallback.
- [ ] **Step 4: Run factory, agent, and provider tests** and verify successful settings changes replace active providers without restart.
- [ ] **Step 5: Commit** with `feat(app): centralize live provider construction`.

### Task 6: Explicit shell prefix behavior

**Files:**
- Modify: `crates/ui/src/local_commands.rs`
- Modify: `crates/ui/src/views/function_view.rs` only if the dispatch path needs a dedicated local action
- Modify: `crates/ui/src` tests

**Interfaces:**
- `resolve_shell_command(input: &str) -> Option<String>` recognizes leading whitespace followed by `> `, trims the remainder, and returns `None` for empty commands.

- [ ] **Step 1: Write failing tests** for `> echo hi`, leading whitespace, `> ` empty input, and ordinary prompts that must continue to AI.
- [ ] **Step 2: Run the local-command tests** and verify the resolver does not yet exist.
- [ ] **Step 3: Implement the resolver and route it before AI/context dispatch through `spawn_shell_command`, retaining existing error/status behavior.
- [ ] **Step 4: Run UI/local-command tests and `cargo check -p function-ui`**.
- [ ] **Step 5: Commit** with `feat(ui): execute explicit shell-prefixed prompts locally`.

### Task 7: Expandable provider settings and professional theme/UI treatment

**Files:**
- Modify: `crates/ui/src/views/settings_view.rs`
- Modify: `crates/ui/src/views/function_view.rs`
- Modify: `crates/ui/src/theme.rs`
- Modify: `crates/platform/src` macOS window/material helpers as needed
- Modify: `crates/ui/src` tests or deterministic style helpers

**Interfaces:**
- Settings state exposes collapsed/expanded advanced provider sections without changing serialized config values.
- Theme rendering uses text labels, vector icons/swatches, and selected/focus states with no decorative emoji.
- macOS material styling is isolated behind cfg-gated helpers and has a readable opaque fallback.

- [ ] **Step 1: Write failing deterministic tests** for expandable-section defaults, no-emoji display labels, theme selection states, and Liquid Glass fallback selection.
- [ ] **Step 2: Run targeted tests** and verify the new state/helpers are absent.
- [ ] **Step 3: Implement compact primary settings plus expandable AI/STT/TTS sections, provider-specific fields, and one Save/Enter persistence path.**
- [ ] **Step 4: Replace emoji labels with professional text/icon/swatch controls and refine contrast, spacing, hover, focus, and selected states.
- [ ] **Step 5: Add subtle macOS-only regular-material styling to the top-level command/settings controls, with accessibility-aware opaque fallback and no glass-on-glass stacking.
- [ ] **Step 6: Run UI compilation and targeted tests** and inspect the rendered hierarchy for collapsed advanced settings and readable labels.
- [ ] **Step 7: Commit** with `feat(ui): add expandable provider settings and refined themes`.

### Task 8: Full verification and delivery

**Files:**
- Modify: documentation only if implementation details or configuration examples need updating.

- [ ] **Step 1: Run `cargo check` for the workspace** and fix all compilation errors.
- [ ] **Step 2: Run `cargo test --workspace`** and fix all regressions.
- [ ] **Step 3: Run `git diff --check` and inspect the final diff** for secrets, emoji labels, unchanged oversized retry paths, and accidental edits to `AI_AGENT_CONTEXT.md`.
- [ ] **Step 4: Run a macOS-targeted compile if the target is available; otherwise record the limitation and verify all cfg-gated helpers on the host target.
- [ ] **Step 5: Commit final documentation/verification changes and push the completed branch.**
