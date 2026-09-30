//! UI presentation crate using GPUI.
//!
//! Exposes views, components, theme tokens, and action bindings for the assistant interface.

pub mod actions;
pub mod components;
pub mod theme;
pub mod views;

pub use actions::*;
pub use components::*;
pub use theme::{Theme, ThemeMode};
pub use views::{AssistantMode, AssistantView};

use std::sync::OnceLock;

static RUNTIME_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

pub fn set_runtime_handle(handle: tokio::runtime::Handle) {
    let _ = RUNTIME_HANDLE.set(handle);
}

pub fn get_runtime_handle() -> Option<tokio::runtime::Handle> {
    RUNTIME_HANDLE.get().cloned()
}
