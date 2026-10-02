//! Keyboard-first action bindings for the function interface.

use gpui::actions;

actions!(
    function,
    [
        OpenFunction,
        CloseFunction,
        SubmitRequest,
        CancelTask,
        ToggleExpanded,
        ToggleTheme,
        ToggleVoice,
        OpenSettings,
        ClearInput,
        ToggleSpotlight,
        QuitFunction
    ]
);

// Backward-compatibility aliases
pub type OpenAssistant = OpenFunction;
pub type CloseAssistant = CloseFunction;
