//! Function Desktop Application Entry Point.
//!
//! Initializes logging, loads configuration, instantiates platform services and the
//! core agent, and launches the native GPUI application shell.
//!
//! If the microphone is not configured or unavailable, automatically opens a Spotlight-like
//! command bar for fast keyboard-first interaction.

use assistant_config::AppConfig;
use assistant_platform::create_native_platform_service;
use assistant_ui::{
    AssistantView, CloseAssistant, SubmitRequest, ToggleExpanded, ToggleSpotlight, ToggleTheme,
    ToggleVoice,
};
use gpui::{
    px, AppContext, Application, Bounds, KeyBinding, Point, Size, TitlebarOptions, WindowBounds,
    WindowOptions,
};

fn main() {
    // 1. Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,assistant=debug".into()),
        )
        .init();

    tracing::info!("Starting Function (Version 1)");

    // 2. Load application configuration
    let config = AppConfig::load();
    tracing::info!(
        hotkey = %config.hotkey,
        model = %config.ai_provider.model,
        speech_enabled = config.speech.enabled,
        sound_enabled = config.sound_enabled,
        "Configuration loaded"
    );

    // 3. Initialize native platform integration
    let platform = create_native_platform_service();
    tracing::info!(platform = platform.platform_name(), "Platform service initialized");

    // Initialize persistent multi-threaded Tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    assistant_ui::set_runtime_handle(runtime.handle().clone());
    let runtime = Box::leak(Box::new(runtime));

    // Check microphone availability and configuration
    let mic_available = runtime.block_on(platform.is_microphone_available());
    let mic_configured = config.speech.is_configured();

    // Register global hotkey
    let hotkey_shortcut = config.hotkey.clone();
    let register_res = runtime.block_on(platform.register_hotkey(&hotkey_shortcut));
    if let Err(e) = register_res {
        tracing::warn!(error = %e, "Failed to register global hotkey");
    }

    let initial_size = if !mic_configured || !mic_available {
        tracing::info!("Microphone unconfigured or unavailable: Opening Spotlight launcher.");
        Size::new(px(620.0), px(356.0))
    } else {
        tracing::info!("Microphone ready: Opening Compact floating assistant bar.");
        Size::new(px(680.0), px(56.0))
    };

    // 4. Initialize Tools, Memory, Provider, and Agent
    let mut tools = assistant_tools::ToolRegistry::new();
    assistant_tools::register_default_tools(&mut tools);
    tracing::info!("Registered {} default computer tools", tools.list().len());

    let memory_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join(".function"))
        .unwrap_or_else(|_| std::env::temp_dir().join(".function"));
    let memory_file = memory_dir.join("memory.json");
    let memory: std::sync::Arc<dyn assistant_memory::MemoryStore> =
        match assistant_memory::FileMemoryStore::new(&memory_file) {
            Ok(store) => {
                tracing::info!(path = %memory_file.display(), "Persistent file memory store initialized");
                std::sync::Arc::new(store)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Falling back to in-memory store");
                std::sync::Arc::new(assistant_memory::InMemoryMemoryStore::new())
            }
        };

    let credentials = assistant_config::InMemoryCredentialStore::new();
    let api_key = config.ai_provider.resolve_api_key(&credentials);

    let provider: std::sync::Arc<dyn assistant_providers::LlmProvider> = if config.ai_provider.is_configured() {
        tracing::info!(model = %config.ai_provider.model, "Using configured OpenAI-compatible LLM provider");
        std::sync::Arc::new(assistant_providers::OpenAiLlmProvider::new(
            &config.ai_provider.base_url,
            api_key.clone(),
            &config.ai_provider.model,
        ))
    } else {
        tracing::info!("AI provider not configured with API key: Using Mock/Standby LLM provider");
        std::sync::Arc::new(assistant_providers::MockLlmProvider::new(
            "Function computer assistant ready. Configure your API key in settings or run computer tools directly."
        ))
    };

    let stt_provider: std::sync::Arc<dyn assistant_providers::SpeechToTextProvider> = if config.ai_provider.is_configured() {
        tracing::info!("Using Whisper STT provider");
        std::sync::Arc::new(assistant_providers::WhisperSttProvider::new(
            &config.ai_provider.base_url,
            api_key,
        ))
    } else {
        tracing::info!("Using Mock STT provider (fallback)");
        std::sync::Arc::new(assistant_providers::MockSttProvider::new(
            "Open my browser and navigate to YouTube",
        ))
    };

    let audio_capture = platform.audio_capture();
    let agent = std::sync::Arc::new(assistant_agent::Agent::new(provider, tools, memory));

    // 5. Launch GPUI desktop application
    Application::new().run(move |cx| {
        // Register default global keybindings
        cx.bind_keys([
            KeyBinding::new("enter", SubmitRequest, None),
            KeyBinding::new("tab", ToggleExpanded, None),
            KeyBinding::new("escape", CloseAssistant, None),
            KeyBinding::new("ctrl-e", ToggleExpanded, None),
            KeyBinding::new("ctrl-s", ToggleSpotlight, None),
            KeyBinding::new("ctrl-t", ToggleTheme, None),
            KeyBinding::new("ctrl-m", ToggleVoice, None),
        ]);

        let (screen_w, screen_h) = {
            #[cfg(target_os = "windows")]
            unsafe {
                extern "system" {
                    fn GetSystemMetrics(nIndex: i32) -> i32;
                }
                (GetSystemMetrics(0) as f32, GetSystemMetrics(1) as f32)
            }
            #[cfg(not(target_os = "windows"))]
            (1920.0, 1080.0)
        };

        let is_upper_third = config.window_position == assistant_config::WindowPositionMode::UpperThird;
        let init_w_f32 = f32::from(initial_size.width);
        let init_h_f32 = f32::from(initial_size.height);
        let origin_x = ((screen_w - init_w_f32) / 2.0).max(0.0);
        let origin_y = if is_upper_third {
            ((screen_h - init_h_f32) * 0.38).max(0.0)
        } else {
            ((screen_h - init_h_f32) / 2.0).max(0.0)
        };

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(px(origin_x), px(origin_y)),
                size: initial_size,
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("Function".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            is_resizable: false,
            ..Default::default()
        };

        let agent_clone = agent.clone();
        let config_clone = config.clone();
        let audio_capture_clone = audio_capture.clone();
        let stt_provider_clone = stt_provider.clone();
        let _window = cx.open_window(window_options, move |_window, cx| {
            cx.new(|cx| {
                AssistantView::new(cx, mic_configured, mic_available)
                    .with_agent(agent_clone, cx)
                    .with_config(config_clone)
                    .with_audio_capture(audio_capture_clone)
                    .with_stt_provider(stt_provider_clone)
            })
        });

        // Ensure small and large native icons and centering are set on the Win32 window
        let init_w = init_w_f32 as i32;
        let init_h = init_h_f32 as i32;
        std::thread::spawn(move || {
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(60));
                assistant_platform::set_window_icon_by_title("Function");
                assistant_platform::center_window_by_title("Function", init_w, init_h, is_upper_third);
            }
        });

        tracing::info!("Function window launched");
    });
}
