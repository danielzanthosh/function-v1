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
    calculator_icon, network_icon, settings_icon, terminal_icon, web_icon,
};
use crate::components::spotlight_bar::{
    get_current_time_string, get_launcher_items, LauncherAction, LauncherIconType,
};
use crate::components::{render_brand_mark, render_logo, ActivityEntry, ActivityStatus};
use crate::local_commands::{resolve_local_command, LocalCommand};
use crate::theme::Theme;
use crate::views::render_settings_view;
use function_agent::AgentState;
use function_config::AppConfig;
use function_platform::{copy_to_clipboard, open_url, play_sound, SoundEffect};
use gpui::prelude::*;
use gpui::{
    div, px, rgba, AsyncApp, Context, FocusHandle, IntoElement, KeyDownEvent, Render, Rgba, Size,
    Task, Timer, WeakEntity, Window,
};
use std::time::Duration;

/// Display mode for the function window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionMode {
    /// Intelligent floating command layer
    Command,
    /// Settings view
    Settings,
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
    pub settings_sound_enabled: bool,
    pub settings_show_key: bool,
    pub settings_focused_field: usize,
    pub settings_status_message: Option<String>,
}

pub type AssistantView = FunctionView;

impl FunctionView {
    pub fn new(cx: &mut Context<Self>, mic_configured: bool, mic_available: bool) -> Self {
        let focus_handle = cx.focus_handle();
        let config = AppConfig::load();
        let settings_api_key = config.ai_provider.api_key.clone().unwrap_or_default();
        let settings_model = config.ai_provider.model.clone();
        let settings_base_url = config.ai_provider.base_url.clone();
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
            animation_tick: 0,
            mic_configured,
            mic_available,
            input_buffer: String::new(),
            active_task: None,
            state: AgentState::Idle,
            activities: Vec::new(),
            latest_result: None,
            listening: false,
            audio_capture: None,
            stt_provider: None,
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
            settings_sound_enabled,
            settings_show_key: false,
            settings_focused_field: 0,
            settings_status_message: None,
        }
    }

    pub fn observe_activation(mut self, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut activated_once = false;
        let _sub = cx.observe_window_activation(window, move |this, window, cx| {
            if window.is_window_active() {
                activated_once = true;
            } else if activated_once && this.is_visible {
                this.dismiss(window, cx);
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
                while let Ok(state) = rx.recv().await {
                    let state_clone = state.clone();
                    let res = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            match &state_clone {
                                AgentState::Processing { thought_summary } => {
                                    view.state = state_clone.clone();
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
                                    view.activities.push(ActivityEntry {
                                        step: view.activities.len() + 1,
                                        description: action_description.clone(),
                                        status: ActivityStatus::Running,
                                    });
                                }
                                AgentState::Completed { summary } => {
                                    view.state = state_clone.clone();
                                    view.latest_result = Some(summary.clone());
                                    for act in &mut view.activities {
                                        act.status = ActivityStatus::Done;
                                    }
                                    view.play_sound_feedback(SoundEffect::Success);
                                }
                                AgentState::Error { message } => {
                                    view.state = state_clone.clone();
                                    view.latest_result = Some(format!("Error: {}", message));
                                    if let Some(last) = view.activities.last_mut() {
                                        last.status = ActivityStatus::Failed;
                                    }
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
        self.agent = Some(agent);
        self
    }

    pub fn with_config(mut self, config: AppConfig) -> Self {
        self.settings_api_key = config.ai_provider.api_key.clone().unwrap_or_default();
        self.settings_model = config.ai_provider.model.clone();
        self.settings_base_url = config.ai_provider.base_url.clone();
        self.settings_sound_enabled = config.sound_enabled;
        self.theme = Theme::from_config(&config);
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

    pub fn play_sound_feedback(&self, effect: SoundEffect) {
        if self.config.sound_enabled {
            play_sound(effect);
        }
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
            // Idle state: Clean, minimal floating command layer with centered brand mark
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

    pub fn toggle_voice(&mut self, _: &ToggleVoice, _window: &mut Window, cx: &mut Context<Self>) {
        if self.listening {
            // Stop recording & begin transcription
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

                        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                            let cx = cx.clone();
                            async move {
                                let result = if let Some(stt) = stt {
                                    stt.transcribe_audio(&wav_bytes, 16000).await
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
            // Start recording
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

    pub fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 1. If in Settings mode, go back to Command mode
        if self.mode == FunctionMode::Settings {
            tracing::info!("Escape: going back from Settings to Command mode");
            self.mode = FunctionMode::Command;
            window.resize(self.target_window_size());
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
            return;
        }

        // 2. If listening/recording voice, cancel recording and return to idle
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

        // 3. If text is selected, deselect it
        if self.is_text_selected {
            tracing::info!("Escape: deselecting input text");
            self.is_text_selected = false;
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
            return;
        }

        // 4. If there is typed text in the input buffer, clear it to go back to clean prompt
        if !self.input_buffer.is_empty() {
            tracing::info!("Escape: clearing input buffer to return to clean prompt");
            self.input_buffer.clear();
            self.selected_index = 0;
            self.cursor_visible = true;
            self.play_sound_feedback(SoundEffect::Select);
            window.resize(self.target_window_size());
            cx.notify();
            return;
        }

        // 5. If there is a task, result, or activity history showing, go back to fresh idle prompt
        if self.active_task.is_some()
            || self.latest_result.is_some()
            || !self.activities.is_empty()
            || self.state != AgentState::Idle
        {
            tracing::info!("Escape: clearing task/result/activities to return to clean idle prompt");
            self.active_task = None;
            self.latest_result = None;
            self.activities.clear();
            self.state = AgentState::Idle;
            self.voice_error = None;
            self.selected_index = 0;
            self.play_sound_feedback(SoundEffect::Select);
            window.resize(self.target_window_size());
            cx.notify();
            return;
        }

        // 6. Already at root idle prompt: do NOT hide the window
        tracing::debug!("Escape pressed at root prompt - window remains open");
    }

    pub fn close(&mut self, _: &CloseFunction, window: &mut Window, cx: &mut Context<Self>) {
        self.go_back(window, cx);
    }

    pub fn cancel(&mut self, _: &CancelTask, window: &mut Window, cx: &mut Context<Self>) {
        self.go_back(window, cx);
    }

    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        tracing::info!("🌙 Dismissing Function window (hiding)");
        self.is_visible = false;
        self.play_sound_feedback(SoundEffect::Select);
        #[cfg(target_os = "macos")]
        function_platform::macos_hide_app();
        #[cfg(not(target_os = "macos"))]
        function_platform::hide_window_by_title("Function");
        cx.notify();
    }

    pub fn summon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_visible = true;
        self.is_text_selected = false;
        let target_size = self.target_window_size();
        tracing::info!(
            ?target_size,
            "✨ Summoning Function window (showing and focusing)"
        );
        tracing::info!("Activating Function window");
        self.input_buffer.clear();
        self.selected_index = 0;
        self.mode = FunctionMode::Command;
        self.play_sound_feedback(SoundEffect::Select);
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
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub fn toggle_visibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(
            current_visibility = self.is_visible,
            "Toggling Function window visibility"
        );
        if self.is_visible {
            self.dismiss(window, cx);
        } else {
            self.summon(window, cx);
        }
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
                let cmd_str = cmd.clone();
                std::thread::spawn(move || {
                    let _ = std::process::Command::new("powershell")
                        .args(["-NoProfile", "-Command", &cmd_str])
                        .spawn();
                });
                self.input_buffer.clear();
                self.active_task = Some(format!("Shell: {}", cmd));
                self.mode = FunctionMode::Command;
                window.resize(self.target_window_size());

                self.activities.clear();
                self.activities.push(ActivityEntry {
                    step: 1,
                    description: format!("Executing shell command: \"{}\"", cmd),
                    status: ActivityStatus::Done,
                });
                self.latest_result = Some(format!("Shell command \"{}\" executed.", cmd));
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
                self.mode = FunctionMode::Settings;
                window.resize(self.target_window_size());
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
            }
            LauncherAction::RunTask(prompt) => {
                self.input_buffer = prompt;
                self.submit(&SubmitRequest, window, cx);
            }
        }
    }

    pub fn submit(&mut self, _: &SubmitRequest, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = self.input_buffer.trim().to_string();
        if prompt.is_empty() {
            return;
        }

        // Local command resolver - intercepts BEFORE AI agent and does NOT require API key
        if let Some(cmd) = resolve_local_command(&prompt) {
            match cmd {
                LocalCommand::Configure => {
                    self.input_buffer.clear();
                    self.selected_index = 0;
                    self.mode = FunctionMode::Settings;
                    self.play_sound_feedback(SoundEffect::Select);
                    window.resize(self.target_window_size());
                    cx.notify();
                    return;
                }
            }
        }

        self.play_sound_feedback(SoundEffect::Execute);
        self.active_task = Some(prompt.clone());
        self.input_buffer.clear();
        self.cursor_visible = true;

        // Retain command surface and adapt window height to show execution progress
        self.mode = FunctionMode::Command;
        window.resize(self.target_window_size());

        self.activities.clear();
        self.activities.push(ActivityEntry {
            step: 1,
            description: format!("Interpreting task: \"{}\"", prompt),
            status: ActivityStatus::Done,
        });
        self.activities.push(ActivityEntry {
            step: 2,
            description: "Orchestrating computer capabilities".to_string(),
            status: ActivityStatus::Running,
        });

        if let Some(agent) = self.agent.clone() {
            self.state = AgentState::Processing {
                thought_summary: None,
            };
            self.activities.push(ActivityEntry {
                step: 2,
                description: "Running agent Observe-Think-Act cycle".to_string(),
                status: ActivityStatus::Running,
            });

            let prompt_clone = prompt.clone();
            let task = async move {
                let _ = agent.execute_task(&prompt_clone).await;
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
            self.state = AgentState::Acting {
                action_description: "Executing task".to_string(),
            };
            self.latest_result = Some(format!(
                "Task \"{}\" initiated. Computer tools ready.",
                prompt
            ));
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

        // ==========================================
        // SECURITY CONFIRMATION INTERCEPTOR
        // ==========================================
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

        // ==========================================
        // SETTINGS MODE KEY HANDLING
        // ==========================================
        if self.mode == FunctionMode::Settings {
            match key {
                "escape" => {
                    self.go_back(window, cx);
                    return;
                }
                "tab" => {
                    self.settings_focused_field = (self.settings_focused_field + 1) % 3;
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "enter" => {
                    // Save and apply settings
                    self.config.ai_provider.api_key = if self.settings_api_key.trim().is_empty() {
                        None
                    } else {
                        Some(self.settings_api_key.trim().to_string())
                    };
                    self.config.ai_provider.model = self.settings_model.trim().to_string();
                    self.config.ai_provider.base_url = self.settings_base_url.trim().to_string();
                    self.config.sound_enabled = self.settings_sound_enabled;
                    self.theme = Theme::from_config(&self.config);

                    match self.config.save() {
                        Ok(_) => {
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
                    let text = match self.settings_focused_field {
                        0 => &self.settings_api_key,
                        1 => &self.settings_model,
                        2 => &self.settings_base_url,
                        _ => "",
                    };
                    if !text.is_empty() {
                        copy_to_clipboard(text);
                        self.play_sound_feedback(SoundEffect::Select);
                    }
                    return;
                }
                "x" if modifiers.secondary() => {
                    let text = match self.settings_focused_field {
                        0 => std::mem::take(&mut self.settings_api_key),
                        1 => std::mem::take(&mut self.settings_model),
                        2 => std::mem::take(&mut self.settings_base_url),
                        _ => String::new(),
                    };
                    if !text.is_empty() {
                        copy_to_clipboard(&text);
                        self.play_sound_feedback(SoundEffect::Select);
                        cx.notify();
                    }
                    return;
                }
                "a" if modifiers.secondary() => {
                    match self.settings_focused_field {
                        0 => self.settings_api_key.clear(),
                        1 => self.settings_model.clear(),
                        2 => self.settings_base_url.clear(),
                        _ => {}
                    }
                    self.play_sound_feedback(SoundEffect::Select);
                    cx.notify();
                    return;
                }
                "v" if modifiers.secondary() => {
                    if let Some(text) = cx
                        .read_from_clipboard()
                        .and_then(|clipboard| clipboard.text())
                    {
                        match self.settings_focused_field {
                            0 => self.settings_api_key.push_str(&text),
                            1 => self.settings_model.push_str(&text),
                            2 => self.settings_base_url.push_str(&text),
                            _ => {}
                        }
                        cx.notify();
                    }
                    return;
                }
                "backspace" => {
                    match self.settings_focused_field {
                        0 => {
                            self.settings_api_key.pop();
                        }
                        1 => {
                            self.settings_model.pop();
                        }
                        2 => {
                            self.settings_base_url.pop();
                        }
                        _ => {}
                    }
                    cx.notify();
                    return;
                }
                "space" => {
                    match self.settings_focused_field {
                        0 => self.settings_api_key.push(' '),
                        1 => self.settings_model.push(' '),
                        2 => self.settings_base_url.push(' '),
                        _ => {}
                    }
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
                ch if ch.len() == 1 && !modifiers.control && !modifiers.alt => {
                    match self.settings_focused_field {
                        0 => self.settings_api_key.push_str(ch),
                        1 => self.settings_model.push_str(ch),
                        2 => self.settings_base_url.push_str(ch),
                        _ => {}
                    }
                    cx.notify();
                    return;
                }
                _ => {}
            }
            return;
        }

        // ==========================================
        // TEXT SELECTION & CLIPBOARD SHORTCUTS
        // ==========================================
        if key == "a" && modifiers.secondary() {
            if !self.input_buffer.is_empty() {
                self.is_text_selected = true;
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
            }
            return;
        }

        if key == "c" && modifiers.secondary() {
            let text_to_copy = if !self.input_buffer.is_empty() {
                self.input_buffer.as_str()
            } else if let Some(ref res) = self.latest_result {
                res.as_str()
            } else {
                ""
            };
            if !text_to_copy.is_empty() {
                copy_to_clipboard(text_to_copy);
                self.play_sound_feedback(SoundEffect::Select);
                cx.notify();
            }
            return;
        }

        if key == "x" && modifiers.secondary() {
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

        if key == "v" && modifiers.secondary() {
            if let Some(text) = cx
                .read_from_clipboard()
                .and_then(|clipboard| clipboard.text())
            {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.is_text_selected = false;
                }
                self.input_buffer.push_str(&text);
                self.selected_index = 0;
                self.cursor_visible = true;
                window.resize(self.target_window_size());
                cx.notify();
            }
            return;
        }

        if modifiers.control && (key == "," || key == "settings") {
            self.toggle_settings(window, cx);
            return;
        }

        // ==========================================
        // GLOBAL HOTKEY DISMISS/SUMMON INTERCEPTOR (Ctrl+Space)
        // ==========================================
        if modifiers.control && (key == " " || key == "space") {
            self.dismiss(window, cx);
            return;
        }

        // ==========================================
        // SPOTLIGHT / COMPACT MODE KEY HANDLING
        // ==========================================
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
            "left" | "right" => {
                if self.is_text_selected {
                    self.is_text_selected = false;
                    cx.notify();
                }
            }
            "up" => {
                self.is_text_selected = false;
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.play_sound_feedback(SoundEffect::Navigate);
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
                }
            }
            "enter" => {
                self.is_text_selected = false;
                let prompt = self.input_buffer.trim().to_string();
                if let Some(cmd) = resolve_local_command(&prompt) {
                    match cmd {
                        LocalCommand::Configure => {
                            self.input_buffer.clear();
                            self.selected_index = 0;
                            self.mode = FunctionMode::Settings;
                            self.play_sound_feedback(SoundEffect::Select);
                            window.resize(self.target_window_size());
                            cx.notify();
                            return;
                        }
                    }
                }

                let items = get_launcher_items(&self.input_buffer);
                if let Some(item) = items.get(self.selected_index).cloned() {
                    self.execute_launcher_action(item.action, window, cx);
                } else if !prompt.is_empty() {
                    self.submit(&SubmitRequest, window, cx);
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
                self.input_buffer.pop();
                self.selected_index = 0;
                self.cursor_visible = true;
                let target_sz = self.target_window_size();
                if window.bounds().size != target_sz {
                    window.resize(target_sz);
                }
                cx.notify();
            }
            "space" => {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.is_text_selected = false;
                }
                self.input_buffer.push(' ');
                self.selected_index = 0;
                self.cursor_visible = true;
                let target_sz = self.target_window_size();
                if window.bounds().size != target_sz {
                    window.resize(target_sz);
                }
                cx.notify();
            }
            ch if ch.len() == 1 && !modifiers.control && !modifiers.alt => {
                if self.is_text_selected {
                    self.input_buffer.clear();
                    self.is_text_selected = false;
                }
                self.input_buffer.push_str(ch);
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                .child(render_settings_view(
                    &self.settings_api_key,
                    &self.settings_model,
                    &self.settings_base_url,
                    self.settings_sound_enabled,
                    self.config.theme_style,
                    self.config.accent_color,
                    self.config.window_position,
                    self.settings_show_key,
                    self.settings_focused_field,
                    self.cursor_visible,
                    self.settings_status_message.as_deref(),
                    &theme,
                ))
                .into_any_element();
        }

        // ==========================================
        // UNIFIED FUNCTION COMMAND LAYER
        // ==========================================
        let has_query = !self.input_buffer.is_empty();
        let has_task = self.active_task.is_some();
        let has_result = self.latest_result.is_some();
        let is_busy = matches!(
            self.state,
            AgentState::Processing { .. }
                | AgentState::Acting { .. }
                | AgentState::WaitingForConfirmation { .. }
        );

        let bg_surface = Rgba {
            a: 1.0,
            ..theme.surface_base
        };

        let placeholder = if self.listening {
            "Listening... Speak clearly".to_string()
        } else if self.is_transcribing {
            "Transcribing speech with Whisper...".to_string()
        } else if let Some(ref err) = self.voice_error {
            format!("Voice error: {}. Type request", err)
        } else {
            "Type a command or ask Function...".to_string()
        };

        let launcher_items = if has_query && !has_task && !has_result && !is_busy {
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
            .border_1()
            .border_color(rgba(0xf1f0ef1f))
            .shadow_xl()
            .overflow_hidden()
            // Upper area: conversational request, execution state, response
            .when(has_task || has_result || is_busy, |parent| {
                parent.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .overflow_hidden()
                        // Conversational user request element (lightweight & secondary)
                        .when(has_task, |p| {
                            let task_str = self.active_task.as_deref().unwrap_or("").to_string();
                            p.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .px_6()
                                    .pt_4()
                                    .pb_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text_muted)
                                            .child("User"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .text_color(theme.text_secondary)
                                            .child(task_str),
                                    ),
                            )
                        })
                        // Compact Function-native execution state
                        .when(is_busy, |p| {
                            let status_text = match &self.state {
                                AgentState::Processing { thought_summary } => thought_summary
                                    .clone()
                                    .unwrap_or_else(|| "Interpreting request".into()),
                                AgentState::Acting { action_description } => {
                                    format!("→ {}", action_description)
                                }
                                AgentState::WaitingForConfirmation { action, .. } => {
                                    format!("Confirmation needed: {}", action)
                                }
                                AgentState::Completed { .. } => "→ Ready".into(),
                                _ => "→ Ready".into(),
                            };

                            p.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .px_6()
                                    .py_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(render_brand_mark(18.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_color(theme.text_muted)
                                                    .child("FUNCTION"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .text_color(theme.text_secondary)
                                            .child(status_text),
                                    ),
                            )
                        })
                        // Security confirmation prompt
                        .when(
                            matches!(self.state, AgentState::WaitingForConfirmation { .. }),
                            |p| {
                                if let AgentState::WaitingForConfirmation { action, details } =
                                    &self.state
                                {
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
                                                            .child(format!(
                                                                "CONFIRMATION: {}",
                                                                action
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.text_muted)
                                                            .child(
                                                                "Enter to allow  •  Esc to deny",
                                                            ),
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
                        // Typography-led spacious response
                        .when(has_result, |p| {
                            let result_str =
                                self.latest_result.as_deref().unwrap_or("").to_string();
                            p.child(
                                div().flex_1().px_6().py_3().overflow_hidden().child(
                                    div()
                                        .text_base()
                                        .line_height(px(24.0))
                                        .font_weight(gpui::FontWeight::NORMAL)
                                        .text_color(theme.text_primary)
                                        .child(result_str),
                                ),
                            )
                        }),
                )
            })
            // Hairline separator before input if upper content exists
            .when(has_task || has_result || is_busy, |parent| {
                parent.child(div().w_full().h(px(1.0)).bg(theme.border_subtle))
            })
            // Brand mark header when idle
            .when(!has_task && !has_result && !is_busy, |parent| {
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
            // Dominant refined command input surface
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(56.0))
                    .px_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .flex_1()
                            .cursor_text()
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.focus_handle.focus(window);
                                    this.is_text_selected = false;
                                    cx.notify();
                                }),
                            )
                            .child(if has_query {
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_1()
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
                                            .text_lg()
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .text_color(theme.text_primary)
                                            .child(self.input_buffer.clone())
                                    })
                                    .child(div().w(px(2.0)).h(px(20.0)).bg(
                                        if self.cursor_visible && !self.is_text_selected {
                                            theme.text_primary
                                        } else {
                                            rgba(0x00000000)
                                        },
                                    ))
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_1()
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
                    // Right: Contextual micro triggers
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
                                                .child("✕"),
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
            // Launcher suggestions list (when typing query without active task)
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
