//! Clean, structured error component for Function chat UI.
//!
//! Replaces raw JSON / API error dumps with a human-readable error card
//! featuring collapsed and expanded states, Gemini quota limit detection,
//! formatted JSON formatting, and one-click copy for technical details.

use crate::theme::Theme;
use gpui::prelude::*;
use gpui::{div, px, Div, Rgba, Window};
use serde_json::Value;

/// Structured error details extracted from raw error strings or JSON payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedErrorInfo {
    pub title: String,
    pub summary: String,
    pub status: Option<String>,
    pub message: Option<String>,
    pub quota_info: Option<String>,
    pub formatted_json: Option<String>,
    pub raw_text: String,
}

/// Detect and parse raw error messages into a structured `ParsedErrorInfo`.
pub fn parse_error_info(raw: &str) -> Option<ParsedErrorInfo> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Check if string looks like an error message
    let is_error_prefixed = trimmed.starts_with("Error:")
        || trimmed.starts_with("Error ")
        || trimmed.starts_with("Provider error:")
        || trimmed.starts_with("API error")
        || trimmed.contains("RESOURCE_EXHAUSTED");

    // Extract potential JSON payload from the raw text
    let json_val = parse_json_from_text(trimmed);

    if let Some(val) = json_val {
        let is_resource_exhausted = trimmed.contains("RESOURCE_EXHAUSTED")
            || val.to_string().contains("RESOURCE_EXHAUSTED")
            || val.get("error").and_then(|e| e.get("status")).and_then(|s| s.as_str()) == Some("RESOURCE_EXHAUSTED")
            || val.get("error").and_then(|e| e.get("code")).and_then(|c| c.as_u64()) == Some(429)
            || val.get("code").and_then(|c| c.as_u64()) == Some(429);

        let pretty_json = serde_json::to_string_pretty(&val).ok();

        if is_resource_exhausted {
            let status = val
                .get("error")
                .and_then(|e| e.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("RESOURCE_EXHAUSTED")
                .to_string();

            let msg = val
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Resource has been exhausted (e.g. check quota).")
                .to_string();

            return Some(ParsedErrorInfo {
                title: "Gemini API Error".to_string(),
                summary: "The AI provider temporarily ran out of available quota.".to_string(),
                status: Some(format!("{} (429)", status)),
                message: Some(msg),
                quota_info: Some("Rate limit or project quota reached. Please wait a moment or check your API quota limits.".to_string()),
                formatted_json: pretty_json,
                raw_text: trimmed.to_string(),
            });
        }

        // Generic JSON error payload
        let err_obj = val.get("error").unwrap_or(&val);
        let code = err_obj.get("code").map(|c| c.to_string());
        let status_str = err_obj.get("status").and_then(|s| s.as_str()).map(|s| s.to_string());
        let msg_str = err_obj
            .get("message")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());

        let title = if let Some(ref s) = status_str {
            match s.as_str() {
                "UNAUTHENTICATED" => "Authentication Error".to_string(),
                "NOT_FOUND" => "Model Not Found".to_string(),
                "UNAVAILABLE" => "Service Unavailable".to_string(),
                "INVALID_ARGUMENT" => "Invalid Request".to_string(),
                _ => "API Request Failed".to_string(),
            }
        } else {
            "Request Failed".to_string()
        };

        let summary = msg_str
            .clone()
            .unwrap_or_else(|| "The AI provider returned an API error response.".to_string());

        let combined_status = match (status_str, code) {
            (Some(s), Some(c)) => Some(format!("{} ({})", s, c)),
            (Some(s), None) => Some(s),
            (None, Some(c)) => Some(format!("HTTP {}", c)),
            (None, None) => None,
        };

        return Some(ParsedErrorInfo {
            title,
            summary,
            status: combined_status,
            message: msg_str,
            quota_info: None,
            formatted_json: pretty_json,
            raw_text: trimmed.to_string(),
        });
    }

    if is_error_prefixed {
        if trimmed.contains("RESOURCE_EXHAUSTED") || trimmed.contains("429") {
            return Some(ParsedErrorInfo {
                title: "Gemini API Error".to_string(),
                summary: "The AI provider temporarily ran out of available quota.".to_string(),
                status: Some("RESOURCE_EXHAUSTED (429)".to_string()),
                message: Some(trimmed.to_string()),
                quota_info: Some("Rate limit or project quota reached. Please wait a moment or check your API quota limits.".to_string()),
                formatted_json: None,
                raw_text: trimmed.to_string(),
            });
        }

        let clean_msg = trimmed
            .trim_start_matches("Error:")
            .trim_start_matches("Provider error:")
            .trim();

        return Some(ParsedErrorInfo {
            title: "Request Failed".to_string(),
            summary: clean_msg.lines().next().unwrap_or(clean_msg).to_string(),
            status: None,
            message: Some(clean_msg.to_string()),
            quota_info: None,
            formatted_json: None,
            raw_text: trimmed.to_string(),
        });
    }

    None
}

