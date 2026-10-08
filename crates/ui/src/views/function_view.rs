//! Main Function view supporting Compact, Spotlight, Expanded, and Settings modes.
//!
//! Recreates Flow Launcher features:
//! - Curved borderless window with no title bar (16px corner radius)
//! - Audio feedback on navigation, selection, execution, and shortcuts
//! - Real-time math calculator plugin (`evaluate_calculation`)
//! - Shell execution plugin (`>`)
//! - Web search plugin (`lucky`, `b`, `g`)
//! - Local IP address inspection and copy (`ipadr`)
//! - In-app AI provider and API key configuration (`Ctrl+,` or `settings`)

use crate::actions::{
    CancelTask, ClearInput, CloseFunction, SubmitRequest, ToggleExpanded, ToggleSpotlight,
    ToggleTheme, ToggleVoice,
};
use crate::components::function_motif::{render_function_motif, MotifState};
use crate::components::launcher_icons::{
    app_icon, calculator_icon, file_icon, folder_icon, network_icon, settings_icon, terminal_icon,
    web_icon,
};
use crate::components::spotlight_bar::{
    get_current_time_string, get_launcher_items, LauncherAction, LauncherIconType,
};
use crate::components::{
    render_brand_mark_with_mode, render_inline_text, render_logo, render_markdown, ActivityEntry,
    ActivityStatus,
};
use crate::conversation::{ChatEntry, ConversationStore, SavedConversation};
use crate::local_commands::{resolve_local_command, resolve_shell_command, LocalCommand};
use crate::theme::Theme;
use crate::views::{render_conversation_view, render_settings_view};
use function_agent::AgentState;
use function_config::AppConfig;
use function_platform::{
    copy_to_clipboard, open_url, play_sound, read_clipboard_image, SoundEffect,
};
use function_providers::ChatMessage;
use gpui::prelude::*;
use gpui::{
    div, px, rgba, AsyncApp, Context, FocusHandle, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, Render, Rgba, ScrollHandle, Size, Task, Timer, WeakEntity, Window,
};
use std::time::Duration;

/// Checks if a key string represents a named control key rather than text to type.
fn is_named_control_key(k: &str) -> bool {
    matches!(
        k,
        "up" | "down"
            | "left"
            | "right"
            | "enter"
            | "return"
            | "tab"
            | "escape"
            | "esc"
            | "backspace"
            | "delete"
            | "home"
            | "end"
            | "pageup"
            | "pagedown"
            | "shift"
            | "control"
            | "ctrl"
            | "alt"
            | "option"
            | "command"
            | "cmd"
            | "super"
            | "capslock"
            | "insert"
            | "printscreen"
            | "scrolllock"
            | "pause"
            | "menu"
            | "f1"
            | "f2"
            | "f3"
            | "f4"
            | "f5"
            | "f6"
            | "f7"
            | "f8"
            | "f9"
            | "f10"
            | "f11"
            | "f12"
    )
}

/// Maps a character to its shifted counterpart on standard keyboards.
fn shift_char(c: char) -> char {
    match c {
        'a'..='z' => c.to_ascii_uppercase(),
        '1' => '!',
        '2' => '@',
        '3' => '#',
        '4' => '$',
        '5' => '%',
        '6' => '^',
        '7' => '&',
        '8' => '*',
        '9' => '(',
        '0' => ')',
        '`' => '~',
        '-' => '_',
        '=' => '+',
        '[' => '{',
        ']' => '}',
        '\\' => '|',
        ';' => ':',
        '\'' => '"',
        ',' => '<',
        '.' => '>',
        '/' => '?',
        other => other,
    }
}

/// Display mode for the function window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionMode {
    /// Intelligent floating command layer
    Command,
    /// Settings view
    Settings,
    /// Conversations and chat history view
    Conversations,
    /// Compatibility aliases
    Compact,
    Spotlight,
    Expanded,
}

// Aliases for backwards compatibility
pub type AssistantMode = FunctionMode;

