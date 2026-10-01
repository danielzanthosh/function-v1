//! Action bar and keyboard shortcut indicator component.

use crate::theme::Theme;
use gpui::prelude::*;
use gpui::{div, IntoElement};

pub fn render_action_bar(theme: &Theme, expanded: bool, listening: bool) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .pt_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(render_shortcut_hint("Enter", "Submit", theme))
                .child(render_shortcut_hint(
                    "Tab",
                    if expanded { "Compact" } else { "Expand" },
                    theme,
                ))
                .child(render_shortcut_hint("Esc", "Dismiss", theme)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(render_badge_button(
                    if listening { "REC" } else { "VOICE" },
                    if listening {
                        theme.status_listening
                    } else {
                        theme.text_muted
                    },
                    theme,
                ))
                .child(render_badge_button(
                    if theme.is_dark() { "Light" } else { "Dark" },
                    theme.text_secondary,
                    theme,
                )),
        )
}

fn render_shortcut_hint(
    key: &'static str,
    action: &'static str,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .px_1()
                .rounded_sm()
                .bg(theme.surface_active)
                .border_1()
                .border_color(theme.border_subtle)
                .text_xs()
                .text_color(theme.text_muted)
                .child(key),
        )
        .child(div().text_xs().text_color(theme.text_muted).child(action))
}

fn render_badge_button(label: &'static str, color: gpui::Rgba, theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .px_2()
        .py_1()
        .rounded_sm()
        .bg(theme.surface_elevated)
        .border_1()
        .border_color(theme.border_subtle)
        .text_xs()
        .text_color(color)
        .hover(|s| s.bg(theme.surface_active))
        .child(label)
}
