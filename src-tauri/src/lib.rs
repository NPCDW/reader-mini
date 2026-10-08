//! reader-mini：Tauri + Vue 的极简桌面阅读器。
//!
//! 分层：
//! - `api`      —— 服务端接口（书架 / 目录 / 正文 / 进度）
//! - `config`   —— 配置（baseUrl、阅读样式、快捷键、阅读窗口尺寸）
//! - `progress` —— 本地阅读进度（每本书读到哪一行）
//! - `hotkey`   —— 快捷键写法 ↔ Tauri 加速键
//! - `tray`     —— 系统托盘
//!
//! 窗口只有两个：`main`（书架 + 设置）和 `reader`（无边框浮动阅读窗口）。
//! 阅读窗口的内容与位置都在前端算，Rust 只负责取数据与建窗口。

mod api;
mod config;
mod hotkey;
mod progress;
mod tray;

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use config::Config;

/// 托盘菜单 / 全局快捷键发来的开关信号。
///
/// 托盘回调与快捷键回调都在别的线程上，直接碰窗口容易死锁，
/// 统一塞进队列，由前端轮询。
pub enum Toggle {
    /// 呼出 / 继续阅读
    Reader,
    /// 收起已经开着的阅读窗口
    Close,
    Quit,
}

pub struct State {
    pub config: Mutex<Config>,
    /// 书架缓存：快捷键续读与托盘「继续阅读」都要用
    pub books: Mutex<Vec<api::Book>>,
    /// 当前在读的书在书架里的下标，-1 表示还没选过
    pub current_book: Mutex<i64>,
    /// 最近一次要展示的正文。阅读窗口是异步起来的，事件可能在它开始监听前就发完了，
    /// 所以内容同时留在这里：窗口起来时自己取走一份，设置页保存样式后
    /// 后端也拿它再交一次手（所以是「留着」而不是「取走就清」）。
    pending: Mutex<Option<api::ReadPayload>>,
    toggles: Mutex<Vec<Toggle>>,
    app: Mutex<Option<AppHandle>>,
}

impl State {
    pub fn emit_toggle(&self, toggle: Toggle) {
        self.toggles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(toggle);
        if let Some(app) = self.app.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let _ = app.emit("reader://tick", ());
        }
    }

    fn set_pending(&self, payload: api::ReadPayload) {
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(payload);
    }

    fn pending(&self) -> Option<api::ReadPayload> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn take_toggle(&self) -> Option<&'static str> {
        let mut guard = self.toggles.lock().unwrap_or_else(|e| e.into_inner());
        let first = guard.first()?;
        let name = match first {
            Toggle::Reader => "reader",
            Toggle::Close => "close",
            Toggle::Quit => "quit",
        };
        guard.clear();
        Some(name)
    }
}

/// 前端轮询的挂起信号：托盘与全局快捷键共用同一条通道
#[tauri::command]
fn poll_toggle(state: tauri::State<'_, Arc<State>>) -> Option<String> {
    state.take_toggle().map(str::to_string)
}

/// 阅读窗口装载完成后自己来要内容。
///
/// 不用「建窗口后 emit」：事件可能在页面开始监听之前就发完了，正文会丢。
#[tauri::command]
fn take_pending(state: tauri::State<'_, Arc<State>>) -> Option<api::ReadPayload> {
    state.pending()
}

