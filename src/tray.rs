//! 系统托盘：主窗口关掉后程序继续在后台运行。
//!
//! 托盘图标由 Slint 的 `SystemTrayIcon` 提供（Linux/BSD 走 ksni，Windows 走
//! `Shell_NotifyIconW`，macOS 走 `NSStatusItem`），需要 `system-tray` feature。
//! 图标用代码画一个 32×32 的"书"，省得再带一份图片资源。

use crate::tray_win::TrayIcon;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// 画一个书本样式的托盘图标
pub fn icon() -> Image {
    const S: usize = 32;
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(S as u32, S as u32);
    let (w, h) = (S as i32, S as i32);
    let r = 6i32;
    for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i % S) as i32, (i / S) as i32);
        // 圆角矩形遮罩：角落落在四分之一圆里才算在内
        let dx = if x < r {
            r - x
        } else if x > w - 1 - r {
            x - (w - 1 - r)
        } else {
            0
        };
        let dy = if y < r {
            r - y
        } else if y > h - 1 - r {
            y - (h - 1 - r)
        } else {
            0
        };
        if dx * dx + dy * dy > r * r {
            continue; // 圆角外：透明
        }
        let (red, green, blue, alpha) = if (8..26).contains(&x) && (6..26).contains(&y) {
            // 书页：白底 + 三条"文字"线
            if (10..12).contains(&y) || (15..17).contains(&y) || (20..22).contains(&y) {
                (0x4a, 0x6f, 0xa5, 0xff)
            } else {
                (0xf5, 0xf5, 0xf7, 0xff)
            }
        } else if x < 6 {
            (0x2f, 0x4d, 0x7a, 0xff) // 书脊
        } else {
            (0x4a, 0x6f, 0xa5, 0xff) // 封面
        };
        *px = Rgba8Pixel {
            r: red,
            g: green,
            b: blue,
            a: alpha,
        };
    }
    Image::from_rgba8(buf)
}

/// 建托盘图标并挂上菜单回调。返回的实例要一直持有，drop 掉图标就消失了。
pub fn setup(
    open_window: impl Fn() + 'static,
    toggle_reader: impl Fn() + 'static,
    quit: impl Fn() + 'static,
) -> Result<TrayIcon, slint::PlatformError> {
    let tray = TrayIcon::new()?;
    tray.set_tray_icon(icon());
    tray.on_open_window(open_window);
    tray.on_toggle_reader(toggle_reader);
    tray.on_quit(quit);
    Ok(tray)
}
