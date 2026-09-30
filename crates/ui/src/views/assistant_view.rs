//! Main assistant view supporting Compact, Spotlight, Expanded, and Settings modes.
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
    CancelTask, ClearInput, CloseAssistant, SubmitRequest, ToggleExpanded, ToggleSpotlight,
    ToggleTheme, ToggleVoice,
};
use crate::components::spotlight_bar::{get_launcher_items, LauncherAction};
use crate::components::{
    get_current_time_string, render_action_bar, render_activity_list, render_logo,
    render_spotlight_bar, render_status_badge, ActivityEntry, ActivityStatus,
};
use crate::theme::Theme;
use crate::views::render_settings_view;
use assistant_agent::AgentState;
use assistant_config::AppConfig;
use assistant_platform::{copy_to_clipboard, open_url, play_sound, SoundEffect};
use gpui::prelude::*;
use gpui::{
    div, px, rgba, AsyncApp, Context, FocusHandle, IntoElement, KeyDownEvent, Render, Size, Task,
    Timer, WeakEntity, Window,
};
use std::time::Duration;

/// Display mode for the assistant window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantMode {
    /// Compact floating bar with voice capability.
    Compact,
    /// Spotlight-like command bar (opened when mic is unconfigured or unavailable).
    Spotlight,
    /// Full workspace showing active task, computer action sequence, and results.
    Expanded,
    /// In-app API key and AI provider configuration screen.
    Settings,
}

