//! 平台层：让阅读窗口能"拖动"和"失去焦点自动关闭"。
//!
//! 无边框、置顶交给 Slint 的 `no-frame` / `always-on-top`。
//! 剩下的两件事无法用纯 Slint 表达，用原生句柄实现：
//! - 拖动：按住标题条时，把鼠标位移加到窗口位置上
//! - 失焦：轮询系统"当前焦点窗口"，不是自己就关闭

use slint::Window;

/// 原生窗口句柄
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Handle {
    #[cfg(target_os = "linux")]
    Xlib {
        window: u64,
        display: usize,
    },
    #[cfg(target_os = "windows")]
    Win32 {
        hwnd: isize,
    },
    Other,
}

/// 从 Slint 窗口取出原生句柄
pub fn handle_of(window: &Window) -> Handle {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let h = window.window_handle();
    let Ok(raw) = h.window_handle() else {
        return Handle::Other;
    };
    match raw.as_raw() {
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(x) => {
            if x.window == 0 {
                return Handle::Other;
            }
            // XlibWindowHandle 只暴露 window；display 由我们自己打开
            Handle::Xlib {
                window: x.window,
                display: 0,
            }
        }
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(w) => Handle::Win32 {
            hwnd: w.hwnd.get() as isize,
        },
        _ => Handle::Other,
    }
}

/// 鼠标当前坐标（全局，逻辑像素）
pub fn mouse_pos() -> Option<(f32, f32)> {
    match mouse_position::mouse_position::Mouse::get_mouse_position() {
        mouse_position::mouse_position::Mouse::Position { x, y } => Some((x as f32, y as f32)),
        _ => None,
    }
}

/// 该窗口当前是否拥有系统焦点
pub fn is_focused(h: Handle) -> bool {
    match h {
        #[cfg(target_os = "linux")]
        Handle::Xlib { window, display } => linux::is_focused(window, display),
        #[cfg(target_os = "windows")]
        Handle::Win32 { hwnd } => windows::is_focused(hwnd),
        _ => true,
    }
}

/// 移动窗口到指定位置（逻辑像素）
pub fn move_to(h: Handle, x: f32, y: f32) {
    match h {
        #[cfg(target_os = "linux")]
        Handle::Xlib { window, display } => linux::move_to(window, display, x as i32, y as i32),
        #[cfg(target_os = "windows")]
        Handle::Win32 { hwnd } => windows::move_to(hwnd, x as i32, y as i32),
        _ => {}
    }
}

/// 当前鼠标是否按下左键（用于判断拖动是否继续）
pub fn left_button_down() -> bool {
    #[cfg(target_os = "windows")]
    {
        return windows::left_button_down();
    }
    #[cfg(target_os = "linux")]
    {
        return linux::left_button_down();
    }
    #[allow(unreachable_code)]
    false
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CString;
    use std::sync::Mutex;
    use x11_dl::xlib;

    static XLIB: Mutex<Option<xlib::Xlib>> = Mutex::new(None);

    fn with_xlib<R>(f: impl FnOnce(&xlib::Xlib, *mut xlib::Display) -> R) -> Option<R> {
        let mut guard = XLIB.lock().ok()?;
        if guard.is_none() {
            *guard = xlib::Xlib::open().ok();
        }
        let x = guard.as_ref()?;
        // 每个线程独立打开 Display
        let name = CString::new("").ok()?;
        unsafe {
            let display = (x.XOpenDisplay)(name.as_ptr());
            if display.is_null() {
                return None;
            }
            let r = f(x, display);
            (x.XCloseDisplay)(display);
            Some(r)
        }
    }

    pub fn is_focused(window: u64, _display: usize) -> bool {
        with_xlib(|x, d| unsafe {
            let mut focus: xlib::Window = 0;
            let mut revert: i32 = 0;
            (x.XGetInputFocus)(d, &mut focus, &mut revert);
            // 焦点可能落在子窗口上，向上找根
            let mut w = focus;
            for _ in 0..8 {
                let mut root: xlib::Window = 0;
                let mut parent: xlib::Window = 0;
                let mut children: *mut xlib::Window = std::ptr::null_mut();
                let mut nchildren: u32 = 0;
                if (x.XQueryTree)(d, w, &mut root, &mut parent, &mut children, &mut nchildren) == 0
                {
                    break;
                }
                if !children.is_null() {
                    (x.XFree)(children as *mut _);
                }
                if parent == 0 {
                    break;
                }
                w = parent;
            }
            w == window || focus == window
        })
        .unwrap_or(true)
    }

    pub fn move_to(window: u64, _display: usize, x: i32, y: i32) {
        with_xlib(|xl, d| unsafe {
            (xl.XMoveWindow)(d, window, x, y);
            (xl.XFlush)(d);
        });
    }

    pub fn left_button_down() -> bool {
        with_xlib(|xl, d| unsafe {
            let mut root: xlib::Window = 0;
            let mut child: xlib::Window = 0;
            let mut rx = 0;
            let mut ry = 0;
            let mut wx = 0;
            let mut wy = 0;
            let mut mask: u32 = 0;
            (xl.XQueryPointer)(
                d,
                (xl.XDefaultRootWindow)(d),
                &mut root,
                &mut child,
                &mut rx,
                &mut ry,
                &mut wx,
                &mut wy,
                &mut mask,
            );
            mask & xlib::Button1Mask != 0
        })
        .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetKeyState, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    pub fn is_focused(hwnd: isize) -> bool {
        unsafe { GetForegroundWindow() as isize == hwnd }
    }

    pub fn move_to(hwnd: isize, x: i32, y: i32) {
        unsafe {
            SetWindowPos(hwnd as _, 0, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
        }
    }

    pub fn left_button_down() -> bool {
        unsafe { GetKeyState(0x01) < 0 }
        // 顺带避免未使用告警
    }

    #[allow(dead_code)]
    fn cursor() -> (i32, i32) {
        let mut p = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut p) };
        (p.x, p.y)
    }
}
