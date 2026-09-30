//! Status badge component displaying agent state with colored dot and label.

use crate::theme::Theme;
use assistant_agent::AgentState;
use gpui::prelude::*;
use gpui::{div, px, IntoElement};

pub fn render_status_badge(state: &AgentState, theme: &Theme) -> impl IntoElement {
    let (color, label) = match state {
        AgentState::Idle => (theme.status_idle, "Ready".to_string()),
        AgentState::Listening => (theme.status_listening, "Listening...".to_string()),
        AgentState::Processing { thought_summary } => {
            let desc = thought_summary.clone().unwrap_or_else(|| "Thinking...".to_string());
            (theme.status_processing, desc)
        }
        AgentState::Acting { action_description } => (theme.status_acting, action_description.clone()),
        AgentState::WaitingForConfirmation { .. } => (theme.status_processing, "Confirmation Required".to_string()),
        AgentState::Completed { .. } => (theme.status_success, "Done".to_string()),
        AgentState::Error { .. } => (theme.status_error, "Error".to_string()),
    };

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.surface_active)
        .border_1()
        .border_color(theme.border_subtle)
        .child(
            div()
                .w(px(7.0))
                .h(px(7.0))
                .rounded_full()
                .bg(color),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.text_secondary)
                .child(label),
        )
}
