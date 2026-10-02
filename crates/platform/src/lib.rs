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

static GLOBAL_HOTKEY_TX: std::sync::RwLock<Option<broadcast::Sender<()>>> =
    std::sync::RwLock::new(None);

pub fn set_global_hotkey_tx(tx: broadcast::Sender<()>) {
    if let Ok(mut lock) = GLOBAL_HOTKEY_TX.write() {
        *lock = Some(tx);
    }
}

pub fn get_global_hotkey_tx() -> Option<broadcast::Sender<()>> {
    GLOBAL_HOTKEY_TX.read().ok().and_then(|lock| lock.clone())
}

pub fn trigger_global_hotkey() {
    if let Some(tx) = get_global_hotkey_tx() {
        tracing::info!("Triggering global hotkey notification from UI / menu bar / tray");
        let _ = tx.send(());
    }
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
            }
        }

        fn cleanup_hotkey_sync(&self) {
            let hk = self.hotkey_ref.swap(0, Ordering::SeqCst);
            let hr = self.handler_ref.swap(0, Ordering::SeqCst);
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
                tracing::info!("🔥 GLOBAL HOTKEY CALLBACK FIRED");
                tracing::info!(
                    event_ptr = ?the_event,
                    "kEventHotKeyPressed event received in Carbon handler"
                );

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
                let target = GetApplicationEventTarget();
                if target.is_null() {
                    tracing::error!("GetApplicationEventTarget() returned NULL!");
                } else {
                    tracing::info!(
                        target = ?target,
                        "Retrieved ApplicationEventTarget for Carbon hotkey handling"
                    );
                }

                let mut handler_ref: *mut std::ffi::c_void = std::ptr::null_mut();
                let mut hotkey_ref: *mut std::ffi::c_void = std::ptr::null_mut();

                tracing::info!(
                    shortcut,
                    key_code,
                    modifier_mask = format!("{:#06x}", modifiers),
                    "Installing Carbon event handler on ApplicationEventTarget"
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

        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title_wide.as_ptr());
            if hwnd != 0 {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let new_ex = (ex | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
                SetWindowPos(
                    hwnd,
                    0,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
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

/// Activate the application on macOS, bringing it to the foreground even if another app is active.
pub fn macos_activate_app() {
    #[cfg(target_os = "macos")]
    unsafe {
        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSendActivate =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, bool) -> *mut std::ffi::c_void;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_activate: MsgSendActivate =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let ns_app_class = objc_getClass(c"NSApplication".as_ptr());
        if !ns_app_class.is_null() {
            let shared_app_sel = sel_registerName(c"sharedApplication".as_ptr());
            let app = msg_send_0(ns_app_class, shared_app_sel);
            if !app.is_null() {
                let activate_sel = sel_registerName(c"activateIgnoringOtherApps:".as_ptr());
                let _ = msg_send_activate(app, activate_sel, true);
                tracing::info!("Activated macOS application via activateIgnoringOtherApps");
            }
        }
    }
}

/// Hide/dismiss the application on macOS, hiding all windows without terminating the process.
pub fn macos_hide_app() {
    #[cfg(target_os = "macos")]
    unsafe {
        type MsgSend0 =
            unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        type MsgSendHide = unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;

        extern "C" {
            fn objc_getClass(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
            fn objc_msgSend();
        }

        let msg_send_0: MsgSend0 = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        let msg_send_hide: MsgSendHide =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());

        let ns_app_class = objc_getClass(c"NSApplication".as_ptr());
        if !ns_app_class.is_null() {
            let shared_app_sel = sel_registerName(c"sharedApplication".as_ptr());
            let app = msg_send_0(ns_app_class, shared_app_sel);
            if !app.is_null() {
                let hide_sel = sel_registerName(c"hide:".as_ptr());
                let _ = msg_send_hide(app, hide_sel, std::ptr::null_mut());
                tracing::info!("Dismissed macOS application via [NSApp hide:]");
            }
        }
    }
}

/// Setup macOS menu bar status item (top bar icon) that toggles Function when clicked.
pub fn setup_macos_menu_bar_icon() {
    #[cfg(target_os = "macos")]
    unsafe {
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
                unsafe extern "C" fn on_click(
                    _this: *mut std::ffi::c_void,
                    _cmd: *mut std::ffi::c_void,
                    _sender: *mut std::ffi::c_void,
                ) {
                    tracing::info!("Top bar (menu bar) status item clicked");
                    trigger_global_hotkey();
                }
                let sel = sel_registerName(c"onStatusItemClick:".as_ptr());
                class_addMethod(target_cls, sel, on_click, c"v@:@".as_ptr());
                objc_registerClassPair(target_cls);
            }
        }

        if target_cls.is_null() {
            tracing::warn!("Failed to create FunctionStatusItemTarget class");
            return;
        }

        let alloc_sel = sel_registerName(c"alloc".as_ptr());
        let init_sel = sel_registerName(c"init".as_ptr());
        let target_inst = msg_send_0(msg_send_0(target_cls, alloc_sel), init_sel);

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

        let button_sel = sel_registerName(c"button".as_ptr());
        let button = msg_send_0(status_item, button_sel);
        if !button.is_null() {
            let ns_string = objc_getClass(c"NSString".as_ptr());
            let utf8_sel = sel_registerName(c"stringWithUTF8String:".as_ptr());

            static TITLE_CSTR: &std::ffi::CStr = c"Function";
            let title = msg_send_cstr(ns_string, utf8_sel, TITLE_CSTR.as_ptr());
            if !title.is_null() {
                let set_title_sel = sel_registerName(c"setTitle:".as_ptr());
                msg_send_1(button, set_title_sel, title);
            }

            static TIP_CSTR: &std::ffi::CStr = c"Function (Command+;)";
            let tip = msg_send_cstr(ns_string, utf8_sel, TIP_CSTR.as_ptr());
            if !tip.is_null() {
                let set_tip_sel = sel_registerName(c"setToolTip:".as_ptr());
                msg_send_1(button, set_tip_sel, tip);
            }

            let set_target_sel = sel_registerName(c"setTarget:".as_ptr());
            msg_send_1(button, set_target_sel, target_inst);

            let set_action_sel = sel_registerName(c"setAction:".as_ptr());
            let action_sel = sel_registerName(c"onStatusItemClick:".as_ptr());
            msg_send_1(button, set_action_sel, action_sel);

            tracing::info!("macOS top bar (menu bar) Function status item created successfully");
        }
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
