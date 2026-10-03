# Function Version 1 Stability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Function’s macOS floating window reliably interactive while preserving Windows behavior and correcting cross-platform launcher shell execution.

**Architecture:** Keep logical visibility in `FunctionView` and native window operations in `function-platform`. Extract deterministic shell selection and activation-transition decisions into small platform helpers so they can be tested without AppKit or a live GPUI window; keep Objective-C calls behind macOS cfg gates.

**Tech Stack:** Rust 2021, Cargo workspace, GPUI 0.2.2, AppKit Objective-C runtime calls on macOS, platform-gated `std::process::Command`, Cargo unit tests.

**Spec:** `docs/superpowers/specs/2026-10-03-function-v1-stability-design.md`

## Global Constraints

- “The UI remains the authority for logical visibility, while platform helpers perform native operations.”
- “Status-bar and menu windows must never be hidden or modified by this flow.”
- “Use PowerShell on Windows.”
- “Use the user’s login shell on macOS and Unix-like systems.”
- “Do not change permission policy for terminal, file, browser, or computer-control tools.”
- “Claiming runtime macOS verification from a Windows host” is prohibited; report runtime verification limits explicitly.

## Review Focus

- A transient inactive callback during summon must not hide the window; cover with the activation-transition decision test in Task 2.
- A real click outside after activation settles must still dismiss; cover with the settled-visible decision test in Task 2.
- macOS status-bar/menu windows must not be ordered out or modified; cover with the native-window filtering test/helper in Task 3 where the platform surface permits, and inspect the AppKit loop manually by code review.
- A missing shell executable or failed spawn must reach the UI instead of being discarded; cover the launch-error mapping test in Task 1 and integration behavior in Task 4.
- Existing legacy and Double Command shortcut spellings must remain accepted; cover the expanded parser tests in Task 3.

## File Map

- Modify `crates/platform/src/lib.rs`: expose deterministic shell-launch helpers, activation-transition decision helpers, and refine macOS native activation/dismissal.
- Modify `crates/platform/src/computer.rs` only if shared shell-launch behavior is reused there; otherwise leave agent terminal behavior unchanged.
- Modify `crates/platform/src/lib.rs` tests: test shell specs, launch-error mapping, activation decisions, and shortcut parsing.
- Modify `crates/ui/src/views/function_view.rs`: use platform shell launching, add a summon/activation settling guard, and surface launch errors without corrupting launcher state.
- Modify `README.md`, `docs/17-PLATFORM.md`, and `docs/20-UX-FLOWS.md`: document Double Command and current Windows shortcut consistently.

### Task 1: Add tested cross-platform shell-launch helpers

**Files:**
- Modify: `crates/platform/src/lib.rs` near the existing platform utility functions and unit-test module.
- Modify: `crates/ui/src/views/function_view.rs:1108-1132` to consume the helper in Task 4, not in this task.

**Interfaces:**
- Produces `pub struct ShellCommandSpec { pub program: String, pub args: Vec<String> }`.
- Produces `pub fn shell_command_spec(command: &str) -> ShellCommandSpec`.
- Produces `pub fn spawn_shell_spec(spec: &ShellCommandSpec) -> Result<(), PlatformError>` for deterministic error-path tests.
- Produces `pub fn spawn_shell_command(command: &str) -> Result<(), PlatformError>`; it maps `std::io::Error` to `PlatformError::SystemApi` with an actionable message.

- [ ] **Step 1: Write the failing platform tests**

  Add tests asserting that the target-specific `shell_command_spec("echo hello")` uses `powershell` with `-NoProfile`, `-Command` on Windows and the login shell with `-lc` on Unix-like targets. Add a test that passes `ShellCommandSpec { program: "function-test-command-that-does-not-exist", args: vec![] }` to `spawn_shell_spec` and asserts a `SystemApi` error containing the executable name; do not execute arbitrary commands in tests.

