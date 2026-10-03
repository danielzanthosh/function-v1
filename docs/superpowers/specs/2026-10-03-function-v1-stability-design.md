# Function Version 1 Stability and macOS Interaction Design

## Goal

Make Function’s existing desktop assistant flow reliable on macOS and preserve Windows behavior while addressing concrete correctness issues discovered during audit. The primary user-visible outcome is that the floating Function window can be summoned, interacted with using mouse and keyboard, resized, and dismissed reliably on macOS.

## Scope

### Included

- macOS window lifecycle and interaction:
  - Identify and operate only on Function’s application window during activation and dismissal.
  - Make activation idempotent and ensure the window can become key/main, receives mouse events, is visible, and is ordered front.
  - Prevent the activation observer from dismissing Function during its own summon or transient AppKit activation/menu transitions.
  - Preserve the intended click-outside-to-dismiss behavior after activation settles.
- Cross-platform shell launcher correctness:
  - Use PowerShell on Windows.
  - Use the user’s login shell on macOS and Unix-like systems.
  - Return/report launch failures instead of silently discarding them.
- Regression coverage for platform-independent state/selection logic and existing macOS shortcut parsing.
- Small documentation and user-facing consistency fixes directly supported by the audit, including stale shortcut text.

### Excluded

- Replacing GPUI or introducing a second windowing framework.
- Redesigning the product’s visual language or adding a new major feature area.
- Changing permission policy for terminal, file, browser, or computer-control tools.
- Claiming runtime macOS verification from a Windows host; Apple-target compilation and tests will be used when available, with real macOS smoke testing reported separately if unavailable.

## Design

### Window lifecycle

The UI remains the authority for logical visibility, while platform helpers perform native operations. The macOS helper will filter for Function’s actual GPUI window using stable properties available through AppKit rather than applying activation changes broadly to every application window. Native activation will be safe to call repeatedly.

The activation sequence is:

1. Capture the previously frontmost application only when it is not Function.
2. Activate/unhide Function.
3. Configure the Function window’s key/main eligibility, mouse-event handling, floating level, visibility, and ordering.
4. Request GPUI activation and focus.
5. Mark the transition settled only after the native activation turn has completed.

Dismissal will order out the Function window, deactivate Function, and restore the retained previous application when available. Status-bar and menu windows must never be hidden or modified by this flow.

### Deactivation observer

The observer will distinguish intentional dismissal from transient deactivation during summon, native menu handling, and key-window exchange. A short settling guard will be owned by the view and cleared by the next stable activation state. Only a confirmed external deactivation after settling will invoke dismissal. The guard must not suppress explicit Escape, hotkey toggle, menu dismissal, or quit behavior.

### Shell execution

The launcher will call a small platform-neutral function that selects the shell based on compile target and returns a launch result. Windows will invoke `powershell -NoProfile -Command`; macOS and Unix-like targets will invoke the configured login shell with `-lc`. The UI will clear/advance its state only after a successful spawn and will show a concise error state when spawning fails.

### Tests and verification

- Add tests for the platform-neutral shell command selection and launch-error handling without executing arbitrary user commands.
- Add tests for visibility/deactivation transition decisions using a deterministic state model or helper rather than AppKit itself.
- Preserve and extend shortcut parsing tests for Double Command and legacy forms.
- Run `cargo fmt --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` where dependencies permit, and a macOS target check/build if an Apple target/toolchain is installed.
- Inspect the final diff for unrelated changes and document any verification limited by the host platform.

## Acceptance criteria

1. On macOS, summoning Function results in a visible, key, mouse-interactive window that accepts text input and button clicks.
2. Repeated summon/toggle calls do not duplicate state changes or hide the window during activation.
3. Clicking outside or pressing Escape dismisses Function and returns focus to the prior app without affecting the status-bar menu.
4. Launcher shell actions select the correct shell on each supported platform and expose spawn failures to the user.
5. Existing workspace tests remain green, new regressions are covered, and no new warnings are introduced in checked targets.

