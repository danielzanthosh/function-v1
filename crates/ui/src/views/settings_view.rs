//! Settings and Customization View for Function (docs/02-BRANDING.md).
//!
//! Provides comprehensive in-app customization:
//! - AI Provider API Key, Model & Base URL
//! - Theme Style (Carbon Dark, Obsidian OLED, Slate Midnight, Studio Light)
//! - Accent Color (White, Cyan, Emerald, Violet, Amber)
//! - Window Positioning (Center, Upper-Third)
//! - Audio Feedback (Sound ON / Muted)
//! - Persistent saving to `~/.function/config.json`

use crate::components::render_logo;
use crate::theme::Theme;
use function_config::{AccentColor, ThemeStyle, WindowPositionMode};
use gpui::prelude::*;
use gpui::{div, px, rgba, IntoElement, Rgba};

pub fn render_settings_view(
    api_key: &str,
    model: &str,
    base_url: &str,
    sound_enabled: bool,
    theme_style: ThemeStyle,
    accent_color: AccentColor,
    window_position: WindowPositionMode,
    show_key: bool,
    focused_field: usize,
    cursor_visible: bool,
    status_message: Option<&str>,
    theme: &Theme,
) -> impl IntoElement {
    let bg_surface = theme.surface_elevated;
    let border_color_val = theme.border_subtle;
    let card_bg = theme.surface_input;
    let card_border = theme.border_subtle;
    let text_muted = theme.text_muted;
    let text_secondary = theme.text_secondary;
    let text_primary = theme.text_primary;
    let accent_col = theme.accent_primary;

    // Display string for API key
    let display_api_key = if api_key.is_empty() {
        "Type or paste your API key here (sk-...)".to_string()
    } else if show_key {
        api_key.to_string()
    } else {
        "•".repeat(api_key.len().min(32))
    };

    let display_model = if model.is_empty() {
        "gpt-4o".to_string()
    } else {
        model.to_string()
    };

    let display_base_url = if base_url.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else {
        base_url.to_string()
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .bg(Rgba {
            a: 0.95,
            ..bg_surface
        })
        .rounded_2xl() // Curved frameless window
        .shadow_xl()
        .overflow_hidden()
        // ==========================================
        // Top Header
        // ==========================================
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(52.0))
                .px_4()
                .border_b_1()
                .border_color(border_color_val)
                .bg(card_bg)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(render_logo(18.0))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(text_primary)
                                        .child("Function Preferences"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_muted)
                                        .child("AI Provider, Appearance & Native Behavior"),
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
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(theme.surface_base)
                                .border_1()
                                .border_color(card_border)
                                .text_xs()
                                .text_color(text_muted)
                                .child("Esc to return"),
                        ),
                ),
        )
        // ==========================================
        // Settings Fields
        // ==========================================
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .p_4()
                .gap_3()
                // Field 0: API Key
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(if focused_field == 0 {
                                            accent_col
                                        } else {
                                            text_secondary
                                        })
                                        .child("AI PROVIDER API KEY (Tab to switch field, Ctrl+H to mask/reveal)"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(if show_key { accent_col } else { text_muted })
                                        .child(if show_key { "[Ctrl+H: Masked]" } else { "[Ctrl+H: Revealed]" }),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(card_bg)
                                .border_1()
                                .border_color(if focused_field == 0 {
                                    accent_col
                                } else {
                                    card_border
                                })
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .flex_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(if api_key.is_empty() {
                                                    text_muted
                                                } else {
                                                    text_primary
                                                })
                                                .child(display_api_key),
                                        )
                                        .child(if focused_field == 0 {
                                            div()
                                                .w(px(2.0))
                                                .h(px(14.0))
                                                .bg(if cursor_visible {
                                                    accent_col
                                                } else {
                                                    rgba(0x00000000)
                                                })
                                        } else {
                                            div()
                                        }),
                                ),
                        ),
                )
                // Field 1: Model
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(if focused_field == 1 {
                                    accent_col
                                } else {
                                    text_secondary
                                })
                                .child("AI MODEL"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(card_bg)
                                .border_1()
                                .border_color(if focused_field == 1 {
                                    accent_col
                                } else {
                                    card_border
                                })
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(text_primary)
                                        .child(display_model),
                                )
                                .child(if focused_field == 1 {
                                    div()
                                        .w(px(2.0))
                                        .h(px(14.0))
                                        .bg(if cursor_visible {
                                            accent_col
                                        } else {
                                            rgba(0x00000000)
                                        })
                                } else {
                                    div()
                                }),
                        ),
                )
                // Field 2: Base URL
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(if focused_field == 2 {
                                    accent_col
                                } else {
                                    text_secondary
                                })
                                .child("API BASE URL"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(card_bg)
                                .border_1()
                                .border_color(if focused_field == 2 {
                                    accent_col
                                } else {
                                    card_border
                                })
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(text_primary)
                                        .child(display_base_url),
                                )
                                .child(if focused_field == 2 {
                                    div()
                                        .w(px(2.0))
                                        .h(px(14.0))
                                        .bg(if cursor_visible {
                                            accent_col
                                        } else {
                                            rgba(0x00000000)
                                        })
                                } else {
                                    div()
                                }),
                        ),
                )
                // Customization Row: Theme Style & Accent Color
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_2p5()
                        .rounded_md()
                        .bg(card_bg)
                        .border_1()
                        .border_color(card_border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(text_primary)
                                        .child("THEME & ACCENT"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_muted)
                                        .child("Ctrl+T: Cycle theme | Ctrl+A: Cycle accent"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .bg(theme.surface_base)
                                        .border_1()
                                        .border_color(card_border)
                                        .text_xs()
                                        .text_color(text_primary)
                                        .child(match theme_style {
                                            ThemeStyle::CarbonDark => "Carbon Dark (Brand)",
                                            ThemeStyle::ObsidianOled => "Obsidian OLED",
                                            ThemeStyle::SlateMidnight => "Slate Midnight",
                                            ThemeStyle::StudioLight => "Studio Light",
                                        }),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .bg(theme.surface_base)
                                        .border_1()
                                        .border_color(accent_col)
                                        .text_xs()
                                        .text_color(accent_col)
                                        .child(match accent_color {
                                            AccentColor::White => "White (Brand)",
                                            AccentColor::Cyan => "Cyan",
                                            AccentColor::Emerald => "Emerald",
                                            AccentColor::Violet => "Violet",
                                            AccentColor::Amber => "Amber",
                                        }),
                                ),
                        ),
                )
                // Customization Row: Centering & Audio
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_2p5()
                        .rounded_md()
                        .bg(card_bg)
                        .border_1()
                        .border_color(card_border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(text_primary)
                                        .child("WINDOW POSITION & SOUND"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_muted)
                                        .child("Ctrl+P: Toggle position | Ctrl+S: Toggle sound"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .bg(theme.surface_base)
                                        .border_1()
                                        .border_color(card_border)
                                        .text_xs()
                                        .text_color(text_primary)
                                        .child(match window_position {
                                            WindowPositionMode::Center => "Position: Center",
                                            WindowPositionMode::UpperThird => "Position: Upper-Third",
                                        }),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .bg(if sound_enabled {
                                            accent_col
                                        } else {
                                            theme.surface_base
                                        })
                                        .border_1()
                                        .border_color(card_border)
                                        .text_xs()
                                        .text_color(if sound_enabled {
                                            theme.surface_base
                                        } else {
                                            text_muted
                                        })
                                        .child(if sound_enabled { "Sound: ON" } else { "Sound: MUTED" }),
                                ),
                        ),
                ),
        )
        // ==========================================
        // Bottom Actions & Status Bar
        // ==========================================
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_4()
                .py_2p5()
                .border_t_1()
                .border_color(border_color_val)
                .bg(card_bg)
                .child(
                    div()
                        .text_xs()
                        .text_color(if status_message.is_some() {
                            accent_col
                        } else {
                            text_muted
                        })
                        .child(
                            status_message
                                .unwrap_or("Press Enter to Save & Apply | Esc to Exit")
                                .to_string(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(theme.surface_base)
                                .border_1()
                                .border_color(card_border)
                                .text_xs()
                                .text_color(text_secondary)
                                .child("Cancel (Esc)"),
                        )
                        .child(
                            div()
                                .px_3p5()
                                .py_1()
                                .rounded_md()
                                .bg(accent_col)
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme.surface_base)
                                .child("Save & Apply (Enter)"),
                        ),
                ),
        )
}
