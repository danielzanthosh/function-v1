//! Native computer control operations: mouse, keyboard, screen, and application management.

use crate::PlatformError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: isize,
    pub title: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenDimensions {
    pub width: u32,
    pub height: u32,
}

pub trait ComputerControl: Send + Sync {
    fn get_screen_dimensions(&self) -> ScreenDimensions;
    fn get_cursor_position(&self) -> (i32, i32);
    fn mouse_move(&self, x: i32, y: i32) -> Result<(), PlatformError>;
    fn mouse_click(&self, button: MouseButton) -> Result<(), PlatformError>;
    fn mouse_double_click(&self, button: MouseButton) -> Result<(), PlatformError>;
    fn mouse_scroll(&self, delta: i32) -> Result<(), PlatformError>;
    fn mouse_drag(
        &self,
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
    ) -> Result<(), PlatformError>;
    fn keyboard_type(&self, text: &str) -> Result<(), PlatformError>;
    /// Move through short interpolated steps so automation looks deliberate
    /// and gives the target application time to process pointer events.
    fn mouse_move_smooth(&self, x: i32, y: i32) -> Result<(), PlatformError> {
        let (start_x, start_y) = self.get_cursor_position();
        let steps = smooth_motion_steps(start_x, start_y, x, y);
        for step in 1..=steps {
            let progress = step as f32 / steps as f32;
            let next_x = start_x as f32 + (x - start_x) as f32 * progress;
            let next_y = start_y as f32 + (y - start_y) as f32 * progress;
            self.mouse_move(next_x.round() as i32, next_y.round() as i32)?;
            if step < steps {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
        }
        Ok(())
    }
    /// Type with a small inter-character cadence instead of injecting the
    /// entire string in one burst.
    fn keyboard_type_smooth(&self, text: &str) -> Result<(), PlatformError> {
        let character_count = text.chars().count();
        for (index, character) in text.chars().enumerate() {
            self.keyboard_type(&character.to_string())?;
            if index + 1 < character_count {
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
        }
        Ok(())
    }
    fn keyboard_press(&self, key: &str) -> Result<(), PlatformError>;
    fn keyboard_shortcut(&self, keys: &[&str]) -> Result<(), PlatformError>;
    fn app_launch(&self, app_path: &str, args: &[&str]) -> Result<u32, PlatformError>;
    fn list_windows(&self) -> Vec<WindowInfo>;
    fn focus_window(&self, title_substring: &str) -> Result<bool, PlatformError>;
    fn take_screenshot(&self) -> Result<Vec<u8>, PlatformError>;
}

pub fn smooth_motion_steps(start_x: i32, start_y: i32, end_x: i32, end_y: i32) -> u32 {
    let distance = (((end_x - start_x) as f64).powi(2) + ((end_y - start_y) as f64).powi(2)).sqrt();
    (distance / 70.0).ceil().clamp(2.0, 24.0) as u32
}

#[cfg(test)]
mod motion_tests {
    use super::smooth_motion_steps;

    #[test]
    fn motion_uses_more_steps_for_longer_paths() {
        assert_eq!(smooth_motion_steps(10, 10, 10, 10), 2);
        assert!(smooth_motion_steps(0, 0, 1000, 0) > smooth_motion_steps(0, 0, 100, 0));
        assert_eq!(smooth_motion_steps(0, 0, 10_000, 0), 24);
    }
}

#[cfg(target_os = "windows")]
pub mod windows {
    use super::*;
    use std::process::Command;

    #[repr(C)]
    #[derive(Default, Copy, Clone)]
    struct POINT {
        x: i32,
        y: i32,
    }

    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;

    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
    const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
    const MOUSEEVENTF_WHEEL: u32 = 0x0800;

    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_UNICODE: u32 = 0x0004;

    const VK_CONTROL: u8 = 0x11;
    const VK_SHIFT: u8 = 0x10;
    const VK_MENU: u8 = 0x12; // Alt
    const VK_RETURN: u8 = 0x0D;
    const VK_ESCAPE: u8 = 0x1B;
    const VK_TAB: u8 = 0x09;
    const VK_BACK: u8 = 0x08;
    const VK_SPACE: u8 = 0x20;

    extern "system" {
        fn GetSystemMetrics(nIndex: i32) -> i32;
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
        fn SetCursorPos(X: i32, Y: i32) -> i32;
        fn mouse_event(dwFlags: u32, dx: u32, dy: u32, dwData: u32, dwExtraInfo: usize);
        fn keybd_event(bVk: u8, bScan: u8, dwFlags: u32, dwExtraInfo: usize);
        fn GetForegroundWindow() -> isize;
        fn SetForegroundWindow(hWnd: isize) -> i32;
        fn GetWindowTextW(hWnd: isize, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn IsWindowVisible(hWnd: isize) -> i32;
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(isize, isize) -> i32,
            lParam: isize,
        ) -> i32;
    }

    pub struct WindowsComputerControl;

    impl WindowsComputerControl {
        pub fn new() -> Self {
            Self
        }

        fn key_name_to_vk(key: &str) -> Option<u8> {
            match key.to_lowercase().as_str() {
                "ctrl" | "control" => Some(VK_CONTROL),
                "shift" => Some(VK_SHIFT),
                "alt" => Some(VK_MENU),
                "enter" | "return" => Some(VK_RETURN),
                "esc" | "escape" => Some(VK_ESCAPE),
                "tab" => Some(VK_TAB),
                "backspace" => Some(VK_BACK),
                "space" => Some(VK_SPACE),
                k if k.len() == 1 => {
                    let ch = k.chars().next().unwrap().to_ascii_uppercase();
                    if ch.is_ascii_alphanumeric() {
                        Some(ch as u8)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

    impl Default for WindowsComputerControl {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ComputerControl for WindowsComputerControl {
        fn get_screen_dimensions(&self) -> ScreenDimensions {
            unsafe {
                let width = GetSystemMetrics(SM_CXSCREEN).max(800) as u32;
                let height = GetSystemMetrics(SM_CYSCREEN).max(600) as u32;
                ScreenDimensions { width, height }
            }
        }

        fn get_cursor_position(&self) -> (i32, i32) {
            unsafe {
                let mut pt = POINT::default();
                GetCursorPos(&mut pt);
                (pt.x, pt.y)
            }
        }

        fn mouse_move(&self, x: i32, y: i32) -> Result<(), PlatformError> {
            unsafe {
                let res = SetCursorPos(x, y);
                if res == 0 {
                    Err(PlatformError::SystemApi(
                        "Failed to set cursor position".into(),
                    ))
                } else {
                    Ok(())
                }
            }
        }

        fn mouse_click(&self, button: MouseButton) -> Result<(), PlatformError> {
            unsafe {
                match button {
                    MouseButton::Left => {
                        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
                        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
                    }
                    MouseButton::Right => {
                        mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
                        mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
                    }
                    MouseButton::Middle => {
                        mouse_event(MOUSEEVENTF_MIDDLEDOWN, 0, 0, 0, 0);
                        mouse_event(MOUSEEVENTF_MIDDLEUP, 0, 0, 0, 0);
                    }
                }
                Ok(())
            }
        }

        fn mouse_double_click(&self, button: MouseButton) -> Result<(), PlatformError> {
            self.mouse_click(button)?;
            std::thread::sleep(std::time::Duration::from_millis(60));
            self.mouse_click(button)
        }

        fn mouse_scroll(&self, delta: i32) -> Result<(), PlatformError> {
            unsafe {
                let wheel_delta = (delta * 120) as u32;
                mouse_event(MOUSEEVENTF_WHEEL, 0, 0, wheel_delta, 0);
                Ok(())
            }
        }

        fn mouse_drag(
            &self,
            start_x: i32,
            start_y: i32,
            end_x: i32,
            end_y: i32,
        ) -> Result<(), PlatformError> {
            self.mouse_move(start_x, start_y)?;
            unsafe {
                mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            self.mouse_move(end_x, end_y)?;
            std::thread::sleep(std::time::Duration::from_millis(50));
            unsafe {
                mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
            }
            Ok(())
        }

        fn keyboard_type(&self, text: &str) -> Result<(), PlatformError> {
            unsafe {
                for ch in text.encode_utf16() {
                    keybd_event(0, ch as u8, KEYEVENTF_UNICODE, 0);
                    keybd_event(0, ch as u8, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, 0);
                }
            }
            Ok(())
        }

        fn keyboard_press(&self, key: &str) -> Result<(), PlatformError> {
            if let Some(vk) = Self::key_name_to_vk(key) {
                unsafe {
                    keybd_event(vk, 0, 0, 0);
                    keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
                }
                Ok(())
            } else {
                self.keyboard_type(key)
            }
        }

        fn keyboard_shortcut(&self, keys: &[&str]) -> Result<(), PlatformError> {
            let vks: Vec<u8> = keys
                .iter()
                .filter_map(|k| Self::key_name_to_vk(k))
                .collect();

            unsafe {
                for &vk in &vks {
                    keybd_event(vk, 0, 0, 0);
                }
                for &vk in vks.iter().rev() {
                    keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
                }
            }
            Ok(())
        }

        fn app_launch(&self, app_path: &str, args: &[&str]) -> Result<u32, PlatformError> {
            let mut cmd = Command::new(app_path);
            cmd.args(args);
            // CREATE_NO_WINDOW if background, or normal spawn
            match cmd.spawn() {
                Ok(child) => Ok(child.id()),
                Err(e) => Err(PlatformError::SystemApi(format!(
                    "Failed to launch {}: {}",
                    app_path, e
                ))),
            }
        }

        fn list_windows(&self) -> Vec<WindowInfo> {
            let mut windows: Vec<WindowInfo> = Vec::new();
            let windows_ptr = &mut windows as *mut Vec<WindowInfo> as isize;

            unsafe extern "system" fn enum_proc(hwnd: isize, lparam: isize) -> i32 {
                if IsWindowVisible(hwnd) != 0 {
                    let mut buf = [0u16; 512];
                    let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
                    if len > 0 {
                        let title = String::from_utf16_lossy(&buf[..len as usize]);
                        let trimmed = title.trim();
                        if !trimmed.is_empty() {
                            let windows_list = &mut *(lparam as *mut Vec<WindowInfo>);
                            let active_hwnd = GetForegroundWindow();
                            windows_list.push(WindowInfo {
                                id: hwnd,
                                title: trimmed.to_string(),
                                is_active: hwnd == active_hwnd,
                            });
                        }
                    }
                }
                1 // continue enumeration
            }

            unsafe {
                EnumWindows(enum_proc, windows_ptr);
            }

            windows
        }

        fn focus_window(&self, title_substring: &str) -> Result<bool, PlatformError> {
            let windows = self.list_windows();
            let needle = title_substring.to_lowercase();
            if let Some(w) = windows
                .iter()
                .find(|w| w.title.to_lowercase().contains(&needle))
            {
                unsafe {
                    SetForegroundWindow(w.id);
                }
                Ok(true)
            } else {
                Ok(false)
            }
        }

        fn take_screenshot(&self) -> Result<Vec<u8>, PlatformError> {
            let temp_dir = std::env::temp_dir();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let path = temp_dir.join(format!("function_screen_{}.png", ts));
            let path_str = path.to_string_lossy().to_string();

            let ps_script = format!(
                "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; \
                $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
                $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height; \
                $g = [System.Drawing.Graphics]::FromImage($bmp); \
                $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size); \
                $bmp.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png); \
                $g.Dispose(); $bmp.Dispose();",
                path_str.replace('\\', "\\\\")
            );

            let out = Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                .output()
                .map_err(|e| {
                    PlatformError::SystemApi(format!("Failed to execute screenshot script: {}", e))
                })?;

            if !out.status.success() || !path.exists() {
                return Err(PlatformError::SystemApi(
                    "Failed to capture Windows screenshot".into(),
                ));
            }

            let bytes = std::fs::read(&path).map_err(|e| {
                PlatformError::SystemApi(format!("Failed to read screenshot image: {}", e))
            })?;
            let _ = std::fs::remove_file(&path);
            Ok(bytes)
        }
    }
}

#[cfg(target_os = "macos")]
pub mod macos {
    use super::*;
    use std::process::Command;

    pub struct MacOsComputerControl;

    impl MacOsComputerControl {
        pub fn new() -> Self {
            Self
        }
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsWide(display: u32) -> usize;
        fn CGDisplayPixelsHigh(display: u32) -> usize;
        fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CGEventSourceCreate(state_id: u32) -> *mut std::ffi::c_void;
        fn CGPreflightPostEventAccess() -> bool;
        fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
        fn CGWarpMouseCursorPosition(new_pos: CGPoint) -> i32;
        fn CGEventCreateMouseEvent(
            source: *const std::ffi::c_void,
            mouse_type: u32,
            mouse_cursor_position: CGPoint,
            mouse_button: u32,
        ) -> *mut std::ffi::c_void;
        fn CGEventCreateScrollWheelEvent(
            source: *const std::ffi::c_void,
            units: u32,
            wheel_count: u32,
            wheel_1: i32,
        ) -> *mut std::ffi::c_void;
        fn CGEventCreateKeyboardEvent(
            source: *const std::ffi::c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut std::ffi::c_void;
        fn CGEventKeyboardSetUnicodeString(
            event: *mut std::ffi::c_void,
            string_length: usize,
            unicode_string: *const u16,
        );
        fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
        fn CGEventSetIntegerValueField(event: *mut std::ffi::c_void, field: u32, value: i64);
        fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
        fn CFRelease(cf: *mut std::ffi::c_void);
    }

    const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
    const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
    const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
    const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
    const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
    const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
    const K_CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;
    const K_CG_EVENT_OTHER_MOUSE_UP: u32 = 26;
    const K_CG_MOUSE_EVENT_CLICK_STATE: u32 = 1;
    const K_CG_HID_EVENT_TAP: u32 = 0;

    impl ComputerControl for MacOsComputerControl {
        fn get_screen_dimensions(&self) -> ScreenDimensions {
            unsafe {
                let disp = CGMainDisplayID();
                let w = CGDisplayPixelsWide(disp) as u32;
                let h = CGDisplayPixelsHigh(disp) as u32;
                if w > 0 && h > 0 {
                    ScreenDimensions {
                        width: w,
                        height: h,
                    }
                } else {
                    ScreenDimensions {
                        width: 1920,
                        height: 1080,
                    }
                }
            }
        }

        fn get_cursor_position(&self) -> (i32, i32) {
            unsafe {
                let ev = CGEventCreate(std::ptr::null());
                if !ev.is_null() {
                    let loc = CGEventGetLocation(ev);
                    CFRelease(ev);
                    (loc.x as i32, loc.y as i32)
                } else {
                    (0, 0)
                }
            }
        }

        fn mouse_move(&self, x: i32, y: i32) -> Result<(), PlatformError> {
            let pt = CGPoint {
                x: x as f64,
                y: y as f64,
            };
            unsafe {
                let _ = CGWarpMouseCursorPosition(pt);
                let ev = CGEventCreateMouseEvent(std::ptr::null(), K_CG_EVENT_MOUSE_MOVED, pt, 0);
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            }
            Ok(())
        }

        fn mouse_click(&self, button: MouseButton) -> Result<(), PlatformError> {
            // CGEventPost requires macOS Post Event permission. Accessibility being
            // enabled in the UI is not enough to assume synthetic events are accepted.
            unsafe {
                if !CGPreflightPostEventAccess() {
                    return Err(PlatformError::SystemApi(
                        "macOS Post Event access is denied. Enable Function under System Settings > Privacy & Security > Accessibility.".into(),
                    ));
                }
            }

            let (x, y) = self.get_cursor_position();
            let pt = CGPoint {
                x: x as f64,
                y: y as f64,
            };
            let (down_type, up_type, btn_num) = match button {
                MouseButton::Left => (K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_UP, 0),
                MouseButton::Right => (K_CG_EVENT_RIGHT_MOUSE_DOWN, K_CG_EVENT_RIGHT_MOUSE_UP, 1),
                MouseButton::Middle => (K_CG_EVENT_OTHER_MOUSE_DOWN, K_CG_EVENT_OTHER_MOUSE_UP, 2),
            };

            unsafe {
                // Use an explicit HID-system event source so synthetic clicks are
                // treated consistently with the real pointer event stream.
                let source = CGEventSourceCreate(1);
                if source.is_null() {
                    return Err(PlatformError::SystemApi(
                        "Failed to create macOS HID event source".into(),
                    ));
                }

                let down = CGEventCreateMouseEvent(source, down_type, pt, btn_num);
                if down.is_null() {
                    CFRelease(source);
                    return Err(PlatformError::SystemApi(
                        "Failed to create mouse-down event".into(),
                    ));
                }
                CGEventSetIntegerValueField(down, K_CG_MOUSE_EVENT_CLICK_STATE, 1);
                CGEventPost(K_CG_HID_EVENT_TAP, down);
                CFRelease(down);

                std::thread::sleep(std::time::Duration::from_millis(35));

                let up = CGEventCreateMouseEvent(source, up_type, pt, btn_num);
                if up.is_null() {
                    CFRelease(source);
                    return Err(PlatformError::SystemApi(
                        "Failed to create mouse-up event".into(),
                    ));
                }
                CGEventSetIntegerValueField(up, K_CG_MOUSE_EVENT_CLICK_STATE, 1);
                CGEventPost(K_CG_HID_EVENT_TAP, up);
                CFRelease(up);
                CFRelease(source);
            }

            tracing::debug!(x, y, ?button, "Posted macOS synthetic mouse click");
            Ok(())
        }

        fn mouse_double_click(&self, button: MouseButton) -> Result<(), PlatformError> {
            self.mouse_click(button)?;
            std::thread::sleep(std::time::Duration::from_millis(60));
            let (x, y) = self.get_cursor_position();
            let pt = CGPoint {
                x: x as f64,
                y: y as f64,
            };
            let (down_type, up_type, btn_num) = match button {
                MouseButton::Left => (K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_UP, 0),
                MouseButton::Right => (K_CG_EVENT_RIGHT_MOUSE_DOWN, K_CG_EVENT_RIGHT_MOUSE_UP, 1),
                MouseButton::Middle => (K_CG_EVENT_OTHER_MOUSE_DOWN, K_CG_EVENT_OTHER_MOUSE_UP, 2),
            };

            unsafe {
                let down = CGEventCreateMouseEvent(std::ptr::null(), down_type, pt, btn_num);
                if !down.is_null() {
                    CGEventSetIntegerValueField(down, K_CG_MOUSE_EVENT_CLICK_STATE, 2);
                    CGEventPost(K_CG_HID_EVENT_TAP, down);
                    CFRelease(down);
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
                let up = CGEventCreateMouseEvent(std::ptr::null(), up_type, pt, btn_num);
                if !up.is_null() {
                    CGEventSetIntegerValueField(up, K_CG_MOUSE_EVENT_CLICK_STATE, 2);
                    CGEventPost(K_CG_HID_EVENT_TAP, up);
                    CFRelease(up);
                }
            }
            Ok(())
        }

        fn mouse_scroll(&self, delta: i32) -> Result<(), PlatformError> {
            unsafe {
                let ev = CGEventCreateScrollWheelEvent(std::ptr::null(), 1, 1, delta);
                if !ev.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, ev);
                    CFRelease(ev);
                }
            }
            Ok(())
        }

        fn mouse_drag(
            &self,
            start_x: i32,
            start_y: i32,
            end_x: i32,
            end_y: i32,
        ) -> Result<(), PlatformError> {
            self.mouse_move(start_x, start_y)?;
            let start_pt = CGPoint {
                x: start_x as f64,
                y: start_y as f64,
            };
            let end_pt = CGPoint {
                x: end_x as f64,
                y: end_y as f64,
            };

            unsafe {
                let down = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_DOWN,
                    start_pt,
                    0,
                );
                if !down.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, down);
                    CFRelease(down);
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
                let drag = CGEventCreateMouseEvent(
                    std::ptr::null(),
                    K_CG_EVENT_LEFT_MOUSE_DRAGGED,
                    end_pt,
                    0,
                );
                if !drag.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, drag);
                    CFRelease(drag);
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
                let up =
                    CGEventCreateMouseEvent(std::ptr::null(), K_CG_EVENT_LEFT_MOUSE_UP, end_pt, 0);
                if !up.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, up);
                    CFRelease(up);
                }
            }
            Ok(())
        }

        fn keyboard_type(&self, text: &str) -> Result<(), PlatformError> {
            for ch in text.chars() {
                let mut utf16_buf = [0u16; 2];
                let encoded = ch.encode_utf16(&mut utf16_buf);
                unsafe {
                    let down = CGEventCreateKeyboardEvent(std::ptr::null(), 0, true);
                    if !down.is_null() {
                        CGEventKeyboardSetUnicodeString(down, encoded.len(), encoded.as_ptr());
                        CGEventPost(K_CG_HID_EVENT_TAP, down);
                        CFRelease(down);
                    }
                    let up = CGEventCreateKeyboardEvent(std::ptr::null(), 0, false);
                    if !up.is_null() {
                        CGEventKeyboardSetUnicodeString(up, encoded.len(), encoded.as_ptr());
                        CGEventPost(K_CG_HID_EVENT_TAP, up);
                        CFRelease(up);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(())
        }

        fn keyboard_press(&self, key: &str) -> Result<(), PlatformError> {
            let keycode = match key.to_lowercase().as_str() {
                "return" | "enter" => 36,
                "tab" => 48,
                "space" => 49,
                "delete" | "backspace" => 51,
                "escape" | "esc" => 53,
                "command" | "cmd" => 55,
                "shift" => 56,
                "option" | "alt" => 58,
                "control" | "ctrl" => 59,
                "up" => 126,
                "down" => 125,
                "left" => 123,
                "right" => 124,
                "a" => 0,
                "s" => 1,
                "d" => 2,
                "f" => 3,
                "h" => 4,
                "g" => 5,
                "z" => 6,
                "x" => 7,
                "c" => 8,
                "v" => 9,
                "b" => 11,
                "q" => 12,
                "w" => 13,
                "e" => 14,
                "r" => 15,
                "y" => 16,
                "t" => 17,
                "1" => 18,
                "2" => 19,
                "3" => 20,
                "4" => 21,
                "6" => 22,
                "5" => 23,
                "9" => 25,
                "7" => 26,
                "8" => 28,
                "0" => 29,
                "o" => 31,
                "u" => 32,
                "i" => 34,
                "p" => 35,
                "l" => 37,
                "j" => 38,
                "k" => 40,
                "n" => 45,
                "m" => 46,
                _ => return self.keyboard_type(key),
            };

            unsafe {
                let down = CGEventCreateKeyboardEvent(std::ptr::null(), keycode, true);
                if !down.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, down);
                    CFRelease(down);
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
                let up = CGEventCreateKeyboardEvent(std::ptr::null(), keycode, false);
                if !up.is_null() {
                    CGEventPost(K_CG_HID_EVENT_TAP, up);
                    CFRelease(up);
                }
            }
            Ok(())
        }

        fn keyboard_shortcut(&self, keys: &[&str]) -> Result<(), PlatformError> {
            let mut flags: u64 = 0;
            let mut main_key = "";

            for &k in keys {
                let lowercase_key = k.to_lowercase();
                match lowercase_key.as_str() {
                    "cmd" | "command" => flags |= 0x0010_0000,
                    "shift" => flags |= 0x0002_0000,
                    "alt" | "option" => flags |= 0x0008_0000,
                    "ctrl" | "control" => flags |= 0x0004_0000,
                    _ => main_key = k,
                }
            }

            if !main_key.is_empty() {
                let keycode = match main_key.to_lowercase().as_str() {
                    "return" | "enter" => 36,
                    "tab" => 48,
                    "space" => 49,
                    "delete" | "backspace" => 51,
                    "escape" | "esc" => 53,
                    "a" => 0,
                    "c" => 8,
                    "v" => 9,
                    "x" => 7,
                    "z" => 6,
                    "q" => 12,
                    "w" => 13,
                    "t" => 17,
                    "r" => 15,
                    "f" => 3,
                    "s" => 1,
                    _ => 0,
                };

                unsafe {
                    let down = CGEventCreateKeyboardEvent(std::ptr::null(), keycode, true);
                    if !down.is_null() {
                        CGEventSetFlags(down, flags);
                        CGEventPost(K_CG_HID_EVENT_TAP, down);
                        CFRelease(down);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    let up = CGEventCreateKeyboardEvent(std::ptr::null(), keycode, false);
                    if !up.is_null() {
                        CGEventSetFlags(up, flags);
                        CGEventPost(K_CG_HID_EVENT_TAP, up);
                        CFRelease(up);
                    }
                }
            }
            Ok(())
        }

        fn app_launch(&self, app_path: &str, args: &[&str]) -> Result<u32, PlatformError> {
            let mut cmd = if app_path.starts_with('/') {
                let mut c = Command::new(app_path);
                c.args(args);
                c
            } else {
                let mut c = Command::new("open");
                c.arg("-a").arg(app_path);
                if !args.is_empty() {
                    c.arg("--args");
                    c.args(args);
                }
                c
            };

            match cmd.spawn() {
                Ok(child) => Ok(child.id()),
                Err(e) => Err(PlatformError::SystemApi(format!(
                    "Failed to launch {}: {}",
                    app_path, e
                ))),
            }
        }

        fn list_windows(&self) -> Vec<WindowInfo> {
            let script = "tell application \"System Events\" to get {name, id} of (every process whose background only is false)";
            let out = Command::new("osascript").args(["-e", script]).output();
            let mut res = Vec::new();
            if let Ok(o) = out {
                if o.status.success() {
                    let text = String::from_utf8_lossy(&o.stdout);
                    for (i, name) in text.split(',').enumerate() {
                        let trimmed = name.trim().trim_matches('"');
                        if !trimmed.is_empty() {
                            res.push(WindowInfo {
                                id: i as isize,
                                title: trimmed.to_string(),
                                is_active: i == 0,
                            });
                        }
                    }
                }
            }
            res
        }

        fn focus_window(&self, title_substring: &str) -> Result<bool, PlatformError> {
            let script = format!(
                "tell application \"System Events\" to set frontmost of (first process whose name contains \"{}\") to true",
                title_substring.replace('"', "\\\"")
            );
            let out = Command::new("osascript").args(["-e", &script]).output();
            if let Ok(o) = out {
                if o.status.success() {
                    return Ok(true);
                }
            }
            let res = Command::new("open").arg("-a").arg(title_substring).spawn();
            Ok(res.is_ok())
        }

        fn take_screenshot(&self) -> Result<Vec<u8>, PlatformError> {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let path = format!("/tmp/function_screenshot_{}.png", ts);

            let out = Command::new("screencapture")
                .args(["-x", "-t", "png", &path])
                .output()
                .map_err(|e| PlatformError::SystemApi(format!("screencapture failed: {}", e)))?;

            if !out.status.success() {
                return Err(PlatformError::PermissionDenied(
                    "Screen recording permission required or screencapture failed. Please allow Screen Recording in System Settings -> Privacy & Security.".into()
                ));
            }

            let path_buf = std::path::PathBuf::from(&path);
            if !path_buf.exists() {
                return Err(PlatformError::PermissionDenied(
                    "Screen recording permission denied. Please grant Screen Recording permission to Function in System Settings -> Privacy & Security.".into()
                ));
            }

            let bytes = std::fs::read(&path_buf).map_err(|e| {
                PlatformError::SystemApi(format!("Failed to read screenshot: {}", e))
            })?;
            let _ = std::fs::remove_file(&path_buf);
            Ok(bytes)
        }
    }
}

pub struct FallbackComputerControl;

impl ComputerControl for FallbackComputerControl {
    fn get_screen_dimensions(&self) -> ScreenDimensions {
        ScreenDimensions {
            width: 1920,
            height: 1080,
        }
    }
    fn get_cursor_position(&self) -> (i32, i32) {
        (0, 0)
    }
    fn mouse_move(&self, _x: i32, _y: i32) -> Result<(), PlatformError> {
        Ok(())
    }
    fn mouse_click(&self, _b: MouseButton) -> Result<(), PlatformError> {
        Ok(())
    }
    fn mouse_double_click(&self, _b: MouseButton) -> Result<(), PlatformError> {
        Ok(())
    }
    fn mouse_scroll(&self, _d: i32) -> Result<(), PlatformError> {
        Ok(())
    }
    fn mouse_drag(&self, _sx: i32, _sy: i32, _ex: i32, _ey: i32) -> Result<(), PlatformError> {
        Ok(())
    }
    fn keyboard_type(&self, _t: &str) -> Result<(), PlatformError> {
        Ok(())
    }
    fn keyboard_press(&self, _k: &str) -> Result<(), PlatformError> {
        Ok(())
    }
    fn keyboard_shortcut(&self, _k: &[&str]) -> Result<(), PlatformError> {
        Ok(())
    }
    fn app_launch(&self, _p: &str, _a: &[&str]) -> Result<u32, PlatformError> {
        Ok(0)
    }
    fn list_windows(&self) -> Vec<WindowInfo> {
        Vec::new()
    }
    fn focus_window(&self, _t: &str) -> Result<bool, PlatformError> {
        Ok(false)
    }
    fn take_screenshot(&self) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported(
            "Screenshot not supported on this platform".into(),
        ))
    }
}

pub fn create_computer_control() -> Box<dyn ComputerControl> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsComputerControl::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsComputerControl::new())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Box::new(FallbackComputerControl)
    }
}
