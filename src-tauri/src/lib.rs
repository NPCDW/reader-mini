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
use std::time::{Duration, Instant};

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
    /// 当前在读的那本书的 `bookUrl`。跨命令认书一律用它 —— 服务端可能按阅读
    /// 时间重排书架，下标会指到别的书上。空串表示还没选过
    pub current_url: Mutex<String>,
    /// 最近一次要展示的正文。阅读窗口是异步起来的，事件可能在它开始监听前就发完了，
    /// 所以内容同时留在这里，窗口上报「已就绪」时自己取走。
    pending: Mutex<Option<api::ReadPayload>>,
    /// 样式版本。设置页每保存一次就加一，阅读窗口据此知道该重新读配置了；
    /// 窗口起来得晚时也靠它判断「手上这份内容」是不是当前样式的
    style_tick: Mutex<u64>,
    /// 阅读窗口开着没有。呼出 / 收起这个开关只认这一个标记
    ///
    /// 不问窗口的 `is_visible`：窗口刚被收起、这一下按键还没放开的时候，
    /// 窗口管理器可能还把它算作可见，收起这一下就又被当成呼出，
    /// 表现就是窗口关掉以后自己又弹回来。
    reader_open: Mutex<bool>,
    /// 上一次认下的快捷键按下时刻。按住不放会连着报「按下」，
    /// 一次按键被算成两次，同样会关掉又打开
    hotkey_at: Mutex<Option<Instant>>,
    toggles: Mutex<Vec<Toggle>>,
    app: Mutex<Option<AppHandle>>,
    /// 上一次上报给服务端的进度（书 `bookUrl`、章、行、位置）。
    /// 翻页攒出来的重复上报在这里被挡掉，不必真发一次请求
    last_saved: Mutex<Option<(String, i64, usize, i64)>>,
}

impl State {
    /// 书架上按 `bookUrl` 找书。
    ///
    /// 书架上的每一处认书都走这里，不按下标取：服务端可能按阅读时间重排书架，
    /// 拿旧下标去取会取到另一本书上（表现是「读了 A，进度记到 B 头上」）。
    fn find_book(books: &[api::Book], book_url: &str) -> Option<api::Book> {
        books.iter().find(|b| b.book_url == book_url).cloned()
    }

    /// 缓存里那本正在读的书。没选过书 / 书已经不在书架上时返回 `None`
    fn current_book(&self) -> Option<api::Book> {
        let url = self
            .current_url
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if url.is_empty() {
            return None;
        }
        let books = self.books.lock().unwrap_or_else(|e| e.into_inner());
        Self::find_book(&books, &url)
    }

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

