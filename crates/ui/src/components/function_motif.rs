//! Function Visual Motif (Geometric Mark).
//!
//! A custom, proprietary visual identity mark for Function.
//! Replaces generic glowing orbs and spinning loaders with a technical,
//! mathematical/computing mark derived from the Function [ƒ] identity.
//!
//! Dynamically morphs between four cognitive states:
//! - Idle: Clean, steady, precision geometric mark.
//! - Listening: Three rhythmic micro-bars breathing with audio input.
//! - Thinking: A subtle synchronized quantum light pulse communicating intelligence.
//! - Acting: A directional execution glyph indicating autonomous computer control.

use crate::theme::Theme;
use function_agent::AgentState;
use gpui::prelude::*;
use gpui::{div, px, IntoElement, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotifState {
    Idle,
    Listening,
    Thinking,
    Acting,
}

impl MotifState {
    pub fn from_agent_state(state: &AgentState, is_listening: bool) -> Self {
        if is_listening {
            return Self::Listening;
        }
        match state {
            AgentState::Idle | AgentState::Completed { .. } => Self::Idle,
            AgentState::Listening => Self::Listening,
            AgentState::Processing { .. } => Self::Thinking,
            AgentState::Acting { .. } | AgentState::WaitingForConfirmation { .. } => Self::Acting,
            AgentState::Error { .. } => Self::Idle,
        }
    }
}

/// Render the animated Function geometric motif.
pub fn render_function_motif(
    motif_state: MotifState,
    theme: &Theme,
    tick: usize,
    size: f32,
) -> impl IntoElement {
    let accent = theme.accent_primary;
    let muted = theme.text_muted;
    let surface_bg = theme.surface_active;

    let outer_size = size;
    let inner_height = size * 0.55;

    match motif_state {
        MotifState::Idle => {
            // Precision geometric mark: A technical bracket frame [ • ]
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(outer_size))
                .h(px(outer_size))
                .rounded_md()
                .bg(surface_bg)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(2.5))
                        // Left bracket stroke
                        .child(
                            div()
                                .w(px(2.0))
                                .h(px(inner_height))
                                .rounded_full()
                                .bg(muted),
                        )
                        // Central precision node
                        .child(div().w(px(3.5)).h(px(3.5)).rounded_full().bg(accent))
                        // Right bracket stroke
                        .child(
                            div()
                                .w(px(2.0))
                                .h(px(inner_height))
                                .rounded_full()
                                .bg(muted),
                        ),
                )
        }
        MotifState::Listening => {
            // Three rhythmic waveform bars breathing with audio cadence
            let phase = tick % 4;
            let h1 = match phase {
                0 => 6.0,
                1 => 12.0,
                2 => 16.0,
                _ => 10.0,
            };
            let h2 = match phase {
                0 => 16.0,
                1 => 8.0,
                2 => 14.0,
                _ => 18.0,
            };
            let h3 = match phase {
                0 => 10.0,
                1 => 16.0,
                2 => 8.0,
                _ => 12.0,
            };

            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(outer_size))
                .h(px(outer_size))
                .rounded_md()
                .bg(surface_bg)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(2.5))
                        .child(div().w(px(2.0)).h(px(h1)).rounded_full().bg(accent))
                        .child(div().w(px(2.5)).h(px(h2)).rounded_full().bg(accent))
                        .child(div().w(px(2.0)).h(px(h3)).rounded_full().bg(accent)),
                )
        }
        MotifState::Thinking => {
            // Technical cognitive pulse: synchronized energy gliding across brackets
            let step = (tick / 2) % 3;
            let (c1, c2, c3) = match step {
                0 => (accent, muted, muted),
                1 => (muted, accent, muted),
                _ => (muted, muted, accent),
            };

            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(outer_size))
                .h(px(outer_size))
                .rounded_md()
                .bg(surface_bg)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(2.5))
                        .child(div().w(px(2.0)).h(px(inner_height)).rounded_full().bg(c1))
                        .child(div().w(px(3.5)).h(px(3.5)).rounded_full().bg(c2))
                        .child(div().w(px(2.0)).h(px(inner_height)).rounded_full().bg(c3)),
                )
        }
        MotifState::Acting => {
            // Autonomous computer execution: forward action glyph [ > ]
            let pulse_color = if (tick / 2) % 2 == 0 {
                accent
            } else {
                Rgba { a: 0.7, ..accent }
            };

            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(outer_size))
                .h(px(outer_size))
                .rounded_md()
                .bg(surface_bg)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(2.0))
                        // Directional execution mark
                        .child(
                            div()
                                .w(px(2.0))
                                .h(px(inner_height * 0.8))
                                .rounded_full()
                                .bg(muted),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(pulse_color)
                                .child("›"),
                        )
                        .child(
                            div()
                                .w(px(2.0))
                                .h(px(inner_height * 0.8))
                                .rounded_full()
                                .bg(muted),
                        ),
                )
        }
    }
}
