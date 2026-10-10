//! 系统托盘：主窗口关掉后程序仍留在后台。
//!
//! 图标直接用应用图标（窗口默认图标，来自 `tauri.conf.json` 的 `bundle.icon`），
//! 不另外画、也不多带图片资源。
//! 桌面没有托盘宿主时建不出图标，程序退回老行为：关掉主窗口即退出。

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::{State, Toggle};

pub fn setup(app: &AppHandle, state: Arc<State>) {
    let items = (|| -> tauri::Result<_> {
        let open = MenuItem::with_id(app, "open", "打开书架", true, None::<&str>)?;
        let toggle = MenuItem::with_id(app, "toggle", "继续阅读 / 收起", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &toggle, &quit])?;
        Ok(menu)
    })();
    let Ok(menu) = items else {
        eprintln!("托盘菜单创建失败，系统托盘不可用");
        return;
    };

    let s = state.clone();
    // 托盘图标 = 应用图标。取不到（配置里没给图标）就只建菜单，托盘照旧可用
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("reader-mini")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            // 和全局快捷键同一个开关：窗口开着这一下是「收起」
            "toggle" => {
                s.emit_toggle(if s.reader_open() {
                    Toggle::Close
                } else {
                    Toggle::Reader
                })
            }
            "quit" => s.emit_toggle(Toggle::Quit),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 左键点图标 = 打开书架
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(win) = tray.app_handle().get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        // 应用图标是彩色的，别让 macOS 当模板图（模板图会被强制成单色）
        builder = builder.icon(icon.clone()).icon_as_template(false);
    } else {
        eprintln!("没有应用图标，托盘将使用系统默认图标");
    }

    if let Err(e) = builder.build(app) {
        eprintln!("系统托盘不可用，关闭主窗口将直接退出: {e}");
    }
}
