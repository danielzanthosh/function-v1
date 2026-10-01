//! Activity feed component rendering the Observe-Think-Act action sequence.

use crate::theme::Theme;
use gpui::prelude::*;
use gpui::{div, IntoElement};

#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub step: usize,
    pub description: String,
    pub status: ActivityStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityStatus {
    Running,
    Done,
    Failed,
}

pub fn render_activity_list(activities: &[ActivityEntry], theme: &Theme) -> impl IntoElement {
    if activities.is_empty() {
        return div()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .rounded_md()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("No tool actions executed yet. Enter a task to begin."),
            );
    }

    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .rounded_md()
        .bg(theme.surface_elevated)
        .border_1()
        .border_color(theme.border_subtle)
        .children(activities.iter().map(|item| {
            let (status_text, status_color) = match item.status {
                ActivityStatus::Running => ("running", theme.status_processing),
                ActivityStatus::Done => ("done", theme.status_success),
                ActivityStatus::Failed => ("failed", theme.status_error),
            };

            div()
                .flex()
                .items_center()
                .justify_between()
                .py_1()
                .px_2()
                .rounded_sm()
                .hover(|s| s.bg(theme.surface_active))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(format!("{}.", item.step)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_primary)
                                .child(item.description.clone()),
                        ),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.surface_base)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_xs()
                        .text_color(status_color)
                        .child(status_text),
                )
        }))
}