pub struct FunctionView {
    pub mode: FunctionMode,
    pub is_visible: bool,
    pub is_active_window: bool,
    pub animation_tick: usize,
    pub mic_configured: bool,
    pub mic_available: bool,
    pub input_buffer: String,
    pub active_task: Option<String>,
    pub state: AgentState,
    pub activities: Vec<ActivityEntry>,
    pub latest_result: Option<String>,
    pub listening: bool,
    pub audio_capture: Option<std::sync::Arc<dyn function_platform::AudioCapture>>,
    pub stt_provider: Option<std::sync::Arc<dyn function_providers::SpeechToTextProvider>>,
    pub tts_provider: Option<std::sync::Arc<dyn function_providers::TextToSpeechProvider>>,
    pub is_transcribing: bool,
    pub voice_error: Option<String>,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub agent: Option<std::sync::Arc<function_agent::Agent>>,
    pub cursor_visible: bool,
    pub selected_index: usize,
    pub is_text_selected: bool,
    pub current_time: String,
    pub _cursor_task: Option<Task<()>>,
    pub _agent_sub_task: Option<Task<()>>,
    pub _activation_sub: Option<gpui::Subscription>,
    // Configuration & Settings State
    pub config: AppConfig,
    pub settings_api_key: String,
    pub settings_model: String,
    pub settings_base_url: String,
    pub settings_provider: String,
    pub settings_stt_api_key: String,
    pub settings_stt_model: String,
    pub settings_stt_base_url: String,
    pub settings_tts_api_key: String,
    pub settings_tts_model: String,
    pub settings_tts_base_url: String,
    pub settings_tts_voice: String,
    pub settings_advanced_expanded: bool,
    pub settings_sound_enabled: bool,
    pub settings_show_key: bool,
    pub settings_focused_field: usize,
    pub settings_status_message: Option<String>,
    /// Visible chat log (User + Assistant turns, for rendering)
    pub chat_display: Vec<ChatEntry>,
    /// API-level message history passed to `execute_with_history` (persists across turns)
    pub chat_history_api: Vec<ChatMessage>,
    /// Persistent conversation store for chat history
    pub conversation_store: ConversationStore,
    pub active_conversation_id: Option<String>,
    pub conversation_selected_index: usize,
    pub conversation_status_message: Option<String>,
    pub chat_scroll_handle: ScrollHandle,
    pub settings_scroll_handle: ScrollHandle,
    pub cursor_offset: usize,
    pub user_scrolled_up: bool,
    pub expanded_errors: std::collections::HashSet<usize>,
    pub activation_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub activation_settling: bool,
    pub last_toggle_time: Option<std::time::Instant>,
    pub is_playing_audio: bool,
    pub audio_session_id: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

pub type AssistantView = FunctionView;

impl FunctionView {
    pub fn new(cx: &mut Context<Self>, mic_configured: bool, mic_available: bool) -> Self {
        let focus_handle = cx.focus_handle();
        let config = AppConfig::load();
        let settings_api_key = config.ai_provider.api_key.clone().unwrap_or_default();
        let settings_model = config.ai_provider.model.clone();
        let settings_base_url = config.ai_provider.base_url.clone();
        let settings_provider = config.ai_provider.provider_name.clone();
        let settings_stt_api_key = config.speech.api_key.clone().unwrap_or_default();
        let settings_stt_model = config.speech.model.clone();
        let settings_stt_base_url = config.speech.base_url.clone();
        let settings_tts_api_key = config.tts.api_key.clone().unwrap_or_default();
        let settings_tts_model = config.tts.model.clone();
        let settings_tts_base_url = config.tts.base_url.clone();
        let settings_tts_voice = config.tts.voice.clone();
        let settings_sound_enabled = config.sound_enabled;

        // Spawn a background timer loop for subtle motif animation ticks (~150ms) and cursor blinking
        let cursor_task = cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                loop {
                    Timer::after(Duration::from_millis(150)).await;
                    let res = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.animation_tick = view.animation_tick.wrapping_add(1);
                            if view.animation_tick % 4 == 0 {
                                view.cursor_visible = !view.cursor_visible;
                            }
                            view.current_time = get_current_time_string();
                            cx.notify();
                        })
                    });
                    if res.is_err() {
                        break;
                    }
                }
            }
        });

        let start_hidden =
            config.start_hidden && !std::env::args().any(|arg| arg == "--show" || arg == "-s");

        Self {
            mode: FunctionMode::Command,
            is_visible: !start_hidden,
            is_active_window: !start_hidden,
            animation_tick: 0,
            mic_configured,
            mic_available,
            input_buffer: String::new(),
            cursor_offset: 0,
            active_task: None,
            state: AgentState::Idle,
            activities: Vec::new(),
            latest_result: None,
            listening: false,
            audio_capture: None,
            stt_provider: None,
            tts_provider: None,
            is_transcribing: false,
            voice_error: None,
            theme: Theme::from_config(&config),
            focus_handle,
            agent: None,
            cursor_visible: true,
            selected_index: 0,
            is_text_selected: false,
            current_time: get_current_time_string(),
            _cursor_task: Some(cursor_task),
            _agent_sub_task: None,
            _activation_sub: None,
            config,
            settings_api_key,
            settings_model,
            settings_base_url,
            settings_provider,
            settings_stt_api_key,
            settings_stt_model,
            settings_stt_base_url,
            settings_tts_api_key,
            settings_tts_model,
            settings_tts_base_url,
            settings_tts_voice,
            settings_advanced_expanded: false,
            settings_sound_enabled,
            settings_show_key: false,
            settings_focused_field: 0,
            settings_status_message: None,
            chat_display: Vec::new(),
            chat_history_api: Vec::new(),
            conversation_store: ConversationStore::load(),
            active_conversation_id: None,
            conversation_selected_index: 0,
            conversation_status_message: None,
            chat_scroll_handle: ScrollHandle::new(),
            settings_scroll_handle: ScrollHandle::new(),
            user_scrolled_up: false,
            expanded_errors: std::collections::HashSet::new(),
            activation_generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            activation_settling: false,
            last_toggle_time: None,
            is_playing_audio: false,
            audio_session_id: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn observe_activation(mut self, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut activated_once = false;
        let deactivation_pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let deactivation_pending_sub = deactivation_pending.clone();
        let activation_generation = self.activation_generation.clone();
        let _sub = cx.observe_window_activation(window, move |this, window, cx| {
            let is_active = window.is_window_active();
            this.is_active_window = is_active;
            if is_active {
                activated_once = true;
                deactivation_pending_sub.store(false, std::sync::atomic::Ordering::SeqCst);
            } else if activated_once
                && this.is_visible
                && !deactivation_pending_sub.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                let pending_flag = deactivation_pending_sub.clone();
                let transition_generation = activation_generation.load(std::sync::atomic::Ordering::SeqCst);
                let generation_guard = activation_generation.clone();
                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let cx = cx.clone();
                    async move {
                        Timer::after(Duration::from_millis(
                            function_platform::MACOS_ACTIVATION_SETTLE_MS + 80,
                        ))
                        .await;
                        let _ = cx.update(|cx| {
                            let _ = this.update(cx, |this, cx| {
                                let same_transition = generation_guard.load(
                                    std::sync::atomic::Ordering::SeqCst,
                                ) == transition_generation;
                                if same_transition
                                    && function_platform::should_dismiss_after_deactivation(
                                        this.is_visible,
                                        activated_once,
                                        this.activation_settling,
                                    )
                                    && !this.is_active_window
                                {
                                    if function_platform::macos_menu_is_active() {
                                        let retry_generation = transition_generation;
                                        let retry_guard = generation_guard.clone();
                                        let retry_pending = pending_flag.clone();
                                        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                                            let cx = cx.clone();
                                            async move {
                                                for _ in 0..20 {
                                                    Timer::after(Duration::from_millis(100)).await;
                                                    let done = cx
                                                        .update(|cx| {
                                                            this.update(cx, |this, cx| {
                                                                if retry_guard.load(
                                                                    std::sync::atomic::Ordering::SeqCst,
                                                                ) != retry_generation
                                                                    || !this.is_visible
                                                                    || this.is_active_window
                                                                {
                                                                    return true;
                                                                }
                                                                if !function_platform::macos_menu_is_active() {
                                                                    tracing::info!(
                                                                        "Window confirmed inactive after menu transition: dismissing"
                                                                    );
                                                                    this.dismiss_after_external_deactivation(cx);
                                                                    true
                                                                } else {
                                                                    false
                                                                }
                                                            })
                                                            .unwrap_or(true)
                                                        })
                                                        .unwrap_or(true);
                                                    if done {
                                                        break;
                                                    }
                                                }
                                                retry_pending.store(
                                                    false,
                                                    std::sync::atomic::Ordering::SeqCst,
                                                );
                                            }
                                        })
                                        .detach();
                                    } else {
                                        tracing::info!("Window confirmed inactive after runloop turn: dismissing");
                                        this.dismiss_after_external_deactivation(cx);
                                        pending_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                                    }
                                } else {
                                    pending_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                                }
                            });
                        });
                    }
                })
                .detach();
            }
        });
        self._activation_sub = Some(_sub);
        self
    }

    pub fn with_agent(
        mut self,
        agent: std::sync::Arc<function_agent::Agent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut rx = agent.subscribe_state();
        let sub_task = cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                loop {
                    // A slow UI frame can let the agent outrun the channel
                    // (e.g. a burst of streamed tokens). Lagging must never end
                    // this loop, or terminal states are never applied and the
                    // view stays in "Thinking..." forever.
                    let state = match rx.recv().await {
                        Ok(state) => state,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                target: "function_ui",
                                skipped,
                                "UI lagged behind agent state updates; resuming from latest"
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!(
                                target: "function_ui",
                                "Agent state channel closed; stopping state subscription"
                            );
                            break;
                        }
                    };
                    let state_clone = state.clone();
                    let res = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            match &state_clone {
                                AgentState::Processing { thought_summary } => {
                                    view.state = state_clone.clone();
                                    for activity in &mut view.activities {
                                        if activity.status == ActivityStatus::Running {
                                            activity.status = ActivityStatus::Done;
                                        }
                                    }
                                    if let Some(thought) = thought_summary {
                                        view.activities.push(ActivityEntry {
                                            step: view.activities.len() + 1,
                                            description: thought.clone(),
                                            status: ActivityStatus::Running,
                                        });
                                    }
                                }
                                AgentState::Acting { action_description } => {
                                    view.state = state_clone.clone();
                                    for activity in &mut view.activities {
                                        if activity.status == ActivityStatus::Running {
                                            activity.status = ActivityStatus::Done;
                                        }
                                    }
                                    view.activities.push(ActivityEntry {
                                        step: view.activities.len() + 1,
                                        description: action_description.clone(),
                                        status: ActivityStatus::Running,
                                    });
                                }
                                AgentState::Streaming {
                                    chunk: _,
                                    accumulated,
                                } => {
                                    view.state = state_clone.clone();
                                    view.latest_result = Some(accumulated.clone());
                                    if let Some(last) = view.chat_display.last_mut() {
                                        if !last.is_user {
                                            last.text = accumulated.clone();
                                        } else {
                                            view.chat_display.push(ChatEntry {
                                                is_user: false,
                                                text: accumulated.clone(),
                                            });
                                        }
                                    } else {
                                        view.chat_display.push(ChatEntry {
                                            is_user: false,
                                            text: accumulated.clone(),
                                        });
                                    }
                                    if !view.user_scrolled_up {
                                        view.chat_scroll_handle.scroll_to_item(
                                            view.chat_display.len().saturating_sub(1),
                                        );
                                    }
                                }
                                AgentState::Completed {
                                    summary,
                                    new_history,
                                } => {
                                    view.state = state_clone.clone();
                                    view.latest_result = Some(summary.clone());
                                    if let Some(last) = view.chat_display.last_mut() {
                                        if !last.is_user {
                                            last.text = summary.clone();
                                        } else if !summary.is_empty() {
                                            view.chat_display.push(ChatEntry {
                                                is_user: false,
                                                text: summary.clone(),
                                            });
                                        }
                                    } else if !summary.is_empty() {
                                        view.chat_display.push(ChatEntry {
                                            is_user: false,
                                            text: summary.clone(),
                                        });
                                    }
                                    view.chat_history_api = new_history.clone();
                                    view.active_task = None;
                                    tracing::info!(
                                        target: "function_ui",
                                        reply_chars = summary.chars().count(),
                                        "response rendering completed"
                                    );
                                    view.activities.clear();
                                    view.play_sound_feedback(SoundEffect::Success);
                                    if view.config.tts.enabled && !summary.is_empty() {
                                        let current_audio_session = view
                                            .audio_session_id
                                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                                            + 1;
                                        let session_guard = view.audio_session_id.clone();
                                        view.is_playing_audio = true;

                                        let text_to_speak = summary.clone();
                                        let tts_opt = view.tts_provider.clone();
                                        let output_format = view.config.tts.output_format.clone();

                                        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                                            let cx = cx.clone();
                                            async move {
                                                if let Some(tts) = tts_opt {
                                                    let synth_res = tts.synthesize_speech(&text_to_speak).await;
                                                    if session_guard.load(std::sync::atomic::Ordering::SeqCst)
                                                        == current_audio_session
                                                    {
                                                        match synth_res {
                                                            Ok(audio) => {
                                                                let _ = function_platform::play_audio_bytes(
                                                                    &audio,
                                                                    &output_format,
                                                                );
                                                            }
                                                            Err(error) => {
                                                                tracing::error!("TTS request failed: {}", error);
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    function_platform::speak_text(&text_to_speak);
                                                }

                                                while function_platform::is_audio_playing() {
                                                    Timer::after(Duration::from_millis(100)).await;
                                                    if session_guard.load(std::sync::atomic::Ordering::SeqCst)
                                                        != current_audio_session
                                                    {
                                                        break;
                                                    }
                                                }

                                                let _ = cx.update(|cx| {
                                                    let _ = this.update(cx, |view, cx| {
                                                        if view
                                                            .audio_session_id
                                                            .load(std::sync::atomic::Ordering::SeqCst)
                                                            == current_audio_session
                                                        {
                                                            view.is_playing_audio = false;
                                                            cx.notify();
                                                        }
                                                    });
                                                });
                                            }
                                        })
                                        .detach();
                                    }
                                    view.save_current_conversation();
                                    if !view.user_scrolled_up {
                                        view.chat_scroll_handle.scroll_to_item(
                                            view.chat_display.len().saturating_sub(1),
                                        );
                                    }
                                }
                                AgentState::Error {
                                    message,
                                    new_history,
                                } => {
                                    view.state = state_clone.clone();
                                    tracing::warn!(
                                        target: "function_ui",
                                        error = %message,
                                        "rendering agent error state"
                                    );
                                    let err_text = format!("Error: {}", message);
                                    view.latest_result = Some(err_text.clone());
                                    view.chat_display.push(ChatEntry {
                                        is_user: false,
                                        text: err_text,
                                    });
                                    if let Some(hist) = new_history {
                                        view.chat_history_api = hist.clone();
                                        view.save_current_conversation();
                                    }
                                    view.active_task = None;
                                    view.activities.clear();
                                    view.play_sound_feedback(SoundEffect::Error);
                                }
                                AgentState::WaitingForConfirmation { action, .. } => {
                                    view.state = state_clone.clone();
                                    view.activities.push(ActivityEntry {
                                        step: view.activities.len() + 1,
                                        description: format!(
                                            "Confirmation required for {}",
                                            action
                                        ),
                                        status: ActivityStatus::Running,
                                    });
                                    view.play_sound_feedback(SoundEffect::Error);
                                }
                                _ => {
                                    view.state = state_clone.clone();
                                }
                            }
                            cx.notify();
                        })
                    });
                    if res.is_err() {
                        break;
                    }
                }
            }
        });
        self._agent_sub_task = Some(sub_task);
        agent.set_request_delay(self.config.request_delay.as_millis());
        agent.set_input_token_limit(self.config.input_token_limit);
        self.agent = Some(agent);
        self
    }

    pub fn with_config(mut self, config: AppConfig) -> Self {
        self.settings_api_key = config.ai_provider.api_key.clone().unwrap_or_default();
        self.settings_model = config.ai_provider.model.clone();
        self.settings_base_url = config.ai_provider.base_url.clone();
        self.settings_provider = config.ai_provider.provider_name.clone();
        self.settings_stt_api_key = config.speech.api_key.clone().unwrap_or_default();
        self.settings_stt_model = config.speech.model.clone();
        self.settings_stt_base_url = config.speech.base_url.clone();
        self.settings_tts_api_key = config.tts.api_key.clone().unwrap_or_default();
        self.settings_tts_model = config.tts.model.clone();
        self.settings_tts_base_url = config.tts.base_url.clone();
        self.settings_tts_voice = config.tts.voice.clone();
        self.settings_sound_enabled = config.sound_enabled;
        self.theme = Theme::from_config(&config);
        if let Some(ref agent) = self.agent {
            agent.set_request_delay(config.request_delay.as_millis());
            agent.set_input_token_limit(config.input_token_limit);
        }
        self.config = config;
        self
    }

    pub fn with_audio_capture(
        mut self,
        capture: std::sync::Arc<dyn function_platform::AudioCapture>,
    ) -> Self {
        self.audio_capture = Some(capture);
        self
    }

    pub fn with_stt_provider(
        mut self,
        stt: std::sync::Arc<dyn function_providers::SpeechToTextProvider>,
    ) -> Self {
        self.stt_provider = Some(stt);
        self
    }

    pub fn with_tts_provider(
        mut self,
        tts: Option<std::sync::Arc<dyn function_providers::TextToSpeechProvider>>,
    ) -> Self {
        self.tts_provider = tts;
        self
    }

    pub fn play_sound_feedback(&self, effect: SoundEffect) {
        if self.config.sound_enabled {
            play_sound(effect);
        }
    }

    pub fn stop_audio(&mut self, cx: &mut Context<Self>) {
        self.audio_session_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        function_platform::stop_audio_playback();
        if self.is_playing_audio {
            self.is_playing_audio = false;
            cx.notify();
        }
    }

    pub fn stop_response(&mut self, cx: &mut Context<Self>) {
        if let Some(ref agent) = self.agent {
            agent.cancel();
        }
        self.stop_audio(cx);
        self.active_task = None;
        self.activities.clear();
        self.state = AgentState::Idle;
        self.play_sound_feedback(SoundEffect::Select);
        cx.notify();
    }

    pub fn set_state(&mut self, state: AgentState, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }

    pub fn target_window_size(&self) -> Size<gpui::Pixels> {
        if self.mode == FunctionMode::Settings {
            return Size {
                width: px(640.0),
                height: px(460.0),
            };
        }

        if self.mode == FunctionMode::Conversations {
            return Size {
                width: px(640.0),
                height: px(460.0),
            };
        }

        if !self.chat_display.is_empty() {
            let total_lines: usize = self
                .chat_display
                .iter()
                .map(|e| e.text.lines().count().max(1))
                .sum();
            let estimated_h =
                56.0 + (total_lines as f32 * 26.0) + (self.chat_display.len() as f32 * 24.0) + 32.0;
            let clamped = estimated_h.min(580.0).max(220.0);
            return Size {
                width: px(640.0),
                height: px(clamped),
            };
        }

        if self.latest_result.is_some() {
            Size {
                width: px(640.0),
                height: px(320.0),
            }
        } else if matches!(
            self.state,
            AgentState::Processing { .. }
                | AgentState::Acting { .. }
                | AgentState::WaitingForConfirmation { .. }
        ) {
            Size {
                width: px(640.0),
                height: px(230.0),
            }
        } else if !self.input_buffer.is_empty() {
            let items = get_launcher_items(&self.input_buffer);
            if !items.is_empty() {
                let count = items.len().min(5);
                let height = 112.0 + (count as f32 * 46.0) + 12.0;
                Size {
                    width: px(640.0),
                    height: px(height),
                }
            } else {
                Size {
                    width: px(640.0),
                    height: px(112.0),
                }
            }
        } else {
            Size {
                width: px(640.0),
                height: px(112.0),
            }
        }
    }

    pub fn toggle_expanded(
        &mut self,
        _: &ToggleExpanded,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.play_sound_feedback(SoundEffect::Select);
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn toggle_spotlight(
        &mut self,
        _: &ToggleSpotlight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.play_sound_feedback(SoundEffect::Select);
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.play_sound_feedback(SoundEffect::Select);
        self.mode = match self.mode {
            FunctionMode::Settings => FunctionMode::Command,
            _ => FunctionMode::Settings,
        };
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn toggle_theme(&mut self, _: &ToggleTheme, _window: &mut Window, cx: &mut Context<Self>) {
        self.theme = self.theme.toggle();
        cx.notify();
    }

    pub fn start_push_to_talk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.listening {
            return;
        }

        #[cfg(target_os = "macos")]
        {
            let perm_status = function_platform::audio::macos_mic::ensure_mic_permission();
            if perm_status == function_platform::audio::macos_mic::MacMicPermission::Denied
                || perm_status == function_platform::audio::macos_mic::MacMicPermission::Restricted
            {
                tracing::error!("[Push-To-Talk] Microphone permission missing or denied");
                let err_msg = "Microphone access denied. Please allow microphone access in System Settings -> Privacy & Security -> Microphone.".to_string();
                self.set_voice_error(err_msg, cx);
                return;
            }
        }

        if !self.mic_available || self.audio_capture.is_none() {
            tracing::error!("[Push-To-Talk] Microphone unavailable");
            let err_msg = "Microphone unavailable. You can type your request directly.".to_string();
            self.set_voice_error(err_msg, cx);
            return;
        }

        if !self.is_visible {
            self.summon(window, cx);
        }

        if let Some(ref capture) = self.audio_capture {
            if let Err(e) = capture.start_recording() {
                tracing::error!(error = %e, "[Push-To-Talk] Failed to start audio recording");
                self.set_voice_error(format!("Mic error: {}. Type your request directly.", e), cx);
                return;
            }
        }

        self.listening = true;
        self.voice_error = None;
        self.state = AgentState::Listening;
        tracing::info!("[Push-To-Talk] Recording started");
        self.play_sound_feedback(SoundEffect::Select);
        cx.notify();
    }

    pub fn stop_push_to_talk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        if !self.listening {
            return;
        }

        self.listening = false;
        tracing::info!("[Push-To-Talk] Recording stopped");
        self.play_sound_feedback(SoundEffect::Select);

        let Some(ref capture) = self.audio_capture else {
            self.state = AgentState::Idle;
            cx.notify();
            return;
        };

        match capture.stop_recording() {
            Ok(wav_bytes) => {
                tracing::info!(byte_count = wav_bytes.len(), "[Push-To-Talk] Audio captured");
                if wav_bytes.is_empty() {
                    self.state = AgentState::Idle;
                    self.set_voice_error("No speech detected. Type your request instead.".to_string(), cx);
                    return;
                }

                self.is_transcribing = true;
                self.state = AgentState::Processing {
                    thought_summary: Some("Transcribing speech with Whisper...".to_string()),
                };
                self.voice_error = None;
                tracing::info!("[Push-To-Talk] Audio sent to STT");
                cx.notify();

                let stt = self.stt_provider.clone();
                let agent = self.agent.clone();
                let runtime_handle = crate::get_runtime_handle();

                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let cx = cx.clone();
                    async move {
                        let result = if let Some(stt) = stt {
                            if let Some(handle) = runtime_handle {
                                match handle
                                    .spawn(async move {
                                        stt.transcribe_audio(&wav_bytes, 16000).await
                                    })
                                    .await
                                {
                                    Ok(result) => result,
                                    Err(error) => Err(function_providers::ProviderError::Network(
                                        format!("Transcription task failed: {error}"),
                                    )),
                                }
                            } else {
                                Err(function_providers::ProviderError::Network(
                                    "Tokio runtime unavailable for transcription".to_string(),
                                ))
                            }
                        } else {
                            Err(function_providers::ProviderError::NotConfigured(
                                "Speech-to-text provider not configured".to_string(),
                            ))
                        };

                        let _ = cx.update(|cx| {
                            this.update(cx, |view, cx| {
                                view.is_transcribing = false;
                                match result {
                                    Ok(transcript) => {
                                        let text = transcript.trim().to_string();
                                        tracing::info!(
                                            transcription = %text,
                                            "[Push-To-Talk] Transcription received"
                                        );

                                        if !text.is_empty() {
                                            view.voice_error = None;
                                            view.play_sound_feedback(SoundEffect::Success);
                                            if let Some(agent_arc) = agent {
                                                agent_arc.cancel();
                                                view.stop_audio(cx);
                                                view.active_task = Some(text.clone());
                                                view.input_buffer.clear();
                                                view.mode = FunctionMode::Command;
                                                view.activities.clear();
                                                view.activities.push(ActivityEntry {
                                                    step: 1,
                                                    description: format!("Spoken prompt: \"{}\"", text),
                                                    status: ActivityStatus::Done,
                                                });
                                                view.activities.push(ActivityEntry {
                                                    step: 2,
                                                    description: "Executing computer task".to_string(),
                                                    status: ActivityStatus::Running,
                                                });
                                                view.state = AgentState::Processing {
                                                    thought_summary: Some("Executing spoken request...".to_string()),
                                                };
                                                let prompt_text = text.clone();
                                                if let Some(handle) = crate::get_runtime_handle() {
                                                    handle.spawn(async move {
                                                        let _ = agent_arc.execute_task(&prompt_text).await;
                                                    });
                                                } else {
                                                    tokio::spawn(async move {
                                                        let _ = agent_arc.execute_task(&prompt_text).await;
                                                    });
                                                }
                                            } else {
                                                view.input_buffer = text;
                                                view.state = AgentState::Idle;
                                            }
                                        } else {
                                            view.voice_error = Some(
                                                "No speech detected. Type your request instead.".to_string(),
                                            );
                                            view.state = AgentState::Idle;
                                            view.play_sound_feedback(SoundEffect::Error);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Voice transcription failed");
                                        view.voice_error = Some(format!(
                                            "Speech error: {}. You can type your request instead.",
                                            e
                                        ));
                                        view.state = AgentState::Idle;
                                        view.play_sound_feedback(SoundEffect::Error);
                                    }
                                }
                                cx.notify();
                            })
                        });
                    }
                })
                .detach();
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to stop audio recording");
                self.state = AgentState::Idle;
                self.set_voice_error(format!("Audio recording error: {}", e), cx);
            }
        }
    }

    pub fn toggle_voice(&mut self, _: &ToggleVoice, _window: &mut Window, cx: &mut Context<Self>) {
        if self.listening {
            self.listening = false;
            self.play_sound_feedback(SoundEffect::Select);

            if let Some(ref capture) = self.audio_capture {
                match capture.stop_recording() {
                    Ok(wav_bytes) => {
                        if wav_bytes.is_empty() {
                            self.state = AgentState::Idle;
                            self.voice_error =
                                Some("No speech detected. Type your request instead.".to_string());
                            cx.notify();
                            return;
                        }

                        self.is_transcribing = true;
                        self.state = AgentState::Processing {
                            thought_summary: Some("Transcribing audio...".to_string()),
                        };
                        self.voice_error = None;
                        cx.notify();

                        let stt = self.stt_provider.clone();
                        let agent = self.agent.clone();
                        let runtime_handle = crate::get_runtime_handle();

                        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                            let cx = cx.clone();
                            async move {
                                let result = if let Some(stt) = stt {
                                    if let Some(handle) = runtime_handle {
                                        match handle
                                            .spawn(async move {
                                                stt.transcribe_audio(&wav_bytes, 16000).await
                                            })
                                            .await
                                        {
                                            Ok(result) => result,
                                            Err(error) => Err(function_providers::ProviderError::Network(
                                                format!("Transcription task failed: {error}"),
                                            )),
                                        }
                                    } else {
                                        Err(function_providers::ProviderError::Network(
                                            "Tokio runtime unavailable for transcription".to_string(),
                                        ))
                                    }
                                } else {
                                    Err(function_providers::ProviderError::NotConfigured(
                                        "Speech-to-text provider not configured".to_string(),
                                    ))
                                };

                                let _ = cx.update(|cx| {
                                    this.update(cx, |view, cx| {
                                        view.is_transcribing = false;
                                        match result {
                                            Ok(transcript) => {
                                                let text = transcript.trim().to_string();
                                                if !text.is_empty() {
                                                    view.voice_error = None;
                                                    view.play_sound_feedback(SoundEffect::Success);
                                                    if let Some(agent_arc) = agent {
                                                        agent_arc.cancel();
                                                        view.stop_audio(cx);
                                                        view.active_task = Some(text.clone());
                                                        view.input_buffer.clear();
                                                        view.mode = FunctionMode::Command;
                                                        view.activities.clear();
                                                        view.activities.push(ActivityEntry {
                                                            step: 1,
                                                            description: format!("Spoken prompt: \"{}\"", text),
                                                            status: ActivityStatus::Done,
                                                        });
                                                        view.activities.push(ActivityEntry {
                                                            step: 2,
                                                            description: "Executing computer task".to_string(),
                                                            status: ActivityStatus::Running,
                                                        });
                                                        view.state = AgentState::Processing {
                                                            thought_summary: Some("Executing spoken request...".to_string()),
                                                        };
                                                        let prompt_text = text.clone();
                                                        if let Some(handle) = crate::get_runtime_handle() {
                                                            handle.spawn(async move {
                                                                let _ = agent_arc.execute_task(&prompt_text).await;
                                                            });
                                                        } else {
                                                            tokio::spawn(async move {
                                                                let _ = agent_arc.execute_task(&prompt_text).await;
                                                            });
                                                        }
                                                    } else {
                                                        view.input_buffer = text;
                                                        view.state = AgentState::Idle;
                                                    }
                                                } else {
                                                    view.voice_error = Some(
                                                        "No speech detected. Type your request instead.".to_string(),
                                                    );
                                                    view.state = AgentState::Idle;
                                                    view.play_sound_feedback(SoundEffect::Error);
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(error = %e, "Voice transcription failed");
                                                view.voice_error = Some(format!(
                                                    "Speech error: {}. You can type your request instead.",
                                                    e
                                                ));
                                                view.state = AgentState::Idle;
                                                view.play_sound_feedback(SoundEffect::Error);
                                            }
                                        }
                                        cx.notify();
                                    })
                                });
                            }
                        })
                        .detach();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to stop audio recording");
                        self.state = AgentState::Idle;
                        self.voice_error = Some(format!("Audio recording error: {}", e));
                        self.play_sound_feedback(SoundEffect::Error);
                        cx.notify();
                    }
                }
            } else {
                self.state = AgentState::Idle;
                cx.notify();
            }
        } else {
            if !self.mic_available || self.audio_capture.is_none() {
                tracing::warn!("Microphone is not available or audio capture uninitialized");
                self.voice_error =
                    Some("Microphone unavailable. You can type your request directly.".to_string());
                self.play_sound_feedback(SoundEffect::Error);
                cx.notify();
                return;
            }

            if let Some(ref capture) = self.audio_capture {
                if let Err(e) = capture.start_recording() {
                    tracing::error!(error = %e, "Failed to start audio recording");
                    self.voice_error =
                        Some(format!("Mic error: {}. Type your request directly.", e));
                    self.play_sound_feedback(SoundEffect::Error);
                    cx.notify();
                    return;
                }
            }

            self.listening = true;
            self.voice_error = None;
            self.state = AgentState::Listening;
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
        }
    }

    pub fn save_current_conversation(&mut self) {
        if self.chat_display.is_empty() {
            return;
        }
        let conv = if let Some(ref id) = self.active_conversation_id {
            if let Some(existing) = self.conversation_store.get(id).cloned() {
                let mut c = existing;
                c.display_messages = self.chat_display.clone();
                c.api_messages = self.chat_history_api.clone();
                if c.title == "New Conversation" {
                    c.title = SavedConversation::derive_title(&self.chat_display);
                }
                c
            } else {
                SavedConversation::new(self.chat_display.clone(), self.chat_history_api.clone())
            }
        } else {
            SavedConversation::new(self.chat_display.clone(), self.chat_history_api.clone())
        };
        self.active_conversation_id = Some(conv.id.clone());
        self.conversation_store.save_conversation(conv);
    }

    pub fn go_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat_display.is_empty() {
            self.save_current_conversation();
        }
        self.mode = FunctionMode::Command;
        self.input_buffer.clear();
        self.chat_display.clear();
        self.chat_history_api.clear();
        self.active_conversation_id = None;
        self.latest_result = None;
        self.active_task = None;
        self.activities.clear();
        self.state = AgentState::Idle;
        self.voice_error = None;
        self.selected_index = 0;
        self.is_text_selected = false;
        self.cursor_visible = true;
        self.listening = false;
        self.conversation_status_message = None;
        self.expanded_errors.clear();
        self.play_sound_feedback(SoundEffect::Select);
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn start_new_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.go_home(window, cx);
    }

    pub fn open_conversations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat_display.is_empty() {
            self.save_current_conversation();
        }
        self.mode = FunctionMode::Conversations;
        self.conversation_selected_index = 0;
        self.conversation_status_message = None;
        self.conversation_store = ConversationStore::load();
        self.play_sound_feedback(SoundEffect::Select);
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn load_conversation(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(conv) = self.conversation_store.get(id).cloned() {
            self.chat_display = conv.display_messages;
            self.chat_history_api = conv.api_messages;
            self.active_conversation_id = Some(conv.id);
            self.mode = FunctionMode::Command;
            self.input_buffer.clear();
            self.selected_index = 0;
            self.latest_result = None;
            self.active_task = None;
            self.activities.clear();
            self.state = AgentState::Idle;
            self.voice_error = None;
            self.play_sound_feedback(SoundEffect::Select);
            window.resize(self.target_window_size());
            self.chat_scroll_handle
                .scroll_to_item(self.chat_display.len().saturating_sub(1));
            cx.notify();
        }
    }

    pub fn set_voice_error(&mut self, err: String, cx: &mut Context<Self>) {
        self.voice_error = Some(err);
        self.play_sound_feedback(SoundEffect::Error);
        cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                Timer::after(Duration::from_secs(6)).await;
                let _ = cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        if view.voice_error.is_some() {
                            view.voice_error = None;
                            cx.notify();
                        }
                    });
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        self.config.ai_provider.api_key = if self.settings_api_key.trim().is_empty() {
            None
        } else {
            Some(self.settings_api_key.trim().to_string())
        };
        self.config.ai_provider.model = self.settings_model.trim().to_string();
        self.config.ai_provider.base_url = self.settings_base_url.trim().to_string();
        self.config.ai_provider.provider_name = self.settings_provider.trim().to_lowercase();
        self.config.speech.api_key = (!self.settings_stt_api_key.trim().is_empty())
            .then(|| self.settings_stt_api_key.trim().to_string());
        self.config.speech.model = self.settings_stt_model.trim().to_string();
        self.config.speech.base_url = self.settings_stt_base_url.trim().to_string();
        self.config.tts.api_key = (!self.settings_tts_api_key.trim().is_empty())
            .then(|| self.settings_tts_api_key.trim().to_string());
        self.config.tts.model = self.settings_tts_model.trim().to_string();
        self.config.tts.base_url = self.settings_tts_base_url.trim().to_string();
        self.config.tts.voice = self.settings_tts_voice.trim().to_string();
        if self
            .config
            .ai_provider
            .provider_name
            .eq_ignore_ascii_case("gemini")
            && (self.config.ai_provider.base_url.trim().is_empty()
                || self.config.ai_provider.base_url.trim() == "https://api.openai.com/v1")
        {
            self.config.ai_provider.base_url =
                function_providers::gemini::GEMINI_OPENAI_BASE_URL.to_string();
        }
        self.config.sound_enabled = self.settings_sound_enabled;
        self.theme = Theme::from_config(&self.config);

        if let Some(ref agent) = self.agent {
            agent.set_request_delay(self.config.request_delay.as_millis());
            agent.set_input_token_limit(self.config.input_token_limit);
        }

        match self.config.save() {
            Ok(_) => {
                self.apply_saved_provider_config();
                self.settings_status_message =
                    Some("Preferences saved to ~/.function/config.json".into());
                self.play_sound_feedback(SoundEffect::Success);
            }
            Err(e) => {
                self.settings_status_message = Some(format!("Save error: {}", e));
                self.play_sound_feedback(SoundEffect::Error);
            }
        }
        cx.notify();
    }

    pub fn begin_chatgpt_login(&mut self, cx: &mut Context<Self>) {
        let authorization = function_providers::chatgpt_oauth::begin_authorization();
        if let Err(error) =
            function_providers::chatgpt_oauth::open_authorization_url(&authorization.url)
        {
            self.settings_status_message = Some(format!("Could not open ChatGPT login: {error}"));
            cx.notify();
            return;
        }
        self.settings_status_message = Some("Complete ChatGPT sign-in in your browser...".into());
        let state = authorization.state().to_string();
        let Some(runtime_handle) = crate::get_runtime_handle() else {
            self.settings_status_message =
                Some("ChatGPT login unavailable: Tokio runtime is not initialized".into());
            cx.notify();
            return;
        };
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let result = runtime_handle.spawn_blocking(move || {
                    let code = function_providers::chatgpt_oauth::wait_for_callback(state)?;
                    function_providers::chatgpt_oauth::exchange_code(authorization, code)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
                let _ = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        match result {
                            Ok(tokens) => {
                                let save_result = function_providers::chatgpt_oauth::save_tokens(&tokens);
                                let install_result = function_providers::chatgpt_oauth::install_access_token_in_codex(&tokens.access_token);
                                if let Err(error) = save_result.or(install_result) {
                                    view.settings_status_message = Some(format!("Could not save ChatGPT login: {error}"));
                                } else {
                                    view.settings_provider = "chatgpt-plan".into();
                                    view.settings_model = "gpt-5".into();
                                    view.settings_base_url.clear();
                                    view.config.ai_provider.provider_name = "chatgpt-plan".into();
                                    view.config.ai_provider.model = "gpt-5".into();
                                    view.config.ai_provider.api_key = None;
                                    view.apply_saved_provider_config();
                                    view.settings_status_message = Some("ChatGPT connected. Save & Apply to use it as the main AI.".into());
                                }
                            }
                            Err(error) => view.settings_status_message = Some(format!("ChatGPT login failed: {error}")),
                        }
                        cx.notify();
                    })
                });
            }
        }).detach();
        cx.notify();
    }

    fn apply_saved_provider_config(&mut self) {
        let credentials = function_config::InMemoryCredentialStore::new();
        let api_key = self.config.ai_provider.resolve_api_key(&credentials);

        let ai_base_url = if self
            .config
            .ai_provider
            .provider_name
            .eq_ignore_ascii_case("gemini")
            && (self.config.ai_provider.base_url.trim().is_empty()
                || self.config.ai_provider.base_url.trim() == "https://api.openai.com/v1")
        {
            function_providers::gemini::GEMINI_OPENAI_BASE_URL.to_string()
        } else {
            self.config.ai_provider.base_url.clone()
        };
        let provider: std::sync::Arc<dyn function_providers::LlmProvider> = if self
            .config
            .ai_provider
            .provider_name
            .eq_ignore_ascii_case("chatgpt-plan")
            || self.config.ai_provider.is_configured()
        {
            if self
                .config
                .ai_provider
                .provider_name
                .eq_ignore_ascii_case("chatgpt-plan")
            {
                std::sync::Arc::new(function_providers::CodexChatGptProvider::new(
                    &self.config.ai_provider.model,
                ))
            } else if self
                .config
                .ai_provider
                .provider_name
                .eq_ignore_ascii_case("gemini")
            {
                std::sync::Arc::new(function_providers::GeminiLlmProvider::new(
                    ai_base_url,
                    api_key.clone(),
                    &self.config.ai_provider.model,
                ))
            } else {
                std::sync::Arc::new(function_providers::OpenAiLlmProvider::new(
                    &self.config.ai_provider.base_url,
                    api_key.clone(),
                    &self.config.ai_provider.model,
                ))
            }
        } else {
            std::sync::Arc::new(function_providers::MockLlmProvider::new(
                "Function computer assistant ready. Configure your API key in settings or run computer tools directly.",
            ))
        };

        if let Some(agent) = &self.agent {
            agent.set_provider(provider);
        }

        self.stt_provider = if self.config.speech.is_configured() {
            Some(std::sync::Arc::new(
                function_providers::WhisperSttProvider::with_model(
                    &self.config.speech.base_url,
                    self.config.speech.resolve_api_key(),
                    &self.config.speech.model,
                ),
            ))
        } else {
            Some(std::sync::Arc::new(
                function_providers::MockSttProvider::new("Open my browser and navigate to YouTube"),
            ))
        };
        self.tts_provider = if self.config.tts.enabled {
            self.config.tts.resolve_api_key().map(|key| {
                std::sync::Arc::new(function_providers::OpenAiTtsProvider::new(
                    &self.config.tts.base_url,
                    Some(key),
                    &self.config.tts.model,
                    &self.config.tts.voice,
                    &self.config.tts.output_format,
                )) as std::sync::Arc<dyn function_providers::TextToSpeechProvider>
            })
        } else {
            None
        };
    }

    pub fn cycle_theme_style(&mut self, cx: &mut Context<Self>) {
        self.config.theme_style = match self.config.theme_style {
            function_config::ThemeStyle::CarbonDark => function_config::ThemeStyle::ObsidianOled,
            function_config::ThemeStyle::ObsidianOled => function_config::ThemeStyle::SlateMidnight,
            function_config::ThemeStyle::SlateMidnight => function_config::ThemeStyle::StudioLight,
            function_config::ThemeStyle::StudioLight => function_config::ThemeStyle::CarbonDark,
        };
        self.theme = Theme::from_config(&self.config);
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    pub fn cycle_request_delay(&mut self, cx: &mut Context<Self>) {
        self.config.request_delay = self.config.request_delay.next();
        if let Some(ref agent) = self.agent {
            agent.set_request_delay(self.config.request_delay.as_millis());
        }
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    pub fn cycle_input_token_limit(&mut self, cx: &mut Context<Self>) {
        self.config.input_token_limit = self.config.input_token_limit.next();
        if let Some(ref agent) = self.agent {
            agent.set_input_token_limit(self.config.input_token_limit);
        }
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    pub fn toggle_advanced_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_advanced_expanded = !self.settings_advanced_expanded;
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    fn focused_settings_value(&self) -> &str {
        match self.settings_focused_field {
            0 => &self.settings_api_key,
            1 => &self.settings_model,
            2 => &self.settings_base_url,
            3 => &self.settings_provider,
            4 => &self.settings_stt_api_key,
            5 => &self.settings_stt_model,
            6 => &self.settings_stt_base_url,
            7 => &self.settings_tts_api_key,
            8 => &self.settings_tts_model,
            9 => &self.settings_tts_base_url,
            10 => &self.settings_tts_voice,
            _ => "",
        }
    }

    fn focused_settings_value_mut(&mut self) -> &mut String {
        match self.settings_focused_field {
            0 => &mut self.settings_api_key,
            1 => &mut self.settings_model,
            2 => &mut self.settings_base_url,
            3 => &mut self.settings_provider,
            4 => &mut self.settings_stt_api_key,
            5 => &mut self.settings_stt_model,
            6 => &mut self.settings_stt_base_url,
            7 => &mut self.settings_tts_api_key,
            8 => &mut self.settings_tts_model,
            9 => &mut self.settings_tts_base_url,
            10 => &mut self.settings_tts_voice,
            _ => &mut self.settings_model,
        }
    }

    pub fn cycle_accent_color(&mut self, cx: &mut Context<Self>) {
        self.config.accent_color = match self.config.accent_color {
            function_config::AccentColor::White => function_config::AccentColor::Cyan,
            function_config::AccentColor::Cyan => function_config::AccentColor::Emerald,
            function_config::AccentColor::Emerald => function_config::AccentColor::Violet,
            function_config::AccentColor::Violet => function_config::AccentColor::Amber,
            function_config::AccentColor::Amber => function_config::AccentColor::White,
        };
        self.theme = Theme::from_config(&self.config);
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    pub fn toggle_window_position_mode(&mut self, cx: &mut Context<Self>) {
        self.config.window_position = match self.config.window_position {
            function_config::WindowPositionMode::Center => {
                function_config::WindowPositionMode::UpperThird
            }
            function_config::WindowPositionMode::UpperThird => {
                function_config::WindowPositionMode::Center
            }
        };
        let sz = self.target_window_size();
        function_platform::center_window_by_title(
            "Function",
            f32::from(sz.width) as i32,
            f32::from(sz.height) as i32,
            self.config.window_position == function_config::WindowPositionMode::UpperThird,
        );
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    pub fn toggle_sound_setting(&mut self, cx: &mut Context<Self>) {
        self.settings_sound_enabled = !self.settings_sound_enabled;
        self.config.sound_enabled = self.settings_sound_enabled;
        self.play_sound_feedback(SoundEffect::Navigate);
        cx.notify();
    }

    pub fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_visible && !self.is_active_window {
            tracing::info!("Escape: window is visible but inactive, dismissing directly");
            self.dismiss(window, cx);
            return;
        }

        if self.voice_error.is_some() {
            tracing::info!("Escape: dismissing voice error message");
            self.voice_error = None;
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
            return;
        }

        if self.mode == FunctionMode::Settings || self.mode == FunctionMode::Conversations {
            tracing::info!("Escape: going back to Command mode");
            self.mode = FunctionMode::Command;
            self.conversation_status_message = None;
            window.resize(self.target_window_size());
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
            return;
        }

        if self.listening {
            tracing::info!("Escape: canceling voice recording");
            self.listening = false;
            if let Some(ref capture) = self.audio_capture {
                let _ = capture.stop_recording();
            }
            self.state = AgentState::Idle;
            self.voice_error = None;
            self.play_sound_feedback(SoundEffect::Select);
            window.resize(self.target_window_size());
            cx.notify();
            return;
        }

        if self.is_text_selected {
            tracing::info!("Escape: deselecting input text");
            self.is_text_selected = false;
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
            return;
        }

        if !self.chat_display.is_empty()
            || self.latest_result.is_some()
            || self.active_task.is_some()
            || self.is_playing_audio
        {
            if let Some(ref agent) = self.agent {
                agent.cancel();
            }
            self.stop_audio(cx);
            tracing::info!("Escape: returning to home state from active chat/results");
            self.chat_display.clear();
            self.chat_history_api.clear();
            self.active_conversation_id = None;
            self.latest_result = None;
            self.active_task = None;
            self.activities.clear();
            self.state = AgentState::Idle;
            self.selected_index = 0;
            self.play_sound_feedback(SoundEffect::Select);
            window.resize(self.target_window_size());
            cx.notify();
            return;
        }

        tracing::info!(
            "Escape: at root prompt, dismissing function window and preserving input text"
        );
        self.dismiss(window, cx);
    }

    pub fn close(&mut self, _: &CloseFunction, window: &mut Window, cx: &mut Context<Self>) {
        self.go_back(window, cx);
    }

    pub fn cancel(&mut self, _: &CancelTask, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_response(cx);
        self.go_back(window, cx);
    }

    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        if !self.is_visible {
            return;
        }
        if let Some(last) = self.last_toggle_time {
            if last.elapsed() < Duration::from_millis(150) {
                return;
            }
        }
        self.last_toggle_time = Some(std::time::Instant::now());
        self.activation_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.activation_settling = false;
        tracing::info!("Dismissing Function window (hiding)");
        self.is_visible = false;
        self.is_active_window = false;
        function_platform::set_window_visibility_state(false);
        tracing::info!("Window transition: visible -> hidden");
        self.play_sound_feedback(SoundEffect::WindowToggle);
        #[cfg(target_os = "macos")]
        function_platform::macos_hide_app();
        #[cfg(not(target_os = "macos"))]
        function_platform::hide_window_by_title("Function");
        cx.notify();
    }

    fn dismiss_after_external_deactivation(&mut self, cx: &mut Context<Self>) {
        if !self.is_visible {
            return;
        }
        if let Some(last) = self.last_toggle_time {
            if last.elapsed() < Duration::from_millis(150) {
                return;
            }
        }
        self.last_toggle_time = Some(std::time::Instant::now());
        self.activation_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.activation_settling = false;
        self.is_visible = false;
        self.is_active_window = false;
        function_platform::set_window_visibility_state(false);
        self.play_sound_feedback(SoundEffect::WindowToggle);
        #[cfg(target_os = "macos")]
        function_platform::macos_hide_app();
        #[cfg(not(target_os = "macos"))]
        function_platform::hide_window_by_title("Function");
        cx.notify();
    }

    pub fn summon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_visible {
            return;
        }
        if let Some(last) = self.last_toggle_time {
            if last.elapsed() < Duration::from_millis(150) {
                return;
            }
        }
        self.last_toggle_time = Some(std::time::Instant::now());
        let activation_generation = self
            .activation_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        self.activation_settling = true;
        let generation_guard = self.activation_generation.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                Timer::after(Duration::from_millis(
                    function_platform::MACOS_ACTIVATION_SETTLE_MS,
                ))
                .await;
                let _ = cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        if generation_guard.load(std::sync::atomic::Ordering::SeqCst)
                            == activation_generation
                        {
                            view.activation_settling = false;
                            cx.notify();
                        }
                    });
                });
            }
        })
        .detach();
        self.is_visible = true;
        self.is_active_window = true;
        self.is_text_selected = false;
        function_platform::set_window_visibility_state(true);
        tracing::info!("Window transition: hidden -> visible");
        let target_size = self.target_window_size();
        tracing::info!(
            ?target_size,
            "Summoning Function window (showing and focusing)"
        );
        tracing::info!("Activating Function window");
        self.cursor_offset = self.input_buffer.chars().count();
        self.cursor_visible = true;
        self.selected_index = 0;
        self.mode = FunctionMode::Command;
        self.play_sound_feedback(SoundEffect::WindowToggle);
        #[cfg(target_os = "macos")]
        {
            tracing::info!("Activating macOS application and ordering window front");
            function_platform::macos_activate_app();
            window.activate_window();
        }
        #[cfg(not(target_os = "macos"))]
        {
            function_platform::show_window_by_title("Function");
            window.activate_window();
        }
        window.resize(target_size);
        let upper_third =
            self.config.window_position == function_config::WindowPositionMode::UpperThird;
        std::thread::spawn(move || {
            function_platform::center_window_by_title(
                "Function",
                f32::from(target_size.width) as i32,
                f32::from(target_size.height) as i32,
                upper_third,
            );
        });
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub fn open_settings(
        &mut self,
        _: &crate::actions::OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_visible {
            self.summon(window, cx);
        }
        self.mode = FunctionMode::Settings;
        self.cursor_offset = 0;
        self.selected_index = 0;
        self.play_sound_feedback(SoundEffect::Select);
        window.resize(self.target_window_size());
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub fn toggle_visibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            current_visibility = self.is_visible,
            is_window_active = window.is_window_active(),
            "Toggling Function window visibility"
        );
        if self.is_visible {
            self.dismiss(window, cx);
        } else {
            self.summon(window, cx);
        }
    }

    pub fn toggle_visibility_from_hotkey(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_visibility(window, cx);
    }

    pub fn clear_input(&mut self, _: &ClearInput, _window: &mut Window, cx: &mut Context<Self>) {
        self.input_buffer.clear();
        self.cursor_visible = true;
        cx.notify();
    }

    pub fn trigger_launcher_item(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = get_launcher_items(&self.input_buffer);
        if let Some(item) = items.get(index).cloned() {
            self.execute_launcher_action(item.action, window, cx);
        }
    }

    pub fn execute_launcher_action(
        &mut self,
        action: LauncherAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            LauncherAction::FillPrefix(prefix) => {
                self.input_buffer = prefix;
                self.selected_index = 0;
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
            }
            LauncherAction::ExecuteShell(cmd) => {
                self.play_sound_feedback(SoundEffect::Execute);
                self.mode = FunctionMode::Command;
                window.resize(self.target_window_size());

                self.activities.clear();
                match function_platform::spawn_shell_command(&cmd) {
                    Ok(()) => {
                        self.input_buffer.clear();
                        self.active_task = Some(format!("Shell: {}", cmd));
                        self.activities.push(ActivityEntry {
                            step: 1,
                            description: format!("Executing shell command: \"{}\"", cmd),
                            status: ActivityStatus::Done,
                        });
                        self.latest_result = Some(format!("Shell command \"{}\" executed.", cmd));
                    }
                    Err(error) => {
                        let message = format!("Could not launch shell command: {error}");
                        self.active_task = None;
                        self.activities.push(ActivityEntry {
                            step: 1,
                            description: message.clone(),
                            status: ActivityStatus::Failed,
                        });
                        self.latest_result = Some(message);
                        self.play_sound_feedback(SoundEffect::Error);
                    }
                }
                cx.notify();
            }
            LauncherAction::CopyResult(val) => {
                copy_to_clipboard(&val);
                self.play_sound_feedback(SoundEffect::Success);
                self.input_buffer = format!("Copied: {}", val);
                cx.notify();
            }
            LauncherAction::OpenUrl(url) => {
                open_url(&url);
                self.play_sound_feedback(SoundEffect::Execute);
                self.input_buffer.clear();
                cx.notify();
            }
            LauncherAction::OpenSettings => {
                self.open_settings(&crate::actions::OpenSettings, window, cx);
            }
            LauncherAction::NewConversation => {
                self.start_new_conversation(window, cx);
            }
            LauncherAction::OpenConversations => {
                self.open_conversations(window, cx);
            }
            LauncherAction::ToggleTheme => {
                self.toggle_theme(&ToggleTheme, window, cx);
                self.input_buffer.clear();
            }
            LauncherAction::ToggleVoice => {
                self.toggle_voice(&ToggleVoice, window, cx);
                self.input_buffer.clear();
            }
            LauncherAction::ToggleSound => {
                self.toggle_sound_setting(cx);
                self.input_buffer.clear();
            }
            LauncherAction::ClearInput => {
                self.clear_input(&ClearInput, window, cx);
            }
            LauncherAction::OpenMemory => {
                let memory_file = function_config::memory_path();
                self.input_buffer.clear();
                if memory_file.exists() {
                    function_platform::open_path(memory_file.to_str().unwrap_or(""));
                } else {
                    self.latest_result = Some(format!("Memory file path: {}", memory_file.display()));
                }
            }
            LauncherAction::OpenAbout => {
                self.input_buffer.clear();
                let about_info = format!(
                    "Function Desktop v1\nAI Model: {}\nSound Feedback: {}",
                    self.config.ai_provider.model,
                    if self.config.sound_enabled { "Enabled" } else { "Disabled" }
                );
                self.latest_result = Some(about_info);
                window.resize(self.target_window_size());
                cx.notify();
            }
            LauncherAction::SetModel(model_name) => {
                self.input_buffer.clear();
                if !model_name.is_empty() {
                    self.settings_model = model_name.clone();
                    self.config.ai_provider.model = model_name.clone();
                    let _ = self.config.save();
                    self.latest_result = Some(format!("AI model set to: {}", model_name));
                } else {
                    self.latest_result = Some(format!("Current AI model: {}", self.config.ai_provider.model));
                }
                window.resize(self.target_window_size());
                cx.notify();
            }
            LauncherAction::RunTask(prompt) => {
                self.input_buffer = prompt;
                self.submit(&SubmitRequest, window, cx);
            }
            LauncherAction::OpenPath(path) => {
                self.play_sound_feedback(SoundEffect::Execute);
                function_platform::open_path(&path);
                self.input_buffer.clear();
                self.cursor_offset = 0;
                self.selected_index = 0;
                self.dismiss(window, cx);
            }
        }
    }

    pub fn open_selected_path(&mut self, cx: &mut Context<Self>) {
        let items = get_launcher_items(&self.input_buffer);
        let target_action = items
            .get(self.selected_index)
            .and_then(|it| match &it.action {
                LauncherAction::OpenPath(p) => Some(p.clone()),
                _ => None,
            })
            .or_else(|| {
                items.iter().find_map(|it| match &it.action {
                    LauncherAction::OpenPath(p) => Some(p.clone()),
                    _ => None,
                })
            });

        if let Some(path) = target_action {
            self.play_sound_feedback(SoundEffect::Execute);
            function_platform::open_path(&path);
            self.input_buffer.clear();
            self.cursor_offset = 0;
            self.selected_index = 0;
            self.is_visible = false;
            self.is_active_window = false;
            #[cfg(target_os = "macos")]
            function_platform::macos_hide_app();
            #[cfg(not(target_os = "macos"))]
            function_platform::hide_window_by_title("Function");
            cx.notify();
        }
    }

    pub fn submit(&mut self, _: &SubmitRequest, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = self.input_buffer.trim().to_string();
        if prompt.is_empty() {
            return;
        }

        let is_busy = matches!(
            self.state,
            AgentState::Processing { .. }
                | AgentState::Streaming { .. }
                | AgentState::Acting { .. }
                | AgentState::WaitingForConfirmation { .. }
        );
        if is_busy || self.is_playing_audio {
            if let Some(ref agent) = self.agent {
                agent.cancel();
            }
            self.stop_audio(cx);
            self.active_task = None;
            self.activities.clear();
            self.state = AgentState::Idle;
        }

        if let Some(command) = resolve_shell_command(&prompt) {
            self.execute_launcher_action(LauncherAction::ExecuteShell(command), window, cx);
            return;
        }

        if let Some(cmd) = resolve_local_command(&prompt) {
            match cmd {
                LocalCommand::Configure => {
                    self.input_buffer.clear();
                    self.open_settings(&crate::actions::OpenSettings, window, cx);
                    return;
                }
                LocalCommand::NewConversation => {
                    self.start_new_conversation(window, cx);
                    return;
                }
                LocalCommand::OpenConversations => {
                    self.open_conversations(window, cx);
                    return;
                }
                LocalCommand::ToggleTheme => {
                    self.execute_launcher_action(LauncherAction::ToggleTheme, window, cx);
                    return;
                }
                LocalCommand::ToggleVoice => {
                    self.execute_launcher_action(LauncherAction::ToggleVoice, window, cx);
                    return;
                }
                LocalCommand::ToggleSound => {
                    self.execute_launcher_action(LauncherAction::ToggleSound, window, cx);
                    return;
                }
                LocalCommand::ClearInput => {
                    self.execute_launcher_action(LauncherAction::ClearInput, window, cx);
                    return;
                }
                LocalCommand::OpenMemory => {
                    self.execute_launcher_action(LauncherAction::OpenMemory, window, cx);
                    return;
                }
                LocalCommand::OpenAbout => {
                    self.execute_launcher_action(LauncherAction::OpenAbout, window, cx);
                    return;
                }
                LocalCommand::Help => {
                    self.execute_launcher_action(
                        LauncherAction::RunTask("Provide a clear overview of Function slash commands and desktop capabilities.".to_string()),
                        window,
                        cx,
                    );
                    return;
                }
                LocalCommand::SetModel(model) => {
                    self.execute_launcher_action(LauncherAction::SetModel(model), window, cx);
                    return;
                }
            }
        }

        self.play_sound_feedback(SoundEffect::Execute);
        self.active_task = Some(prompt.clone());
        self.input_buffer.clear();
        self.cursor_visible = true;

        self.chat_display.push(ChatEntry {
            is_user: true,
            text: prompt.clone(),
        });

        self.mode = FunctionMode::Command;
        window.resize(self.target_window_size());
        self.chat_scroll_handle
            .scroll_to_item(self.chat_display.len().saturating_sub(1));

        self.activities.clear();
        self.activities.push(ActivityEntry {
            step: 1,
            description: format!("Thinking…"),
            status: ActivityStatus::Running,
        });

        if let Some(agent) = self.agent.clone() {
            self.state = AgentState::Processing {
                thought_summary: None,
            };

            let history_snapshot = self.chat_history_api.clone();
            let prompt_clone = prompt.clone();

            let task = async move {
                let _ = agent
                    .execute_with_history(&prompt_clone, history_snapshot)
                    .await;
            };

            if let Some(handle) = crate::get_runtime_handle() {
                handle.spawn(task);
            } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(task);
            } else {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(task);
                    }
                });
            }
        } else {
            self.state = AgentState::Idle;
            let no_agent_reply =
                "No AI provider configured. Go to Settings (Ctrl+,) to add your API key."
                    .to_string();
            self.chat_display.push(ChatEntry {
                is_user: false,
                text: no_agent_reply.clone(),
            });
            self.latest_result = Some(no_agent_reply);
        }

        cx.notify();
    }

    pub fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor_visible = true;
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if let AgentState::WaitingForConfirmation { action, .. } = &self.state {
            match key {
                "enter" | "y" => {
                    self.play_sound_feedback(SoundEffect::Execute);
                    let action_name = action.clone();
                    self.state = AgentState::Acting {
                        action_description: format!("Approved: {}", action_name),
                    };
                    self.latest_result = Some(format!(
                        "Action '{}' approved by user. Proceeding.",
                        action_name
                    ));
                    cx.notify();
                    return;
                }
                "escape" | "n" => {
                    self.play_sound_feedback(SoundEffect::Select);
                    self.state = AgentState::Idle;
                    self.latest_result = Some("Action denied by user. Operation canceled.".into());
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }

        if self.mode == FunctionMode::Settings {
            match key {
                "escape" => {
                    self.go_back(window, cx);
                    return;
                }
                "tab" => {
                    self.settings_focused_field = (self.settings_focused_field + 1) % 11;
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "enter" => {
                    self.save_settings(cx);
                    return;
                }
                "t" if modifiers.control => {
                    self.config.theme_style = match self.config.theme_style {
                        function_config::ThemeStyle::CarbonDark => {
                            function_config::ThemeStyle::ObsidianOled
                        }
                        function_config::ThemeStyle::ObsidianOled => {
                            function_config::ThemeStyle::SlateMidnight
                        }
                        function_config::ThemeStyle::SlateMidnight => {
                            function_config::ThemeStyle::StudioLight
                        }
                        function_config::ThemeStyle::StudioLight => {
                            function_config::ThemeStyle::CarbonDark
                        }
                    };
                    self.theme = Theme::from_config(&self.config);
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "a" if modifiers.control => {
                    self.config.accent_color = match self.config.accent_color {
                        function_config::AccentColor::White => function_config::AccentColor::Cyan,
                        function_config::AccentColor::Cyan => function_config::AccentColor::Emerald,
                        function_config::AccentColor::Emerald => {
                            function_config::AccentColor::Violet
                        }
                        function_config::AccentColor::Violet => function_config::AccentColor::Amber,
                        function_config::AccentColor::Amber => function_config::AccentColor::White,
                    };
                    self.theme = Theme::from_config(&self.config);
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "p" if modifiers.control => {
                    self.config.window_position = match self.config.window_position {
                        function_config::WindowPositionMode::Center => {
                            function_config::WindowPositionMode::UpperThird
                        }
                        function_config::WindowPositionMode::UpperThird => {
                            function_config::WindowPositionMode::Center
                        }
                    };
                    let sz = self.target_window_size();
                    function_platform::center_window_by_title(
                        "Function",
                        f32::from(sz.width) as i32,
                        f32::from(sz.height) as i32,
                        self.config.window_position
                            == function_config::WindowPositionMode::UpperThird,
                    );
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "c" if modifiers.secondary() => {
                    let text = self.focused_settings_value();
                    if !text.is_empty() {
                        copy_to_clipboard(text);
                        self.play_sound_feedback(SoundEffect::Select);
                    }
                    return;
                }
                "x" if modifiers.secondary() => {
                    let text = std::mem::take(self.focused_settings_value_mut());
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        self.play_sound_feedback(SoundEffect::Select);
                        cx.notify();
                    }
                    return;
                }
                "a" if modifiers.secondary() => {
                    self.focused_settings_value_mut().clear();
                    self.play_sound_feedback(SoundEffect::Select);
                    cx.notify();
                    return;
                }
                "v" if modifiers.secondary() || modifiers.control || modifiers.platform => {
                    if let Some(text) = cx
                        .read_from_clipboard()
                        .and_then(|clipboard| clipboard.text())
                    {
                        self.focused_settings_value_mut().push_str(&text);
                        cx.notify();
                    }
                    return;
                }
                "backspace" => {
                    self.focused_settings_value_mut().pop();
                    cx.notify();
                    return;
                }
                "space" => {
                    self.focused_settings_value_mut().push(' ');
                    cx.notify();
                    return;
                }
                "h" if modifiers.control => {
                    self.settings_show_key = !self.settings_show_key;
                    cx.notify();
                    return;
                }
                "m" | "s" if modifiers.control => {
                    self.settings_sound_enabled = !self.settings_sound_enabled;
                    self.config.sound_enabled = self.settings_sound_enabled;
                    cx.notify();
                    return;
                }
                ch if !is_named_control_key(ch) && !modifiers.control && !modifiers.alt => {
                    let text = if ch.chars().count() == 1 {
                        let c = ch.chars().next().unwrap();
                        let typed = if modifiers.shift { shift_char(c) } else { c };
                        typed.to_string()
                    } else {
                        ch.to_string()
                    };
                    self.focused_settings_value_mut().push_str(&text);
                    cx.notify();
                    return;
                }
                _ => {}
            }
            return;
        }

        if self.mode == FunctionMode::Conversations {
            let convs = self.conversation_store.list();
            let total_items = convs.len() + 1;

            match key {
                "escape" => {
                    self.mode = FunctionMode::Command;
                    self.conversation_status_message = None;
                    window.resize(self.target_window_size());
                    self.play_sound_feedback(SoundEffect::Select);
                    cx.notify();
                }
                "up" => {
                    self.conversation_selected_index =
                        self.conversation_selected_index.saturating_sub(1);
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                }
                "down" => {
                    if self.conversation_selected_index + 1 < total_items {
                        self.conversation_selected_index += 1;
                    }
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                }
                "enter" | "return" => {
                    if self.conversation_selected_index == 0 {
                        self.start_new_conversation(window, cx);
                    } else {
                        let idx = self.conversation_selected_index - 1;
                        if let Some(conv) = convs.get(idx) {
                            let id = conv.id.clone();
                            self.load_conversation(&id, window, cx);
                        }
                    }
                }
                "n" if !modifiers.control && !modifiers.alt => {
                    self.start_new_conversation(window, cx);
                }
                "delete" | "backspace" => {
                    if self.conversation_selected_index > 0 {
                        let idx = self.conversation_selected_index - 1;
                        if let Some(conv) = convs.get(idx) {
                            let id = conv.id.clone();
                            if self.conversation_store.delete_conversation(&id) {
                                if self.active_conversation_id.as_deref() == Some(&id) {
                                    self.active_conversation_id = None;
                                    self.chat_display.clear();
                                    self.chat_history_api.clear();
                                }
                                self.conversation_status_message =
                                    Some("Conversation removed.".to_string());
                                self.conversation_selected_index = self
                                    .conversation_selected_index
                                    .min(self.conversation_store.list().len());
                                self.play_sound_feedback(SoundEffect::Select);
                                cx.notify();
                            }
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        if key == "a" && (modifiers.secondary() || modifiers.control || modifiers.platform) {
            if !self.input_buffer.is_empty() {
                self.is_text_selected = true;
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
            }
            return;
        }

        if key == "c" && (modifiers.secondary() || modifiers.control || modifiers.platform) {
            let text_to_copy = if self.is_text_selected && !self.input_buffer.is_empty() {
                self.input_buffer.clone()
            } else if let Some(ref res) = self.latest_result {
                res.clone()
            } else if let Some(last_msg) = self.chat_display.iter().rev().find(|m| !m.is_user) {
                last_msg.text.clone()
            } else if !self.input_buffer.is_empty() {
                self.input_buffer.clone()
            } else {
                String::new()
            };
            if !text_to_copy.is_empty() {
                copy_to_clipboard(&text_to_copy);
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
            }
            return;
        }

        if key == "x" && (modifiers.secondary() || modifiers.control || modifiers.platform) {
            if !self.input_buffer.is_empty() {
                copy_to_clipboard(&self.input_buffer);
                self.input_buffer.clear();
                self.is_text_selected = false;
                self.selected_index = 0;
                self.play_sound_feedback(SoundEffect::Select);
                window.resize(self.target_window_size());
                cx.notify();
            }
            return;
        }

        if key == "v" && (modifiers.secondary() || modifiers.control || modifiers.platform) {
            if let Some(img_path) = read_clipboard_image() {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.cursor_offset = 0;
                    self.is_text_selected = false;
                }
                let img_str = format!("[Photo: {}] ", img_path.display());
                let insert_pos = self.cursor_offset.min(self.input_buffer.chars().count());
                let byte_pos = self
                    .input_buffer
                    .char_indices()
                    .nth(insert_pos)
                    .map(|(pos, _)| pos)
                    .unwrap_or(self.input_buffer.len());
                self.input_buffer.insert_str(byte_pos, &img_str);
                self.cursor_offset = insert_pos + img_str.chars().count();
                self.selected_index = 0;
                self.cursor_visible = true;
                let target_sz = self.target_window_size();
                if window.bounds().size != target_sz {
                    window.resize(target_sz);
                }
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
                return;
            }

            if let Some(text) = cx
                .read_from_clipboard()
                .and_then(|clipboard| clipboard.text())
            {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.cursor_offset = 0;
                    self.is_text_selected = false;
                }
                let trimmed = text.trim();
                let path = std::path::Path::new(trimmed);
                let text_to_insert = if path.exists() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if matches!(
                        ext.as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
                    ) {
                        format!("[Photo: {}] ", path.display())
                    } else {
                        text
                    }
                } else {
                    text
                };
                let insert_pos = self.cursor_offset.min(self.input_buffer.chars().count());
                let byte_pos = self
                    .input_buffer
                    .char_indices()
                    .nth(insert_pos)
                    .map(|(pos, _)| pos)
                    .unwrap_or(self.input_buffer.len());
                self.input_buffer.insert_str(byte_pos, &text_to_insert);
                self.cursor_offset = insert_pos + text_to_insert.chars().count();
                self.selected_index = 0;
                self.cursor_visible = true;
                let target_sz = self.target_window_size();
                if window.bounds().size != target_sz {
                    window.resize(target_sz);
                }
                cx.notify();
            }
            return;
        }

        if (modifiers.control || modifiers.secondary() || modifiers.platform)
            && (key == "," || key == "settings")
        {
            self.open_settings(&crate::actions::OpenSettings, window, cx);
            return;
        }

        if (modifiers.control || modifiers.secondary() || modifiers.platform) && key == "q" {
            std::process::exit(0);
        }

        if modifiers.control && (key == " " || key == "space") {
            self.dismiss(window, cx);
            return;
        }

        if modifiers.alt && !modifiers.control {
            match key {
                "1" => {
                    self.trigger_launcher_item(0, window, cx);
                    return;
                }
                "2" => {
                    self.trigger_launcher_item(1, window, cx);
                    return;
                }
                "3" => {
                    self.trigger_launcher_item(2, window, cx);
                    return;
                }
                "4" => {
                    self.trigger_launcher_item(3, window, cx);
                    return;
                }
                "5" => {
                    self.trigger_launcher_item(4, window, cx);
                    return;
                }
                _ => {}
            }
        }

        match key {
            "left" => {
                self.is_text_selected = false;
                if self.cursor_offset > 0 {
                    self.cursor_offset -= 1;
                    self.cursor_visible = true;
                    cx.notify();
                }
            }
            "right" => {
                self.is_text_selected = false;
                let char_count = self.input_buffer.chars().count();
                if self.cursor_offset < char_count {
                    self.cursor_offset += 1;
                    self.cursor_visible = true;
                    cx.notify();
                }
            }
            "home" => {
                self.is_text_selected = false;
                self.cursor_offset = 0;
                self.cursor_visible = true;
                cx.notify();
            }
            "end" => {
                self.is_text_selected = false;
                self.cursor_offset = self.input_buffer.chars().count();
                self.cursor_visible = true;
                cx.notify();
            }
            "pageup" => {
                self.user_scrolled_up = true;
                let current = self.chat_scroll_handle.top_item();
                self.chat_scroll_handle
                    .scroll_to_item(current.saturating_sub(2));
                cx.notify();
                return;
            }
            "pagedown" => {
                let current = self.chat_scroll_handle.bottom_item();
                let last_item = self.chat_display.len().saturating_sub(1);
                if current + 2 >= last_item {
                    self.user_scrolled_up = false;
                }
                self.chat_scroll_handle.scroll_to_item(current + 2);
                cx.notify();
                return;
            }
            "up" => {
                self.is_text_selected = false;
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                } else if !self.chat_display.is_empty() {
                    self.user_scrolled_up = true;
                    let current = self.chat_scroll_handle.top_item();
                    self.chat_scroll_handle
                        .scroll_to_item(current.saturating_sub(1));
                    cx.notify();
                }
            }
            "down" => {
                self.is_text_selected = false;
                let items = get_launcher_items(&self.input_buffer);
                if self.selected_index + 1 < items.len() {
                    self.selected_index += 1;
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                } else if !self.chat_display.is_empty() {
                    let current = self.chat_scroll_handle.bottom_item();
                    let last_item = self.chat_display.len().saturating_sub(1);
                    if current + 1 >= last_item {
                        self.user_scrolled_up = false;
                    }
                    self.chat_scroll_handle.scroll_to_item(current + 1);
                    cx.notify();
                }
            }
            "enter" | "return" => {
                if modifiers.shift {
                    let insert_pos = self.cursor_offset.min(self.input_buffer.chars().count());
                    let byte_pos = self
                        .input_buffer
                        .char_indices()
                        .nth(insert_pos)
                        .map(|(pos, _)| pos)
                        .unwrap_or(self.input_buffer.len());
                    self.input_buffer.insert(byte_pos, '\n');
                    self.cursor_offset = insert_pos + 1;
                    self.cursor_visible = true;
                    cx.notify();
                    return;
                }
                self.is_text_selected = false;
                let prompt = self.input_buffer.trim().to_string();
                if let Some(cmd) = resolve_local_command(&prompt) {
                    match cmd {
                        LocalCommand::Configure => {
                            self.input_buffer.clear();
                            self.open_settings(&crate::actions::OpenSettings, window, cx);
                            return;
                        }
                        LocalCommand::NewConversation => {
                            self.start_new_conversation(window, cx);
                            return;
                        }
                        LocalCommand::OpenConversations => {
                            self.open_conversations(window, cx);
                            return;
                        }
                        LocalCommand::ToggleTheme => {
                            self.execute_launcher_action(LauncherAction::ToggleTheme, window, cx);
                            return;
                        }
                        LocalCommand::ToggleVoice => {
                            self.execute_launcher_action(LauncherAction::ToggleVoice, window, cx);
                            return;
                        }
                        LocalCommand::ToggleSound => {
                            self.execute_launcher_action(LauncherAction::ToggleSound, window, cx);
                            return;
                        }
                        LocalCommand::ClearInput => {
                            self.execute_launcher_action(LauncherAction::ClearInput, window, cx);
                            return;
                        }
                        LocalCommand::OpenMemory => {
                            self.execute_launcher_action(LauncherAction::OpenMemory, window, cx);
                            return;
                        }
                        LocalCommand::OpenAbout => {
                            self.execute_launcher_action(LauncherAction::OpenAbout, window, cx);
                            return;
                        }
                        LocalCommand::Help => {
                            self.execute_launcher_action(
                                LauncherAction::RunTask("Provide a clear overview of Function slash commands and desktop capabilities.".to_string()),
                                window,
                                cx,
                            );
                            return;
                        }
                        LocalCommand::SetModel(model) => {
                            self.execute_launcher_action(LauncherAction::SetModel(model), window, cx);
                            return;
                        }
                    }
                }

                let items = get_launcher_items(&self.input_buffer);
                let has_openable = items
                    .iter()
                    .any(|item| matches!(item.action, LauncherAction::OpenPath(_)));

                if has_openable
                    && function_platform::is_file_search_open_shortcut(
                        modifiers.control,
                        modifiers.secondary(),
                        modifiers.alt,
                    )
                {
                    self.open_selected_path(cx);
                    return;
                }

                if has_openable {
                    if !prompt.is_empty() {
                        self.submit(&SubmitRequest, window, cx);
                    }
                    return;
                }

                if !has_openable {
                    if let Some(item) = items.get(self.selected_index).cloned() {
                        self.execute_launcher_action(item.action, window, cx);
                    } else if !prompt.is_empty() {
                        self.submit(&SubmitRequest, window, cx);
                    }
                    return;
                }
            }
            "tab" => {
                self.is_text_selected = false;
                self.toggle_expanded(&ToggleExpanded, window, cx);
            }
            "escape" => {
                self.go_back(window, cx);
            }
            "backspace" => {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.cursor_offset = 0;
                    self.is_text_selected = false;
                    self.selected_index = 0;
                    self.cursor_visible = true;
                    let target_sz = self.target_window_size();
                    if window.bounds().size != target_sz {
                        window.resize(target_sz);
                    }
                    cx.notify();
                    return;
                }
                if self.cursor_offset > 0 {
                    let char_idx = self.cursor_offset - 1;
                    if let Some((byte_pos, c)) = self.input_buffer.char_indices().nth(char_idx) {
                        self.input_buffer.drain(byte_pos..byte_pos + c.len_utf8());
                        self.cursor_offset -= 1;
                    }
                    self.selected_index = 0;
                    self.cursor_visible = true;
                    let target_sz = self.target_window_size();
                    if window.bounds().size != target_sz {
                        window.resize(target_sz);
                    }
                    cx.notify();
                }
            }
            "delete" => {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.cursor_offset = 0;
                    self.is_text_selected = false;
                    self.selected_index = 0;
                    self.cursor_visible = true;
                    let target_sz = self.target_window_size();
                    if window.bounds().size != target_sz {
                        window.resize(target_sz);
                    }
                    cx.notify();
                    return;
                }
                let char_count = self.input_buffer.chars().count();
                if self.cursor_offset < char_count {
                    if let Some((byte_pos, c)) =
                        self.input_buffer.char_indices().nth(self.cursor_offset)
                    {
                        self.input_buffer.drain(byte_pos..byte_pos + c.len_utf8());
                    }
                    self.selected_index = 0;
                    self.cursor_visible = true;
                    let target_sz = self.target_window_size();
                    if window.bounds().size != target_sz {
                        window.resize(target_sz);
                    }
                    cx.notify();
                }
            }
            "space" => {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.cursor_offset = 0;
                    self.is_text_selected = false;
                }
                let insert_pos = self.cursor_offset.min(self.input_buffer.chars().count());
                let byte_pos = self
                    .input_buffer
                    .char_indices()
                    .nth(insert_pos)
                    .map(|(pos, _)| pos)
                    .unwrap_or(self.input_buffer.len());
                self.input_buffer.insert(byte_pos, ' ');
                self.cursor_offset = insert_pos + 1;
                self.selected_index = 0;
                self.cursor_visible = true;
                let target_sz = self.target_window_size();
                if window.bounds().size != target_sz {
                    window.resize(target_sz);
                }
                cx.notify();
            }
            ch if !is_named_control_key(ch) && !modifiers.control && !modifiers.alt => {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.cursor_offset = 0;
                    self.is_text_selected = false;
                }
                let insert_pos = self.cursor_offset.min(self.input_buffer.chars().count());
                let byte_pos = self
                    .input_buffer
                    .char_indices()
                    .nth(insert_pos)
                    .map(|(pos, _)| pos)
                    .unwrap_or(self.input_buffer.len());
                if ch.chars().count() == 1 {
                    let c = ch.chars().next().unwrap();
                    let typed = if modifiers.shift { shift_char(c) } else { c };
                    self.input_buffer.insert(byte_pos, typed);
                    self.cursor_offset = insert_pos + 1;
                } else {
                    self.input_buffer.insert_str(byte_pos, ch);
                    self.cursor_offset = insert_pos + ch.chars().count();
                }
                self.selected_index = 0;
                self.cursor_visible = true;
                let target_sz = self.target_window_size();
                if window.bounds().size != target_sz {
                    window.resize(target_sz);
                }
                cx.notify();
            }
            _ => {}
        }
    }
}