/// 真正退出程序（托盘菜单的「退出」）。
///
/// 前端已经先把阅读窗口的进度收干净了，这里只负责收尾。
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_config(state: tauri::State<'_, Arc<State>>) -> Config {
    state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 保存设置。快捷键变了顺手重新注册，保存即生效。
#[tauri::command]
fn save_config(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
    config: Config,
) -> Result<Config, String> {
    let old = {
        let mut guard = state.config.lock().unwrap_or_else(|e| e.into_inner());
        let old = guard.hotkey.clone();
        *guard = config.clone();
        guard.save().map_err(|e| e.to_string())?;
        old
    };
    if hotkey::normalize(&old) == hotkey::normalize(&config.hotkey) {
        return Ok(config);
    }
    rebind_hotkey(&app, &old, &config.hotkey).map(|_| config)
}

/// 换快捷键：先注册新的，成功了再注销旧的；新的失败就把旧的装回去。
fn rebind_hotkey(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let new_acc = hotkey::to_accelerator(new).ok_or_else(|| format!("快捷键「{new}」无法识别"))?;
    match gs.register(new_acc.as_str()) {
        Ok(()) => {
            if let Some(old_acc) = hotkey::to_accelerator(old) {
                let _ = gs.unregister(old_acc.as_str());
            }
            Ok(())
        }
        Err(e) => Err(format!("注册快捷键「{new}」失败: {e}")),
    }
}

#[tauri::command]
async fn get_bookshelf(state: tauri::State<'_, Arc<State>>) -> Result<Vec<api::Book>, String> {
    let base = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .base_url
        .clone();
    api::get_bookshelf(&base).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_chapter_list(
    state: tauri::State<'_, Arc<State>>,
    book_url: String,
) -> Result<Vec<api::Chapter>, String> {
    let base = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .base_url
        .clone();
    api::get_chapter_list(&base, &book_url)
        .await
        .map_err(|e| e.to_string())
}

/// 主窗口刷新书架后缓存下来，供快捷键 / 托盘续读使用
#[tauri::command]
fn cache_books(state: tauri::State<'_, Arc<State>>, books: Vec<api::Book>) {
    *state.books.lock().unwrap_or_else(|e| e.into_inner()) = books;
}

#[tauri::command]
fn set_current_book(state: tauri::State<'_, Arc<State>>, index: i64) {
    *state.current_book.lock().unwrap_or_else(|e| e.into_inner()) = index;
}

/// 阅读窗口每次装载 / 拉伸后回写，下次打开还是这个大小
#[tauri::command]
fn save_reader_size(state: tauri::State<'_, Arc<State>>, width: f32, height: f32) {
    if width < 260.0 || height < 200.0 {
        return;
    }
    let mut cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
    cfg.reader_width = width;
    cfg.reader_height = height;
    let _ = cfg.save();
}

/// 打开（或原地刷新）阅读窗口。
///
/// 呼出前会先看一眼：窗口已经开着就把这次按键当成「收起」，同步进度后藏起来，
/// 返回 `false`，前端不再拿内容去刷它。这样呼出 / 关闭就是同一个开关。
#[tauri::command]
async fn open_reader(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
    book_index: usize,
    chapter_index: i64,
    chapter_title: String,
    start_line: usize,
) -> Result<bool, String> {
    // 全局快捷键按第二下：窗口开着就收起，进度照常写回
    if let Some(win) = app.get_webview_window("reader") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
            let _ = save_reading_progress(&state, chapter_index, &chapter_title, start_line).await;
            return Ok(false);
        }
    }
    let payload =
        load_chapter(&state, book_index, chapter_index, chapter_title, start_line).await?;
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    present(&state, &app, &cfg, payload)
}