pub struct AssistantView {
    pub mode: AssistantMode,
    pub mic_configured: bool,
    pub mic_available: bool,
    pub input_buffer: String,
    pub active_task: Option<String>,
    pub state: AgentState,
    pub activities: Vec<ActivityEntry>,
    pub latest_result: Option<String>,
    pub listening: bool,
    pub audio_capture: Option<std::sync::Arc<dyn assistant_platform::AudioCapture>>,
    pub stt_provider: Option<std::sync::Arc<dyn assistant_providers::SpeechToTextProvider>>,
    pub is_transcribing: bool,
    pub voice_error: Option<String>,
    pub theme: Theme,
    pub focus_handle: FocusHandle,
    pub agent: Option<std::sync::Arc<assistant_agent::Agent>>,
    pub cursor_visible: bool,
    pub selected_index: usize,
    pub current_time: String,
    pub _cursor_task: Option<Task<()>>,
    pub _agent_sub_task: Option<Task<()>>,
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

impl AssistantView {
    pub fn new(cx: &mut Context<Self>, mic_configured: bool, mic_available: bool) -> Self {
        let focus_handle = cx.focus_handle();
        // If microphone is not configured or unavailable, automatically launch in Spotlight mode
        let initial_mode = if !mic_configured || !mic_available {
            AssistantMode::Spotlight
        } else {
            AssistantMode::Compact
        };

        let config = AppConfig::load();
        let settings_api_key = config.ai_provider.api_key.clone().unwrap_or_default();
        let settings_model = config.ai_provider.model.clone();
        let settings_base_url = config.ai_provider.base_url.clone();
        let settings_sound_enabled = config.sound_enabled;

        // Spawn a background timer loop for standard native cursor blinking (~530ms) & real-time clock
        let cursor_task = cx.spawn(|this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                loop {
                    Timer::after(Duration::from_millis(530)).await;
                    let res = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.cursor_visible = !view.cursor_visible;
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

        Self {
            mode: initial_mode,
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
            current_time: get_current_time_string(),
            _cursor_task: Some(cursor_task),
            _agent_sub_task: None,
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

    pub fn with_agent(
        mut self,
        agent: std::sync::Arc<assistant_agent::Agent>,
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

    pub fn with_audio_capture(mut self, capture: std::sync::Arc<dyn assistant_platform::AudioCapture>) -> Self {
        self.audio_capture = Some(capture);
        self
    }

    pub fn with_stt_provider(mut self, stt: std::sync::Arc<dyn assistant_providers::SpeechToTextProvider>) -> Self {
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
        match self.mode {
            AssistantMode::Compact => Size {
                width: px(680.0),
                height: px(56.0),
            },
            AssistantMode::Spotlight => Size {
                width: px(620.0),
                height: px(356.0),
            },
            AssistantMode::Expanded => Size {
                width: px(680.0),
                height: px(520.0),
            },
            AssistantMode::Settings => Size {
                width: px(620.0),
                height: px(460.0),
            },
        }
    }

    pub fn toggle_expanded(&mut self, _: &ToggleExpanded, window: &mut Window, cx: &mut Context<Self>) {
        self.play_sound_feedback(SoundEffect::Select);
        self.mode = match self.mode {
            AssistantMode::Expanded => {
                if !self.mic_configured || !self.mic_available {
                    AssistantMode::Spotlight
                } else {
                    AssistantMode::Compact
                }
            }
            _ => AssistantMode::Expanded,
        };
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn toggle_spotlight(&mut self, _: &ToggleSpotlight, window: &mut Window, cx: &mut Context<Self>) {
        self.play_sound_feedback(SoundEffect::Select);
        self.mode = match self.mode {
            AssistantMode::Spotlight => AssistantMode::Compact,
            _ => AssistantMode::Spotlight,
        };
        window.resize(self.target_window_size());
        cx.notify();
    }

    pub fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.play_sound_feedback(SoundEffect::Select);
        self.mode = match self.mode {
            AssistantMode::Settings => {
                if !self.mic_configured || !self.mic_available {
                    AssistantMode::Spotlight
                } else {
                    AssistantMode::Compact
                }
            }
            _ => AssistantMode::Settings,
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
                            self.voice_error = Some("No speech detected. Type your request instead.".to_string());
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
                                    Err(assistant_providers::ProviderError::NotConfigured(
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
                                                        view.mode = AssistantMode::Expanded;
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
                    self.voice_error = Some(format!("Mic error: {}. Type your request directly.", e));
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

    pub fn close(&mut self, _: &CloseAssistant, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == AssistantMode::Expanded || self.mode == AssistantMode::Settings {
            self.mode = if !self.mic_configured || !self.mic_available {
                AssistantMode::Spotlight
            } else {
                AssistantMode::Compact
            };
            window.resize(self.target_window_size());
            self.play_sound_feedback(SoundEffect::Select);
            cx.notify();
        }
    }

    pub fn cancel(&mut self, _: &CancelTask, window: &mut Window, cx: &mut Context<Self>) {
        if !self.input_buffer.is_empty() {
            self.input_buffer.clear();
        } else if self.mode == AssistantMode::Expanded || self.mode == AssistantMode::Settings {
            self.mode = if !self.mic_configured || !self.mic_available {
                AssistantMode::Spotlight
            } else {
                AssistantMode::Compact
            };
            window.resize(self.target_window_size());
        }
        self.state = AgentState::Idle;
        self.listening = false;
        self.cursor_visible = true;
        self.play_sound_feedback(SoundEffect::Select);
        cx.notify();
    }

    pub fn clear_input(&mut self, _: &ClearInput, _window: &mut Window, cx: &mut Context<Self>) {
        self.input_buffer.clear();
        self.cursor_visible = true;
        cx.notify();
    }

    pub fn trigger_launcher_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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
                self.mode = AssistantMode::Expanded;
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
                self.mode = AssistantMode::Settings;
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

        self.play_sound_feedback(SoundEffect::Execute);
        self.active_task = Some(prompt.clone());
        self.input_buffer.clear();
        self.cursor_visible = true;

        // Switch to expanded workspace to show execution progress
        self.mode = AssistantMode::Expanded;
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

    pub fn handle_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        // ==========================================
        // SETTINGS MODE KEY HANDLING
        // ==========================================
        if self.mode == AssistantMode::Settings {
            match key {
                "escape" => {
                    self.mode = if !self.mic_configured || !self.mic_available {
                        AssistantMode::Spotlight
                    } else {
                        AssistantMode::Compact
                    };
                    window.resize(self.target_window_size());
                    self.play_sound_feedback(SoundEffect::Select);
                    cx.notify();
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
                            let sz = self.target_window_size();
                            assistant_platform::center_window_by_title(
                                "Function",
                                f32::from(sz.width) as i32,
                                f32::from(sz.height) as i32,
                                self.config.window_position == assistant_config::WindowPositionMode::UpperThird,
                            );
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
                        assistant_config::ThemeStyle::CarbonDark => assistant_config::ThemeStyle::ObsidianOled,
                        assistant_config::ThemeStyle::ObsidianOled => assistant_config::ThemeStyle::SlateMidnight,
                        assistant_config::ThemeStyle::SlateMidnight => assistant_config::ThemeStyle::StudioLight,
                        assistant_config::ThemeStyle::StudioLight => assistant_config::ThemeStyle::CarbonDark,
                    };
                    self.theme = Theme::from_config(&self.config);
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "a" if modifiers.control => {
                    self.config.accent_color = match self.config.accent_color {
                        assistant_config::AccentColor::White => assistant_config::AccentColor::Cyan,
                        assistant_config::AccentColor::Cyan => assistant_config::AccentColor::Emerald,
                        assistant_config::AccentColor::Emerald => assistant_config::AccentColor::Violet,
                        assistant_config::AccentColor::Violet => assistant_config::AccentColor::Amber,
                        assistant_config::AccentColor::Amber => assistant_config::AccentColor::White,
                    };
                    self.theme = Theme::from_config(&self.config);
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                    return;
                }
                "p" if modifiers.control => {
                    self.config.window_position = match self.config.window_position {
                        assistant_config::WindowPositionMode::Center => assistant_config::WindowPositionMode::UpperThird,
                        assistant_config::WindowPositionMode::UpperThird => assistant_config::WindowPositionMode::Center,
                    };
                    let sz = self.target_window_size();
                    assistant_platform::center_window_by_title(
                        "Function",
                        f32::from(sz.width) as i32,
                        f32::from(sz.height) as i32,
                        self.config.window_position == assistant_config::WindowPositionMode::UpperThird,
                    );
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
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
                "m" if modifiers.control => {
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
        // GLOBAL SETTINGS SHORTCUT (Ctrl+,)
        // ==========================================
        if modifiers.control && (key == "," || key == "settings") {
            self.toggle_settings(window, cx);
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
            "up" => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                }
            }
            "down" => {
                let items = get_launcher_items(&self.input_buffer);
                if self.selected_index + 1 < items.len() {
                    self.selected_index += 1;
                    self.play_sound_feedback(SoundEffect::Navigate);
                    cx.notify();
                }
            }
            "enter" => {
                let items = get_launcher_items(&self.input_buffer);
                if let Some(item) = items.get(self.selected_index).cloned() {
                    self.execute_launcher_action(item.action, window, cx);
                } else if !self.input_buffer.trim().is_empty() {
                    self.submit(&SubmitRequest, window, cx);
                }
            }
            "tab" => {
                self.toggle_expanded(&ToggleExpanded, window, cx);
            }
            "escape" => {
                self.cancel(&CancelTask, window, cx);
            }
            "backspace" => {
                self.input_buffer.pop();
                self.selected_index = 0;
                self.cursor_visible = true;
                cx.notify();
            }
            "space" => {
                self.input_buffer.push(' ');
                self.selected_index = 0;
                self.cursor_visible = true;
                cx.notify();
            }
            ch if ch.len() == 1 && !modifiers.control && !modifiers.alt => {
                self.input_buffer.push_str(ch);
                self.selected_index = 0;
                self.cursor_visible = true;
                cx.notify();
            }
            _ => {}
        }
    }
}

impl Render for AssistantView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let is_listening = self.listening;
        let mic_active = self.mic_configured && self.mic_available;

        match self.mode {
            AssistantMode::Spotlight => {
                // ==========================================
                // SPOTLIGHT COMMAND BAR (Flow Launcher Style)
                // ==========================================
                div()
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|this, a: &SubmitRequest, window, cx| this.submit(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleExpanded, window, cx| this.toggle_expanded(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleSpotlight, window, cx| this.toggle_spotlight(a, window, cx)))
                    .on_action(cx.listener(|this, a: &CloseAssistant, window, cx| this.close(a, window, cx)))
                    .on_action(cx.listener(|this, a: &CancelTask, window, cx| this.cancel(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleTheme, window, cx| this.toggle_theme(a, window, cx)))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key_down(event, window, cx);
                    }))
                    .w_full()
                    .h_full()
                    .child(render_spotlight_bar(
                        &self.input_buffer,
                        &theme,
                        self.selected_index,
                        self.cursor_visible,
                        &self.current_time,
                    ))
            }
            AssistantMode::Settings => {
                // ==========================================
                // SETTINGS VIEW (API Key & Provider Config)
                // ==========================================
                div()
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
            }
            AssistantMode::Compact => {
                // ==========================================
                // COMPACT FLOATING BAR (When mic is active)
                // ==========================================
                let is_placeholder = self.input_buffer.is_empty();
                let display_text = if self.listening {
                    "Listening... Speak clearly (Press Ctrl+M or click REC to finish)".to_string()
                } else if self.is_transcribing {
                    "Transcribing speech with Whisper...".to_string()
                } else if let Some(ref err) = self.voice_error {
                    format!("Speech note: {} (Type request)", err)
                } else if is_placeholder {
                    "Ask Function... (Alt+Space to focus, Tab to expand, Ctrl+M for voice)".to_string()
                } else {
                    self.input_buffer.clone()
                };

                div()
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|this, a: &SubmitRequest, window, cx| this.submit(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleExpanded, window, cx| this.toggle_expanded(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleSpotlight, window, cx| this.toggle_spotlight(a, window, cx)))
                    .on_action(cx.listener(|this, a: &CloseAssistant, window, cx| this.close(a, window, cx)))
                    .on_action(cx.listener(|this, a: &CancelTask, window, cx| this.cancel(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleTheme, window, cx| this.toggle_theme(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleVoice, window, cx| this.toggle_voice(a, window, cx)))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key_down(event, window, cx);
                    }))
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .h(px(56.0))
                    .px_4()
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .rounded_xl()
                    .shadow_lg()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .child(render_logo(20.0))
                            .child(render_status_badge(&self.state, &theme))
                            .child(if !is_placeholder {
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.text_primary)
                                            .child(display_text),
                                    )
                                    .child(
                                        div()
                                            .w(px(2.0))
                                            .h(px(14.0))
                                            .bg(if self.cursor_visible {
                                                theme.accent_primary
                                            } else {
                                                rgba(0x00000000)
                                            }),
                                    )
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_1()
                                    .child(
                                        div()
                                            .w(px(2.0))
                                            .h(px(14.0))
                                            .bg(if self.cursor_visible {
                                                theme.accent_primary
                                            } else {
                                                rgba(0x00000000)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.text_muted)
                                            .child(display_text),
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
                                    .flex()
                                    .items_center()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.85))
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                        this.toggle_voice(&ToggleVoice, window, cx);
                                    }))
                                    .bg(if is_listening {
                                        theme.status_listening
                                    } else {
                                        theme.surface_active
                                    })
                                    .text_xs()
                                    .text_color(if is_listening {
                                        theme.surface_base
                                    } else {
                                        theme.text_secondary
                                    })
                                    .child(if is_listening {
                                        "REC"
                                    } else if mic_active {
                                        "VOICE"
                                    } else {
                                        "MUTED"
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme.surface_active)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_xs()
                                    .text_color(theme.text_secondary)
                                    .hover(|s| s.bg(theme.surface_floating))
                                    .child("Expand"),
                            ),
                    )
            }
            AssistantMode::Expanded => {
                // ==========================================
                // EXPANDED WORKSPACE VIEW (520px)
                // ==========================================
                let is_placeholder = self.input_buffer.is_empty();
                let display_text = if self.listening {
                    "Listening... Speak clearly (Press Ctrl+M to transcribe)".to_string()
                } else if self.is_transcribing {
                    "Transcribing speech with Whisper...".to_string()
                } else if let Some(ref err) = self.voice_error {
                    format!("Note: {} (Type request)", err)
                } else if is_placeholder {
                    "Type a task or press Ctrl+M for voice...".to_string()
                } else {
                    self.input_buffer.clone()
                };

                div()
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|this, a: &SubmitRequest, window, cx| this.submit(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleExpanded, window, cx| this.toggle_expanded(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleSpotlight, window, cx| this.toggle_spotlight(a, window, cx)))
                    .on_action(cx.listener(|this, a: &CloseAssistant, window, cx| this.close(a, window, cx)))
                    .on_action(cx.listener(|this, a: &CancelTask, window, cx| this.cancel(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleTheme, window, cx| this.toggle_theme(a, window, cx)))
                    .on_action(cx.listener(|this, a: &ToggleVoice, window, cx| this.toggle_voice(a, window, cx)))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.handle_key_down(event, window, cx);
                    }))
                    .flex()
                    .flex_col()
                    .w_full()
                    .h_full()
                    .bg(theme.surface_base)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .rounded_xl()
                    .shadow_lg()
                    .overflow_hidden()
                    // Top Header Bar
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.border_subtle)
                            .bg(theme.surface_elevated)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(render_logo(16.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child("Function"),
                                    )
                                    .child(render_status_badge(&self.state, &theme)),
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
                                            .bg(theme.surface_active)
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .hover(|s| s.bg(theme.surface_floating))
                                            .child(if theme.is_dark() { "Light" } else { "Dark" }),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_sm()
                                            .bg(theme.surface_active)
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .hover(|s| s.bg(theme.surface_floating))
                                            .child("Compact"),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_sm()
                                            .bg(theme.surface_active)
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .hover(|s| s.bg(theme.surface_floating))
                                            .child("Esc"),
                                    ),
                            ),
                    )
                    // Middle Workspace Area
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .p_4()
                            .gap_3()
                            // Active Task Section
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child("FUNCTION INTENT"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.text_primary)
                                            .child(
                                                self.active_task
                                                    .as_deref()
                                                    .unwrap_or("No task running. Type an instruction below.")
                                                    .to_string(),
                                            ),
                                    ),
                            )
                            // Activity Timeline Section
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child("FUNCTION ACTIONS"),
                                    )
                                    .child(render_activity_list(&self.activities, &theme)),
                            )
                            // Result Container Section
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_muted)
                                            .child("RESULT / STATUS"),
                                    )
                                    .child(
                                        div()
                                            .p_3()
                                            .rounded_md()
                                            .bg(theme.surface_elevated)
                                            .border_1()
                                            .border_color(theme.border_subtle)
                                            .flex_1()
                                            .text_sm()
                                            .text_color(theme.text_secondary)
                                            .child(
                                                self.latest_result
                                                    .as_deref()
                                                    .unwrap_or("System ready for computer operation.")
                                                    .to_string(),
                                            ),
                                    ),
                            ),
                    )
                    // Bottom Input Area & Action Bar
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .px_4()
                            .py_3()
                            .border_t_1()
                            .border_color(theme.border_subtle)
                            .bg(theme.surface_elevated)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .p_2()
                                    .rounded_md()
                                    .bg(theme.surface_input)
                                    .border_1()
                                    .border_color(theme.border_focus)
                                    .child(if !is_placeholder {
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(theme.text_primary)
                                                    .child(display_text),
                                            )
                                            .child(
                                                div()
                                                    .w(px(2.0))
                                                    .h(px(14.0))
                                                    .bg(if self.cursor_visible {
                                                        theme.accent_primary
                                                    } else {
                                                        rgba(0x00000000)
                                                    }),
                                            )
                                    } else {
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .w(px(2.0))
                                                    .h(px(14.0))
                                                    .bg(if self.cursor_visible {
                                                        theme.accent_primary
                                                    } else {
                                                        rgba(0x00000000)
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(theme.text_muted)
                                                    .child(display_text),
                                            )
                                    })
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
                                                    .bg(if is_listening {
                                                        theme.status_listening
                                                    } else {
                                                        theme.surface_active
                                                    })
                                                    .text_xs()
                                                    .text_color(if is_listening {
                                                        theme.surface_base
                                                    } else {
                                                        theme.text_secondary
                                                    })
                                                    .child(if is_listening {
                                                        "REC"
                                                    } else if mic_active {
                                                        "VOICE"
                                                    } else {
                                                        "MUTED"
                                                    }),
                                            ),
                                    ),
                            )
                            .child(render_action_bar(&theme, true, is_listening)),
                    )
            }
        }
    }
}
