//! reader-mini 核心逻辑（供 bin 与测试共用）

/// 主窗口 UI 绑定（由 ui/app.slint 生成）
pub mod app {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/app.rs"));
}

/// 阅读窗口 UI 绑定（由 ui/reader_win.slint 生成）
pub mod reader_win {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/reader_win.rs"));
}

pub mod api;
pub mod config;
pub mod hotkey;
pub mod net;
pub mod platform;
pub mod progress;
pub mod reader_view;

/// 供集成测试调用的分页入口
pub fn paginate_for_test(text: &str, chars_per_page: usize) -> Vec<String> {
    api::paginate(text, chars_per_page)
}