/// 设置改完，把已经开着的阅读窗口重新刷一遍。
///
/// 样式（背景 / 字色 / 字号 / 行高）是阅读窗口从自己那份配置里读的，
/// 主窗口保存完不通知它，它就还按老样式显示 —— 这就是「设置不管用」。
/// 正文本身没变，所以只是再交一次手，阅读窗口收到会重新读配置、重排。
#[tauri::command]
async fn refresh_reader_style(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
) -> Result<(), String> {
    let Some(win) = app.get_webview_window("reader") else {
        return Ok(());
    };
    if !win.is_visible().unwrap_or(false) {
        return Ok(());
    }
    // 手上还留着正文就原样再交一次手，阅读窗口收到会重新读配置并原地重排；
    // 没有正文（比如刚启动还没读过）就只发个通知
    match state.pending() {
        Some(payload) => {
            win.emit("reader://load", &payload)
                .map_err(|e| e.to_string())?;
        }
        None => {
            win.emit("reader://style", ()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 本章翻到头：PgDn 取下一章、PgUp 取上一章，就地刷新同一个窗口。
///
/// `current_index` 由阅读窗口给出 —— 窗口里的章号才是最新的。
#[tauri::command]
async fn switch_chapter(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
    current_index: i64,
    direction: i32,
) -> Result<(), String> {
    let book_index = *state.current_book.lock().unwrap_or_else(|e| e.into_inner());
    if book_index < 0 {
        return Err("请先从书架打开一本书".into());
    }
    let book_index = book_index as usize;
    let (base, book) = {
        let cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
        let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
        let book = books.get(book_index).cloned().ok_or("书架上找不到这本书")?;
        (cfg.base_url.clone(), book)
    };
    let chapters = api::get_chapter_list(&base, &book.book_url)
        .await
        .map_err(|e| e.to_string())?;
    let target = current_index + direction as i64;
    if target < 0 || target as usize >= chapters.len() {
        return Err(if direction > 0 {
            "已经是最后一章"
        } else {
            "已经是第一章"
        }
        .into());
    }
    let raw = &chapters[target as usize].title;
    let title = if raw.is_empty() {
        format!("第{}章", target + 1)
    } else {
        raw.clone()
    };
    // 往回翻落到上一章末尾，往下翻从开头读
    let start_line = if direction < 0 { usize::MAX } else { 0 };
    let payload = load_chapter(&state, book_index, target, title, start_line).await?;
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    present(&state, &app, &cfg, payload)?;
    Ok(())
}

async fn load_chapter(
    state: &Arc<State>,
    book_index: usize,
    chapter_index: i64,
    chapter_title: String,
    start_line: usize,
) -> Result<api::ReadPayload, String> {
    let (base, book) = {
        let cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
        let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
        let book = books.get(book_index).cloned().ok_or("书架上找不到这本书")?;
        (cfg.base_url.clone(), book)
    };
    let text = api::get_book_content(&base, &book.book_url, chapter_index)
        .await
        .map_err(|e| e.to_string())?;
    Ok(api::ReadPayload {
        book_index,
        chapter_index,
        chapter_title,
        start_line,
        text,
    })
}

/// 按配置里的尺寸摆好阅读窗口（在它显示之前调用）
fn set_reader_size(win: &tauri::WebviewWindow, cfg: &Config) -> Result<(), String> {
    win.set_size(tauri::LogicalSize::new(
        cfg.reader_width as f64,
        cfg.reader_height as f64,
    ))
    .map_err(|e| e.to_string())
}

/// 把一份内容交给阅读窗口：已开着就原地刷新，没有才新建。
fn present(
    state: &Arc<State>,
    app: &AppHandle,
    cfg: &Config,
    payload: api::ReadPayload,
) -> Result<bool, String> {
    state.set_pending(payload.clone());
    match app.get_webview_window("reader") {
        Some(win) => {
            win.emit("reader://load", &payload)
                .map_err(|e| e.to_string())?;
            let _ = win.show();
            let _ = win.set_focus();
            Ok(true)
        }
        None => {
            let win =
                WebviewWindowBuilder::new(app, "reader", WebviewUrl::App("reader.html".into()))
                    .title("reader")
                    .inner_size(cfg.reader_width as f64, cfg.reader_height as f64)
                    .min_inner_size(260.0, 200.0)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .resizable(true)
                    .visible(false)
                    .build()
                    .map_err(|e| e.to_string())?;
            // 尺寸在窗口显示之前先定死，再 show —— 让 show 去套用 inner_size 的话，
            // 页面会先按上一个尺寸排一遍，分页也就跟着错一屏
            let _ = set_reader_size(&win, cfg);
            win.emit("reader://load", &payload)
                .map_err(|e| e.to_string())?;
            win.show().map_err(|e| e.to_string())?;
            win.set_focus().map_err(|e| e.to_string())?;
            Ok(false)
        }
    }
}

/// 进度收尾：本地记下读到哪一行，服务端记下读到哪一章。
///
/// 阅读窗口自己收起、快捷键收起、托盘退出，三处都要，所以抽出来一份。
async fn save_reading_progress(
    state: &Arc<State>,
    chapter_index: i64,
    chapter_title: &str,
    line: usize,
) -> Result<(), String> {
    let book_index = *state.current_book.lock().unwrap_or_else(|e| e.into_inner());
    if book_index < 0 {
        return Ok(());
    }
    let (base, book) = {
        let cfg = state.config.lock().unwrap_or_else(|e| e.into_inner());
        let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
        let Some(book) = books.get(book_index as usize).cloned() else {
            return Ok(());
        };
        (cfg.base_url.clone(), book)
    };
    progress::remember(
        &book.book_url,
        chapter_index,
        chapter_title.to_string(),
        line,
    );
    let _ = api::save_book_progress(&base, &book, chapter_index, 0, chapter_title).await;
    Ok(())
}

/// 阅读窗口被关掉时收尾：同步进度到服务端 + 本地记行号。
#[tauri::command]
async fn close_reader(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
    chapter_index: i64,
    chapter_title: String,
    line: usize,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("reader") {
        let _ = win.hide();
    }
    save_reading_progress(&state, chapter_index, &chapter_title, line).await
}

/// 续读点：服务端说读哪一章，本地说读到哪一行
#[tauri::command]
fn resume_point(
    state: tauri::State<'_, Arc<State>>,
    book_index: usize,
) -> Result<progress::Resume, String> {
    let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
    let book = books.get(book_index).ok_or("书架上找不到这本书")?;
    Ok(progress::resume(
        &book.book_url,
        book.dur_chapter_index,
        &book.dur_chapter_title,
    ))
}

pub fn run() {
    let cfg = Config::load();
    let state = Arc::new(State {
        config: Mutex::new(cfg.clone()),
        books: Mutex::new(Vec::new()),
        current_book: Mutex::new(-1),
        pending: Mutex::new(None),
        toggles: Mutex::new(Vec::new()),
        app: Mutex::new(None),
    });

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let Some(state) = app.try_state::<Arc<State>>() else {
                        return;
                    };
                    // 同一个快捷键既是呼出也是关闭：窗口开着就走 Close，
                    // 主窗口收到后叫阅读窗口自己把进度收干净再藏起来
                    let open = app
                        .get_webview_window("reader")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    state.emit_toggle(if open { Toggle::Close } else { Toggle::Reader });
                })
                .build(),
        )
        .manage(state.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            *state.app.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle.clone());

            // 快捷键装不上不致命：设置页里还能再改
            if let Some(acc) = hotkey::to_accelerator(&cfg.hotkey) {
                if let Err(e) = handle.global_shortcut().register(acc.as_str()) {
                    eprintln!("全局快捷键「{}」注册失败: {e}", cfg.hotkey);
                }
            }

            tray::setup(&handle, state.clone());

            // 关主窗口只是藏起来，进程留在托盘里
            if let Some(main) = handle.get_webview_window("main") {
                let w = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            poll_toggle,
            take_pending,
            quit_app,
            get_config,
            save_config,
            get_bookshelf,
            get_chapter_list,
            open_reader,
            switch_chapter,
            close_reader,
            cache_books,
            set_current_book,
            save_reader_size,
            refresh_reader_style,
            resume_point,
        ])
        .run(tauri::generate_context!())
        .expect("reader-mini 启动失败");
}
