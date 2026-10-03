//! Conversation History View for Function.
//!
//! Provides a dedicated interface to browse past chats, resume previous conversations,
//! or initiate fresh chats. Controlled via the "conversation" command.

use crate::components::render_brand_mark_with_mode;
use crate::conversation::SavedConversation;
use crate::theme::Theme;
use gpui::prelude::*;
use gpui::{div, px, rgba, IntoElement, Rgba};
use std::time::{SystemTime, UNIX_EPOCH};

fn format_relative_time(timestamp: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let diff = now.saturating_sub(timestamp);
    if diff < 60 {
        "Just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

pub fn render_conversation_view(
    conversations: &[SavedConversation],
    selected_index: usize,
    active_id: Option<&str>,
    theme: &Theme,
    status_message: Option<&str>,
) -> impl IntoElement {
    let bg_surface = theme.surface_elevated;
    let card_bg = theme.surface_input;
    let text_muted = theme.text_muted;
    let text_secondary = theme.text_secondary;
    let text_primary = theme.text_primary;
    let accent_col = theme.accent_primary;

    div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .bg(bg_surface)
        .rounded_2xl()
        .border_1()
        .border_color(rgba(0xf1f0ef1f))
        .shadow_xl()
        .overflow_hidden()
        .child(
            // Header Bar
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_5()
                .py_3()
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(render_brand_mark_with_mode(18.0, theme.mode == crate::theme::ThemeMode::Light))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(text_primary)
                                        .child("Conversations & History"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_muted)
                                        .child("Browse old chats or start a new conversation"),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(card_bg)
                                .text_color(text_secondary)
                                .child("Esc: Back"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(card_bg)
                                .text_color(text_secondary)
                                .child("N: New Chat"),
                        ),
                ),
        )
        // Status banner if present
        .when_some(status_message, |parent, msg| {
            parent.child(
                div()
                    .px_5()
                    .py_2()
                    .bg(Rgba { a: 0.15, ..accent_col })
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(accent_col)
                            .child(msg.to_string()),
                    ),
            )
        })
        // Scrollable list of actions and conversations
        .child(
            div()
                .id("conversation_scroll")
                .flex()
                .flex_col()
                .flex_1()
                .overflow_y_scroll()
                .px_4()
                .py_3()
                .gap_2()
                // Action 0: Start New Conversation
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_3()
                        .rounded_xl()
                        .bg(if selected_index == 0 {
                            Rgba { a: 0.12, ..accent_col }
                        } else {
                            card_bg
                        })
                        .border_1()
                        .border_color(if selected_index == 0 {
                            accent_col
                        } else {
                            theme.border_subtle
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(accent_col)
                                        .child("+"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(text_primary)
                                                .child("Start New Chat"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(text_muted)
                                                .child("Clear active context and return to home prompt"),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(if selected_index == 0 { accent_col } else { text_muted })
                                .child("Enter ↵"),
                        ),
                )
                // Separator if we have conversations
                .when(!conversations.is_empty(), |p| {
                    p.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pt_2()
                            .pb_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(text_muted)
                                    .child("PREVIOUS CHATS"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(1.0))
                                    .bg(theme.border_subtle),
                            ),
                    )
                })
                // Empty state if no saved conversations
                .when(conversations.is_empty(), |p| {
                    p.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .py_8()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(text_muted)
                                    .child("No previous conversations saved yet."),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_muted)
                                    .child("Chats are automatically saved as you converse with Function."),
                            ),
                    )
                })
                // Saved conversations list (index 1..=conversations.len())
                .children(conversations.iter().enumerate().map(|(idx, conv)| {
                    let is_selected = selected_index == idx + 1;
                    let is_current = active_id.map(|id| id == conv.id).unwrap_or(false);
                    let msg_count = conv.display_messages.len();
                    let time_str = format_relative_time(conv.updated_at);
                    let preview_text = conv.preview();

                    div()
                        .flex()
                        .flex_col()
                        .px_4()
                        .py_2p5()
                        .rounded_xl()
                        .bg(if is_selected {
                            Rgba { a: 0.12, ..accent_col }
                        } else {
                            card_bg
                        })
                        .border_1()
                        .border_color(if is_selected {
                            accent_col
                        } else {
                            theme.border_subtle
                        })
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(text_primary)
                                                .child(conv.title.clone()),
                                        )
                                        .when(is_current, |p| {
                                            p.child(
                                                div()
                                                    .text_xs()
                                                    .px_1p5()
                                                    .py(px(1.0))
                                                    .rounded_sm()
                                                    .bg(Rgba { a: 0.2, ..accent_col })
                                                    .text_color(accent_col)
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .child("Active"),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(text_muted)
                                                .child(format!("{} msgs", msg_count)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(text_muted)
                                                .child("•"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(text_muted)
                                                .child(time_str),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_secondary)
                                        .child(preview_text),
                                )
                                .when(is_selected, |p| {
                                    p.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(text_muted)
                                                    .child("Del: Delete"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(accent_col)
                                                    .child("Enter ↵"),
                                            ),
                                    )
                                }),
                        )
                })),
        )
        // Footer bar with keyboard hints
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_5()
                .py_2()
                .border_t_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .text_xs()
                        .text_color(text_muted)
                        .child("↑/↓: Navigate  •  Enter: Select  •  N: New Chat  •  Del: Remove  •  Esc: Back"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(text_muted)
                        .child(format!("{} saved", conversations.len())),
                ),
        )
}
