//! Function Command & Intent Launcher (docs/02-BRANDING.md).
//!
//! Recreates Function's core brand identity:
//! - Quiet, confident, technical minimalism (Carbon #0A0A0A, Graphite #141414, Ash #242424)
//! - Geometric white logo emerging from darkness
//! - Natural intent-to-action execution bar
//! - Dynamic plugins: Shell (>), Math/Calculator (=), Web Intelligence (?), Network (ip), Settings
//! - Zero decorative emojis or cartoon graphics; restrained technical vector icons

use crate::components::calculator::{evaluate_calculation, format_result};
use crate::components::launcher_icons::{
    app_icon, calculator_icon, file_icon, folder_icon, network_icon, settings_icon, terminal_icon,
    web_icon,
};
use crate::components::render_logo;
use crate::theme::Theme;
use function_platform::{search_apps_and_files, SearchItemKind};
use gpui::prelude::*;
use gpui::{div, px, rgba, IntoElement, Rgba};

fn activation_shortcut_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘ ⌘"
    }
    #[cfg(target_os = "windows")]
    {
        "Ctrl+Space"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Ctrl+Space"
    }
}

fn file_search_open_shortcut_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "⌘+Enter"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Ctrl+Enter"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherIconType {
    Function,
    Terminal,
    Calculator,
    Web,
    Network,
    Settings,
    App,
    Folder,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherAction {
    FillPrefix(String),
    ExecuteShell(String),
    CopyResult(String),
    OpenUrl(String),
    OpenSettings,
    NewConversation,
    OpenConversations,
    ToggleTheme,
    ToggleVoice,
    ToggleSound,
    ClearInput,
    OpenMemory,
    OpenAbout,
    SetModel(String),
    RunTask(String),
    OpenPath(String),
}

#[derive(Debug, Clone)]
pub struct LauncherItem {
    pub keyword: String,
    pub description: String,
    pub shortcut: String,
    pub icon_type: LauncherIconType,
    pub action: LauncherAction,
}

/// Get formatted local time string.
pub fn get_current_time_string() -> String {
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        extern "system" {
            fn GetLocalTime(lp_system_time: *mut SystemTime);
        }
        let mut st = SystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        unsafe {
            GetLocalTime(&mut st);
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.year, st.month, st.day, st.hour, st.minute, st.second
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        "Current System Time".to_string()
    }
}

/// Get the current local IP address string.
pub fn get_local_ip_string() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("8.8.8.8:80").map(|_| s.local_addr()))
        .map(|addr| {
            addr.map(|a| a.ip().to_string())
                .unwrap_or_else(|_| "127.0.0.1".into())
        })
        .unwrap_or_else(|_| "127.0.0.1".into())
}

