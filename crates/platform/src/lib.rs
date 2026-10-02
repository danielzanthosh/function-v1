//! Platform integration layer for Windows and macOS.
//!
//! Provides isolated, first-class native OS abstractions for global hotkeys,
//! accessibility permissions, microphone availability detection, and window control.

pub mod audio;
pub mod computer;
pub mod search;
pub use audio::*;
pub use computer::*;
pub use search::*;

use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::broadcast;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("Hotkey registration failed: {0}")]
    HotkeyRegistrationFailed(String),
    #[error("Permission denied for capability: {0}")]
    PermissionDenied(String),
    #[error("System API error: {0}")]
    SystemApi(String),
    #[error("Unsupported platform feature: {0}")]
    Unsupported(String),
}

/// Status of system permissions required by the desktop assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub screen_recording: bool,
    pub microphone: bool,
}

/// Unified platform service abstraction.
#[async_trait]
pub trait PlatformService: Send + Sync {
    /// Return the OS name / identifier.
    fn platform_name(&self) -> &'static str;

    /// Register the global activation hotkey (e.g. "Ctrl+Space" on Windows, "Command+;" on macOS).
    async fn register_hotkey(&self, shortcut: &str) -> Result<(), PlatformError>;

    /// Unregister any previously registered hotkey.
    async fn unregister_hotkey(&self) -> Result<(), PlatformError>;

    /// Subscribe to global hotkey activation events.
    fn subscribe_hotkey(&self) -> broadcast::Receiver<()>;

    /// Programmatically trigger a hotkey activation event (e.g. for testing or menu triggers).
    fn trigger_hotkey(&self);

    /// Query current system permission statuses.
    async fn check_permissions(&self) -> PermissionStatus;

    /// Request necessary permissions if missing.
    async fn request_permissions(&self) -> Result<PermissionStatus, PlatformError>;

    /// Check whether a microphone device is connected and accessible by the OS.
    async fn is_microphone_available(&self) -> bool;

    /// Return the active audio capture service.
    fn audio_capture(&self) -> Arc<dyn AudioCapture>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformCommand {
    ToggleWindow,
    OpenSettings,
    Quit,
}

static GLOBAL_HOTKEY_TX: std::sync::RwLock<Option<broadcast::Sender<()>>> =
    std::sync::RwLock::new(None);
static GLOBAL_COMMAND_TX: std::sync::RwLock<Option<broadcast::Sender<PlatformCommand>>> =
    std::sync::RwLock::new(None);

pub fn set_global_hotkey_tx(tx: broadcast::Sender<()>) {
    if let Ok(mut lock) = GLOBAL_HOTKEY_TX.write() {
        *lock = Some(tx);
    }
}

pub fn get_global_hotkey_tx() -> Option<broadcast::Sender<()>> {
    GLOBAL_HOTKEY_TX.read().ok().and_then(|lock| lock.clone())
}

pub fn set_global_command_tx(tx: broadcast::Sender<PlatformCommand>) {
    if let Ok(mut lock) = GLOBAL_COMMAND_TX.write() {
        *lock = Some(tx);
    }
}

pub fn get_global_command_tx() -> Option<broadcast::Sender<PlatformCommand>> {
    GLOBAL_COMMAND_TX.read().ok().and_then(|lock| lock.clone())
}

pub fn send_platform_command(cmd: PlatformCommand) {
    if let Some(tx) = get_global_command_tx() {
        let _ = tx.send(cmd);
    } else if cmd == PlatformCommand::ToggleWindow {
        if let Some(tx) = get_global_hotkey_tx() {
            let _ = tx.send(());
        }
    }
}

pub fn trigger_global_hotkey() {
    tracing::info!("Triggering global hotkey notification from UI / menu bar / tray");
    send_platform_command(PlatformCommand::ToggleWindow);
}

/// Ensure only a single instance of Function runs at a time.
/// Returns true if this process is the primary instance, or false if an existing instance is already running.
pub fn ensure_single_instance() -> bool {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn CreateMutexW(
                lpMutexAttributes: *mut std::ffi::c_void,
                bInitialOwner: i32,
                lpName: *const u16,
            ) -> isize;
            fn GetLastError() -> u32;
        }
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let name: Vec<u16> = OsStr::new("Local\\FunctionAssistantSingleInstanceMutex")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
            const ERROR_ALREADY_EXISTS: u32 = 183;
            if handle != 0 && GetLastError() == ERROR_ALREADY_EXISTS {
                tracing::warn!("Another instance of Function is already running. Activating existing instance and exiting.");
                show_window_by_title("Function");
                return false;
            }
        }
        true
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;

        extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let lock_dir = format!("{}/.function", home);
        let _ = std::fs::create_dir_all(&lock_dir);
        let lock_path = format!("{}/function.lock", lock_dir);

        // LOCK_EX = 2, LOCK_NB = 4
        const LOCK_EX: i32 = 2;
        const LOCK_NB: i32 = 4;

        if let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
        {
            let fd = file.as_raw_fd();
            unsafe {
                if flock(fd, LOCK_EX | LOCK_NB) != 0 {
                    tracing::warn!("Another instance of Function is already running on macOS. Activating existing instance and exiting.");
                    trigger_global_hotkey();
                    return false;
                }
            }
            // Retain the file handle so the advisory lock remains held for the process lifetime
            std::mem::forget(file);
        }
        true
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        true
    }
}

/// Speak text using the operating system's native speech facilities (TTS fallback).
pub fn speak_text(text: &str) {
    let clean_text = text.replace(['"', '\\', '\n', '\r'], " ");
    let text_owned = clean_text.trim().to_string();
    if text_owned.is_empty() {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(move || {
            let _ = std::process::Command::new("say")
                .arg(&text_owned)
                .spawn();
        });
    }

    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            let script = format!(
                "Add-Type -AssemblyName System.Speech; $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer; $synth.Speak('{}')",
                text_owned.replace('\'', "''")
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .spawn();
        });
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = text_owned;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacShortcutInfo {
    pub key_char: String,
    pub modifier_mask: usize,
    pub display_label: String,
}

/// Parse a hotkey string (e.g. "Command+;", "Option+Space", "Ctrl+Space") into AppKit key equivalent info.
pub fn parse_macos_shortcut(hotkey: &str) -> Option<MacShortcutInfo> {
    let mut modifier_mask = 0usize;
    let mut symbols = Vec::new();
    let mut key_char = String::new();
    let mut key_name = String::new();

    const NSEVENT_MODIFIER_FLAG_SHIFT: usize = 0x0002_0000;
    const NSEVENT_MODIFIER_FLAG_CONTROL: usize = 0x0004_0000;
    const NSEVENT_MODIFIER_FLAG_OPTION: usize = 0x0008_0000;
    const NSEVENT_MODIFIER_FLAG_COMMAND: usize = 0x0010_0000;

    for part in hotkey.split('+') {
        let trimmed = part.trim();
        match trimmed.to_lowercase().as_str() {
            "command" | "cmd" | "super" => {
                modifier_mask |= NSEVENT_MODIFIER_FLAG_COMMAND;
                symbols.push("⌘");
            }
            "option" | "alt" => {
                modifier_mask |= NSEVENT_MODIFIER_FLAG_OPTION;
                symbols.push("⌥");
            }
            "control" | "ctrl" => {
                modifier_mask |= NSEVENT_MODIFIER_FLAG_CONTROL;
                symbols.push("⌃");
            }
            "shift" => {
                modifier_mask |= NSEVENT_MODIFIER_FLAG_SHIFT;
                symbols.push("⇧");
            }
            "space" => {
                key_char = " ".to_string();
                key_name = "Space".to_string();
            }
            ";" | "semicolon" => {
                key_char = ";".to_string();
                key_name = ";".to_string();
            }
            "," | "comma" => {
                key_char = ",".to_string();
                key_name = ",".to_string();
            }
            other if !other.is_empty() => {
                let first = other.chars().next().unwrap();
                key_char = first.to_ascii_lowercase().to_string();
                key_name = first.to_ascii_uppercase().to_string();
            }
            _ => {}
        }
    }

    if key_char.is_empty() {
        return None;
    }

    if modifier_mask == 0 {
        modifier_mask = NSEVENT_MODIFIER_FLAG_COMMAND;
        symbols.push("⌘");
    }

    let display_label = format!("{}{}", symbols.concat(), key_name);
    Some(MacShortcutInfo {
        key_char,
        modifier_mask,
        display_label,
    })
}

#[cfg(target_os = "windows")]
pub mod windows {
    use super::*;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::{Arc, RwLock};

    pub struct WindowsPlatformService {
        registered_hotkey: Arc<RwLock<Option<String>>>,
        hotkey_tx: broadcast::Sender<()>,
        audio_capture: Arc<dyn AudioCapture>,
        worker_thread_id: Arc<AtomicU32>,
        worker_hwnd: Arc<AtomicIsize>,
    }

    impl WindowsPlatformService {
        pub fn new() -> Self {
            let (hotkey_tx, _) = broadcast::channel(16);
            set_global_hotkey_tx(hotkey_tx.clone());
            Self {
                registered_hotkey: Arc::new(RwLock::new(None)),
                hotkey_tx,
                audio_capture: Arc::new(CpalAudioCapture::new()),
                worker_thread_id: Arc::new(AtomicU32::new(0)),
                worker_hwnd: Arc::new(AtomicIsize::new(0)),
            }
        }

        pub fn with_audio_capture(mut self, audio: Arc<dyn AudioCapture>) -> Self {
            self.audio_capture = audio;
            self
        }