impl Render for FunctionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_visible {
            return div().size_0().into_any_element();
        }

        let theme = self.theme;
        let is_listening = self.listening;
        let motif_state = MotifState::from_agent_state(&self.state, is_listening);

        if self.mode == FunctionMode::Settings {
            return div()
                .track_focus(&self.focus_handle)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.handle_key_down(event, window, cx);
                }))
                .w_full()
                .h_full()
                .child(render_settings_view(self, cx))
                .into_any_element();
        }

        if self.mode == FunctionMode::Conversations {
            let conv_list = self.conversation_store.list();
            return div()
                .track_focus(&self.focus_handle)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.handle_key_down(event, window, cx);
                }))
                .w_full()
                .h_full()
                .child(render_conversation_view(
                    &conv_list,
                    self.conversation_selected_index,
                    self.active_conversation_id.as_deref(),
                    &theme,
                    self.conversation_status_message.as_deref(),
                ))
                .into_any_element();
        }

        let has_query = !self.input_buffer.is_empty();
        let is_busy = matches!(
            self.state,
            AgentState::Processing { .. }
                | AgentState::Acting { .. }
                | AgentState::WaitingForConfirmation { .. }
        );
        let has_chat = !self.chat_display.is_empty();
        let is_focused = self.focus_handle.is_focused(window);

        let target_sz = self.target_window_size();
        if window.bounds().size != target_sz {
            window.resize(target_sz);
        }

        let bg_surface = Rgba {
            a: if cfg!(target_os = "macos") { 0.94 } else { 1.0 },
            ..theme.surface_base
        };

        let placeholder = if self.listening {
            "Listening... Speak clearly".to_string()
        } else if self.is_transcribing {
            "Transcribing speech with Whisper...".to_string()
        } else if let Some(ref err) = self.voice_error {
            format!("Voice error: {}. Type request", err)
        } else if has_chat {
            "Ask a follow-up…".to_string()
        } else {
            "Type a command or ask Function...".to_string()
        };

        let launcher_items = if has_query && !has_chat && !is_busy {
            get_launcher_items(&self.input_buffer)
        } else {
            Vec::new()
        };
        let selected_idx = if launcher_items.is_empty() {
            0
        } else {
            self.selected_index
                .min(launcher_items.len().saturating_sub(1))
        };

        div()
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, a: &SubmitRequest, window, cx| this.submit(a, window, cx)),
            )
            .on_action(cx.listener(|this, a: &ToggleExpanded, window, cx| {
                this.toggle_expanded(a, window, cx)
            }))
            .on_action(cx.listener(|this, a: &ToggleSpotlight, window, cx| {
                this.toggle_spotlight(a, window, cx)
            }))
            .on_action(cx.listener(|this, a: &CloseFunction, window, cx| this.close(a, window, cx)))
            .on_action(cx.listener(|this, a: &CancelTask, window, cx| this.cancel(a, window, cx)))
            .on_action(
                cx.listener(|this, a: &ToggleTheme, window, cx| this.toggle_theme(a, window, cx)),
            )
            .on_action(
                cx.listener(|this, a: &ToggleVoice, window, cx| this.toggle_voice(a, window, cx)),
            )
            .on_action(
                cx.listener(|this, a: &crate::actions::OpenSettings, window, cx| {
                    this.open_settings(a, window, cx)
                }),
            )
            .on_action(
                cx.listener(|_this, _a: &crate::actions::QuitFunction, _window, cx| {
                    cx.quit();
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_key_down(event, window, cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, _| {
                    this.focus_handle.focus(window);
                }),
            )
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .bg(bg_surface)
            .rounded_2xl()
            .shadow_xl()
            .overflow_hidden()
            .when(!self.chat_display.is_empty() || is_busy, |parent| {
                let chat_entries = self.chat_display.clone();
                let busy_state_text: Option<String> = if is_busy {
                    Some(match &self.state {
                        AgentState::Processing { thought_summary } => thought_summary
                            .clone()
                            .unwrap_or_else(|| "Thinking…".into()),
                        AgentState::Acting { action_description } => {
                            format!("→ {}", action_description)
                        }
                        AgentState::WaitingForConfirmation { action, .. } => {
                            format!("Confirm: {}", action)
                        }
                        _ => "Thinking…".into(),
                    })
                } else {
                    None
                };

                parent.child(
                    div()
                        .id("chat_scroll_area")
                        .track_scroll(&self.chat_scroll_handle)
                        .on_scroll_wheel(cx.listener(
                            |this, event: &gpui::ScrollWheelEvent, _, cx| {
                                if event.delta.pixel_delta(px(20.0)).y > px(0.0) {
                                    this.user_scrolled_up = true;
                                    cx.notify();
                                }
                            },
                        ))
                        .flex()
                        .flex_col()
                        .flex_1()
                        .overflow_y_scroll()
                        .px(px(20.0))
                        .pt(px(12.0))
                        .pb(px(8.0))
                        .gap(px(8.0))
                        .children(chat_entries.into_iter().enumerate().map(|(idx, entry)| {
                            if entry.is_user {
                                let user_bg = if theme.is_dark() {
                                    gpui::rgb(0x2f2c22)
                                } else {
                                    gpui::rgb(0xe5e1d8)
                                };
                                div()
                                    .id(("user_msg", idx))
                                    .flex_shrink_0()
                                    .flex()
                                    .justify_end()
                                    .child(
                                        div()
                                            .px(px(14.0))
                                            .py(px(8.0))
                                            .rounded_2xl()
                                            .bg(user_bg)
                                            .border_1()
                                            .border_color(Rgba {
                                                a: 0.18,
                                                ..theme.accent_primary
                                            })
                                            .shadow_sm()
                                            .max_w(px(460.0))
                                            .child(render_inline_text(&entry.text, &theme)),
                                    )
                                    .into_any_element()
                            } else {
                                if let Some(error_info) =
                                    crate::components::parse_error_info(&entry.text)
                                {
                                    let is_expanded = self.expanded_errors.contains(&idx);
                                    let raw_tech_details = error_info
                                        .formatted_json
                                        .clone()
                                        .unwrap_or_else(|| error_info.raw_text.clone());

                                    crate::components::render_error_card(
                                        &error_info,
                                        is_expanded,
                                        &theme,
                                        cx.listener(move |this, _, _, cx| {
                                            if this.expanded_errors.contains(&idx) {
                                                this.expanded_errors.remove(&idx);
                                            } else {
                                                this.expanded_errors.insert(idx);
                                            }
                                            this.play_sound_feedback(SoundEffect::Select);
                                            cx.notify();
                                        }),
                                        cx.listener(move |this, _, _, cx| {
                                            copy_to_clipboard(&raw_tech_details);
                                            this.play_sound_feedback(SoundEffect::Select);
                                            cx.notify();
                                        }),
                                    )
                                    .into_any_element()
                                } else {
                                    let reply_text = entry.text.clone();
                                    div()
                                        .id(("assistant_msg", idx))
                                        .flex_shrink_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .w_full()
                                        .max_w(px(580.0))
                                        .child(
                                            div()
                                                .id(("assistant_header", idx))
                                                .w_full()
                                                .flex_shrink_0()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .px_1()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(render_brand_mark_with_mode(
                                                            14.0,
                                                            theme.mode
                                                                == crate::theme::ThemeMode::Light,
                                                        ))
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_weight(
                                                                    gpui::FontWeight::SEMIBOLD,
                                                                )
                                                                .text_color(theme.accent_primary)
                                                                .child("Function"),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id(("copy_btn", idx))
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .gap_1()
                                                        .px_2()
                                                        .py_0p5()
                                                        .rounded_md()
                                                        .bg(theme.surface_input)
                                                        .border_1()
                                                        .border_color(theme.border_subtle)
                                                        .hover(|s| s.bg(theme.surface_active))
                                                        .text_xs()
                                                        .text_color(theme.text_secondary)
                                                        .child("Copy")
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            cx.listener(move |this, _, _, cx| {
                                                                copy_to_clipboard(&reply_text);
                                                                this.play_sound_feedback(
                                                                    SoundEffect::Select,
                                                                );
                                                                cx.notify();
                                                            }),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id(("assistant_body", idx))
                                                .w_full()
                                                .max_w_full()
                                                .flex_shrink_0()
                                                .px(px(4.0))
                                                .py(px(2.0))
                                                .overflow_hidden()
                                                .child(render_markdown(&entry.text, &theme)),
                                        )
                                        .into_any_element()
                                }
                            }
                        }))
                        .when_some(busy_state_text, |p, status| {
                            p.child(
                                div()
                                    .id("thinking_indicator")
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(render_brand_mark_with_mode(
                                                16.0,
                                                theme.mode == crate::theme::ThemeMode::Light,
                                            ))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(gpui::FontWeight::NORMAL)
                                                    .text_color(theme.text_muted)
                                                    .child(status),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("stop_response_btn")
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .bg(theme.surface_input)
                                            .border_1()
                                            .border_color(theme.border_subtle)
                                            .hover(|s| s.bg(theme.surface_active))
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.status_error)
                                            .child("Stop Response")
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.stop_response(cx);
                                                }),
                                            ),
                                    ),
                            )
                        })
                        .when(self.is_playing_audio && !is_busy, |p| {
                            p.child(
                                div()
                                    .id("audio_playing_indicator")
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(render_brand_mark_with_mode(
                                                16.0,
                                                theme.mode == crate::theme::ThemeMode::Light,
                                            ))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.accent_primary)
                                                    .child("Playing audio response..."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("stop_audio_btn")
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .bg(theme.surface_input)
                                            .border_1()
                                            .border_color(theme.border_subtle)
                                            .hover(|s| s.bg(theme.surface_active))
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.status_error)
                                            .child("Stop Audio")
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.stop_audio(cx);
                                                }),
                                            ),
                                    ),
                            )
                        }),
                )
            })
            .when(
                matches!(self.state, AgentState::WaitingForConfirmation { .. }),
                |p| {
                    if let AgentState::WaitingForConfirmation { action, details } = &self.state {
                        p.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .mx_6()
                                .my_2()
                                .p_3()
                                .rounded_lg()
                                .bg(theme.surface_elevated)
                                .border_1()
                                .border_color(theme.status_error)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(theme.status_error)
                                                .child(format!("CONFIRMATION: {}", action)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.text_muted)
                                                .child("Enter to allow  •  Esc to deny"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_secondary)
                                        .child(details.clone()),
                                ),
                        )
                    } else {
                        p
                    }
                },
            )
            .when(!self.chat_display.is_empty() || is_busy, |parent| {
                parent.child(div().h(px(2.0)))
            })
            .when(self.chat_display.is_empty() && !is_busy, |parent| {
                parent.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .pt_3()
                        .pb_1()
                        .cursor_pointer()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.focus_handle.focus(window);
                                this.toggle_voice(&ToggleVoice, window, cx);
                            }),
                        )
                        .child(render_function_motif(
                            motif_state,
                            &theme,
                            self.animation_tick,
                            48.0,
                        )),
                )
            })
            .child(
                div()
                    .id("command_input_bar")
                    .mx(px(14.0))
                    .mb(px(12.0))
                    .mt(px(4.0))
                    .min_h(px(52.0))
                    .max_h(px(160.0))
                    .px(px(14.0))
                    .py(px(8.0))
                    .rounded_xl()
                    .bg(if is_focused {
                        theme.surface_floating
                    } else {
                        theme.surface_input
                    })
                    .shadow_md()
                    .border_1()
                    .border_color(if is_focused {
                        Rgba {
                            a: 0.25,
                            ..theme.accent_primary
                        }
                    } else {
                        Rgba {
                            a: 0.05,
                            ..theme.accent_primary
                        }
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .flex_1()
                            .overflow_hidden()
                            .cursor_text()
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                    this.focus_handle.focus(window);
                                    this.is_text_selected = false;
                                    let char_count = this.input_buffer.chars().count();
                                    if char_count == 0 {
                                        this.cursor_offset = 0;
                                    } else {
                                        let click_x = f32::from(event.position.x) - 28.0;
                                        if click_x <= 0.0 {
                                            this.cursor_offset = 0;
                                        } else {
                                            let approx_idx = (click_x / 10.2).round() as usize;
                                            this.cursor_offset = approx_idx.min(char_count);
                                        }
                                    }
                                    this.cursor_visible = true;
                                    cx.notify();
                                }),
                            )
                            .child(if has_query {
                                let total_chars = self.input_buffer.chars().count();
                                let cursor_pos = self.cursor_offset.min(total_chars);
                                let (before_cursor, after_cursor) = {
                                    let mut before = String::new();
                                    let mut after = String::new();
                                    for (i, c) in self.input_buffer.chars().enumerate() {
                                        if i < cursor_pos {
                                            before.push(c);
                                        } else {
                                            after.push(c);
                                        }
                                    }
                                    (before, after)
                                };

                                div()
                                    .flex()
                                    .items_center()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(if self.is_text_selected {
                                        div()
                                            .bg(Rgba {
                                                a: 0.35,
                                                ..theme.accent_primary
                                            })
                                            .rounded_sm()
                                            .px_1()
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .font_weight(gpui::FontWeight::NORMAL)
                                                    .text_color(theme.text_primary)
                                                    .child(self.input_buffer.clone()),
                                            )
                                    } else {
                                        div()
                                            .flex()
                                            .items_center()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .overflow_hidden()
                                                    .font_weight(gpui::FontWeight::NORMAL)
                                                    .text_color(theme.text_primary)
                                                    .child(before_cursor),
                                            )
                                            .child(div().w(px(2.0)).h(px(20.0)).bg(
                                                if self.cursor_visible {
                                                    theme.text_primary
                                                } else {
                                                    rgba(0x00000000)
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .overflow_hidden()
                                                    .font_weight(gpui::FontWeight::NORMAL)
                                                    .text_color(theme.text_primary)
                                                    .child(after_cursor),
                                            )
                                    })
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(div().w(px(2.0)).h(px(20.0)).bg(
                                        if self.cursor_visible {
                                            theme.text_primary
                                        } else {
                                            rgba(0x00000000)
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_lg()
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .text_color(theme.text_muted)
                                            .child(placeholder),
                                    )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .when(has_query, |p| {
                                p.child(
                                    div()
                                        .cursor_pointer()
                                        .p_1()
                                        .rounded_full()
                                        .hover(|s| s.bg(theme.surface_active))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(|this, _, window, cx| {
                                                this.input_buffer.clear();
                                                this.is_text_selected = false;
                                                this.selected_index = 0;
                                                this.focus_handle.focus(window);
                                                window.resize(this.target_window_size());
                                                cx.notify();
                                            }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(theme.text_muted)
                                                .child("Close"),
                                        ),
                                )
                            })
                            .when(is_busy, |p| {
                                p.child(
                                    div()
                                        .id("bar_stop_response_btn")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.surface_input)
                                        .border_1()
                                        .border_color(theme.border_subtle)
                                        .hover(|s| s.bg(theme.surface_active))
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.status_error)
                                        .child("Stop Response")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.stop_response(cx);
                                            }),
                                        ),
                                )
                            })
                            .when(!is_busy && self.is_playing_audio, |p| {
                                p.child(
                                    div()
                                        .id("bar_stop_audio_btn")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.surface_input)
                                        .border_1()
                                        .border_color(theme.border_subtle)
                                        .hover(|s| s.bg(theme.surface_active))
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.status_error)
                                        .child("Stop Audio")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.stop_audio(cx);
                                            }),
                                        ),
                                )
                            })
                            .when(is_listening, |p| {
                                p.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.status_listening)
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.surface_base)
                                        .child("REC"),
                                )
                            })
                            .when(!is_listening, |p| {
                                p.child(
                                    div()
                                        .cursor_pointer()
                                        .p_1()
                                        .rounded_md()
                                        .hover(|s| s.bg(theme.surface_active))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(|this, _, window, cx| {
                                                this.toggle_settings(window, cx);
                                            }),
                                        )
                                        .child(settings_icon(15.0)),
                                )
                            }),
                    ),
            )
            .when(!launcher_items.is_empty(), |parent| {
                let mut list_container = div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .border_t_1()
                    .border_color(theme.surface_input)
                    .py_2()
                    .px_3();

                for (idx, item) in launcher_items.into_iter().enumerate().take(5) {
                    let is_sel = idx == selected_idx;
                    let icon_el = match item.icon_type {
                        LauncherIconType::Function => render_logo(16.0).into_any_element(),
                        LauncherIconType::Terminal => terminal_icon(16.0).into_any_element(),
                        LauncherIconType::Calculator => calculator_icon(16.0).into_any_element(),
                        LauncherIconType::Web => web_icon(16.0).into_any_element(),
                        LauncherIconType::Network => network_icon(16.0).into_any_element(),
                        LauncherIconType::Settings => settings_icon(16.0).into_any_element(),
                        LauncherIconType::App => app_icon(16.0).into_any_element(),
                        LauncherIconType::Folder => folder_icon(16.0).into_any_element(),
                        LauncherIconType::File => file_icon(16.0).into_any_element(),
                    };

                    let action_clone = item.action.clone();
                    let item_el = div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .h(px(46.0))
                        .px_3()
                        .rounded_lg()
                        .cursor_pointer()
                        .bg(if is_sel {
                            theme.surface_active
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(|s| s.bg(theme.surface_input))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.focus_handle.focus(window);
                                this.execute_launcher_action(action_clone.clone(), window, cx);
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .flex_1()
                                .child(icon_el)
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(if is_sel {
                                                    gpui::FontWeight::SEMIBOLD
                                                } else {
                                                    gpui::FontWeight::NORMAL
                                                })
                                                .text_color(if is_sel {
                                                    theme.text_primary
                                                } else {
                                                    theme.text_secondary
                                                })
                                                .child(item.keyword),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.text_muted)
                                                .child(item.description),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(theme.surface_input)
                                .text_xs()
                                .text_color(theme.text_muted)
                                .child(item.shortcut),
                        );

                    list_container = list_container.child(item_el);
                }

                parent.child(list_container)
            })
            .into_any_element()
    }
}
