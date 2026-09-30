pub mod app_tool;
pub mod browser_tool;
pub mod file_tool;
pub mod keyboard_tool;
pub mod mouse_tool;
pub mod screen_tool;
pub mod search_tool;
pub mod terminal_tool;

pub use app_tool::ApplicationTool;
pub use browser_tool::BrowserTool;
pub use file_tool::FileTool;
pub use keyboard_tool::KeyboardTool;
pub use mouse_tool::MouseTool;
pub use screen_tool::ScreenTool;
pub use search_tool::WebSearchTool;
pub use terminal_tool::TerminalTool;

use crate::ToolRegistry;

/// Populate a registry with all standard computer control and utility tools.
pub fn register_default_tools(registry: &mut ToolRegistry) {
    registry.register(MouseTool::new());
    registry.register(KeyboardTool::new());
    registry.register(ScreenTool::new());
    registry.register(ApplicationTool::new());
    registry.register(BrowserTool::new());
    registry.register(WebSearchTool::new());
    registry.register(FileTool::new());
    registry.register(TerminalTool::new());
}
