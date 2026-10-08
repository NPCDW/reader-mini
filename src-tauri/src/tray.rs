//! 系统托盘：主窗口关掉后程序仍留在后台。
//!
//! 图标由代码画（一个圆角「书」），不额外带图片资源。
//! 桌面没有托盘宿主时建不出图标，程序退回老行为：关掉主窗口即退出。

use std::sync::Arc;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::{State, Toggle};

const SIZE: u32 = 32;

/// 画一个书本样式的托盘图标（RGBA，交给 Tauri 转成平台图像）
fn icon() -> Image<'static> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let r = 6i32;
    let n = SIZE as i32;
    for y in 0..n {
        for x in 0..n {
            // 圆角遮罩：角落落在四分之一圆以外就透明
            let dx = if x < r {
                r - x
            } else if x > n - 1 - r {
                x - (n - 1 - r)
            } else {
                0
            };
            let dy = if y < r {
                r - y
            } else if y > n - 1 - r {
                y - (n - 1 - r)
            } else {
                0
            };
            let i = ((y * n + x) * 4) as usize;
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let px = if (8..26).contains(&x) && (6..26).contains(&y) {
                // 书页：白底 + 三条「文字」线
                if (10..12).contains(&y) || (15..17).contains(&y) || (20..22).contains(&y) {
                    [0x4a, 0x6f, 0xa5, 0xff]
                } else {
                    [0xf5, 0xf5, 0xf7, 0xff]
                }
            } else if x < 6 {
                [0x2f, 0x4d, 0x7a, 0xff] // 书脊
            } else {
                [0x4a, 0x6f, 0xa5, 0xff] // 封面
            };
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

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
    let builder = TrayIconBuilder::with_id("main")
        .icon(icon())
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
            // 和全局快捷键同一个开关
            "toggle" => s.emit_toggle(Toggle::Reader),
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

    if let Err(e) = builder.build(app) {
        eprintln!("系统托盘不可用，关闭主窗口将直接退出: {e}");
    }
}
