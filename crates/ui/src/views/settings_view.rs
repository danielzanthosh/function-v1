//! Settings and Customization View for Function (docs/02-BRANDING.md).
//!
//! Provides comprehensive in-app customization:
//! - AI Provider API Key, Model & Base URL
//! - Theme Style (Carbon Dark, Obsidian OLED, Slate Midnight, Studio Light)
//! - Accent Color (White, Cyan, Emerald, Violet, Amber)
//! - Window Positioning (Center, Upper-Third)
//! - Audio Feedback (Sound ON / Muted)
//! - Persistent saving to `~/.function/config.json`
//! - Interactive mouse click triggers and scrollable content area

use crate::components::render_logo_with_mode;
use crate::views::function_view::FunctionView;
use function_config::{AccentColor, ThemeStyle, WindowPositionMode};
use gpui::prelude::*;
use gpui::{div, px, rgba, Context, IntoElement, MouseButton, Rgba};

fn provider_field(
    view: &FunctionView,
    cx: &mut Context<FunctionView>,
    label: &'static str,
    value: &str,
    index: usize,
) -> impl IntoElement {
    let focused = view.settings_focused_field == index;
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(view.theme.text_muted).child(label))
        .child(
            div()
                .cursor_text()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(view.theme.surface_input)
                .border_1()
                .border_color(if focused { view.theme.accent_primary } else { view.theme.border_subtle })
                .text_sm()
                .text_color(view.theme.text_primary)
                .child(value.to_string())
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    this.settings_focused_field = index;
                    cx.notify();
                })),
        )
}