        fn cleanup_hotkey_sync(&self) {
            let thread_id = self.worker_thread_id.swap(0, Ordering::SeqCst);
            let hwnd = self.worker_hwnd.swap(0, Ordering::SeqCst);
            if hwnd != 0 {
                unsafe {
                    extern "system" {
                        fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize)
                            -> i32;
                    }
                    const WM_CLOSE: u32 = 0x0010;
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                }
            } else if thread_id != 0 {
                unsafe {
                    extern "system" {
                        fn PostThreadMessageW(
                            idThread: u32,
                            Msg: u32,
                            wParam: usize,
                            lParam: isize,
                        ) -> i32;
                    }
                    const WM_QUIT: u32 = 0x0012;
                    PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
                }
            }
        }
    }

    impl Drop for WindowsPlatformService {
        fn drop(&mut self) {
            self.cleanup_hotkey_sync();
        }
    }

    impl Default for WindowsPlatformService {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl PlatformService for WindowsPlatformService {
        fn platform_name(&self) -> &'static str {
            "Windows"
        }

        async fn register_hotkey(&self, shortcut: &str) -> Result<(), PlatformError> {
            {
                let mut current = self.registered_hotkey.write().unwrap();
                if let Some(ref existing) = *current {
                    if existing == shortcut {
                        tracing::info!(
                            shortcut,
                            "Hotkey already registered, skipping duplicate registration"
                        );
                        return Ok(());
                    }
                    self.cleanup_hotkey_sync();
                }
                *current = Some(shortcut.to_string());
            }

            tracing::info!(shortcut, "Registering Windows global hotkey");
            let tx = self.hotkey_tx.clone();
            let sc = shortcut.to_string();
            let worker_thread_id = self.worker_thread_id.clone();
            let worker_hwnd = self.worker_hwnd.clone();

            std::thread::spawn(move || {
                #[repr(C)]
                struct Point {
                    x: i32,
                    y: i32,
                }

                #[repr(C)]
                struct Msg {
                    hwnd: isize,
                    message: u32,
                    wparam: usize,
                    lparam: isize,
                    time: u32,
                    pt: Point,
                }

                #[repr(C)]
                struct NotifyIconDataW {
                    cb_size: u32,
                    hwnd: isize,
                    uid: u32,
                    uflags: u32,
                    ucallback_message: u32,
                    hicon: isize,
                    sztip: [u16; 128],
                    dwstate: u32,
                    dwstatemask: u32,
                    szinfo: [u16; 256],
                    utimeout_or_version: u32,
                    szinfotitle: [u16; 64],
                    dwinfoflags: u32,
                    guiditem: [u8; 16],
                    hballoonicon: isize,
                }

                type PfnShellNotifyIconW =
                    unsafe extern "system" fn(u32, *const NotifyIconDataW) -> i32;

                extern "system" {
                    fn GetCurrentThreadId() -> u32;
                    fn RegisterHotKey(hWnd: isize, id: i32, fsModifiers: u32, vk: u32) -> i32;
                    fn UnregisterHotKey(hWnd: isize, id: i32) -> i32;
                    fn GetMessageW(
                        lpMsg: *mut Msg,
                        hWnd: isize,
                        wMsgFilterMin: u32,
                        wMsgFilterMax: u32,
                    ) -> i32;
                    fn TranslateMessage(lpMsg: *const Msg) -> i32;
                    fn DispatchMessageW(lpMsg: *const Msg) -> isize;
                    fn LoadIconW(hInstance: isize, lpIconName: isize) -> isize;
                    fn LoadLibraryA(lpLibFileName: *const u8) -> isize;
                    fn GetProcAddress(hModule: isize, lpProcName: *const u8) -> *const ();
                    fn GetLastError() -> u32;
                }

                const MOD_ALT: u32 = 0x0001;
                const MOD_CONTROL: u32 = 0x0002;
                const MOD_SHIFT: u32 = 0x0004;
                const MOD_WIN: u32 = 0x0008;
                const MOD_NOREPEAT: u32 = 0x4000;
                const VK_SPACE: u32 = 0x20;
                const WM_HOTKEY: u32 = 0x0312;
                const WM_USER: u32 = 0x0400;
                const WM_TRAYICON: u32 = WM_USER + 101;
                const NIM_ADD: u32 = 0x00000000;
                const NIM_DELETE: u32 = 0x00000002;
                const NIF_MESSAGE: u32 = 0x00000001;
                const NIF_ICON: u32 = 0x00000002;
                const NIF_TIP: u32 = 0x00000004;
                const IDI_APPLICATION: isize = 32512;

                let tid = unsafe { GetCurrentThreadId() };
                worker_thread_id.store(tid, Ordering::SeqCst);

                let mut mods = MOD_NOREPEAT;
                let sc_upper = sc.to_uppercase();
                if sc_upper.contains("CTRL") || sc_upper.contains("CONTROL") {
                    mods |= MOD_CONTROL;
                }
                if sc_upper.contains("ALT") || sc_upper.contains("OPTION") {
                    mods |= MOD_ALT;
                }
                if sc_upper.contains("SHIFT") {
                    mods |= MOD_SHIFT;
                }
                if sc_upper.contains("WIN") || sc_upper.contains("SUPER") {
                    mods |= MOD_WIN;
                }
                if mods == MOD_NOREPEAT {
                    mods |= MOD_CONTROL;
                }

                let mut vk = VK_SPACE;
                for part in sc_upper.split('+').map(|p| p.trim()) {
                    match part {
                        "SPACE" => vk = 0x20,
                        ";" | "SEMICOLON" => vk = 0xBA, // VK_OEM_1 (;:)
                        "ESC" | "ESCAPE" => vk = 0x1B,
                        "TAB" => vk = 0x09,
                        k if k.len() == 1 => {
                            let ch = k.chars().next().unwrap();
                            if ch.is_ascii_alphanumeric() {
                                vk = ch as u32;
                            }
                        }
                        _ => {}
                    }
                }

                unsafe {
                    let mut reg_res = RegisterHotKey(0, 101, mods, vk);
                    if reg_res == 0 && (mods & MOD_NOREPEAT != 0) {
                        // Fallback without MOD_NOREPEAT for compatibility
                        reg_res = RegisterHotKey(0, 101, mods & !MOD_NOREPEAT, vk);
                    }
                    if reg_res == 0 && sc_upper.contains("ALT") {
                        tracing::warn!(
                            error_code = GetLastError(),
                            "Failed to register Alt-based hotkey (occupied by Windows system menu). Falling back to Ctrl+Space..."
                        );
                        let fallback_mods = MOD_CONTROL | MOD_NOREPEAT;
                        reg_res = RegisterHotKey(0, 101, fallback_mods, VK_SPACE);
                        if reg_res == 0 {
                            reg_res = RegisterHotKey(0, 101, MOD_CONTROL, VK_SPACE);
                        }
                        if reg_res != 0 {
                            tracing::info!("Fallback Windows global hotkey registered successfully: Ctrl+Space");
                        }
                    }
                    if reg_res == 0 {
                        tracing::warn!(
                            error_code = GetLastError(),
                            "Failed to register Windows global hotkey (might be occupied by another app). System tray icon remains active."
                        );
                    } else {
                        tracing::info!("Windows global hotkey registered successfully: {}", sc);
                    }

                    // Dynamically resolve Shell_NotifyIconW
                    let shell32 = LoadLibraryA(b"shell32.dll\0".as_ptr());
                    let pfn_notify: Option<PfnShellNotifyIconW> = if shell32 != 0 {
                        let proc = GetProcAddress(shell32, b"Shell_NotifyIconW\0".as_ptr());
                        if !proc.is_null() {
                            Some(std::mem::transmute(proc))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    extern "system" {
                        fn LoadImageW(
                            hInst: isize,
                            name: *const u16,
                            type_: u32,
                            cx: i32,
                            cy: i32,
                            fuLoad: u32,
                        ) -> isize;
                    }
                    const IMAGE_ICON: u32 = 1;
                    const LR_LOADFROMFILE: u32 = 0x0010;

                    let mut hicon = 0isize;
                    if let Some(icon_path) = find_icon_path() {
                        if icon_path.extension().and_then(|e| e.to_str()) == Some("ico") {
                            let wide_path: Vec<u16> = icon_path
                                .to_string_lossy()
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();
                            hicon = LoadImageW(
                                0,
                                wide_path.as_ptr(),
                                IMAGE_ICON,
                                16,
                                16,
                                LR_LOADFROMFILE,
                            );
                        }
                    }
                    if hicon == 0 {
                        hicon = LoadIconW(0, IDI_APPLICATION);
                    }

                    // Register system tray icon to keep background process accessible
                    let mut nid: NotifyIconDataW = std::mem::zeroed();
                    nid.cb_size = std::mem::size_of::<NotifyIconDataW>() as u32;
                    nid.hwnd = 0;
                    nid.uid = 1001;
                    nid.uflags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                    nid.ucallback_message = WM_TRAYICON;
                    nid.hicon = hicon;

                    let tip = format!("Function ({})\0", sc);
                    for (i, c) in tip.encode_utf16().enumerate().take(127) {
                        nid.sztip[i] = c;
                    }
                    if let Some(notify) = pfn_notify {
                        let res = notify(NIM_ADD, &nid);
                        tracing::info!(res, "Registered Windows system tray icon");
                    }

                    let mut msg: Msg = std::mem::zeroed();
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                        if msg.message == WM_HOTKEY {
                            tracing::info!("Windows WM_HOTKEY received - triggering Function");
                            let _ = tx.send(());
                        } else if msg.message == WM_TRAYICON {
                            // Left click or double click on tray icon toggles Function
                            if msg.lparam == 0x0202 /* WM_LBUTTONUP */
                                || msg.lparam == 0x0203 /* WM_LBUTTONDBLCLK */
                                || msg.lparam == 0x0205
                            /* WM_RBUTTONUP */
                            {
                                tracing::info!("Windows system tray icon clicked - toggling Function");
                                let _ = tx.send(());
                            }
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    // Clean unregistration on thread exit
                    let _ = UnregisterHotKey(0, 101);
                    if let Some(notify) = pfn_notify {
                        let _ = notify(NIM_DELETE, &nid);
                    }
                    worker_thread_id.store(0, Ordering::SeqCst);
                    worker_hwnd.store(0, Ordering::SeqCst);
                }
            });

            Ok(())
        }

        async fn unregister_hotkey(&self) -> Result<(), PlatformError> {
            tracing::info!("Unregistering Windows global hotkey");
            self.cleanup_hotkey_sync();
            let mut current = self.registered_hotkey.write().unwrap();
            *current = None;
            Ok(())
        }

        fn subscribe_hotkey(&self) -> broadcast::Receiver<()> {
            self.hotkey_tx.subscribe()
        }

        fn trigger_hotkey(&self) {
            let _ = self.hotkey_tx.send(());
        }

        async fn check_permissions(&self) -> PermissionStatus {
            PermissionStatus {
                accessibility: true,
                screen_recording: true,
                microphone: self.audio_capture.is_microphone_available(),
            }
        }

        async fn request_permissions(&self) -> Result<PermissionStatus, PlatformError> {
            Ok(self.check_permissions().await)
        }

        async fn is_microphone_available(&self) -> bool {
            self.audio_capture.is_microphone_available()
        }

        fn audio_capture(&self) -> Arc<dyn AudioCapture> {
            self.audio_capture.clone()
        }
    }
}

#[cfg(target_os = "macos")]
pub mod macos {
    use super::*;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::{Arc, RwLock};

    pub struct MacOsPlatformService {
        registered_hotkey: Arc<RwLock<Option<String>>>,
        hotkey_tx: broadcast::Sender<()>,
        audio_capture: Arc<dyn AudioCapture>,
        hotkey_ref: Arc<AtomicIsize>,
        handler_ref: Arc<AtomicIsize>,
        tx_ptr_ref: Arc<AtomicIsize>,
    }

    impl MacOsPlatformService {
        pub fn new() -> Self {
            let (hotkey_tx, _) = broadcast::channel(16);
            set_global_hotkey_tx(hotkey_tx.clone());
            Self {
                registered_hotkey: Arc::new(RwLock::new(None)),
                hotkey_tx,
                audio_capture: Arc::new(CpalAudioCapture::new()),
                hotkey_ref: Arc::new(AtomicIsize::new(0)),
                handler_ref: Arc::new(AtomicIsize::new(0)),
                tx_ptr_ref: Arc::new(AtomicIsize::new(0)),
            }
        }

        fn cleanup_hotkey_sync(&self) {
            let hk = self.hotkey_ref.swap(0, Ordering::SeqCst);
            let hr = self.handler_ref.swap(0, Ordering::SeqCst);
            let tx_ptr = self.tx_ptr_ref.swap(0, Ordering::SeqCst);
            if hk != 0 || hr != 0 {
                unsafe {
                    #[link(name = "Carbon", kind = "framework")]
                    extern "C" {
                        fn UnregisterEventHotKey(hotKey: *mut std::ffi::c_void) -> i32;
                        fn RemoveEventHandler(handlerRef: *mut std::ffi::c_void) -> i32;
                    }
                    if hk != 0 {
                        UnregisterEventHotKey(hk as *mut std::ffi::c_void);
                    }
                    if hr != 0 {
                        RemoveEventHandler(hr as *mut std::ffi::c_void);
                    }
                }
            }
            if tx_ptr != 0 {
                unsafe {
                    let _ = Box::from_raw(tx_ptr as *mut broadcast::Sender<()>);
                }
            }
        }
    }

    impl Drop for MacOsPlatformService {
        fn drop(&mut self) {
            self.cleanup_hotkey_sync();
        }
    }

    impl Default for MacOsPlatformService {
        fn default() -> Self {
            Self::new()
        }
    }

    fn parse_hotkey(shortcut: &str) -> Result<(u32, u32), PlatformError> {
        const OPTION_KEY: u32 = 0x0800;
        const CONTROL_KEY: u32 = 0x1000;
        const CMD_KEY: u32 = 0x0100;
        const SHIFT_KEY: u32 = 0x0200;

        let mut modifiers = 0;
        let mut key_code = None;
        for part in shortcut.split('+').map(|part| part.trim().to_uppercase()) {
            match part.as_str() {
                "OPTION" | "ALT" => modifiers |= OPTION_KEY,
                "CTRL" | "CONTROL" => modifiers |= CONTROL_KEY,
                "CMD" | "COMMAND" => modifiers |= CMD_KEY,
                "SHIFT" => modifiers |= SHIFT_KEY,
                "SPACE" => key_code = Some(49),           // kVK_Space
                ";" | "SEMICOLON" => key_code = Some(41), // kVK_ANSI_Semicolon
                "" => {}
                key => {
                    return Err(PlatformError::Unsupported(format!(
                        "Unsupported macOS hotkey key: {key}"
                    )));
                }
            }
        }

        if modifiers == 0 {
            modifiers = OPTION_KEY;
        }

        Ok((
            key_code.ok_or_else(|| {
                PlatformError::Unsupported("macOS hotkey must include a key".to_string())
            })?,
            modifiers,
        ))
    }

    #[async_trait]
    impl PlatformService for MacOsPlatformService {
        fn platform_name(&self) -> &'static str {
            "macOS"
        }

        async fn register_hotkey(&self, shortcut: &str) -> Result<(), PlatformError> {
            let (key_code, modifiers) = parse_hotkey(shortcut)?;
            {
                let mut current = self.registered_hotkey.write().unwrap();
                if let Some(ref existing) = *current {
                    if existing == shortcut {
                        tracing::info!(
                            shortcut,
                            "macOS hotkey already registered, skipping duplicate registration"
                        );
                        return Ok(());
                    }
                    self.cleanup_hotkey_sync();
                }
                *current = Some(shortcut.to_string());
            }

            // Ensure macOS double-tap Command listener is active
            setup_macos_double_command_listener();

            tracing::info!(shortcut, "Registering macOS global hotkey");

            // Do not touch NSApplication here. GPUI must create its `GPUIApplication`
            // subclass before any code asks AppKit for the shared application; otherwise
            // AppKit creates a plain NSApplication and GPUI cannot store its platform
            // state on its subclass. The accessory activation policy is applied from
            // the GPUI launch callback in `function-app` instead.

            #[repr(C)]
            #[derive(Copy, Clone)]
            struct EventHotKeyID {
                signature: u32,
                id: u32,
            }

            #[repr(C)]
            struct EventTypeSpec {
                event_class: u32,
                event_kind: u32,
            }

            #[link(name = "Carbon", kind = "framework")]
            #[allow(non_snake_case)]
            extern "C" {
                fn GetApplicationEventTarget() -> *mut std::ffi::c_void;
                fn GetEventDispatcherTarget() -> *mut std::ffi::c_void;
                fn InstallEventHandler(
                    inTarget: *mut std::ffi::c_void,
                    inHandler: unsafe extern "C" fn(
                        nextHandler: *mut std::ffi::c_void,
                        theEvent: *mut std::ffi::c_void,
                        userData: *mut std::ffi::c_void,
                    ) -> i32,
                    inNumTypes: u32,
                    inList: *const EventTypeSpec,
                    inUserData: *mut std::ffi::c_void,
                    outHandlerRef: *mut *mut std::ffi::c_void,
                ) -> i32;
                fn RegisterEventHotKey(
                    inHotKeyCode: u32,
                    inHotKeyModifiers: u32,
                    inHotKeyID: EventHotKeyID,
                    inTarget: *mut std::ffi::c_void,
                    inOptions: u32,
                    outHotKeyRef: *mut *mut std::ffi::c_void,
                ) -> i32;
            }

            unsafe extern "C" fn carbon_hotkey_handler(
                _next: *mut std::ffi::c_void,
                the_event: *mut std::ffi::c_void,
                user_data: *mut std::ffi::c_void,
            ) -> i32 {
                tracing::info!("Carbon global hotkey callback fired");
                tracing::info!("Hotkey event received: Command+;");
                tracing::info!(
                    event_ptr = ?the_event,
                    "kEventHotKeyPressed event received in Carbon handler"
                );

                trigger_global_hotkey();

                if !user_data.is_null() {
                    let tx = &*(user_data as *const broadcast::Sender<()>);
                    tracing::info!(
                        subscribers = tx.receiver_count(),
                        "Carbon callback notifying application broadcast channel"
                    );
                    match tx.send(()) {
                        Ok(num) => {
                            tracing::info!(num, "Dispatched hotkey event to application broadcast channel");
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "Failed to send hotkey event - no receivers listening");
                        }
                    }
                } else {
                    tracing::error!("Carbon hotkey callback received null user_data pointer");
                }
                0
            }

            const K_EVENT_CLASS_KEYBOARD: u32 = 0x6b657962; // 'keyb'
            const K_EVENT_HOT_KEY_PRESSED: u32 = 1;
            let tx_box = Box::new(self.hotkey_tx.clone());
            let tx_ptr = Box::into_raw(tx_box) as *mut std::ffi::c_void;

            let spec = EventTypeSpec {
                event_class: K_EVENT_CLASS_KEYBOARD,
                event_kind: K_EVENT_HOT_KEY_PRESSED,
            };

            unsafe {
                let mut target = GetEventDispatcherTarget();
                if target.is_null() {
                    tracing::warn!("GetEventDispatcherTarget() returned NULL, falling back to GetApplicationEventTarget()");
                    target = GetApplicationEventTarget();
                }

                if target.is_null() {
                    tracing::error!("Both GetEventDispatcherTarget() and GetApplicationEventTarget() returned NULL!");
                } else {
                    tracing::info!(
                        target = ?target,
                        "Retrieved EventTarget for Carbon hotkey handling"
                    );
                }

                let mut handler_ref: *mut std::ffi::c_void = std::ptr::null_mut();
                let mut hotkey_ref: *mut std::ffi::c_void = std::ptr::null_mut();

                tracing::info!(
                    shortcut,
                    key_code,
                    modifier_mask = format!("{:#06x}", modifiers),
                    "Installing Carbon event handler on EventDispatcherTarget"
                );

                let h_res = InstallEventHandler(
                    target,
                    carbon_hotkey_handler,
                    1,
                    &spec,
                    tx_ptr,
                    &mut handler_ref,
                );
                tracing::info!(
                    h_res,
                    handler_ref = ?handler_ref,
                    "InstallEventHandler completed"
                );

                let hotkey_id = EventHotKeyID {
                    signature: 0x46554e43, // 'FUNC'
                    id: 1,
                };
                tracing::info!(
                    signature = format!("{:#010x}", hotkey_id.signature),
                    id = hotkey_id.id,
                    key_code,
                    modifier_mask = format!("{:#06x}", modifiers),
                    "Registering EventHotKey with Carbon"
                );

                let r_res =
                    RegisterEventHotKey(key_code, modifiers, hotkey_id, target, 0, &mut hotkey_ref);
                tracing::info!(
                    r_res,
                    hotkey_ref = ?hotkey_ref,
                    "RegisterEventHotKey completed"
                );

                if h_res == 0 && r_res == 0 {
                    self.tx_ptr_ref.store(tx_ptr as isize, Ordering::SeqCst);
                    self.handler_ref
                        .store(handler_ref as isize, Ordering::SeqCst);
                    self.hotkey_ref.store(hotkey_ref as isize, Ordering::SeqCst);
                    tracing::info!(
                        "macOS Carbon global hotkey registered successfully: {}",
                        shortcut
                    );
                } else {
                    tracing::warn!(h_res, r_res, "Failed to register macOS Carbon hotkey");
                }
            }

            Ok(())
        }

        async fn unregister_hotkey(&self) -> Result<(), PlatformError> {
            tracing::info!("Unregistering macOS global hotkey");
            self.cleanup_hotkey_sync();
            let mut current = self.registered_hotkey.write().unwrap();
            *current = None;
            Ok(())
        }

        fn subscribe_hotkey(&self) -> broadcast::Receiver<()> {
            self.hotkey_tx.subscribe()
        }

        fn trigger_hotkey(&self) {
            let _ = self.hotkey_tx.send(());
        }

        async fn check_permissions(&self) -> PermissionStatus {
            PermissionStatus {
                accessibility: true,
                screen_recording: true,
                microphone: self.audio_capture.is_microphone_available(),
            }
        }

        async fn request_permissions(&self) -> Result<PermissionStatus, PlatformError> {
            Ok(self.check_permissions().await)
        }

        async fn is_microphone_available(&self) -> bool {
            self.audio_capture.is_microphone_available()
        }

        fn audio_capture(&self) -> Arc<dyn AudioCapture> {
            self.audio_capture.clone()
        }
    }
}

