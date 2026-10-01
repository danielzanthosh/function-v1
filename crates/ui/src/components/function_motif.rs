//! Function Visual Motif (Geometric Brand Mark).
//!
//! Renders the official Function monochrome geometric mark (semi-circle with
//! dithered particles) and brings it alive across cognitive states:
//! - Idle: Clean, steady, precision brand mark.
//! - Listening: Subtle organic breath modulation inspired by particle frequency.
//! - Thinking: Precision cognitive rhythm across the dithered horizon.
//! - Acting: Understated directional execution transition.

use super::logo::render_brand_mark;
use crate::theme::Theme;
use function_agent::AgentState;
use gpui::prelude::*;
use gpui::{div, px, IntoElement};

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

/// Render the animated Function geometric motif using the actual brand asset.
pub fn render_function_motif(
    motif_state: MotifState,
    theme: &Theme,
    tick: usize,
    width: f32,
) -> impl IntoElement {
    let text_sec = theme.text_secondary;
    let height = width / 2.0;

    match motif_state {
        MotifState::Idle => div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(width))
            .h(px(height))
            .child(render_brand_mark(width)),
        MotifState::Listening => {
            // Subtle rhythmic acoustic breath (shifts opacity slightly between 0.75 and 1.0)
            let opacity = match tick % 4 {
                0 => 0.75,
                1 => 0.90,
                2 => 1.00,
                _ => 0.85,
            };
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(width))
                .h(px(height))
                .opacity(opacity)
                .child(render_brand_mark(width))
        }
        MotifState::Thinking => {
            // Precision cognitive pulse across the particle horizon
            let opacity = match (tick / 2) % 4 {
                0 => 0.60,
                1 => 0.80,
                2 => 1.00,
                _ => 0.75,
            };
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(width))
                .h(px(height))
                .opacity(opacity)
                .child(render_brand_mark(width))
        }
        MotifState::Acting => div()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .child(render_brand_mark(width * 0.85))
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(text_sec)
                    .child("→"),
            ),
    }
}