/// Build list of assistant items based on user search query.
pub fn get_launcher_items(query: &str) -> Vec<LauncherItem> {
    let trimmed = query.trim();
    let lower = trimmed.to_lowercase();

    // 0. Slash commands plugin (starts with '/')
    if lower.starts_with('/') {
        let all_slash_commands = vec![
            LauncherItem {
                keyword: "/memory".to_string(),
                description: "Inspect persistent file memory and user preferences store".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::OpenMemory,
            },
            LauncherItem {
                keyword: "/conversations".to_string(),
                description: "Open Conversations page and browse chat history".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::OpenConversations,
            },
            LauncherItem {
                keyword: "/settings".to_string(),
                description: "Configure AI Provider, API Key, Model & Preferences".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Settings,
                action: LauncherAction::OpenSettings,
            },
            LauncherItem {
                keyword: "/themes".to_string(),
                description: "Cycle application visual themes and color palettes".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::ToggleTheme,
            },
            LauncherItem {
                keyword: "/help".to_string(),
                description: "View assistance guide and Function assistant capabilities".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::RunTask("Provide a clear overview of Function slash commands and desktop capabilities.".to_string()),
            },
            LauncherItem {
                keyword: "/clear".to_string(),
                description: "Clear current prompt input and reset view".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::ClearInput,
            },
            LauncherItem {
                keyword: "/new".to_string(),
                description: "Start a fresh conversation and clear prompt history".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::NewConversation,
            },
            LauncherItem {
                keyword: "/model".to_string(),
                description: "View or select active AI provider model".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Settings,
                action: LauncherAction::OpenSettings,
            },
            LauncherItem {
                keyword: "/voice".to_string(),
                description: "Toggle voice push-to-talk recording mode".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::ToggleVoice,
            },
            LauncherItem {
                keyword: "/sound".to_string(),
                description: "Toggle UI audio feedback and sound effects".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::ToggleSound,
            },
            LauncherItem {
                keyword: "/about".to_string(),
                description: "Display Function build information and system details".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::OpenAbout,
            },
        ];

        let typed_sub = lower.strip_prefix('/').unwrap_or("").trim();
        let filtered: Vec<LauncherItem> = all_slash_commands
            .into_iter()
            .filter(|item| {
                if typed_sub.is_empty() {
                    true
                } else {
                    let cmd_name = item.keyword.strip_prefix('/').unwrap_or(&item.keyword).to_lowercase();
                    cmd_name.starts_with(typed_sub) || item.keyword.to_lowercase().starts_with(&lower)
                }
            })
            .collect();

        if !filtered.is_empty() {
            return filtered;
        }
    }

    // 1. Math calculation plugin
    if let Some(result) = evaluate_calculation(trimmed) {
        let formatted = format_result(result);
        return vec![
            LauncherItem {
                keyword: format!("= {}", formatted),
                description: "Calculation Result — Press Enter to copy to clipboard".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Calculator,
                action: LauncherAction::CopyResult(formatted),
            },
            LauncherItem {
                keyword: "Execute as Task".to_string(),
                description: format!("Run computer task: \"{}\"", trimmed),
                shortcut: "Alt+2".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::RunTask(trimmed.to_string()),
            },
        ];
    }

    // 2. Shell plugin (starts with '>')
    if let Some(cmd) = trimmed.strip_prefix('>') {
        let cmd = cmd.trim();
        return vec![LauncherItem {
            keyword: if cmd.is_empty() {
                "> [type command]".to_string()
            } else {
                format!("> {}", cmd)
            },
            description: if cmd.is_empty() {
                "Execute command line instruction in native PowerShell".to_string()
            } else {
                format!("Run \"{}\" in native PowerShell", cmd)
            },
            shortcut: "Enter".to_string(),
            icon_type: LauncherIconType::Terminal,
            action: LauncherAction::ExecuteShell(cmd.to_string()),
        }];
    }

    // 3. Web & Documentation query ('? ' or 'web ')
    if lower.starts_with("? ") || lower.starts_with("web ") || lower.starts_with("lucky ") {
        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let search_term = if parts.len() > 1 { parts[1].trim() } else { "" };
        let url = format!("https://www.google.com/search?q={}", search_term);
        return vec![LauncherItem {
            keyword: format!("Search Web: {}", search_term),
            description: "Query online documentation and live web resources".to_string(),
            shortcut: "Enter".to_string(),
            icon_type: LauncherIconType::Web,
            action: LauncherAction::OpenUrl(url),
        }];
    }

    // 4. IP address & Network inspection
    if lower == "ip" || lower == "ipadr" || lower == "network" {
        let ip = get_local_ip_string();
        return vec![LauncherItem {
            keyword: format!("Local IPv4: {}", ip),
            description: "Active Network Interface — Press Enter to copy to clipboard".to_string(),
            shortcut: "Enter".to_string(),
            icon_type: LauncherIconType::Network,
            action: LauncherAction::CopyResult(ip),
        }];
    }

    // 5. Settings & Customization query
    if lower == "settings"
        || lower == "configure"
        || lower == "preferences"
        || lower == "config"
        || lower == "theme"
        || lower == "api"
        || lower == "key"
    {
        return vec![LauncherItem {
            keyword: "Function Preferences & Configuration".to_string(),
            description: "Configure API Key, Model, Sound & Appearance (Local Command)".to_string(),
            shortcut: "Enter".to_string(),
            icon_type: LauncherIconType::Settings,
            action: LauncherAction::OpenSettings,
        }];
    }

    // 6. Conversation & Chat History query
    if lower == "conversation"
        || lower == "conversations"
        || lower == "history"
        || lower == "chats"
        || lower == "chat"
        || lower == "new chat"
        || lower.starts_with("conversation ")
    {
        return vec![
            LauncherItem {
                keyword: "Start New Conversation".to_string(),
                description: "Save active chat and start fresh with a clean prompt".to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::NewConversation,
            },
            LauncherItem {
                keyword: "Browse Past Conversations".to_string(),
                description: "View and restore previous conversations from chat history"
                    .to_string(),
                shortcut: "Alt+2".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::OpenConversations,
            },
        ];
    }

    // 7. Default assistant actions when input is empty
    if trimmed.is_empty() {
        return vec![
            LauncherItem {
                keyword: "Execute Computer Task".to_string(),
                description:
                    "Type an intent to orchestrate computer tools, analyze screen, or automate apps"
                        .to_string(),
                shortcut: "Enter".to_string(),
                icon_type: LauncherIconType::Function,
                action: LauncherAction::RunTask(String::new()),
            },
            LauncherItem {
                keyword: "Run Shell Command (>)".to_string(),
                description: "Execute native PowerShell or command line instruction directly"
                    .to_string(),
                shortcut: "Alt+1".to_string(),
                icon_type: LauncherIconType::Terminal,
                action: LauncherAction::FillPrefix("> ".to_string()),
            },
            LauncherItem {
                keyword: "Evaluate Expression (=)".to_string(),
                description:
                    "Compute arithmetic formulas, unit conversions, or logical expressions"
                        .to_string(),
                shortcut: "Alt+2".to_string(),
                icon_type: LauncherIconType::Calculator,
                action: LauncherAction::FillPrefix("= ".to_string()),
            },
            LauncherItem {
                keyword: "Search Live Web (?)".to_string(),
                description: "Query online technical documentation and web references".to_string(),
                shortcut: "Alt+3".to_string(),
                icon_type: LauncherIconType::Web,
                action: LauncherAction::FillPrefix("? ".to_string()),
            },
            LauncherItem {
                keyword: "Function Preferences".to_string(),
                description: "Customize theme, accent, AI models, API keys & audio feedback"
                    .to_string(),
                shortcut: "Ctrl+,".to_string(),
                icon_type: LauncherIconType::Settings,
                action: LauncherAction::OpenSettings,
            },
        ];
    }

    // 7. Dynamic freeform task execution & integrated app/folder/file search
    let search_results = search_apps_and_files(trimmed);
    let mut items = Vec::new();

    for res in search_results {
        let (icon, desc_prefix) = match res.kind {
            SearchItemKind::Application => (LauncherIconType::App, "Application"),
            SearchItemKind::Folder => (LauncherIconType::Folder, "Folder"),
            SearchItemKind::File => (LauncherIconType::File, "File"),
        };
        items.push(LauncherItem {
            keyword: res.name,
            description: format!(
                "{} • {} to Open • Enter for AI",
                desc_prefix,
                file_search_open_shortcut_label()
            ),
            shortcut: file_search_open_shortcut_label().to_string(),
            icon_type: icon,
            action: LauncherAction::OpenPath(res.path),
        });
    }

    items.push(LauncherItem {
        keyword: format!("Ask AI: \"{}\"", trimmed),
        description: "Send prompt directly to Function AI agent".to_string(),
        shortcut: "Enter".to_string(),
        icon_type: LauncherIconType::Function,
        action: LauncherAction::RunTask(trimmed.to_string()),
    });

    items.push(LauncherItem {
        keyword: format!("Search Web: \"{}\"", trimmed),
        description: "Query live web search in default browser".to_string(),
        shortcut: "Alt+2".to_string(),
        icon_type: LauncherIconType::Web,
        action: LauncherAction::OpenUrl(format!("https://www.google.com/search?q={}", trimmed)),
    });

    items
}