/// Fallback / mock platform service for tests and cross-compilation environments.
pub struct FallbackPlatformService {
    hotkey_tx: broadcast::Sender<()>,
    audio_capture: Arc<dyn AudioCapture>,
}

impl FallbackPlatformService {
    pub fn new() -> Self {
        let (hotkey_tx, _) = broadcast::channel(16);
        Self {
            hotkey_tx,
            audio_capture: Arc::new(MockAudioCapture::new()),
        }
    }

    pub fn with_mic(mut self, available: bool) -> Self {
        if !available {
            struct DisabledMic;
            impl AudioCapture for DisabledMic {
                fn is_microphone_available(&self) -> bool {
                    false
                }
                fn start_recording(&self) -> Result<(), PlatformError> {
                    Err(PlatformError::SystemApi(
                        "Microphone not available".to_string(),
                    ))
                }
                fn stop_recording(&self) -> Result<Vec<u8>, PlatformError> {
                    Ok(Vec::new())
                }
                fn is_recording(&self) -> bool {
                    false
                }
            }
            self.audio_capture = Arc::new(DisabledMic);
        } else {
            self.audio_capture = Arc::new(MockAudioCapture::new());
        }
        self
    }

    pub fn with_audio_capture(mut self, audio: Arc<dyn AudioCapture>) -> Self {
        self.audio_capture = audio;
        self
    }
}