- [ ] **Step 2: Run the focused tests and verify the expected failure**

  Run `cargo test -p function-platform shell_command_spec -- --nocapture` and confirm compilation/test failure because the new types/functions do not yet exist.

- [ ] **Step 3: Implement the minimal helpers**

  Build `ShellCommandSpec` from the compile target. On Windows use `powershell` and arguments `-NoProfile`, `-Command`, command. On non-Windows use `std::env::var("SHELL")`, falling back to `/bin/sh`, with arguments `-lc`, command. `spawn_shell_spec` should call `.spawn()` and return `PlatformError::SystemApi` on failure; `spawn_shell_command` should construct the spec and delegate to it.

- [ ] **Step 4: Run focused and platform tests**

  Run `cargo test -p function-platform shell_command_spec -- --nocapture` and `cargo test -p function-platform`; expect all tests to pass.

- [ ] **Step 5: Commit**

  `git add crates/platform/src/lib.rs && git commit -m "feat(platform): add tested cross-platform shell launcher"`

### Task 2: Add deterministic activation-transition state decisions

**Files:**
- Modify: `crates/platform/src/lib.rs` with a small pure activation-state helper and tests.
- Modify: `crates/ui/src/views/function_view.rs` fields and `observe_activation`/`summon`/`dismiss` methods in Task 3.

**Interfaces:**
- Produces `pub fn should_dismiss_after_deactivation(is_visible: bool, has_activated_once: bool, transition_settling: bool) -> bool`.
- Produces `pub const MACOS_ACTIVATION_SETTLE_MS: u64` for the UI guard duration.

- [ ] **Step 1: Write the failing decision tests**

  Add one test for each required decision: hidden never dismisses; first activation never dismisses; visible + settling never dismisses; visible + activated + settled dismisses.

- [ ] **Step 2: Run the focused tests and verify they fail**

  Run `cargo test -p function-platform should_dismiss_after_deactivation -- --nocapture`; confirm failure because the helper is absent.

- [ ] **Step 3: Implement the pure helper**

  Return `is_visible && has_activated_once && !transition_settling`; expose the settle constant as a single source of truth for the UI timer.

- [ ] **Step 4: Run the focused and full platform tests**

  Run `cargo test -p function-platform should_dismiss_after_deactivation -- --nocapture` followed by `cargo test -p function-platform`; expect PASS.

- [ ] **Step 5: Commit**

  `git add crates/platform/src/lib.rs && git commit -m "test(platform): model macOS activation dismissal decisions"`

### Task 3: Stabilize macOS native activation and UI focus lifecycle

**Files:**
- Modify: `crates/platform/src/lib.rs` in `macos_activate_app` and `macos_hide_app`.
- Modify: `crates/ui/src/views/function_view.rs` in `FunctionView`, `observe_activation`, `summon`, and `dismiss`.
- Modify: `crates/platform/src/lib.rs` shortcut-parser tests if coverage is incomplete.

**Interfaces:**
- Consumes `should_dismiss_after_deactivation` and `MACOS_ACTIVATION_SETTLE_MS` from Task 2.
- Preserves public `macos_activate_app()`, `macos_hide_app()`, and `parse_macos_shortcut()` signatures.

- [ ] **Step 1: Add/adjust regression tests before native code changes**

  Extend `parse_macos_shortcut` tests for `Double Command`, `Command+Command`, `cmd+cmd`, `⌘ ⌘`, and the legacy command-plus-key forms. Add a test that the activation helper’s filtering predicate excludes status-bar/menu/panel class names and accepts the GPUI window class shape, if the predicate is extracted as a pure helper.

- [ ] **Step 2: Run the focused tests and verify the new cases fail where behavior is missing**

  Run `cargo test -p function-platform parse_macos_shortcut -- --nocapture`; confirm any newly added expectations fail before implementation changes.

