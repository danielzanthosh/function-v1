//! Keyboard-first action bindings for the assistant interface.

use gpui::actions;

actions!(
    assistant,
    [
        OpenAssistant,
        CloseAssistant,
        SubmitRequest,
        CancelTask,
        ToggleExpanded,
        ToggleTheme,
        ToggleVoice,
        OpenSettings,
        ClearInput,
        ToggleSpotlight
    ]
);