impl Default for FallbackPlatformService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformService for FallbackPlatformService {
    fn platform_name(&self) -> &'static str {
        "Fallback"
    }

    async fn register_hotkey(&self, shortcut: &str) -> Result<(), PlatformError> {
        tracing::debug!(shortcut, "Mock hotkey registered");
        Ok(())
    }

    async fn unregister_hotkey(&self) -> Result<(), PlatformError> {
        tracing::debug!("Mock hotkey unregistered");
        Ok(())
    }

    fn subscribe_hotkey(&self) -> broadcast::Receiver<()> {
        self.hotkey_tx.subscribe()
    }

    fn trigger_hotkey(&self) {
        let _ = self.hotkey_tx.send(());
    }

    async fn check_permissions(&self) -> PermissionStatus {
        PermissionStatus {
            accessibility: true,
            screen_recording: true,
            microphone: self.audio_capture.is_microphone_available(),
        }
    }

    async fn request_permissions(&self) -> Result<PermissionStatus, PlatformError> {
        Ok(self.check_permissions().await)
    }

    async fn is_microphone_available(&self) -> bool {
        self.audio_capture.is_microphone_available()
    }

    fn audio_capture(&self) -> Arc<dyn AudioCapture> {
        self.audio_capture.clone()
    }
}

/// Locate the application icon file (`icon.ico`) across development,
/// distribution, and execution contexts without relying on hardcoded absolute paths.
pub fn find_icon_path() -> Option<std::path::PathBuf> {
    // 1. Explicit environment variable override
    if let Ok(env_path) = std::env::var("FUNCTION_ICON_PATH") {
        let p = std::path::PathBuf::from(env_path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. Relative to current working directory
    let cwd_candidates = [
        std::path::PathBuf::from("assets/icon.ico"),
        std::path::PathBuf::from("../assets/icon.ico"),
        std::path::PathBuf::from("../../assets/icon.ico"),
    ];
    for candidate in &cwd_candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }

    // 3. Search macOS bundle structure: Function.app/Contents/Resources/
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(macos_dir) = exe_path.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                let res_ico = contents_dir.join("Resources").join("icon.ico");
                if res_ico.exists() {
                    return Some(res_ico);
                }
                let res_png = contents_dir.join("Resources").join("icon.png");
                if res_png.exists() {
                    return Some(res_png);
                }
            }
        }
    }

    // 4. Search directory hierarchy of the current running executable (up to 5 levels)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for ancestor in exe_dir.ancestors().take(5) {
                let candidate = ancestor.join("assets").join("icon.ico");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    // 4. Compile-time crate manifest directory fallback
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        let manifest_path = std::path::PathBuf::from(manifest);
        for ancestor in manifest_path.ancestors().take(4) {
            let candidate = ancestor.join("assets").join("icon.ico");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 5. Runtime CARGO_MANIFEST_DIR environment variable
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = std::path::PathBuf::from(manifest);
        for ancestor in manifest_path.ancestors().take(4) {
            let candidate = ancestor.join("assets").join("icon.ico");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Helper to explicitly set the Win32 window icon (both small caption icon and large taskbar icon).
pub fn set_window_icon_by_title(title: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn GetModuleHandleW(lpModuleName: *const u16) -> isize;
            fn LoadImageW(
                hInst: isize,
                name: *const u16,
                type_: u32,
                cx: i32,
                cy: i32,
                fuLoad: u32,
            ) -> isize;
            fn SendMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> isize;
            fn LoadLibraryA(lpLibFileName: *const u8) -> isize;
            fn GetProcAddress(hModule: isize, lpProcName: *const u8) -> *const ();
        }

        const IMAGE_ICON: u32 = 1;
        const LR_LOADFROMFILE: u32 = 0x00000010;
        const LR_SHARED: u32 = 0x00008000;
        const WM_SETICON: u32 = 0x0080;
        const ICON_SMALL: usize = 0;
        const ICON_BIG: usize = 1;
        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWCP_ROUND: u32 = 2;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                // Apply Windows 11 rounded corners to window frame if DwmSetWindowAttribute is supported
                let dwm = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
                if dwm != 0 {
                    let proc = GetProcAddress(dwm, b"DwmSetWindowAttribute\0".as_ptr());
                    if !proc.is_null() {
                        type FnDwmSetWindowAttribute = unsafe extern "system" fn(
                            isize,
                            u32,
                            *const std::ffi::c_void,
                            u32,
                        )
                            -> i32;
                        let set_attr: FnDwmSetWindowAttribute = std::mem::transmute(proc);
                        let corner_pref = DWMWCP_ROUND;
                        let _ = set_attr(
                            hwnd,
                            DWMWA_WINDOW_CORNER_PREFERENCE,
                            &corner_pref as *const u32 as *const std::ffi::c_void,
                            std::mem::size_of::<u32>() as u32,
                        );
                    }
                }

                let hmod = GetModuleHandleW(std::ptr::null());
                let mut hicon_sm = LoadImageW(hmod, 1 as *const u16, IMAGE_ICON, 16, 16, LR_SHARED);
                let mut hicon_lg = LoadImageW(hmod, 1 as *const u16, IMAGE_ICON, 32, 32, LR_SHARED);

                // If embedded resource 1 was not found, load directly from filesystem icon.ico
                if hicon_sm == 0 || hicon_lg == 0 {
                    if let Some(icon_path) = find_icon_path() {
                        let icon_path_wide: Vec<u16> = OsStr::new(icon_path.as_os_str())
                            .encode_wide()
                            .chain(std::iter::once(0))
                            .collect();

                        if hicon_sm == 0 {
                            hicon_sm = LoadImageW(
                                0,
                                icon_path_wide.as_ptr(),
                                IMAGE_ICON,
                                16,
                                16,
                                LR_LOADFROMFILE,
                            );
                        }
                        if hicon_lg == 0 {
                            hicon_lg = LoadImageW(
                                0,
                                icon_path_wide.as_ptr(),
                                IMAGE_ICON,
                                32,
                                32,
                                LR_LOADFROMFILE,
                            );
                        }
                    }
                }

                if hicon_sm != 0 {
                    SendMessageW(hwnd, WM_SETICON, ICON_SMALL, hicon_sm);
                }
                if hicon_lg != 0 {
                    SendMessageW(hwnd, WM_SETICON, ICON_BIG, hicon_lg);
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
    }
}

///// Helper to explicitly center or position the Win32 window on the active monitor without disrupting size or swapchain.
pub fn center_window_by_title(title: &str, _width: i32, _height: i32, upper_third: bool) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        #[repr(C)]
        struct Rect {
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
        }

        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn GetSystemMetrics(nIndex: i32) -> i32;
            fn GetWindowRect(hWnd: isize, lpRect: *mut Rect) -> i32;
            fn SetWindowPos(
                hWnd: isize,
                hWndInsertAfter: isize,
                X: i32,
                Y: i32,
                cx: i32,
                cy: i32,
                uFlags: u32,
            ) -> i32;
        }

        const SM_CXSCREEN: i32 = 0;
        const SM_CYSCREEN: i32 = 1;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_NOACTIVATE: u32 = 0x0010;
        const SWP_ASYNCWINDOWPOS: u32 = 0x4000;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                let mut rc = Rect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                if GetWindowRect(hwnd, &mut rc) != 0 {
                    let w = rc.right - rc.left;
                    let h = rc.bottom - rc.top;
                    let screen_w = GetSystemMetrics(SM_CXSCREEN);
                    let screen_h = GetSystemMetrics(SM_CYSCREEN);
                    let x = (screen_w - w) / 2;
                    let y = if upper_third {
                        (screen_h - h) * 38 / 100
                    } else {
                        (screen_h - h) / 2
                    };
                    SetWindowPos(
                        hwnd,
                        0,
                        x,
                        y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                    );
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (title, _width, _height, upper_third);
    }
}

/// Check if a native window with the given title exists.
pub fn has_window_by_title(title: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
        }

        unsafe { FindWindowW(std::ptr::null(), title_wide.as_ptr()) != 0 }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
        false
    }
}

/// Hide the native window completely from the desktop and taskbar.
pub fn hide_window_by_title(title: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn ShowWindowAsync(hWnd: isize, nCmdShow: i32) -> i32;
        }

        const SW_HIDE: i32 = 0;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                ShowWindowAsync(hwnd, SW_HIDE);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
    }
}

/// Reveal and focus the native window instantly and asynchronously without re-entering the UI thread WndProc.
pub fn show_window_by_title(title: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn ShowWindowAsync(hWnd: isize, nCmdShow: i32) -> i32;
            fn SetForegroundWindow(hWnd: isize) -> i32;
        }

        const SW_SHOW: i32 = 5;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                ShowWindowAsync(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
    }
}

/// Ensure window is styled as an accessory/tool window (never shown in taskbar or Alt-Tab).
pub fn set_window_as_tool_window_by_title(title: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let title_wide: Vec<u16> = OsStr::new(title)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn GetWindowLongPtrW(hWnd: isize, nIndex: i32) -> isize;
            fn SetWindowLongPtrW(hWnd: isize, nIndex: i32, dwNewLong: isize) -> isize;
            fn SetWindowPos(
                hWnd: isize,
                hWndInsertAfter: isize,
                X: i32,
                Y: i32,
                cx: i32,
                cy: i32,
                uFlags: u32,
            ) -> i32;
        }

        const GWL_EXSTYLE: i32 = -20;
        const WS_EX_TOOLWINDOW: isize = 0x00000080;
        const WS_EX_APPWINDOW: isize = 0x00040000;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_FRAMECHANGED: u32 = 0x0020;
        const SWP_ASYNCWINDOWPOS: u32 = 0x4000;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let new_ex = (ex | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
                if ex != new_ex {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
                    SetWindowPos(
                        hwnd,
                        0,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_ASYNCWINDOWPOS,
                    );
                }

                // Eliminate Windows 11 DWM rectangular ghost border and unwanted frame shadows
                #[repr(C)]
                #[allow(non_snake_case)]
                struct Margins {
                    cxLeftWidth: i32,
                    cxRightWidth: i32,
                    cyTopHeight: i32,
                    cyBottomHeight: i32,
                }
                extern "system" {
                    fn LoadLibraryA(lpLibFileName: *const u8) -> isize;
                    fn GetProcAddress(hModule: isize, lpProcName: *const u8) -> *const ();
                }
                let dwm = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
                if dwm != 0 {
                    type FnDwmSetWindowAttribute = unsafe extern "system" fn(
                        isize,
                        u32,
                        *const std::ffi::c_void,
                        u32,
                    ) -> i32;
                    type FnDwmExtendFrame = unsafe extern "system" fn(isize, *const Margins) -> i32;

                    let p_set = GetProcAddress(dwm, b"DwmSetWindowAttribute\0".as_ptr());
                    if !p_set.is_null() {
                        let dwm_set: FnDwmSetWindowAttribute = std::mem::transmute(p_set);
                        // DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_DONOTROUND = 1 (removes outer rectangular rounded halo)
                        let corner_pref: u32 = 1;
                        let _ = dwm_set(
                            hwnd,
                            33,
                            &corner_pref as *const u32 as *const std::ffi::c_void,
                            4,
                        );
                        // DWMWA_BORDER_COLOR = 34, DWMWA_COLOR_NONE = 0xFFFFFFFE (removes DWM outer frame border)
                        let border_color: u32 = 0xFFFFFFFE;
                        let _ = dwm_set(
                            hwnd,
                            34,
                            &border_color as *const u32 as *const std::ffi::c_void,
                            4,
                        );
                    }

                    let p_ext = GetProcAddress(dwm, b"DwmExtendFrameIntoClientArea\0".as_ptr());
                    if !p_ext.is_null() {
                        let dwm_ext: FnDwmExtendFrame = std::mem::transmute(p_ext);
                        let m = Margins {
                            cxLeftWidth: -1,
                            cxRightWidth: -1,
                            cyTopHeight: -1,
                            cyBottomHeight: -1,
                        };
                        let _ = dwm_ext(hwnd, &m);
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = title;
        set_macos_activation_policy_accessory();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = title;
    }
}

/// Ensure Function is configured as an accessory application on macOS (not shown in Dock).
pub fn set_macos_activation_policy_accessory() {
    #[cfg(target_os = "macos")]
    unsafe {
        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSendSetPolicy =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, isize) -> isize;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_policy: MsgSendSetPolicy =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let ns_app_class = objc_getClass(c"NSApplication".as_ptr());
        if !ns_app_class.is_null() {
            let shared_app_sel = sel_registerName(c"sharedApplication".as_ptr());
            let app = msg_send_0(ns_app_class, shared_app_sel);
            if !app.is_null() {
                let set_policy_sel = sel_registerName(c"setActivationPolicy:".as_ptr());
                // NSApplicationActivationPolicyAccessory = 1
                let _ = msg_send_policy(app, set_policy_sel, 1isize);
                tracing::info!("Configured macOS application activation policy as accessory (dock icon hidden)");
            }
        }
    }
}