    fn style_tick(&self) -> u64 {
        *self.style_tick.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_style_tick(&self, tick: u64) {
        *self.style_tick.lock().unwrap_or_else(|e| e.into_inner()) = tick;
    }

    /// 阅读窗口开着没有：呼出 / 收起这个开关唯一的判据
    fn reader_open(&self) -> bool {
        *self.reader_open.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_reader_open(&self, open: bool) {
        *self.reader_open.lock().unwrap_or_else(|e| e.into_inner()) = open;
    }

    /// 认下这一次快捷键按下；太密的那几次是按住不放的重复，不算。
    fn mark_hotkey(&self) -> bool {
        let mut guard = self.hotkey_at.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if guard.map_or(false, |t| {
            now.duration_since(t) < Duration::from_millis(250)
        }) {
            return false;
        }
        *guard = Some(now);
        true
    }

    fn take_pending(&self) -> Option<api::ReadPayload> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
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
    state.take_pending()
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

/// 把 `#rgb` / `#rrggbb` 解析成窗口底色。
///
/// 阅读窗口是暗色的（默认 `#181818`），不声明底色的话窗口从无到有那一帧是白的，
/// 每次呼出都闪一下。认不出来的写法退回到默认那档暗色。
fn parse_color(hex: &str) -> tauri::window::Color {
    let s = hex.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2).unwrap_or(""), 16).ok();
    let rgb = match s.len() {
        6 => byte(0).zip(byte(2)).zip(byte(4)),
        3 => s
            .chars()
            .nth(0)
            .and_then(|a| {
                s.chars()
                    .nth(1)
                    .and_then(|b| s.chars().nth(2).map(|c| (a, b, c)))
            })
            .and_then(|(r, g, b)| {
                let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
                expand(r).zip(expand(g)).zip(expand(b))
            }),
        _ => None,
    };
    match rgb {
        Some(((r, g), b)) => tauri::window::Color(r, g, b, 255),
        None => tauri::window::Color(0x18, 0x18, 0x18, 255),
    }
}

/// 保存设置。快捷键变了顺手重新注册，保存即生效。
#[tauri::command]
fn save_config(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
    config: Config,
) -> Result<Config, String> {
    let (old_hotkey, old_bg) = {
        let mut guard = state.config.lock().unwrap_or_else(|e| e.into_inner());
        let old = (guard.hotkey.clone(), guard.read_bg.clone());
        *guard = config.clone();
        guard.save().map_err(|e| e.to_string())?;
        old
    };
    // 阅读底色换了就把窗口底色也换掉：下次呼出（以及窗口缩放露出的那几像素）
    // 才会是新的色，不会先白一下再变暗
    if old_bg != config.read_bg {
        if let Some(win) = app.get_webview_window("reader") {
            let _ = win.set_background_color(Some(parse_color(&config.read_bg)));
        }
    }
    if hotkey::normalize(&old_hotkey) == hotkey::normalize(&config.hotkey) {
        return Ok(config);
    }
    rebind_hotkey(&app, &old_hotkey, &config.hotkey).map(|_| config)
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

/// 主窗口刷新书架后缓存下来，供快捷键 / 托盘续读使用。
///
/// 回一个「手上这本还在不在」：书架是被远端刷新的，正在读的那本书可能已经被
/// 移出书架了（此时进度不该再往服务端写）。
#[tauri::command]
fn cache_books(state: tauri::State<'_, Arc<State>>, books: Vec<api::Book>) -> bool {
    *state.books.lock().unwrap_or_else(|e| e.into_inner()) = books;
    state.current_book().is_some()
}

/// 记下「现在读的是哪本书」。收 `bookUrl` 而不是下标：远端重排书架之后，
/// 主窗口手上那个下标可能已经指到别的书上了
#[tauri::command]
fn set_current_book(state: tauri::State<'_, Arc<State>>, book_url: String) {
    *state.current_url.lock().unwrap_or_else(|e| e.into_inner()) = book_url;
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
    book_url: String,
    chapter_index: i64,
    chapter_title: String,
    start_line: usize,
    pos: i64,
    style_tick: u64,
) -> Result<bool, String> {
    state.set_style_tick(style_tick);
    // 主窗口手上的下标可能已经过期（远端刚重排过书架），所以认书只认 bookUrl
    {
        let mut cur = state.current_url.lock().unwrap_or_else(|e| e.into_inner());
        *cur = book_url.clone();
    }
    // 第二下（快捷键 / 托盘同一个开关）：窗口开着就收起，进度照常写回。
    if let Some(win) = app.get_webview_window("reader") {
        if state.reader_open() {
            let _ = win.hide();
            state.set_reader_open(false);
            // `pos` 是阅读窗口回传的当前那一屏的字数位置。不能用交到窗口手上时
            // 那个位置：那是打开时的，报回去等于把这一路翻的几屏全退回去
            if save_reading_progress(&state, chapter_index, &chapter_title, start_line, pos)
                .await
                .is_ok()
            {
                let _ = app.emit("reader://closed", ());
            }
            return Ok(false);
        }
    }
    let payload = load_chapter(&state, &book_url, chapter_index, chapter_title, start_line).await?;
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
    style_tick: u64,
) -> Result<(), String> {
    let Some(win) = app.get_webview_window("reader") else {
        return Ok(());
    };
    if !win.is_visible().unwrap_or(false) {
        return Ok(());
    }
    state.set_style_tick(style_tick);
    // 手上还留着正文就原样再交一次手，阅读窗口收到会重新读配置；
    // 没有正文（比如刚启动还没读过）就只发个通知
    match pending_payload(&state) {
        Some(payload) => {
            let payload = api::ReadPayload {
                style_tick,
                // 只是换样式：正文还是这一章，读者正看着的那一页要留住，
                // 不能按服务端记的位置把他挪走
                reposition: false,
                ..payload
            };
            state.set_pending(payload.clone());
            win.emit("reader://load", &payload)
                .map_err(|e| e.to_string())?;
        }
        None => {
            win.emit("reader://style", style_tick)
                .map_err(|e| e.to_string())?;
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
    let book = state.current_book().ok_or("请先从书架打开一本书")?;
    let base = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .base_url
        .clone();
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
    let payload = load_chapter(&state, &book.book_url, target, title, start_line).await?;
    let cfg = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    present(&state, &app, &cfg, payload)?;
    Ok(())
}

/// 阅读窗口当前该显示的那份内容（后端留着的那一份）
fn pending_payload(state: &Arc<State>) -> Option<api::ReadPayload> {
    state
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

async fn load_chapter(
    state: &Arc<State>,
    book_url: &str,
    chapter_index: i64,
    chapter_title: String,
    start_line: usize,
) -> Result<api::ReadPayload, String> {
    let base = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .base_url
        .clone();
    let book = {
        let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
        State::find_book(&books, book_url).ok_or("书架上找不到这本书")?
    };
    // 正文位置以字数计（本章第一屏是 0，翻到第 N 屏就是前 N-1 屏那几个字）。
    // 载入时把书架上那个值带上，阅读窗口据此定位到上次读的那一屏，
    // 之后按读者真正停下的那一屏回写；换章时归零 —— 新章从头读起
    let dur_chapter_pos = if chapter_index == book.dur_chapter_index {
        book.dur_chapter_pos
    } else {
        0
    };
    let text = api::get_book_content(&base, &book.book_url, chapter_index)
        .await
        .map_err(|e| e.to_string())?;
    Ok(api::ReadPayload {
        book_url: book.book_url.clone(),
        chapter_index,
        chapter_title,
        start_line,
        dur_chapter_pos,
        style_tick: 0,
        reposition: true,
        text,
    })
}

/// 把一份内容交给阅读窗口：已开着就原地刷新，没有才新建。
fn present(
    state: &Arc<State>,
    app: &AppHandle,
    cfg: &Config,
    payload: api::ReadPayload,
) -> Result<bool, String> {
    let payload = api::ReadPayload {
        style_tick: state.style_tick(),
        ..payload
    };
    state.set_pending(payload.clone());
    match app.get_webview_window("reader") {
        Some(win) => {
            win.emit("reader://load", &payload)
                .map_err(|e| e.to_string())?;
            let _ = win.show();
            let _ = win.set_focus();
            state.set_reader_open(true);
            Ok(true)
        }
        None => {
            let win =
                WebviewWindowBuilder::new(app, "reader", WebviewUrl::App("reader.html".into()))
                    .title("reader")
                    .inner_size(cfg.reader_width as f64, cfg.reader_height as f64)
                    .min_inner_size(260.0, 200.0)
                    .decorations(false)
                    // Windows 上无边框窗口默认是带投影的：tao 会为它留出一圈透明内边距
                    // （Win11 还顺带圆角），阅读窗口贴着屏幕看就是糊了一圈灰边。关掉投影，
                    // 窗口尺寸也才是「要多大就多大」，不用再减那圈内边距
                    .shadow(false)
                    .skip_taskbar(true)
                    .resizable(true)
                    .visible(false)
                    // 底色先给成阅读底色：窗口从无到有那一帧不再是一片白
                    .background_color(parse_color(&cfg.read_bg))
                    .build()
                    .map_err(|e| e.to_string())?;
            win.emit("reader://load", &payload)
                .map_err(|e| e.to_string())?;
            win.show().map_err(|e| e.to_string())?;
            win.set_focus().map_err(|e| e.to_string())?;
            state.set_reader_open(true);
            Ok(false)
        }
    }
}

/// 进度上报：本地记下读到哪一行，服务端记下读到哪一章 / 哪个位置。
///
/// 翻页、阅读窗口自己收起、快捷键收起、托盘退出，都要走这一份，所以抽出来。
/// `pos` 是正在阅读的正文位置，以字数计：本章第一屏是 0，翻到第 N 屏
/// 就是前 N-1 屏那几个字；换章时给 0（从头读起）。
async fn save_reading_progress(
    state: &Arc<State>,
    chapter_index: i64,
    chapter_title: &str,
    line: usize,
    pos: i64,
) -> Result<(), String> {
    // 读的是哪本书由 `current_url` 说了算 —— 藏书那本书的位置没有别的来源。
    // 书不在书架上（被远端移走了）就没有 bookUrl 可用，这次不写
    let Some(book) = state.current_book() else {
        return Ok(());
    };
    let base = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .base_url
        .clone();
    progress::remember(
        &book.book_url,
        chapter_index,
        chapter_title.to_string(),
        line,
    );
    let _ = api::save_book_progress(&base, &book, chapter_index, pos, chapter_title).await;
    Ok(())
}

/// 翻页时上报进度。
///
/// 阅读窗口每翻一屏就调一次，所以这里只写服务端、并加一层「和上次送出去的一样就不发」的
/// 去重：翻页的节流（多久报一次）由前端拿主意，后端只负责别把重复的请求打出去。
/// 本地那份 `progress.json` 交给收起时的 `close_reader` 收尾，一次翻页不必落一次盘。
#[tauri::command]
async fn save_progress(
    state: tauri::State<'_, Arc<State>>,
    book_url: String,
    chapter_index: i64,
    chapter_title: String,
    line: usize,
    pos: i64,
) -> Result<(), String> {
    {
        let mut last = state.last_saved.lock().unwrap_or_else(|e| e.into_inner());
        let same = last
            .as_ref()
            .is_some_and(|p| p.0 == book_url && p.1 == chapter_index && p.2 == line && p.3 == pos);
        if same {
            return Ok(());
        }
        *last = Some((book_url.clone(), chapter_index, line, pos));
    }
    let base = state
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .base_url
        .clone();
    // 按 bookUrl 认书：阅读窗口手上那个下标是它拿到正文时的，远端重排过书架就不作数了
    let book = {
        let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
        State::find_book(&books, &book_url).ok_or("书架上找不到这本书")?
    };
    api::save_book_progress(&base, &book, chapter_index, pos, &chapter_title)
        .await
        .map_err(|e| e.to_string())?;
    // 服务端收下了就顺手记进本地：万一这次没走正常收起（进程被杀），
    // 下次打开也还接得上刚才翻到的地方
    progress::remember(&book.book_url, chapter_index, chapter_title, line);
    Ok(())
}

/// 阅读窗口发来的进度收尾。
///
/// 两种情形走的是同一个命令：
/// - `hide: true` —— 窗口真的要收起（Esc、点关闭、失焦、快捷键第二下），
///   先把窗口藏了；
/// - `hide: false` —— 只是收起前的最后一次同步：窗口还开着，收不收由窗口自己决定
///   （比如失焦那一瞬间鼠标还按在窗口上拖着，这次就不该收）。
#[tauri::command]
async fn close_reader(
    app: AppHandle,
    state: tauri::State<'_, Arc<State>>,
    chapter_index: i64,
    chapter_title: String,
    line: usize,
    pos: i64,
    hide: bool,
) -> Result<(), String> {
    if hide {
        if let Some(win) = app.get_webview_window("reader") {
            let _ = win.hide();
        }
        state.set_reader_open(false);
    }
    save_reading_progress(&state, chapter_index, &chapter_title, line, pos).await?;
    // 进度落到服务端之后才通知主窗口刷新书架：书架上「读至第几章」要跟着变成
    // 刚读到的那一章。这一下没写成功就别刷新 —— 刷回来的是一份旧进度
    let _ = app.emit("reader://closed", ());
    Ok(())
}

/// 续读点：服务端说读哪一章，本地说读到哪一行。
///
/// 收 `bookUrl`：主窗口点「阅读」时手上的下标可能已经过期，
/// 拿下标去取会续到另一本书上（本地那份行号也跟着串过去）
#[tauri::command]
fn resume_point(
    state: tauri::State<'_, Arc<State>>,
    book_url: String,
) -> Result<progress::Resume, String> {
    let books = state.books.lock().unwrap_or_else(|e| e.into_inner());
    let book = State::find_book(&books, &book_url).ok_or("书架上找不到这本书")?;
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
        current_url: Mutex::new(String::new()),
        pending: Mutex::new(None),
        style_tick: Mutex::new(0),
        reader_open: Mutex::new(false),
        hotkey_at: Mutex::new(None),
        toggles: Mutex::new(Vec::new()),
        app: Mutex::new(None),
        last_saved: Mutex::new(None),
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
                    // 按住不放会连着报「按下」，一次按键只认一次：
                    // 认两次的话，收起那一下紧接着又被当成呼出
                    if !state.mark_hotkey() {
                        return;
                    }
                    // 同一个快捷键既是呼出也是关闭：窗口开着就走 Close，
                    // 主窗口收到后叫阅读窗口自己把进度收干净再藏起来
                    let open = state.reader_open();
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
            save_progress,
            cache_books,
            set_current_book,
            save_reader_size,
            refresh_reader_style,
            resume_point,
        ])
        .run(tauri::generate_context!())
        .expect("reader-mini 启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(url: &str, name: &str) -> api::Book {
        api::Book {
            book_url: url.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    /// 书架是照书找的，不是照下标。
    ///
    /// 远端按阅读时间重排书架之后，主窗口手上那个下标已经指到别的书上，
    /// 照下标续读就会「读 A 续到 B」—— 这正是要按 `bookUrl` 认书的原因。
    #[test]
    fn book_is_found_by_url_after_the_shelf_is_reordered() {
        let shelf = [
            book("https://example.org/a", "甲"),
            book("https://example.org/b", "乙"),
        ];
        // 读者点的是「甲」（下标 0）
        let target = shelf[0].book_url.clone();

        // 远端把「乙」排到了前面：原来那个下标下已经是另一本书了
        let reordered = [
            book("https://example.org/b", "乙"),
            book("https://example.org/a", "甲"),
        ];
        assert_eq!(reordered[0].name, "乙");
        assert_eq!(State::find_book(&reordered, &target).unwrap().name, "甲");
    }

    /// 书被远端移出书架之后找不到，也就不该再往上写进度
    #[test]
    fn missing_book_has_no_match() {
        let shelf = vec![book("https://example.org/a", "甲")];
        assert!(State::find_book(&shelf, "https://example.org/gone").is_none());
    }

    /// 交给阅读窗口的正文里必须带上 `bookUrl`：阅读窗口只看过这份 payload，
    /// 上报进度时下标可能已经过期，唯一靠得住的认书凭据就是这个字段
    #[test]
    fn read_payload_carries_the_book_url() {
        let payload = api::ReadPayload {
            book_url: "https://example.org/a".into(),
            chapter_index: 2,
            chapter_title: "第二章".into(),
            start_line: 0,
            dur_chapter_pos: 0,
            style_tick: 0,
            reposition: true,
            text: "正文".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(
            json.contains(r#""bookUrl":"https://example.org/a""#),
            "{json}"
        );
        assert!(!json.contains("bookIndex"), "不该再有下标字段: {json}");
    }
}