- [ ] **Step 3: Refine native activation**

  Keep Objective-C calls macOS-gated. Make the native loop operate only on Function’s regular GPUI window, leave status/menu/panel windows untouched, and make repeated calls safe. Ensure the selected window has `canBecomeKeyWindow`/`canBecomeMainWindow`, `ignoresMouseEvents = false`, visible/floating ordering, and key/main activation before returning. Avoid calling activation logic synchronously from a key-window callback.

- [ ] **Step 4: Add the UI settling guard**

  Add a per-view activation transition generation/settling state. `summon` starts a settling timer using `MACOS_ACTIVATION_SETTLE_MS`; `dismiss` invalidates pending timers and explicitly clears logical visibility. `observe_activation` defers external deactivation, checks the generation and `should_dismiss_after_deactivation`, and only then calls the existing dismissal path. Explicit Escape/hotkey/menu commands must continue to call their existing direct paths.

- [ ] **Step 5: Run formatting and platform tests**

  Run `cargo fmt --check`, `cargo test -p function-platform`, and `cargo check -p function-ui`; expect no new warnings/errors. If a macOS target is installed, also run `cargo check --target <installed-apple-target> -p function-platform -p function-ui`.

- [ ] **Step 6: Commit**

  `git add crates/platform/src/lib.rs crates/ui/src/views/function_view.rs && git commit -m "fix(macos): keep Function window interactive during activation"`

### Task 4: Integrate launcher errors and refresh platform documentation

**Files:**
- Modify: `crates/ui/src/views/function_view.rs` in the `LauncherAction::ExecuteShell` branch.
- Modify: `README.md` shortcut and feature text.
- Modify: `docs/17-PLATFORM.md` macOS/Windows shortcut sections.
- Modify: `docs/20-UX-FLOWS.md` open-assistant flow.

**Interfaces:**
- Consumes `function_platform::spawn_shell_command` and its `Result<(), PlatformError>` from Task 1.
- Produces user-visible `latest_result`/activity error text while preserving the input-clearing and task-state behavior on successful launch.

- [ ] **Step 1: Confirm the integration test seam and baseline behavior**

  Run `cargo test -p function-platform` and inspect the existing `function-ui` crate configuration. Because `function-ui` currently sets `lib.test = false`, use the Task 1 `spawn_shell_spec` error test as the executable launch-error regression and record the UI integration limitation rather than adding a mock-only test.

- [ ] **Step 2: Run the baseline package checks before integration**

  Run `cargo check -p function-ui` and confirm the current launcher compiles before replacing its hard-coded shell invocation.

- [ ] **Step 3: Integrate the helper in `execute_launcher_action`**

  Replace the hard-coded PowerShell thread with `spawn_shell_command`. On success, preserve current success activity/result behavior. On failure, keep the launcher visible, set `latest_result` and an activity entry with a concise actionable error, and do not claim that the command executed.

- [ ] **Step 4: Update documentation and verify no stale shortcuts remain**

  Change the documented macOS shortcut to Double Command (`⌘ ⌘`) and the Windows shortcut to the actual configured default (`Ctrl+Space`) consistently in the three files. Search with `rg -n "Command \+ ;|Alt \+ Space|Command\+;" README.md docs` and expect no stale references.

- [ ] **Step 5: Run the full verification suite**

  Run `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets --all-features -- -D warnings` where dependencies permit. Run an Apple-target check if available. Review `git diff --check` and inspect the final diff for unrelated changes.

- [ ] **Step 6: Commit**

  `git add crates/ui/src/views/function_view.rs README.md docs/17-PLATFORM.md docs/20-UX-FLOWS.md && git commit -m "fix(ui): report launcher failures and refresh shortcuts"`

## Final Review

- Confirm every acceptance criterion in the spec maps to Tasks 1–4.
- Confirm all new production functions have a failing test before implementation.
- Confirm the existing untracked `AI_AGENT_CONTEXT.md` remains untouched.
- Request an independent code review against the spec and final diff before declaring completion.