#[cfg(target_os = "macos")]
static PREVIOUS_APP: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Activate the application on macOS, bringing it to the foreground even if another app is active.
pub fn macos_activate_app() {
    #[cfg(target_os = "macos")]
    unsafe {
        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSend1 = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        type MsgSendPid = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
        type MsgSendActivate =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, bool) -> *mut std::ffi::c_void;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_1: MsgSend1 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_pid: MsgSendPid = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_activate: MsgSendActivate =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        // 1. Capture the previously frontmost application so we can return focus to it on dismiss
        let ns_workspace_cls = objc_getClass(c"NSWorkspace".as_ptr());
        if !ns_workspace_cls.is_null() {
            let shared_ws_sel = sel_registerName(c"sharedWorkspace".as_ptr());
            let ws = msg_send_0(ns_workspace_cls, shared_ws_sel);
            if !ws.is_null() {
                let frontmost_sel = sel_registerName(c"frontmostApplication".as_ptr());
                let front_app = msg_send_0(ws, frontmost_sel);
                if !front_app.is_null() {
                    let pid_sel = sel_registerName(c"processIdentifier".as_ptr());
                    let front_pid = msg_send_pid(front_app, pid_sel);
                    let my_pid = std::process::id() as i32;
                    if front_pid != my_pid {
                        let retain_sel = sel_registerName(c"retain".as_ptr());
                        let retained = msg_send_0(front_app, retain_sel);
                        let old_ptr = PREVIOUS_APP.swap(retained as isize, std::sync::atomic::Ordering::SeqCst);
                        if old_ptr != 0 {
                            let release_sel = sel_registerName(c"release".as_ptr());
                            let _ = msg_send_0(old_ptr as *mut std::ffi::c_void, release_sel);
                        }
                    }
                }
            }
        }

        // 2. Unhide and activate Function
        let ns_app_class = objc_getClass(c"NSApplication".as_ptr());
        if !ns_app_class.is_null() {
            let shared_app_sel = sel_registerName(c"sharedApplication".as_ptr());
            let app = msg_send_0(ns_app_class, shared_app_sel);
            if !app.is_null() {
                let unhide_sel = sel_registerName(c"unhide:".as_ptr());
                let _ = msg_send_1(app, unhide_sel, std::ptr::null_mut());

                let activate_sel = sel_registerName(c"activateIgnoringOtherApps:".as_ptr());
                let _ = msg_send_activate(app, activate_sel, true);
                tracing::info!("Activated macOS application via activateIgnoringOtherApps");
            }
        }
    }
}

/// Hide/dismiss the application on macOS, hiding all windows without terminating the process
/// and returning focus to the previously active application.
pub fn macos_hide_app() {
    #[cfg(target_os = "macos")]
    unsafe {
        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSend1 = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        type MsgSendUsize =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> usize;
        type MsgSendObjectAtIndex =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> *mut std::ffi::c_void;
        type MsgSendActivateOptions =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize) -> bool;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_1: MsgSend1 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_usize: MsgSendUsize =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_at_index: MsgSendObjectAtIndex =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_activate_options: MsgSendActivateOptions =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let ns_app_class = objc_getClass(c"NSApplication".as_ptr());
        if !ns_app_class.is_null() {
            let shared_app_sel = sel_registerName(c"sharedApplication".as_ptr());
            let app = msg_send_0(ns_app_class, shared_app_sel);
            if !app.is_null() {
                // 1. Order out all windows so they immediately disappear
                let windows_sel = sel_registerName(c"windows".as_ptr());
                let windows = msg_send_0(app, windows_sel);
                if !windows.is_null() {
                    let count_sel = sel_registerName(c"count".as_ptr());
                    let object_at_index_sel = sel_registerName(c"objectAtIndex:".as_ptr());
                    let order_out_sel = sel_registerName(c"orderOut:".as_ptr());
                    let count = msg_send_usize(windows, count_sel);
                    for i in 0..count {
                        let win = msg_send_at_index(windows, object_at_index_sel, i);
                        if !win.is_null() {
                            let _ = msg_send_1(win, order_out_sel, std::ptr::null_mut());
                        }
                    }
                }

                // 2. Hide application via [NSApp hide:]
                let hide_sel = sel_registerName(c"hide:".as_ptr());
                let _ = msg_send_1(app, hide_sel, std::ptr::null_mut());

                // 3. Deactivate application so macOS window manager shifts focus
                let deactivate_sel = sel_registerName(c"deactivate".as_ptr());
                let _ = msg_send_0(app, deactivate_sel);

                // 4. Return focus to previous application if known
                let prev_ptr = PREVIOUS_APP.swap(0, std::sync::atomic::Ordering::SeqCst);
                if prev_ptr != 0 {
                    let prev_app = prev_ptr as *mut std::ffi::c_void;
                    // NSApplicationActivateIgnoringOtherApps = 1 << 1 (2)
                    let activate_options_sel = sel_registerName(c"activateWithOptions:".as_ptr());
                    let _ = msg_send_activate_options(prev_app, activate_options_sel, 2);
                    let release_sel = sel_registerName(c"release".as_ptr());
                    let _ = msg_send_0(prev_app, release_sel);
                    tracing::info!("Returned focus to previous macOS application via activateWithOptions");
                }

                tracing::info!("Dismissed macOS application: ordered out windows, hid app, deactivated");
            }
        }
    }
}

/// Helper to find icon.png for macOS menu bar icon
#[cfg(target_os = "macos")]
fn find_macos_icon_path() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [
                dir.join("../Resources/icon.png"),
                dir.join("Resources/icon.png"),
                dir.join("icon.png"),
                dir.join("assets/icon.png"),
                dir.join("../assets/icon.png"),
                dir.join("../../assets/icon.png"),
                dir.join("../../../assets/icon.png"),
            ];
            for c in candidates {
                if c.exists() {
                    return Some(c);
                }
            }
        }
    }
    let cwd_candidates = [
        std::path::PathBuf::from("assets/icon.png"),
        std::path::PathBuf::from("Resources/icon.png"),
        std::path::PathBuf::from("icon.png"),
    ];
    for c in cwd_candidates {
        if c.exists() {
            return Some(c);
        }
    }
    None
}

