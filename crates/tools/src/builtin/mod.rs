pub mod app_tool;
pub mod browser_tool;
pub mod desktop_tools;
pub mod file_tool;
pub mod keyboard_tool;
pub mod mouse_tool;
pub mod screen_tool;
pub mod search_tool;
pub mod terminal_tool;

pub use app_tool::ApplicationTool;
pub use browser_tool::BrowserTool;
pub use desktop_tools::{
    ClickTool, CloseAppTool, DoubleClickTool, ExecuteCommandTool, OpenAppTool, PressKeyTool,
    ScrollTool, TakeScreenshotTool, TypeTextTool,
};
pub use file_tool::FileTool;
pub use keyboard_tool::KeyboardTool;
pub use mouse_tool::MouseTool;
pub use screen_tool::ScreenTool;
pub use search_tool::WebSearchTool;
pub use terminal_tool::TerminalTool;

use crate::ToolRegistry;

/// Populate a registry with all standard computer control and utility tools.
pub fn register_default_tools(registry: &mut ToolRegistry) {
    // High-level native desktop control tools (preferred for OS-aware operations)
    registry.register(OpenAppTool::new());
    registry.register(CloseAppTool::new());
    registry.register(TakeScreenshotTool::new());
    registry.register(ClickTool::new());
    registry.register(DoubleClickTool::new());
    registry.register(TypeTextTool::new());
    registry.register(PressKeyTool::new());
    registry.register(ScrollTool::new());
    registry.register(ExecuteCommandTool::new());

    // Legacy / low-level tools
    registry.register(MouseTool::new());
    registry.register(KeyboardTool::new());
    registry.register(ScreenTool::new());
    registry.register(ApplicationTool::new());
    registry.register(BrowserTool::new());
    registry.register(WebSearchTool::new());
    registry.register(FileTool::new());
    registry.register(TerminalTool::new());
}