/// Helper to parse JSON from text, even if enclosed inside markdown blocks or error prefixes.
fn parse_json_from_text(text: &str) -> Option<Value> {
    if let Ok(val) = serde_json::from_str::<Value>(text) {
        if val.is_object() {
            return Some(val);
        }
    }

    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let candidate = &text[start..=end];
                if let Ok(val) = serde_json::from_str::<Value>(candidate) {
                    if val.is_object() {
                        return Some(val);
                    }
                }
            }
        }
    }

    None
}

/// Render structured error component.
pub fn render_error_card<F, C>(
    info: &ParsedErrorInfo,
    is_expanded: bool,
    theme: &Theme,
    on_toggle: F,
    on_copy: C,
) -> Div
where
    F: Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    C: Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
{
    let card_bg = theme.surface_elevated;
    let border_color = Rgba {
        a: 0.35,
        ..theme.status_error
    };
    let text_primary = theme.text_primary;
    let text_muted = theme.text_muted;
    let text_secondary = theme.text_secondary;
    let error_red = theme.status_error;

    div()
        .w_full()
        .max_w(px(580.0))
        .flex()
        .flex_col()
        .p_3()
        .rounded_xl()
        .bg(card_bg)
        .border_1()
        .border_color(border_color)
        .shadow_sm()
        .gap_2()
        // Collapsed Header State
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .overflow_hidden()
                        // Error indicator icon
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(20.0))
                                .h(px(20.0))
                                .rounded_full()
                                .bg(Rgba { a: 0.15, ..error_red })
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(error_red)
                                .child("!"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(error_red)
                                        .child(info.title.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_secondary)
                                        .overflow_hidden()
                                        .child(info.summary.clone()),
                                ),
                        ),
                )
                .child(
                    div()
                        .cursor_pointer()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(theme.surface_input)
                        .hover(|s| s.bg(theme.surface_active))
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(text_muted)
                        .child(if is_expanded { "Hide details" } else { "Show details" })
                        .on_mouse_down(gpui::MouseButton::Left, on_toggle),
                ),
        )
        // Expanded Details Section
        .when(is_expanded, |p| {
            p.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(theme.border_subtle)
                    // Status & Quota Tags
                    .when_some(info.status.clone(), |p, status| {
                        p.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(text_muted)
                                        .child("STATUS:"),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(Rgba { a: 0.15, ..error_red })
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(error_red)
                                        .child(status),
                                ),
                        )
                    })
                    .when_some(info.quota_info.clone(), |p, quota| {
                        p.child(
                            div()
                                .px_2p5()
                                .py_1p5()
                                .rounded_md()
                                .bg(Rgba { a: 0.1, ..theme.accent_primary })
                                .text_xs()
                                .text_color(text_primary)
                                .child(format!("💡 {}", quota)),
                        )
                    })
                    // Formatted JSON or Technical Details Block
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
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(text_muted)
                                            .child("TECHNICAL DETAILS"),
                                    )
                                    .child(
                                        div()
                                            .cursor_pointer()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(theme.surface_input)
                                            .hover(|s| s.bg(theme.surface_active))
                                            .text_xs()
                                            .text_color(text_secondary)
                                            .child("Copy details")
                                            .on_mouse_down(gpui::MouseButton::Left, on_copy),
                                    ),
                            )
                            .child(
                                div()
                                    .p_2p5()
                                    .rounded_lg()
                                    .bg(theme.surface_input)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                        .overflow_hidden()
                                    .child(
                                        div()
                                            .text_xs()
                                            .line_height(px(18.0))
                                            .text_color(text_secondary)
                                            .child(
                                                info.formatted_json
                                                    .clone()
                                                    .unwrap_or_else(|| info.raw_text.clone()),
                                            ),
                                    ),
                            ),
                    ),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_resource_exhausted_detection() {
        let json_err = r#"{
            "error": {
                "code": 429,
                "message": "Resource has been exhausted (e.g. check quota).",
                "status": "RESOURCE_EXHAUSTED"
            }
        }"#;

        let parsed = parse_error_info(json_err).expect("Failed to parse Gemini quota error");
        assert_eq!(parsed.title, "Gemini API Error");
        assert_eq!(parsed.summary, "The AI provider temporarily ran out of available quota.");
        assert_eq!(parsed.status, Some("RESOURCE_EXHAUSTED (429)".to_string()));
        assert!(parsed.formatted_json.is_some());
        assert!(parsed.quota_info.is_some());
    }

    #[test]
    fn test_prefixed_error_string_detection() {
        let raw = "Error: Provider error: API error (429): Resource has been exhausted";
        let parsed = parse_error_info(raw).expect("Failed to parse prefixed error");
        assert_eq!(parsed.title, "Gemini API Error");
        assert_eq!(parsed.summary, "The AI provider temporarily ran out of available quota.");
    }

    #[test]
    fn test_normal_message_returns_none() {
        let text = "Here is the response to your question about Rust programming.";
        assert!(parse_error_info(text).is_none());
    }
}