/// Setup macOS menu bar status item (top bar icon) that displays the template icon and native AppKit menu.
pub fn setup_macos_menu_bar_icon(_hotkey_str: &str) {
    #[cfg(target_os = "macos")]
    unsafe {
        let hotkey_str = _hotkey_str;
        static STATUS_ITEM_INITIALIZED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        static G_STATUS_ITEM: std::sync::atomic::AtomicIsize =
            std::sync::atomic::AtomicIsize::new(0);

        if STATUS_ITEM_INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            tracing::info!("macOS menu bar icon already initialized, skipping duplicate setup");
            return;
        }

        const NSEVENT_MODIFIER_FLAG_COMMAND: usize = 0x0010_0000;

        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSend1 = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        type MsgSendCStr = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *const std::os::raw::c_char,
        ) -> *mut std::ffi::c_void;
        type MsgSendFloat = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            f64,
        ) -> *mut std::ffi::c_void;
        type MsgSendBool = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            std::os::raw::c_schar,
        ) -> *mut std::ffi::c_void;
        type MsgSendUsize = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            usize,
        ) -> *mut std::ffi::c_void;

        #[repr(C)]
        #[derive(Copy, Clone)]
        struct NSSize {
            width: f64,
            height: f64,
        }
        type MsgSendSize = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            NSSize,
        ) -> *mut std::ffi::c_void;

        type MsgSendInitItem = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_allocateClassPair(
                superclass: *mut std::ffi::c_void,
                name: *const std::os::raw::c_char,
                extraBytes: usize,
            ) -> *mut std::ffi::c_void;
            fn class_addMethod(
                cls: *mut std::ffi::c_void,
                name: *mut std::ffi::c_void,
                imp: unsafe extern "C" fn(
                    *mut std::ffi::c_void,
                    *mut std::ffi::c_void,
                    *mut std::ffi::c_void,
                ),
                types: *const std::os::raw::c_char,
            ) -> bool;
            fn objc_registerClassPair(cls: *mut std::ffi::c_void);
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_1: MsgSend1 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_cstr: MsgSendCStr =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_float: MsgSendFloat =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_bool: MsgSendBool =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_usize: MsgSendUsize =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_size: MsgSendSize =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_init_item: MsgSendInitItem =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let alloc_sel = sel_registerName(c"alloc".as_ptr());
        let init_sel = sel_registerName(c"init".as_ptr());
        let retain_sel = sel_registerName(c"retain".as_ptr());
        let set_mask_sel = sel_registerName(c"setKeyEquivalentModifierMask:".as_ptr());

        // Register FunctionStatusItemTarget class if not already registered
        let mut target_cls = objc_getClass(c"FunctionStatusItemTarget".as_ptr());
        if target_cls.is_null() {
            let ns_object = objc_getClass(c"NSObject".as_ptr());
            target_cls = objc_allocateClassPair(
                ns_object,
                c"FunctionStatusItemTarget".as_ptr(),
                0,
            );
            if !target_cls.is_null() {
                unsafe extern "C" fn on_show_function(
                    _this: *mut std::ffi::c_void,
                    _cmd: *mut std::ffi::c_void,
                    _sender: *mut std::ffi::c_void,
                ) {
                    tracing::info!("Menu item 'Show Function' clicked");
                    send_platform_command(PlatformCommand::ToggleWindow);
                }

                unsafe extern "C" fn on_settings_click(
                    _this: *mut std::ffi::c_void,
                    _cmd: *mut std::ffi::c_void,
                    _sender: *mut std::ffi::c_void,
                ) {
                    tracing::info!("Menu item 'Settings' clicked");
                    send_platform_command(PlatformCommand::OpenSettings);
                }

                unsafe extern "C" fn on_quit_function(
                    _this: *mut std::ffi::c_void,
                    _cmd: *mut std::ffi::c_void,
                    _sender: *mut std::ffi::c_void,
                ) {
                    tracing::info!("Function quit requested from menu");
                    send_platform_command(PlatformCommand::Quit);
                    std::process::exit(0);
                }

                let show_sel = sel_registerName(c"onShowFunction:".as_ptr());
                class_addMethod(target_cls, show_sel, on_show_function, c"v@:@".as_ptr());

                let settings_sel = sel_registerName(c"onSettingsClick:".as_ptr());
                class_addMethod(target_cls, settings_sel, on_settings_click, c"v@:@".as_ptr());

                let quit_sel = sel_registerName(c"onQuitFunction:".as_ptr());
                class_addMethod(target_cls, quit_sel, on_quit_function, c"v@:@".as_ptr());

                // Backward-compatible selector for button click
                class_addMethod(
                    target_cls,
                    sel_registerName(c"onStatusItemClick:".as_ptr()),
                    on_show_function,
                    c"v@:@".as_ptr(),
                );

                objc_registerClassPair(target_cls);
            }
        }

        if target_cls.is_null() {
            tracing::warn!("Failed to create FunctionStatusItemTarget class");
            return;
        }

        let target_inst = msg_send_0(msg_send_0(target_cls, alloc_sel), init_sel);
        if !target_inst.is_null() {
            let _ = msg_send_0(target_inst, retain_sel);
        }

        let ns_status_bar = objc_getClass(c"NSStatusBar".as_ptr());
        if ns_status_bar.is_null() {
            return;
        }
        let system_bar_sel = sel_registerName(c"systemStatusBar".as_ptr());
        let bar = msg_send_0(ns_status_bar, system_bar_sel);
        if bar.is_null() {
            return;
        }

        let status_item_sel = sel_registerName(c"statusItemWithLength:".as_ptr());
        // -1.0 is NSVariableStatusItemLength
        let status_item = msg_send_float(bar, status_item_sel, -1.0);
        if status_item.is_null() {
            tracing::warn!("Failed to create NSStatusItem on systemStatusBar");
            return;
        }
        // Crucial: Retain status_item so AppKit autorelease pool does not deallocate it!
        let _ = msg_send_0(status_item, retain_sel);
        G_STATUS_ITEM.store(status_item as isize, std::sync::atomic::Ordering::SeqCst);

        let button_sel = sel_registerName(c"button".as_ptr());
        let button = msg_send_0(status_item, button_sel);
        let ns_string = objc_getClass(c"NSString".as_ptr());
        let utf8_sel = sel_registerName(c"stringWithUTF8String:".as_ptr());

        let mut icon_loaded = false;
        if let Some(icon_path) = find_macos_icon_path() {
            if let Ok(c_path) = std::ffi::CString::new(icon_path.to_string_lossy().as_bytes()) {
                let path_str = msg_send_cstr(ns_string, utf8_sel, c_path.as_ptr());
                if !path_str.is_null() {
                    let ns_image = objc_getClass(c"NSImage".as_ptr());
                    let image_alloc = msg_send_0(ns_image, alloc_sel);
                    let init_file_sel = sel_registerName(c"initWithContentsOfFile:".as_ptr());
                    let image = msg_send_1(image_alloc, init_file_sel, path_str);
                    if !image.is_null() {
                        let _ = msg_send_0(image, retain_sel);
                        let set_template_sel = sel_registerName(c"setTemplate:".as_ptr());
                        msg_send_bool(image, set_template_sel, 1);

                        let set_size_sel = sel_registerName(c"setSize:".as_ptr());
                        msg_send_size(image, set_size_sel, NSSize { width: 18.0, height: 18.0 });

                        if !button.is_null() {
                            let set_image_sel = sel_registerName(c"setImage:".as_ptr());
                            msg_send_1(button, set_image_sel, image);
                            icon_loaded = true;
                            tracing::info!(path = %icon_path.display(), "Loaded template icon for macOS menu bar item");
                        }
                    }
                }
            }
        }

        // Derive Show Function shortcut info from the actual authoritative configuration
        let shortcut_info = parse_macos_shortcut(hotkey_str);

        if !button.is_null() {
            if !icon_loaded {
                static TITLE_CSTR: &std::ffi::CStr = c"ƒ";
                let title = msg_send_cstr(ns_string, utf8_sel, TITLE_CSTR.as_ptr());
                if !title.is_null() {
                    let set_title_sel = sel_registerName(c"setTitle:".as_ptr());
                    msg_send_1(button, set_title_sel, title);
                }
            }

            let tip_label = if let Some(ref sc) = shortcut_info {
                format!("Function ({})", sc.display_label)
            } else {
                "Function".to_string()
            };
            if let Ok(c_tip) = std::ffi::CString::new(tip_label.as_bytes()) {
                let tip = msg_send_cstr(ns_string, utf8_sel, c_tip.as_ptr());
                if !tip.is_null() {
                    let set_tip_sel = sel_registerName(c"setToolTip:".as_ptr());
                    msg_send_1(button, set_tip_sel, tip);
                }
            }
        }

        // Build native NSMenu
        let ns_menu_cls = objc_getClass(c"NSMenu".as_ptr());
        let ns_menu_item_cls = objc_getClass(c"NSMenuItem".as_ptr());
        if !ns_menu_cls.is_null() && !ns_menu_item_cls.is_null() {
            let menu_alloc = msg_send_0(ns_menu_cls, alloc_sel);
            let menu_init_sel = sel_registerName(c"initWithTitle:".as_ptr());
            let menu_title = msg_send_cstr(ns_string, utf8_sel, c"FunctionMenu".as_ptr());
            let menu = msg_send_1(menu_alloc, menu_init_sel, menu_title);
            if !menu.is_null() {
                let _ = msg_send_0(menu, retain_sel);

                let set_autoenables_sel = sel_registerName(c"setAutoenablesItems:".as_ptr());
                msg_send_bool(menu, set_autoenables_sel, 0);

                let add_item_sel = sel_registerName(c"addItem:".as_ptr());
                let sep_sel = sel_registerName(c"separatorItem".as_ptr());
                let init_item_sel = sel_registerName(c"initWithTitle:action:keyEquivalent:".as_ptr());
                let set_target_sel = sel_registerName(c"setTarget:".as_ptr());
                let set_enabled_sel = sel_registerName(c"setEnabled:".as_ptr());

                // 1. "Function" (disabled header)
                let f_title = msg_send_cstr(ns_string, utf8_sel, c"Function".as_ptr());
                let empty_key = msg_send_cstr(ns_string, utf8_sel, c"".as_ptr());
                let header_item = msg_send_init_item(
                    msg_send_0(ns_menu_item_cls, alloc_sel),
                    init_item_sel,
                    f_title,
                    std::ptr::null_mut(),
                    empty_key,
                );
                msg_send_bool(header_item, set_enabled_sel, 0);
                msg_send_1(menu, add_item_sel, header_item);

                // Separator
                let sep1 = msg_send_0(ns_menu_item_cls, sep_sel);
                msg_send_1(menu, add_item_sel, sep1);

                // 2. "Show Function" (action: onShowFunction:, derived keyEquivalent & modifiers)
                let show_title = msg_send_cstr(ns_string, utf8_sel, c"Show Function".as_ptr());
                let (show_key_str, show_mask) = if let Some(ref sc) = shortcut_info {
                    (sc.key_char.clone(), sc.modifier_mask)
                } else {
                    ("".to_string(), 0usize)
                };
                let show_key_cstr = std::ffi::CString::new(show_key_str.as_bytes()).unwrap_or_default();
                let show_key = msg_send_cstr(ns_string, utf8_sel, show_key_cstr.as_ptr());
                let show_act = sel_registerName(c"onShowFunction:".as_ptr());
                let show_item = msg_send_init_item(
                    msg_send_0(ns_menu_item_cls, alloc_sel),
                    init_item_sel,
                    show_title,
                    show_act,
                    show_key,
                );
                msg_send_usize(show_item, set_mask_sel, show_mask);
                msg_send_1(show_item, set_target_sel, target_inst);
                msg_send_bool(show_item, set_enabled_sel, 1);
                msg_send_1(menu, add_item_sel, show_item);

                // Separator
                let sep2 = msg_send_0(ns_menu_item_cls, sep_sel);
                msg_send_1(menu, add_item_sel, sep2);

                // 3. "Settings" (action: onSettingsClick:, key: ⌘,)
                let settings_title = msg_send_cstr(ns_string, utf8_sel, c"Settings".as_ptr());
                let comma_key = msg_send_cstr(ns_string, utf8_sel, c",".as_ptr());
                let settings_act = sel_registerName(c"onSettingsClick:".as_ptr());
                let settings_item = msg_send_init_item(
                    msg_send_0(ns_menu_item_cls, alloc_sel),
                    init_item_sel,
                    settings_title,
                    settings_act,
                    comma_key,
                );
                msg_send_usize(settings_item, set_mask_sel, NSEVENT_MODIFIER_FLAG_COMMAND);
                msg_send_1(settings_item, set_target_sel, target_inst);
                msg_send_bool(settings_item, set_enabled_sel, 1);
                msg_send_1(menu, add_item_sel, settings_item);

                // Separator
                let sep3 = msg_send_0(ns_menu_item_cls, sep_sel);
                msg_send_1(menu, add_item_sel, sep3);

                // 4. "Quit Function" (action: onQuitFunction:, key: ⌘Q)
                let quit_title = msg_send_cstr(ns_string, utf8_sel, c"Quit Function".as_ptr());
                let q_key = msg_send_cstr(ns_string, utf8_sel, c"q".as_ptr());
                let quit_act = sel_registerName(c"onQuitFunction:".as_ptr());
                let quit_item = msg_send_init_item(
                    msg_send_0(ns_menu_item_cls, alloc_sel),
                    init_item_sel,
                    quit_title,
                    quit_act,
                    q_key,
                );
                msg_send_usize(quit_item, set_mask_sel, NSEVENT_MODIFIER_FLAG_COMMAND);
                msg_send_1(quit_item, set_target_sel, target_inst);
                msg_send_bool(quit_item, set_enabled_sel, 1);
                msg_send_1(menu, add_item_sel, quit_item);

                let set_menu_sel = sel_registerName(c"setMenu:".as_ptr());
                msg_send_1(status_item, set_menu_sel, menu);
            }
        }

        tracing::info!("Function menu bar item initialized");
    }
}

