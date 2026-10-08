use slint::{ComponentHandle, ModelRc, VecModel};

use crate::app::{AppWindow, BookItem, ChapterItem};
use crate::reader_win::ReaderWindow;
use std::cell::RefCell;
use std::rc::Rc;

const PAGE_CHARS: usize = 380;

/// 关闭阅读窗口时的额外钩子：(book_index, chapter_index, chapter_title)
pub type CloseHook = dyn Fn(usize, i64, String);

thread_local! {
    static CLOSE_HOOK: RefCell<Option<std::rc::Rc<CloseHook>>> = RefCell::new(None);
}

pub fn set_close_hook(hook: std::rc::Rc<CloseHook>) {
    CLOSE_HOOK.with(|c| *c.borrow_mut() = Some(hook));
}

#[derive(Default)]
pub struct ReaderState {
    pub visible: bool,
    pub book_index: usize,
    pub chapter_index: i64,
    pub chapter_title: String,
    pub text: String,
    pub pages: Vec<String>,
    pub page: usize,
    /// 拖动时记录：按下时的鼠标坐标与窗口坐标
    pub drag_origin: Option<((f32, f32), (f32, f32))>,
}

/// 打开阅读窗口所需参数
pub struct ShowRequest {
    pub book_index: usize,
    pub chapter_index: i64,
    pub chapter_title: String,
    pub text: String,
    pub start_page: usize,
}

/// 打开阅读窗口并呈现指定章节正文。
pub fn show_reader(
    app: &AppWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    cfg: &crate::config::Config,
    req: ShowRequest,
) -> anyhow::Result<ReaderWindow> {
    let ShowRequest {
        book_index,
        chapter_index,
        chapter_title,
        text,
        start_page,
    } = req;
    let reader = ReaderWindow::new()?;

    reader.set_bg(parse_color(&cfg.read_bg));
    reader.set_fg(parse_color(&cfg.read_fg));
    reader.set_font_size((cfg.read_font_size as f32) * 1.0);
    reader.window().set_size(slint::PhysicalSize::new(
        cfg.reader_width as u32,
        cfg.reader_height as u32,
    ));
    reader.set_title_text(chapter_title.clone().into());

    let pages = crate::api::paginate(&text, PAGE_CHARS);
    let start = start_page.min(pages.len().saturating_sub(1));
    {
        let mut s = state.borrow_mut();
        s.visible = true;
        s.book_index = book_index;
        s.chapter_index = chapter_index;
        s.chapter_title = chapter_title;
        s.text = text;
        s.pages = pages;
        s.page = start;
    }

    render_page(&reader, state);
    wire_reader(&reader, app, state);
    Ok(reader)
}

fn render_page(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>) {
    let s = state.borrow();
    let total = s.pages.len().max(1);
    let text = s.pages.get(s.page).cloned().unwrap_or_default();
    reader.set_page_text(text.into());
    reader.set_page_info(format!("{}/{}", s.page + 1, total).into());
}

fn step(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>, next: bool) {
    {
        let mut s = state.borrow_mut();
        if next {
            if s.page + 1 < s.pages.len() {
                s.page += 1;
            }
        } else if s.page > 0 {
            s.page -= 1;
        }
    }
    render_page(reader, state);
}

