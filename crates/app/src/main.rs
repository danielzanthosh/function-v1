//! Function Desktop Application Entry Point.
//!
//! Initializes logging, loads configuration, instantiates platform services and the
//! core agent, and launches the native GPUI application shell.
//!
//! If the microphone is not configured or unavailable, automatically opens a Spotlight-like
//! command bar for fast keyboard-first interaction.

use function_config::AppConfig;
use function_platform::create_native_platform_service;
use function_ui::{
    CloseFunction, FunctionView, SubmitRequest, ToggleExpanded, ToggleSpotlight, ToggleTheme,
    ToggleVoice,
};
use gpui::{
    px, AppContext, Application, Bounds, KeyBinding, Point, Size, WindowBackgroundAppearance,
    WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};

#[cfg(not(target_os = "macos"))]
use gpui::TitlebarOptions;

fn main() {
    // 1. Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,function=debug".into()),
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
    tracing::info!(
        platform = platform.platform_name(),
        "Platform service initialized"
    );

    // Initialize persistent multi-threaded Tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    function_ui::set_runtime_handle(runtime.handle().clone());
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

    let initial_size = Size::new(px(640.0), px(64.0));

    // 4. Initialize Tools, Memory, Provider, and Agent
    let mut tools = function_tools::ToolRegistry::new();
    function_tools::register_default_tools(&mut tools);
    tracing::info!("Registered {} default computer tools", tools.list().len());

    let memory_file = function_config::memory_path();
    let memory: std::sync::Arc<dyn function_memory::MemoryStore> =
        match function_memory::FileMemoryStore::new(&memory_file) {
            Ok(store) => {
                tracing::info!(path = %memory_file.display(), "Persistent file memory store initialized");
                std::sync::Arc::new(store)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Falling back to in-memory store");
                std::sync::Arc::new(function_memory::InMemoryMemoryStore::new())
            }
        };

    let credentials = function_config::InMemoryCredentialStore::new();
    let api_key = config.ai_provider.resolve_api_key(&credentials);

    let provider: std::sync::Arc<dyn function_providers::LlmProvider> = if config
        .ai_provider
        .is_configured()
    {
        tracing::info!(model = %config.ai_provider.model, "Using configured OpenAI-compatible LLM provider");
        std::sync::Arc::new(function_providers::OpenAiLlmProvider::new(
            &config.ai_provider.base_url,
            api_key.clone(),
            &config.ai_provider.model,
        ))
    } else {
        tracing::info!("AI provider not configured with API key: Using Mock/Standby LLM provider");
        std::sync::Arc::new(function_providers::MockLlmProvider::new(
            "Function computer assistant ready. Configure your API key in settings or run computer tools directly."
        ))
    };

    let stt_provider: std::sync::Arc<dyn function_providers::SpeechToTextProvider> =
        if config.ai_provider.is_configured() {
            tracing::info!("Using Whisper STT provider");
            std::sync::Arc::new(function_providers::WhisperSttProvider::new(
                &config.ai_provider.base_url,
                api_key,
            ))
        } else {
            tracing::info!("Using Mock STT provider (fallback)");
            std::sync::Arc::new(function_providers::MockSttProvider::new(
                "Open my browser and navigate to YouTube",
            ))
        };

    let audio_capture = platform.audio_capture();
    let agent = std::sync::Arc::new(function_agent::Agent::new(provider, tools, memory));

    // 5. Launch GPUI desktop application
    Application::new().run(move |cx| {
        // Register default global keybindings
        cx.bind_keys([
            KeyBinding::new("enter", SubmitRequest, None),
            KeyBinding::new("tab", ToggleExpanded, None),
            KeyBinding::new("escape", CloseFunction, None),
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

        let is_upper_third =
            config.window_position == function_config::WindowPositionMode::UpperThird;
        let init_w_f32 = f32::from(initial_size.width);
        let init_h_f32 = f32::from(initial_size.height);
        let origin_x = ((screen_w - init_w_f32) / 2.0).max(0.0);
        let origin_y = if is_upper_third {
            ((screen_h - init_h_f32) * 0.38).max(0.0)
        } else {
            ((screen_h - init_h_f32) / 2.0).max(0.0)
        };

        // Platform-specific window configuration:
        // macOS: Fully frameless (no titlebar, no traffic lights) with vibrancy
        // Windows: Transparent titlebar with client-side decorations
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(px(origin_x), px(origin_y)),
                size: initial_size,
            })),
            // macOS: None removes the titlebar and traffic lights entirely.
            // Windows: Transparent titlebar preserves window management without visible chrome.
            titlebar: {
                #[cfg(target_os = "macos")]
                {
                    None
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Some(TitlebarOptions {
                        title: Some("Function".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    })
                }
            },
            is_resizable: false,
            // macOS: Blurred gives subtle NSVisualEffectView vibrancy behind
            // semi-transparent content, matching the native Spotlight aesthetic.
            // Windows/other: Transparent removes the opaque native background
            // that would otherwise show through rounded corners.
            window_background: {
                #[cfg(target_os = "macos")]
                {
                    WindowBackgroundAppearance::Blurred
                }
                #[cfg(not(target_os = "macos"))]
                {
                    WindowBackgroundAppearance::Transparent
                }
            },
            window_decorations: Some(WindowDecorations::Client),
            kind: WindowKind::PopUp,
            ..Default::default()
        };

        let agent_clone = agent.clone();
        let config_clone = config.clone();
        let audio_capture_clone = audio_capture.clone();
        let stt_provider_clone = stt_provider.clone();
        let _window = cx.open_window(window_options, move |_window, cx| {
            cx.new(|cx| {
                FunctionView::new(cx, mic_configured, mic_available)
                    .with_agent(agent_clone, cx)
                    .with_config(config_clone)
                    .with_audio_capture(audio_capture_clone)
                    .with_stt_provider(stt_provider_clone)
            })
        });
        if let Ok(window_handle) = _window {
            let mut hotkey_rx = platform.subscribe_hotkey();
            let handle_clone = window_handle.clone();
            cx.spawn(move |cx: &mut gpui::AsyncApp| {
                let cx = cx.clone();
                async move {
                    while let Ok(()) = hotkey_rx.recv().await {
                        let _ = cx.update(|cx| {
                            let _ = handle_clone.update(cx, |view, window, cx| {
                                view.toggle_visibility(window, cx);
                            });
                        });
                    }
                }
            })
            .detach();
        }

        // Ensure small and large native icons and centering are set on the Win32 window
        let init_w = init_w_f32 as i32;
        let init_h = init_h_f32 as i32;
        std::thread::spawn(move || {
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(60));
                function_platform::set_window_icon_by_title("Function");
                function_platform::center_window_by_title(
                    "Function",
                    init_w,
                    init_h,
                    is_upper_third,
                );
            }
        });

        tracing::info!("Function window launched");
    });
}
