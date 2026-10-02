pub mod conversation_view;
pub mod function_view;
pub mod settings_view;

pub use conversation_view::render_conversation_view;
pub use function_view::{FunctionMode, FunctionView};

// Backward-compatibility aliases
pub type AssistantMode = FunctionMode;
pub type AssistantView = FunctionView;
pub use function_view as assistant_view;
pub use settings_view::render_settings_view;