fn render_item_icon(icon_type: LauncherIconType) -> impl IntoElement {
    match icon_type {
        LauncherIconType::Function => render_logo(18.0).into_any_element(),
        LauncherIconType::Terminal => terminal_icon(18.0).into_any_element(),
        LauncherIconType::Calculator => calculator_icon(18.0).into_any_element(),
        LauncherIconType::Web => web_icon(18.0).into_any_element(),
        LauncherIconType::Network => network_icon(18.0).into_any_element(),
        LauncherIconType::Settings => settings_icon(18.0).into_any_element(),
        LauncherIconType::App => app_icon(18.0).into_any_element(),
        LauncherIconType::Folder => folder_icon(18.0).into_any_element(),
        LauncherIconType::File => file_icon(18.0).into_any_element(),
    }
}

pub fn render_spotlight_bar(
    query: &str,
    theme: &Theme,
    selected_index: usize,
    cursor_visible: bool,
    _current_time: &str,
) -> impl IntoElement {
    let has_query = !query.is_empty();
    let items = get_launcher_items(query);
    let selected_index = if items.is_empty() {
        0
    } else {
        selected_index.min(items.len().saturating_sub(1))
    };

    let bg_surface = Rgba {
        a: 0.95,
        ..theme.surface_elevated
    };
    let divider_color = theme.surface_input;
    let selected_bg = theme.surface_active;
    let hover_bg = theme.surface_input;
    let text_muted_val = theme.text_muted;
    let text_light = theme.text_primary;
    let badge_bg = theme.surface_input;
    let badge_border = theme.border_subtle;
    let accent_color = theme.accent_primary;

    div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .bg(bg_surface)
        .rounded_2xl()
        .shadow_xl()
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(52.0))
                .px_4()
                .border_b_1()
                .border_color(divider_color)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_1()
                        .gap_3()
                        .child(render_logo(18.0))
                        .child(if has_query {
                            div()
                                .flex()
                                .items_center()
                                .flex_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(text_light)
                                        .child(query.to_string()),
                                )
                                .child(div().w(px(2.0)).h(px(16.0)).bg(if cursor_visible {
                                    accent_color
                                } else {
                                    rgba(0x00000000)
                                }))
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .flex_1()
                                .child(div().w(px(2.0)).h(px(16.0)).bg(if cursor_visible {
                                    accent_color
                                } else {
                                    rgba(0x00000000)
                                }))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(text_muted_val)
                                        .child("Ask Function or type a command..."),
                                )
                        }),
                )
                .child(
                    div().flex().items_center().gap_2().child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(badge_bg)
                            .border_1()
                            .border_color(badge_border)
                            .text_xs()
                            .text_color(text_muted_val)
                            .child(activation_shortcut_label()),
                    ),
                ),
        )
        .child(
            div().flex().flex_col().flex_1().p_2().gap_1().children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let is_selected = idx == selected_index;

                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(54.0))
                            .rounded_md()
                            .bg(if is_selected {
                                selected_bg
                            } else {
                                rgba(0x00000000)
                            })
                            .hover(move |s| {
                                if is_selected {
                                    s.bg(selected_bg)
                                } else {
                                    s.bg(hover_bg)
                                }
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h_full()
                                    .child(
                                        div()
                                            .w(px(3.0))
                                            .h(px(24.0))
                                            .rounded_full()
                                            .bg(if is_selected {
                                                accent_color
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .ml_1()
                                            .mr_2p5(),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(26.0))
                                            .h(px(26.0))
                                            .mr_3()
                                            .child(render_item_icon(item.icon_type)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .justify_center()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .text_color(if is_selected {
                                                        text_light
                                                    } else {
                                                        theme.text_secondary
                                                    })
                                                    .child(item.keyword),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(text_muted_val)
                                                    .child(item.description),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .mr_3()
                                    .rounded_sm()
                                    .bg(badge_bg)
                                    .border_1()
                                    .border_color(badge_border)
                                    .text_xs()
                                    .text_color(if is_selected {
                                        text_light
                                    } else {
                                        text_muted_val
                                    })
                                    .child(item.shortcut),
                            )
                    })
                    .collect::<Vec<_>>(),
            ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_commands_list_and_filtering() {
        let items_all = get_launcher_items("/");
        assert!(items_all.len() >= 11);
        let keywords: Vec<String> = items_all.iter().map(|i| i.keyword.clone()).collect();
        assert!(keywords.contains(&"/memory".to_string()));
        assert!(keywords.contains(&"/conversations".to_string()));
        assert!(keywords.contains(&"/settings".to_string()));
        assert!(keywords.contains(&"/themes".to_string()));
        assert!(keywords.contains(&"/help".to_string()));
        assert!(keywords.contains(&"/clear".to_string()));
        assert!(keywords.contains(&"/new".to_string()));
        assert!(keywords.contains(&"/model".to_string()));
        assert!(keywords.contains(&"/voice".to_string()));
        assert!(keywords.contains(&"/sound".to_string()));
        assert!(keywords.contains(&"/about".to_string()));

        let items_se = get_launcher_items("/se");
        assert_eq!(items_se.len(), 1);
        assert_eq!(items_se[0].keyword, "/settings");

        let items_mem = get_launcher_items("/mem");
        assert_eq!(items_mem.len(), 1);
        assert_eq!(items_mem[0].keyword, "/memory");
    }
}
