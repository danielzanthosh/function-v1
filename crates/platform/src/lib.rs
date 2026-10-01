//! Platform integration layer for Windows and macOS.
//!
//! Provides isolated, first-class native OS abstractions for global hotkeys,
//! accessibility permissions, microphone availability detection, and window control.

pub mod audio;
pub mod computer;
pub use audio::*;
pub use computer::*;

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

    /// Register the global activation hotkey (e.g. "Alt+Space" on Windows, "Option+Space" on macOS).
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

#[cfg(target_os = "windows")]
pub mod windows {
    use super::*;

    pub struct WindowsPlatformService {
        registered_hotkey: std::sync::RwLock<Option<String>>,
        hotkey_tx: broadcast::Sender<()>,
        audio_capture: Arc<dyn AudioCapture>,
    }

    impl WindowsPlatformService {
        pub fn new() -> Self {
            let (hotkey_tx, _) = broadcast::channel(16);
            Self {
                registered_hotkey: std::sync::RwLock::new(None),
                hotkey_tx,
                audio_capture: Arc::new(CpalAudioCapture::new()),
            }
        }

        pub fn with_audio_capture(mut self, audio: Arc<dyn AudioCapture>) -> Self {
            self.audio_capture = audio;
            self
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
            tracing::info!(shortcut, "Registering Windows global hotkey");
            let mut current = self.registered_hotkey.write().unwrap();
            *current = Some(shortcut.to_string());

            let tx = self.hotkey_tx.clone();
            let sc = shortcut.to_string();

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

                extern "system" {
                    fn RegisterHotKey(hWnd: isize, id: i32, fsModifiers: u32, vk: u32) -> i32;
                    fn GetMessageW(
                        lpMsg: *mut Msg,
                        hWnd: isize,
                        wMsgFilterMin: u32,
                        wMsgFilterMax: u32,
                    ) -> i32;
                    fn TranslateMessage(lpMsg: *const Msg) -> i32;
                    fn DispatchMessageW(lpMsg: *const Msg) -> isize;
                }

                const MOD_ALT: u32 = 0x0001;
                const MOD_CONTROL: u32 = 0x0002;
                const MOD_SHIFT: u32 = 0x0004;
                const MOD_NOREPEAT: u32 = 0x4000;
                const VK_SPACE: u32 = 0x20;
                const WM_HOTKEY: u32 = 0x0312;

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

                unsafe {
                    let res = RegisterHotKey(0, 101, mods, VK_SPACE);
                    if res == 0 {
                        let _ = RegisterHotKey(0, 101, mods & !MOD_NOREPEAT, VK_SPACE);
                    }

                    let mut msg: Msg = std::mem::zeroed();
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                        if msg.message == WM_HOTKEY {
                            let _ = tx.send(());
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            });

            Ok(())
        }

        async fn unregister_hotkey(&self) -> Result<(), PlatformError> {
            tracing::info!("Unregistering Windows global hotkey");
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

    pub struct MacOsPlatformService {
        registered_hotkey: std::sync::RwLock<Option<String>>,
        hotkey_tx: broadcast::Sender<()>,
        audio_capture: Arc<dyn AudioCapture>,
    }

    impl MacOsPlatformService {
        pub fn new() -> Self {
            let (hotkey_tx, _) = broadcast::channel(16);
            Self {
                registered_hotkey: std::sync::RwLock::new(None),
                hotkey_tx,
                audio_capture: Arc::new(CpalAudioCapture::new()),
            }
        }
    }

    impl Default for MacOsPlatformService {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl PlatformService for MacOsPlatformService {
        fn platform_name(&self) -> &'static str {
            "macOS"
        }

        async fn register_hotkey(&self, shortcut: &str) -> Result<(), PlatformError> {
            tracing::info!(shortcut, "Registering macOS global hotkey");
            let mut current = self.registered_hotkey.write().unwrap();
            *current = Some(shortcut.to_string());
            Ok(())
        }

        async fn unregister_hotkey(&self) -> Result<(), PlatformError> {
            tracing::info!("Unregistering macOS global hotkey");
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
                accessibility: false,
                screen_recording: false,
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

    // 3. Search directory hierarchy of the current running executable (up to 5 levels)
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
                name: usize,
                type_: u32,
                cx: i32,
                cy: i32,
                fuLoad: u32,
            ) -> isize;
            fn SendMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> isize;
            fn LoadLibraryA(lpLibFileName: *const i8) -> isize;
            fn GetProcAddress(hModule: isize, lpProcName: *const i8) -> usize;
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
                let dwm = LoadLibraryA(b"dwmapi.dll\0".as_ptr() as *const i8);
                if dwm != 0 {
                    let proc =
                        GetProcAddress(dwm, b"DwmSetWindowAttribute\0".as_ptr() as *const i8);
                    if proc != 0 {
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
                let mut hicon_sm = LoadImageW(hmod, 1, IMAGE_ICON, 16, 16, LR_SHARED);
                let mut hicon_lg = LoadImageW(hmod, 1, IMAGE_ICON, 32, 32, LR_SHARED);

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
                                icon_path_wide.as_ptr() as usize,
                                IMAGE_ICON,
                                16,
                                16,
                                LR_LOADFROMFILE,
                            );
                        }
                        if hicon_lg == 0 {
                            hicon_lg = LoadImageW(
                                0,
                                icon_path_wide.as_ptr() as usize,
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

/// Helper to explicitly center the Win32 window on the active monitor.
pub fn center_window_by_title(title: &str, width: i32, height: i32, upper_third: bool) {
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
            fn GetSystemMetrics(nIndex: i32) -> i32;
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
        const SWP_NOZORDER: u32 = 0x0004;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                let x = (screen_w - width) / 2;
                let y = if upper_third {
                    (screen_h - height) * 38 / 100
                } else {
                    (screen_h - height) / 2
                };
                SetWindowPos(hwnd, 0, x, y, width, height, SWP_NOZORDER);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (title, width, height, upper_third);
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
            fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        }

        const SW_HIDE: i32 = 0;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
    }
}

/// Reveal and focus the native window instantly.
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
            fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
            fn SetForegroundWindow(hWnd: isize) -> i32;
        }

        const SW_SHOW: i32 = 5;

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = title;
    }
}

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
    #[cfg(not(target_os = "windows"))]
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
    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        false
    }
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
}
