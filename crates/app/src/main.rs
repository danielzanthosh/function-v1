//! Function Desktop Application Entry Point.
//!
//! Initializes logging, loads configuration, instantiates platform services and the
//! core agent, and launches the native GPUI application shell.
//!
//! If the microphone is not configured or unavailable, automatically opens a Spotlight-like
//! command bar for fast keyboard-first interaction.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use function_config::AppConfig;
use function_platform::{create_native_platform_service, PlatformCommand, PlatformService};
use function_ui::{
    CloseFunction, FunctionView, OpenSettings, QuitFunction, SubmitRequest, ToggleExpanded,
    ToggleSpotlight, ToggleTheme, ToggleVoice,
};

mod providers;
use gpui::{
    px, AppContext, Application, Bounds, KeyBinding, Point, Size, WindowBackgroundAppearance,
    WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};

#[cfg(not(target_os = "macos"))]
use gpui::TitlebarOptions;

fn main() {
    if !function_platform::ensure_single_instance() {
        eprintln!("Function is already running. Exiting duplicate instance.");
        return;
    }

    // 1. Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,function=debug,gpui::platform::windows::directx_devices=off".into()),
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
    let platform: std::sync::Arc<dyn PlatformService> = std::sync::Arc::from(create_native_platform_service());
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

    // Register global hotkey (Windows uses native RegisterHotKey; macOS uses global Double Command)
    #[cfg(not(target_os = "macos"))]
    {
        let hotkey_shortcut = config.hotkey.clone();
        let register_res = runtime.block_on(platform.register_hotkey(&hotkey_shortcut));
        if let Err(e) = register_res {
            tracing::warn!(error = %e, "Failed to register global hotkey");
        }
    }

    let initial_size = Size::new(px(640.0), px(112.0));

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

    let provider = providers::build_llm_provider(&config);
    let stt_provider = providers::build_stt_provider(&config);
    let tts_provider = providers::build_tts_provider(&config);

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
            KeyBinding::new("cmd-,", OpenSettings, None),
            KeyBinding::new("ctrl-,", OpenSettings, None),
            KeyBinding::new("cmd-q", QuitFunction, None),
        ]);

        // Use GPUI's active primary display instead of a fixed 1920x1080
        // fallback. This respects Retina/logical points, display scaling, and
        // non-standard monitor dimensions on macOS, Windows, and Linux.
        let display_bounds = cx
            .primary_display()
            .map(|display| display.bounds())
            .unwrap_or_else(|| Bounds {
                origin: Point::new(px(0.0), px(0.0)),
                size: Size::new(px(1920.0), px(1080.0)),
            });
        let screen_w = f32::from(display_bounds.size.width);
        let screen_h = f32::from(display_bounds.size.height);
        let screen_x = f32::from(display_bounds.origin.x);
        let screen_y = f32::from(display_bounds.origin.y);

        let is_upper_third =
            config.window_position == function_config::WindowPositionMode::UpperThird;
        let init_w_f32 = f32::from(initial_size.width);
        let init_h_f32 = f32::from(initial_size.height);
        let origin_x = screen_x + ((screen_w - init_w_f32) / 2.0).max(0.0);
        let origin_y = if is_upper_third {
            screen_y + ((screen_h - init_h_f32) / 3.0).max(0.0)
        } else {
            screen_y + ((screen_h - init_h_f32) / 2.0).max(0.0)
        };

        // Platform-specific window configuration:
        let start_hidden =
            config.start_hidden && !std::env::args().any(|arg| arg == "--show" || arg == "-s");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = start_hidden;

        // macOS: Fully frameless (no titlebar, no traffic lights) with an opaque surface
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
                        appears_transparent: false,
                        traffic_light_position: None,
                    })
                }
            },
            is_resizable: false,
            focus: {
                #[cfg(target_os = "macos")]
                {
                    !start_hidden
                }
                #[cfg(not(target_os = "macos"))]
                {
                    true
                }
            },
            show: {
                #[cfg(target_os = "macos")]
                {
                    !start_hidden
                }
                #[cfg(not(target_os = "macos"))]
                {
                    true
                }
            },
            // macOS uses a translucent native surface so the top-level rounded
            // command shell can provide a restrained Liquid Glass treatment.
            // Windows/other keeps the transparent native background for rounded corners.
            window_background: {
                #[cfg(target_os = "macos")]
                {
                    WindowBackgroundAppearance::Transparent
                }
                #[cfg(not(target_os = "macos"))]
                {
                    WindowBackgroundAppearance::Transparent
                }
            },
            window_decorations: Some(WindowDecorations::Client),
            kind: WindowKind::Normal,
            ..Default::default()
        };

        let agent_clone = agent.clone();
        let config_clone = config.clone();
        let audio_capture_clone = audio_capture.clone();
        let stt_provider_clone = stt_provider.clone();
        let _window = cx.open_window(window_options, move |window, cx| {
            // Keep a stable native marker even though the macOS titlebar is hidden.
            // Platform activation uses this marker to target only Function's window.
            window.set_window_title("Function");
            cx.new(|cx| {
                FunctionView::new(cx, mic_configured, mic_available)
                    .with_agent(agent_clone, cx)
                    .with_config(config_clone)
                    .with_audio_capture(audio_capture_clone)
                    .with_stt_provider(stt_provider_clone)
                    .with_tts_provider(tts_provider.clone())
                    .observe_activation(window, cx)
            })
        });
        let (command_tx, mut command_rx) =
            tokio::sync::broadcast::channel::<PlatformCommand>(16);
        function_platform::set_global_command_tx(command_tx);

        if let Ok(window_handle) = _window {
            let mut hotkey_rx = platform.subscribe_hotkey();
            let handle_clone = window_handle.clone();
            let platform_keepalive = platform.clone();
            cx.spawn(move |cx: &mut gpui::AsyncApp| {
                let cx = cx.clone();
                let _platform = platform_keepalive;
                async move {
                    let _platform_guard = _platform;
                    tracing::info!("GPUI event listener started, awaiting hotkey & platform commands");
                    loop {
                        tokio::select! {
                            hotkey_res = hotkey_rx.recv() => {
                                match hotkey_res {
                                    Ok(()) => {
                                        tracing::info!("GPUI global hotkey received");
                                        let update_res = cx.update(|cx| {
                                            let res = handle_clone.update(cx, |view, window, cx| {
                                                view.toggle_visibility_from_hotkey(window, cx);
                                            });
                                            if let Err(e) = res {
                                                tracing::error!(error = ?e, "Failed to update FunctionView on hotkey");
                                            }
                                        });
                                        if let Err(e) = update_res {
                                            tracing::error!(error = ?e, "Failed cx.update on hotkey");
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                            cmd_res = command_rx.recv() => {
                                match cmd_res {
                                    Ok(PlatformCommand::ToggleWindow) => {
                                        tracing::info!("PlatformCommand::ToggleWindow received");
                                        let update_res = cx.update(|cx| {
                                            let res = handle_clone.update(cx, |view, window, cx| {
                                                view.toggle_visibility_from_hotkey(window, cx);
                                            });
                                            if let Err(e) = res {
                                                tracing::error!(error = ?e, "Failed to update FunctionView on toggle");
                                            }
                                        });
                                        if let Err(e) = update_res {
                                            tracing::error!(error = ?e, "Failed cx.update on toggle");
                                        }
                                    }
                                    Ok(PlatformCommand::DismissWindow) => {
                                        tracing::info!("PlatformCommand::DismissWindow received");
                                        let update_res = cx.update(|cx| {
                                            let res = handle_clone.update(cx, |view, window, cx| {
                                                if view.is_visible {
                                                    view.dismiss(window, cx);
                                                }
                                            });
                                            if let Err(e) = res {
                                                tracing::error!(error = ?e, "Failed to update FunctionView on dismiss");
                                            }
                                        });
                                        if let Err(e) = update_res {
                                            tracing::error!(error = ?e, "Failed cx.update on dismiss");
                                        }
                                    }
                                    Ok(PlatformCommand::OpenSettings) => {
                                        tracing::info!("PlatformCommand::OpenSettings received");
                                        let update_res = cx.update(|cx| {
                                            let res = handle_clone.update(cx, |view, window, cx| {
                                                view.open_settings(&OpenSettings, window, cx);
                                            });
                                            if let Err(e) = res {
                                                tracing::error!(error = ?e, "Failed to open settings on FunctionView");
                                            }
                                        });
                                        if let Err(e) = update_res {
                                            tracing::error!(error = ?e, "Failed cx.update on OpenSettings");
                                        }
                                    }
                                    Ok(PlatformCommand::Quit) => {
                                        tracing::info!("PlatformCommand::Quit received, shutting down");
                                        let _ = cx.update(|cx| {
                                            cx.quit();
                                        });
                                        std::process::exit(0);
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                    tracing::warn!("GPUI async listener ended");
                }
            })
            .detach();
        } else if let Err(ref e) = _window {
            tracing::error!(error = ?e, "Failed to open GPUI window on launch");
        }

        // On macOS, configure accessory policy so Function doesn't appear in the Dock,
        // add native top bar / status menu item, and register as modern SMAppService login item for auto-start.
        #[cfg(target_os = "macos")]
        {
            function_platform::set_macos_activation_policy_accessory();
            function_platform::setup_macos_menu_bar_icon();
            function_platform::setup_macos_double_command_listener();
            function_platform::register_macos_login_item();
        }

        // Ensure small and large native icons, tool window styling (no taskbar presence), and centering on Win32
        #[cfg(target_os = "windows")]
        {
            let init_w = init_w_f32 as i32;
            let init_h = init_h_f32 as i32;
            std::thread::spawn(move || {
                for _ in 0..15 {
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    if function_platform::has_window_by_title("Function") {
                        function_platform::set_window_icon_by_title("Function");
                        function_platform::set_window_as_tool_window_by_title("Function");
                        function_platform::center_window_by_title(
                            "Function",
                            init_w,
                            init_h,
                            is_upper_third,
                        );
                        if start_hidden {
                            function_platform::hide_window_by_title("Function");
                        }
                        break;
                    }
                }
            });
        }

        tracing::info!("Function window launched");
    });
}