fn wire_reader(
    reader: &ReaderWindow,
    app: &AppWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
) {
    let weak_next = reader.as_weak();
    let st = state.clone();
    reader.on_next_page(move || {
        if let Some(r) = weak_next.upgrade() {
            step(&r, &st, true);
        }
    });

    let weak_prev = reader.as_weak();
    let st = state.clone();
    reader.on_prev_page(move || {
        if let Some(r) = weak_prev.upgrade() {
            step(&r, &st, false);
        }
    });

    // 关闭：记住窗口尺寸、保存进度、隐藏
    let weak_close = reader.as_weak();
    let st = state.clone();
    reader.on_closed(move || {
        if let Some(r) = weak_close.upgrade() {
            save_window_size(&r);
            let snap = {
                let s = st.borrow();
                (
                    s.book_index,
                    s.chapter_index,
                    s.chapter_title.clone(),
                    s.page,
                )
            };
            let _ = r.hide();
            st.borrow_mut().visible = false;
            crate::progress::remember(snap.0, snap.1, snap.2.clone(), snap.3);
            // 通过回调把进度同步到服务端（由 main.rs 提供实现）
            if let Some(cb) = CLOSE_HOOK.with(|c| c.borrow().clone()) {
                cb(snap.0, snap.1, snap.2);
            }
        }
    });

    // 拖动：Slint 的 TouchArea.moved 只给相对位移，这里用系统鼠标位置换算
    let weak_drag = reader.as_weak();
    let st_drag = state.clone();
    reader.on_drag_start(move || {
        let _ = weak_drag.upgrade();
        if let Some((mx, my)) = crate::platform::mouse_pos() {
            if let Some(r) = weak_drag.upgrade() {
                let p = r.window().position();
                st_drag.borrow_mut().drag_origin = Some(((mx, my), (p.x as f32, p.y as f32)));
            }
        }
    });

    let weak_move = reader.as_weak();
    let st_move = state.clone();
    reader.on_dragging(move || {
        let origin = st_move.borrow().drag_origin;
        let Some((start_mouse, start_win)) = origin else {
            return;
        };
        let Some((mx, my)) = crate::platform::mouse_pos() else {
            return;
        };
        let target = (
            start_win.0 + (mx - start_mouse.0),
            start_win.1 + (my - start_mouse.1),
        );
        if let Some(r) = weak_move.upgrade() {
            r.window().set_position(slint::PhysicalPosition::new(
                target.0.round() as i32,
                target.1.round() as i32,
            ));
        }
    });

    let weak_end = reader.as_weak();
    let st_end = state.clone();
    reader.on_drag_end(move || {
        let _ = weak_end.upgrade();
        st_end.borrow_mut().drag_origin = None;
    });

    // 失焦自动关闭：主循环里轮询（见 main.rs），这里提供判断依据
    let _ = app;
}

/// 窗口尺寸写回配置
pub fn save_window_size(reader: &ReaderWindow) {
    let mut cfg = crate::config::Config::load();
    cfg.reader_width = reader.window().size().width as f32;
    cfg.reader_height = reader.window().size().height as f32;
    let _ = cfg.save();
}

/// 供 UI 显示用的书架模型
pub fn books_model(books: &[crate::api::Book]) -> ModelRc<BookItem> {
    let items: Vec<BookItem> = books
        .iter()
        .map(|b| BookItem {
            name: b.name.clone().into(),
            author: b.author.clone().into(),
            intro: b.intro.replace('\n', " ").into(),
            progress_text: format!("读至 {} 章 · {}", b.dur_chapter_index, b.dur_chapter_title)
                .into(),
            latest: format!("最新 {}", b.latest_chapter_title).into(),
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

pub fn chapters_model(chapters: &[crate::api::Chapter]) -> ModelRc<ChapterItem> {
    let items: Vec<ChapterItem> = chapters
        .iter()
        .enumerate()
        .map(|(i, c)| ChapterItem {
            title: c.title.clone().into(),
            index: i as i32,
        })
        .collect();
    ModelRc::new(VecModel::from(items))
}

/// 解析 "#rrggbb" 或 "#aarrggbb" 颜色字符串
pub fn parse_color(spec: &str) -> slint::Brush {
    let hex = spec.trim().trim_start_matches('#');
    let parse = |h: &str| u8::from_str_radix(h, 16).unwrap_or(0);
    let (a, r, g, b) = match hex.len() {
        8 => (
            parse(&hex[0..2]),
            parse(&hex[2..4]),
            parse(&hex[4..6]),
            parse(&hex[6..8]),
        ),
        6 => (255, parse(&hex[0..2]), parse(&hex[2..4]), parse(&hex[4..6])),
        _ => (255, 245, 240, 225),
    };
    slint::Color::from_argb_u8(a, r, g, b).into()
}