/// Register Function to launch at macOS login/startup using modern SMAppService (macOS 13+).
/// This ensures Function appears in System Settings -> General -> Login Items.
pub fn register_macos_login_item() {
    #[cfg(target_os = "macos")]
    unsafe {
        // Objective-C BOOL is signed char (i8) on Apple platforms: 0 is NO, non-zero is YES.
        type ObjcBool = std::os::raw::c_schar;

        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSendStatus =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> isize;
        type MsgSendRegister = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut *mut std::ffi::c_void,
        ) -> ObjcBool;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_status: MsgSendStatus =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_register: MsgSendRegister =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let sm_app_service_class = objc_getClass(c"SMAppService".as_ptr());
        if !sm_app_service_class.is_null() {
            let main_app_sel = sel_registerName(c"mainAppService".as_ptr());
            let service = msg_send_0(sm_app_service_class, main_app_sel);
            if !service.is_null() {
                let status_sel = sel_registerName(c"status".as_ptr());
                // SMAppServiceStatusEnabled = 1
                let status = msg_send_status(service, status_sel);
                if status != 1 {
                    let register_sel = sel_registerName(c"registerAndReturnError:".as_ptr());
                    let mut err: *mut std::ffi::c_void = std::ptr::null_mut();
                    let res: ObjcBool = msg_send_register(
                        service,
                        register_sel,
                        &mut err as *mut *mut std::ffi::c_void,
                    );
                    let success: bool = res != 0;
                    if success {
                        tracing::info!(
                            "Registered Function as modern macOS SMAppService Login Item"
                        );
                    } else {
                        tracing::warn!(
                            "Failed to register Function as macOS Login Item (status: {})",
                            status
                        );
                    }
                }
            }
        }
    }
}

/// Setup double-tap Command key listener on macOS using AppKit NSEvent monitors.
///
/// Listens for rapid double presses of the Command key (either Left or Right Command)
/// and invokes `trigger_global_hotkey()` to toggle the Function window.
#[cfg(target_os = "macos")]
pub fn setup_macos_double_command_listener() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Instant;

    static LISTENER_INITIALIZED: AtomicBool = AtomicBool::new(false);
    if LISTENER_INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    struct CommandTapState {
        last_down: Option<Instant>,
        last_tap_up: Option<Instant>,
        canceled: bool,
    }

    static TAP_STATE: Mutex<CommandTapState> = Mutex::new(CommandTapState {
        last_down: None,
        last_tap_up: None,
        canceled: false,
    });

    #[repr(C)]
    struct BlockDescriptor {
        reserved: usize,
        size: usize,
    }

    static BLOCK_DESCRIPTOR: BlockDescriptor = BlockDescriptor {
        reserved: 0,
        size: std::mem::size_of::<GlobalBlockLiteral>(),
    };

    #[repr(C)]
    struct GlobalBlockLiteral {
        isa: *const std::ffi::c_void,
        flags: i32,
        reserved: i32,
        invoke: unsafe extern "C" fn(*mut GlobalBlockLiteral, *mut std::ffi::c_void),
        descriptor: *const BlockDescriptor,
    }

    #[repr(C)]
    struct LocalBlockLiteral {
        isa: *const std::ffi::c_void,
        flags: i32,
        reserved: i32,
        invoke: unsafe extern "C" fn(*mut LocalBlockLiteral, *mut std::ffi::c_void) -> *mut std::ffi::c_void,
        descriptor: *const BlockDescriptor,
    }

    unsafe fn handle_event(event: *mut std::ffi::c_void) {
        if event.is_null() {
            return;
        }

        type MsgSendUsize = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> usize;
        type MsgSendU16 = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> u16;

        extern "C" {
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_usize: MsgSendUsize = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_u16: MsgSendU16 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let type_sel = sel_registerName(c"type".as_ptr());
        let event_type = msg_send_usize(event, type_sel);

        // NSEventTypeKeyDown = 10
        const NSEVENT_TYPE_KEY_DOWN: usize = 10;
        // NSEventTypeFlagsChanged = 12
        const NSEVENT_TYPE_FLAGS_CHANGED: usize = 12;

        if event_type == NSEVENT_TYPE_KEY_DOWN {
            // Any normal key down cancels an active Command double-tap candidate
            if let Ok(mut state) = TAP_STATE.lock() {
                state.canceled = true;
                state.last_down = None;
                state.last_tap_up = None;
            }
            return;
        }

        if event_type != NSEVENT_TYPE_FLAGS_CHANGED {
            return;
        }

        let key_code_sel = sel_registerName(c"keyCode".as_ptr());
        let modifier_flags_sel = sel_registerName(c"modifierFlags".as_ptr());

        let key_code = msg_send_u16(event, key_code_sel);
        let flags = msg_send_usize(event, modifier_flags_sel);

        // Left Command = 55 (0x37), Right Command = 54 (0x36)
        if key_code == 54 || key_code == 55 {
            // NSEventModifierFlagCommand = 0x0010_0000 (bit 20)
            let is_cmd_down = (flags & 0x0010_0000) != 0;

            // Check if other modifiers are pressed: Shift (bit 17), Control (bit 18), Option (bit 19)
            let other_modifiers = flags & (0x0002_0000 | 0x0004_0000 | 0x0008_0000);
            if other_modifiers != 0 {
                if let Ok(mut state) = TAP_STATE.lock() {
                    state.canceled = true;
                    state.last_down = None;
                    state.last_tap_up = None;
                }
                return;
            }

            let now = Instant::now();
            if let Ok(mut state) = TAP_STATE.lock() {
                if is_cmd_down {
                    state.last_down = Some(now);
                    state.canceled = false;
                } else {
                    if state.canceled {
                        state.canceled = false;
                        state.last_down = None;
                        return;
                    }

                    if let Some(down_time) = state.last_down.take() {
                        let down_duration = now.duration_since(down_time);
                        // A quick tap should be held for at most 350ms
                        if down_duration.as_millis() <= 350 {
                            if let Some(prev_up) = state.last_tap_up.take() {
                                let interval = now.duration_since(prev_up);
                                // Interval between tap 1 release and tap 2 release: max 450ms
                                if interval.as_millis() <= 450 {
                                    tracing::info!(
                                        interval_ms = interval.as_millis(),
                                        "macOS double-tap Command detected - toggling Function window"
                                    );
                                    trigger_global_hotkey();
                                    state.last_down = None;
                                    state.last_tap_up = None;
                                    return;
                                }
                            }
                            state.last_tap_up = Some(now);
                        } else {
                            state.last_tap_up = None;
                        }
                    }
                }
            }
        } else {
            // Any other modifier key changed
            if let Ok(mut state) = TAP_STATE.lock() {
                state.canceled = true;
                state.last_tap_up = None;
            }
        }
    }

    unsafe extern "C" fn global_monitor_invoke(
        _block: *mut GlobalBlockLiteral,
        event: *mut std::ffi::c_void,
    ) {
        handle_event(event);
    }

    unsafe extern "C" fn local_monitor_invoke(
        _block: *mut LocalBlockLiteral,
        event: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void {
        handle_event(event);
        event
    }

    extern "C" {
        #[link_name = "_NSConcreteGlobalBlock"]
        static NSConcreteGlobalBlock: std::ffi::c_void;
        fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
        fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
        fn objc_msgSend();
    }

    static mut GLOBAL_BLOCK: GlobalBlockLiteral = GlobalBlockLiteral {
        isa: std::ptr::null(),
        flags: 1 << 28, // BLOCK_IS_GLOBAL
        reserved: 0,
        invoke: global_monitor_invoke,
        descriptor: &BLOCK_DESCRIPTOR,
    };

    static mut LOCAL_BLOCK: LocalBlockLiteral = LocalBlockLiteral {
        isa: std::ptr::null(),
        flags: 1 << 28, // BLOCK_IS_GLOBAL
        reserved: 0,
        invoke: local_monitor_invoke,
        descriptor: &BLOCK_DESCRIPTOR,
    };

    unsafe {
        (*std::ptr::addr_of_mut!(GLOBAL_BLOCK)).isa = &NSConcreteGlobalBlock as *const _ as *const std::ffi::c_void;
        (*std::ptr::addr_of_mut!(LOCAL_BLOCK)).isa = &NSConcreteGlobalBlock as *const _ as *const std::ffi::c_void;

        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSendAddMonitor = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            u64,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_add: MsgSendAddMonitor =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let ns_event_class = objc_getClass(c"NSEvent".as_ptr());
        if ns_event_class.is_null() {
            tracing::warn!("Failed to get NSEvent class for double-tap Command monitor");
            return;
        }

        let add_global_sel = sel_registerName(c"addGlobalMonitorForEventsMatchingMask:handler:".as_ptr());
        let add_local_sel = sel_registerName(c"addLocalMonitorForEventsMatchingMask:handler:".as_ptr());
        let retain_sel = sel_registerName(c"retain".as_ptr());

        // Mask: FlagsChanged (1 << 12) | KeyDown (1 << 10)
        let mask: u64 = (1u64 << 12) | (1u64 << 10);

        let global_mon = msg_send_add(
            ns_event_class,
            add_global_sel,
            mask,
            std::ptr::addr_of_mut!(GLOBAL_BLOCK) as *mut std::ffi::c_void,
        );
        if !global_mon.is_null() {
            let _ = msg_send_0(global_mon, retain_sel);
            tracing::info!("macOS global double-tap Command monitor registered successfully");
        } else {
            tracing::warn!("Failed to register macOS global double-tap Command monitor");
        }

        let local_mon = msg_send_add(
            ns_event_class,
            add_local_sel,
            mask,
            std::ptr::addr_of_mut!(LOCAL_BLOCK) as *mut std::ffi::c_void,
        );
        if !local_mon.is_null() {
            let _ = msg_send_0(local_mon, retain_sel);
            tracing::info!("macOS local double-tap Command monitor registered successfully");
        } else {
            tracing::warn!("Failed to register macOS local double-tap Command monitor");
        }
    }
}

/// Fallback for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub fn setup_macos_double_command_listener() {}


/// Sound effect types inspired by Flow Launcher feedback sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEffect {
    /// Navigation click when arrowing through items
    Navigate,
    /// Selection or shortcut trigger
    Select,
    /// Command execution trigger
    Execute,
    /// Success chime when task completes or config is saved
    Success,
    /// Error or warning tone
    Error,
    /// Tactile mechanical keyboard switch press / click (played strictly when toggling via keybind)
    HotkeyToggle,
}