pub fn render_settings_view(
    view: &FunctionView,
    cx: &mut Context<FunctionView>,
) -> impl IntoElement {
    let theme = &view.theme;
    let bg_surface = theme.surface_elevated;
    let border_color_val = theme.border_subtle;
    let card_bg = theme.surface_input;
    let card_border = theme.border_subtle;
    let text_muted = theme.text_muted;
    let text_secondary = theme.text_secondary;
    let text_primary = theme.text_primary;
    let accent_col = theme.accent_primary;

    let api_key = &view.settings_api_key;
    let model = &view.settings_model;
    let base_url = &view.settings_base_url;
    let sound_enabled = view.settings_sound_enabled;
    let theme_style = view.config.theme_style;
    let accent_color = view.config.accent_color;
    let window_position = view.config.window_position;
    let show_key = view.settings_show_key;
    let focused_field = view.settings_focused_field;
    let cursor_visible = view.cursor_visible;
    let status_message = view.settings_status_message.as_deref();

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
            a: 0.98,
            ..bg_surface
        })
        .rounded_2xl()
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
                        .child(render_logo_with_mode(18.0, theme.mode == crate::theme::ThemeMode::Light))
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
                        .cursor_pointer()
                        .px_2p5()
                        .py_1()
                        .rounded_md()
                        .bg(theme.surface_base)
                        .border_1()
                        .border_color(card_border)
                        .hover(|s| s.bg(theme.surface_active))
                        .text_xs()
                        .text_color(text_secondary)
                        .child("Close (Esc)")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.go_back(window, cx);
                            }),
                        ),
                ),
        )
        // ==========================================
        // Scrollable Settings Fields
        // ==========================================
        .child(
            div()
                .id("settings_scroll_area")
                .track_scroll(&view.settings_scroll_handle)
                .overflow_y_scroll()
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
                                        .child("AI PROVIDER API KEY"),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .hover(|s| s.bg(theme.surface_active))
                                        .text_xs()
                                        .text_color(if show_key { accent_col } else { text_muted })
                                        .child(if show_key { "Revealed" } else { "Masked" })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.settings_show_key = !this.settings_show_key;
                                                cx.notify();
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .cursor_text()
                                .flex()
                                .items_center()
                                .justify_between()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(card_bg)
                                .border_1()
                                .border_color(if focused_field == 0 {
                                    accent_col
                                } else {
                                    card_border
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings_focused_field = 0;
                                        cx.notify();
                                    }),
                                )
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
                                .cursor_text()
                                .flex()
                                .items_center()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(card_bg)
                                .border_1()
                                .border_color(if focused_field == 1 {
                                    accent_col
                                } else {
                                    card_border
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings_focused_field = 1;
                                        cx.notify();
                                    }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .flex_1()
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
                                .cursor_text()
                                .flex()
                                .items_center()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(card_bg)
                                .border_1()
                                .border_color(if focused_field == 2 {
                                    accent_col
                                } else {
                                    card_border
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings_focused_field = 2;
                                        cx.notify();
                                    }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .flex_1()
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
                        ),
                )
                // Expandable advanced provider settings
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_3()
                        .rounded_md()
                        .bg(card_bg)
                        .border_1()
                        .border_color(card_border)
                        .child(
                            div()
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_between()
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                    this.toggle_advanced_settings(cx);
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).text_color(text_primary).child("ADVANCED PROVIDERS"))
                                        .child(div().text_xs().text_color(text_muted).child("AI, speech recognition, and speech synthesis")),
                                )
                                .child(div().text_sm().text_color(accent_col).child(if view.settings_advanced_expanded { "Hide" } else { "Show" })),
                        )
                        .when(view.settings_advanced_expanded, |section| {
                            section
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .cursor_pointer()
                                                .px_2()
                                                .py_1()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(card_border)
                                                .text_xs()
                                                .text_color(if view.config.speech.enabled { accent_col } else { text_muted })
                                                .child(if view.config.speech.enabled { "STT Enabled" } else { "STT Disabled" })
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                    this.config.speech.enabled = !this.config.speech.enabled;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            div()
                                                .cursor_pointer()
                                                .px_2()
                                                .py_1()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(card_border)
                                                .text_xs()
                                                .text_color(if view.config.tts.enabled { accent_col } else { text_muted })
                                                .child(if view.config.tts.enabled { "TTS Enabled" } else { "TTS Disabled" })
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                    this.config.tts.enabled = !this.config.tts.enabled;
                                                    cx.notify();
                                                })),
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
                                                .text_color(text_muted)
                                                .child("MODEL PRESETS"),
                                        )
                                        .child(
                                            div()
                                                .px_2()
                                                .py_1()
                                                .rounded_sm()
                                                .bg(card_bg)
                                                .border_1()
                                                .border_color(card_border)
                                                .text_xs()
                                                .text_color(text_primary)
                                                .child("ChatGPT")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.settings_provider = "chatgpt-plan".into();
                                                        this.settings_model = "gpt-5".into();
                                                        this.settings_base_url.clear();
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .px_2()
                                                .py_1()
                                                .rounded_sm()
                                                .bg(card_bg)
                                                .border_1()
                                                .border_color(card_border)
                                                .text_xs()
                                                .text_color(text_primary)
                                                .child("OpenAI API")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.settings_provider = "openai-compatible".into();
                                                        this.settings_model = "gpt-4o".into();
                                                        this.settings_base_url = "https://api.openai.com/v1".into();
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .px_2()
                                                .py_1()
                                                .rounded_sm()
                                                .bg(card_bg)
                                                .border_1()
                                                .border_color(card_border)
                                                .text_xs()
                                                .text_color(text_primary)
                                                .child("Gemini")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.settings_provider = "gemini".into();
                                                        this.settings_model = "gemini-2.5-flash".into();
                                                        this.settings_base_url = "https://generativelanguage.googleapis.com/v1beta/openai/".into();
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .px_2()
                                                .py_1()
                                                .rounded_sm()
                                                .bg(accent_col)
                                                .text_xs()
                                                .text_color(card_bg)
                                                .child("Sign in with ChatGPT")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        this.begin_chatgpt_login(cx);
                                                    }),
                                                ),
                                        )
                                )
                                .child(provider_field(view, cx, "AI PROVIDER", &view.settings_provider, 3))
                                .child(provider_field(view, cx, "STT API KEY", &view.settings_stt_api_key, 4))
                                .child(provider_field(view, cx, "STT MODEL", &view.settings_stt_model, 5))
                                .child(provider_field(view, cx, "STT BASE URL", &view.settings_stt_base_url, 6))
                                .child(provider_field(view, cx, "TTS API KEY", &view.settings_tts_api_key, 7))
                                .child(provider_field(view, cx, "TTS MODEL", &view.settings_tts_model, 8))
                                .child(provider_field(view, cx, "TTS BASE URL", &view.settings_tts_base_url, 9))
                                .child(provider_field(view, cx, "TTS VOICE", &view.settings_tts_voice, 10))
                        }),
                )
                // Customization Row: Theme Style & Accent Color
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_3()
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
                                        .child("Click badge to cycle appearance"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .px_2p5()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.surface_base)
                                        .border_1()
                                        .border_color(card_border)
                                        .hover(|s| s.border_color(accent_col))
                                        .text_xs()
                                        .text_color(text_primary)
                                        .child(match theme_style {
                                            ThemeStyle::CarbonDark => "Carbon Dark",
                                            ThemeStyle::ObsidianOled => "Obsidian OLED",
                                            ThemeStyle::SlateMidnight => "Slate Midnight",
                                            ThemeStyle::StudioLight => "Studio Light",
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.cycle_theme_style(cx);
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .px_2p5()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.surface_base)
                                        .border_1()
                                        .border_color(accent_col)
                                        .hover(|s| s.bg(theme.surface_active))
                                        .text_xs()
                                        .text_color(accent_col)
                                        .child(match accent_color {
                                            AccentColor::White => "White",
                                            AccentColor::Cyan => "Cyan",
                                            AccentColor::Emerald => "Emerald",
                                            AccentColor::Violet => "Violet",
                                            AccentColor::Amber => "Amber",
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.cycle_accent_color(cx);
                                            }),
                                        ),
                                ),
                        ),
                )
                // Customization Row: Centering & Audio
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_3()
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
                                        .child("Click badge to toggle mode"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .px_2p5()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.surface_base)
                                        .border_1()
                                        .border_color(card_border)
                                        .hover(|s| s.border_color(accent_col))
                                        .text_xs()
                                        .text_color(text_primary)
                                        .child(match window_position {
                                            WindowPositionMode::Center => "Center",
                                            WindowPositionMode::UpperThird => "Upper-Third",
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_window_position_mode(cx);
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .px_2p5()
                                        .py_1()
                                        .rounded_md()
                                        .bg(if sound_enabled {
                                            accent_col
                                        } else {
                                            theme.surface_base
                                        })
                                        .border_1()
                                        .border_color(card_border)
                                        .hover(|s| s.border_color(accent_col))
                                        .text_xs()
                                        .text_color(if sound_enabled {
                                            theme.surface_base
                                        } else {
                                            text_muted
                                        })
                                        .child(if sound_enabled { "Sound: ON" } else { "Sound: OFF" })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_sound_setting(cx);
                                            }),
                                        ),
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
                                .cursor_pointer()
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .bg(theme.surface_base)
                                .border_1()
                                .border_color(card_border)
                                .hover(|s| s.bg(theme.surface_active))
                                .text_xs()
                                .text_color(text_secondary)
                                .child("Cancel (Esc)")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.go_back(window, cx);
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .px_3p5()
                                .py_1p5()
                                .rounded_md()
                                .bg(accent_col)
                                .hover(|s| s.opacity(0.9))
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme.surface_base)
                                .child("Save & Apply (Enter)")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        this.save_settings(cx);
                                        this.go_back(window, cx);
                                    }),
                                ),
                        ),
                ),
        )
}
