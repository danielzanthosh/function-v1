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
    fn keyboard_press(&self, key: &str) -> Result<(), PlatformError>;
    fn keyboard_shortcut(&self, keys: &[&str]) -> Result<(), PlatformError>;
    fn app_launch(&self, app_path: &str, args: &[&str]) -> Result<u32, PlatformError>;
    fn list_windows(&self) -> Vec<WindowInfo>;
    fn focus_window(&self, title_substring: &str) -> Result<bool, PlatformError>;
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
}

pub fn create_computer_control() -> Box<dyn ComputerControl> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsComputerControl::new())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Box::new(FallbackComputerControl)
    }
}