/// Play native system audio feedback without blocking the UI thread.
pub fn play_sound(effect: SoundEffect) {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            extern "system" {
                fn MessageBeep(uType: u32) -> i32;
                fn Beep(dwFreq: u32, dwDuration: u32) -> i32;
            }
            unsafe {
                match effect {
                    SoundEffect::HotkeyToggle => {
                        // Crisp high-frequency tactile mechanical switch click
                        let _ = Beep(2400, 10);
                        let _ = Beep(1300, 6);
                    }
                    SoundEffect::Navigate => {
                        let _ = Beep(1350, 10);
                    }
                    SoundEffect::Select => {
                        let _ = Beep(1700, 18);
                    }
                    SoundEffect::Execute => {
                        let _ = MessageBeep(0xFFFFFFFF);
                    }
                    SoundEffect::Success => {
                        let _ = MessageBeep(0x00000040); // Asterisk chime
                    }
                    SoundEffect::Error => {
                        let _ = MessageBeep(0x00000010); // Hand error sound
                    }
                }
            }
        });
    }
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn AudioServicesPlaySystemSound(inSystemSoundID: u32);
        }
        match effect {
            SoundEffect::HotkeyToggle => unsafe {
                // 1104 is the native tactile Apple keyboard click sound
                AudioServicesPlaySystemSound(1104);
            },
            SoundEffect::Success => unsafe {
                AudioServicesPlaySystemSound(1001);
            },
            SoundEffect::Error => unsafe {
                AudioServicesPlaySystemSound(1053);
            },
            _ => {}
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = effect;
    }
}

/// Copy text to the native system clipboard.
pub fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        extern "system" {
            fn OpenClipboard(hWndNewOwner: isize) -> i32;
            fn CloseClipboard() -> i32;
            fn EmptyClipboard() -> i32;
            fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> isize;
            fn GlobalLock(hMem: isize) -> *mut u8;
            fn GlobalUnlock(hMem: isize) -> i32;
            fn SetClipboardData(uFormat: u32, hMem: isize) -> isize;
        }

        const CF_UNICODETEXT: u32 = 13;
        const GMEM_MOVEABLE: u32 = 0x0002;

        let wide: Vec<u16> = OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            if OpenClipboard(0) == 0 {
                return false;
            }
            EmptyClipboard();
            let bytes_len = wide.len() * std::mem::size_of::<u16>();
            let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes_len);
            if h_mem != 0 {
                let ptr = GlobalLock(h_mem) as *mut u16;
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                    GlobalUnlock(h_mem);
                    SetClipboardData(CF_UNICODETEXT, h_mem);
                }
            }
            CloseClipboard();
            true
        }
    }
    #[cfg(target_os = "macos")]
    {
        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSendCStr = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *const std::os::raw::c_char,
        ) -> *mut std::ffi::c_void;
        type MsgSendSetString = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> std::os::raw::c_schar;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        unsafe {
            let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            let msg_send_cstr: MsgSendCStr =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            let msg_send_set_string: MsgSendSetString =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

            let ns_pasteboard = objc_getClass(c"NSPasteboard".as_ptr());
            let ns_string = objc_getClass(c"NSString".as_ptr());
            if !ns_pasteboard.is_null() && !ns_string.is_null() {
                let general_sel = sel_registerName(c"generalPasteboard".as_ptr());
                let pb = msg_send_0(ns_pasteboard, general_sel);
                if !pb.is_null() {
                    let clear_sel = sel_registerName(c"clearContents".as_ptr());
                    let _ = msg_send_0(pb, clear_sel);

                    let utf8_sel = sel_registerName(c"stringWithUTF8String:".as_ptr());
                    if let Ok(c_text) = std::ffi::CString::new(text) {
                        let str_obj = msg_send_cstr(ns_string, utf8_sel, c_text.as_ptr());
                        let pboard_type = msg_send_cstr(
                            ns_string,
                            utf8_sel,
                            c"public.utf8-plain-text".as_ptr(),
                        );

                        if !str_obj.is_null() && !pboard_type.is_null() {
                            let set_str_sel = sel_registerName(c"setString:forType:".as_ptr());
                            let res = msg_send_set_string(pb, set_str_sel, str_obj, pboard_type);
                            return res != 0;
                        }
                    }
                }
            }
            false
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = text;
        false
    }
}

/// Read an image from the clipboard, saving it to ~/.function/attachments/ if it's raw bitmap data,
/// or resolving its path if an image file was copied.
pub fn read_clipboard_image() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn OpenClipboard(hWndNewOwner: isize) -> i32;
            fn CloseClipboard() -> i32;
            fn IsClipboardFormatAvailable(format: u32) -> i32;
            fn GetClipboardData(uFormat: u32) -> isize;
            fn DragQueryFileW(hDrop: isize, iFile: u32, lpszFile: *mut u16, cch: u32) -> u32;
        }

        const CF_BITMAP: u32 = 2;
        const CF_DIB: u32 = 8;
        const CF_HDROP: u32 = 15;

        unsafe {
            if OpenClipboard(0) != 0 {
                // 1. Check for copied files (CF_HDROP)
                if IsClipboardFormatAvailable(CF_HDROP) != 0 {
                    let h_drop = GetClipboardData(CF_HDROP);
                    if h_drop != 0 {
                        let mut buf = [0u16; 1024];
                        let len = DragQueryFileW(h_drop, 0, buf.as_mut_ptr(), 1024);
                        if len > 0 {
                            let path_str = String::from_utf16_lossy(&buf[..len as usize]);
                            let path = std::path::PathBuf::from(path_str);
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if matches!(
                                ext.as_str(),
                                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
                            ) {
                                CloseClipboard();
                                return Some(path);
                            }
                        }
                    }
                }

                // 2. Check for raw bitmap / DIB image data (screenshots, copied images from browser)
                let has_image = IsClipboardFormatAvailable(CF_BITMAP) != 0
                    || IsClipboardFormatAvailable(CF_DIB) != 0;
                CloseClipboard();

                if has_image {
                    let home = std::env::var_os("USERPROFILE")
                        .or_else(|| std::env::var_os("HOME"))
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let mut dir = home;
                    dir.push(".function");
                    dir.push("attachments");
                    let _ = std::fs::create_dir_all(&dir);

                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let out_path = dir.join(format!("photo_{}.png", ts));

                    let ps_script = format!(
                        "Add-Type -AssemblyName System.Windows.Forms; $img = [System.Windows.Forms.Clipboard]::GetImage(); if ($img -ne $null) {{ $img.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png); exit 0 }} else {{ exit 1 }}",
                        out_path.display().to_string().replace('\\', "\\\\")
                    );

                    let res = std::process::Command::new("powershell")
                        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                        .output();

                    if let Ok(out) = res {
                        if out.status.success() && out_path.exists() {
                            return Some(out_path);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let mut dir = home;
        dir.push(".function");
        dir.push("attachments");
        let _ = std::fs::create_dir_all(&dir);

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let out_path = dir.join(format!("photo_{}.png", ts));

        let osascript = format!(
            "set png_data to the clipboard as «class PNGf»\nset fp to open for access POSIX file \"{}\" with write permission\nwrite png_data to fp\nclose access fp",
            out_path.display()
        );

        let res = std::process::Command::new("osascript")
            .args(["-e", &osascript])
            .output();

        if let Ok(out) = res {
            if out.status.success() && out_path.exists() {
                return Some(out_path);
            }
        }
    }

    None
}

/// Open a URL or shell command target with default system handler.
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("cmd").args(["/c", "start", "", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        use std::process::Command;
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

/// Open a local file, folder, or application in the default native system handler.
pub fn open_path(path: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let p = std::path::Path::new(path);
        if p.is_dir() {
            let _ = Command::new("explorer").arg(path).spawn();
        } else {
            let _ = Command::new("cmd").args(["/c", "start", "", path]).spawn();
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let _ = Command::new("open").arg(path).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        use std::process::Command;
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}

/// Helper function to create the native platform service for the current target OS.
pub fn create_native_platform_service() -> Box<dyn PlatformService> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsPlatformService::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsPlatformService::new())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Box::new(FallbackPlatformService::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_icon_path() {
        let icon_path = find_icon_path();
        assert!(
            icon_path.is_some(),
            "find_icon_path() should resolve icon.ico in workspace"
        );
        let path = icon_path.unwrap();
        assert!(
            path.exists(),
            "Resolved icon path does not exist on disk: {:?}",
            path
        );
        assert!(
            path.ends_with("icon.ico"),
            "Resolved path does not end with icon.ico: {:?}",
            path
        );
    }

    #[tokio::test]
    async fn test_platform_service() {
        let service = create_native_platform_service();
        assert!(!service.platform_name().is_empty());
        let res = service.register_hotkey("Alt+Space").await;
        assert!(res.is_ok());

        let mut rx = service.subscribe_hotkey();
        service.trigger_hotkey();
        assert!(rx.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_mic_availability() {
        let fallback = FallbackPlatformService::new().with_mic(false);
        assert!(!fallback.is_microphone_available().await);

        let fallback_with_mic = FallbackPlatformService::new().with_mic(true);
        assert!(fallback_with_mic.is_microphone_available().await);
    }

    #[tokio::test]
    async fn test_audio_recording_wav() {
        let audio = MockAudioCapture::new();
        assert!(audio.is_microphone_available());
        assert!(!audio.is_recording());
        audio.start_recording().unwrap();
        assert!(audio.is_recording());
        let wav_bytes = audio.stop_recording().unwrap();
        assert!(!audio.is_recording());
        assert!(wav_bytes.len() > 44);
        // Validate standard RIFF and WAVE magic headers
        assert_eq!(&wav_bytes[0..4], b"RIFF");
        assert_eq!(&wav_bytes[8..12], b"WAVE");
    }

    #[test]
    fn test_parse_macos_shortcut() {
        // Command+;
        let sc1 = parse_macos_shortcut("Command+;").expect("should parse Command+;");
        assert_eq!(sc1.key_char, ";");
        assert_eq!(sc1.modifier_mask, 0x0010_0000);
        assert_eq!(sc1.display_label, "⌘;");

        // Option+Space
        let sc2 = parse_macos_shortcut("Option+Space").expect("should parse Option+Space");
        assert_eq!(sc2.key_char, " ");
        assert_eq!(sc2.modifier_mask, 0x0008_0000);
        assert_eq!(sc2.display_label, "⌥Space");

        // Ctrl+Space / Alt+Space
        let sc3 = parse_macos_shortcut("Ctrl+Space").expect("should parse Ctrl+Space");
        assert_eq!(sc3.key_char, " ");
        assert_eq!(sc3.modifier_mask, 0x0004_0000);
        assert_eq!(sc3.display_label, "⌃Space");

        // Command+Shift+F
        let sc4 = parse_macos_shortcut("Command+Shift+f").expect("should parse Command+Shift+f");
        assert_eq!(sc4.key_char, "f");
        assert_eq!(sc4.modifier_mask, 0x0010_0000 | 0x0002_0000);
        assert_eq!(sc4.display_label, "⌘⇧F");
    }

    #[tokio::test]
    async fn test_platform_commands() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(16);
        set_global_command_tx(tx);

        send_platform_command(PlatformCommand::ToggleWindow);
        assert_eq!(rx.recv().await.unwrap(), PlatformCommand::ToggleWindow);

        send_platform_command(PlatformCommand::OpenSettings);
        assert_eq!(rx.recv().await.unwrap(), PlatformCommand::OpenSettings);

        send_platform_command(PlatformCommand::Quit);
        assert_eq!(rx.recv().await.unwrap(), PlatformCommand::Quit);
    }
}

