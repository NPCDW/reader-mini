use slint::{ComponentHandle, ModelRc, VecModel};

use crate::app::{AppWindow, BookItem, ChapterItem};
use crate::reader_win::ReaderWindow;
use std::cell::RefCell;
use std::rc::Rc;

/// 每行最多显示的字数。窗口定宽，按字号估算列数，用于「光标行 -> 断行行号」换算。
pub const LINE_CHARS_PER_ROW: f32 = 19.0;

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
    /// 正文全文（不再分页）
    pub text: String,
    /// 正文按显示宽度折行后的行号 -> 各行的字符偏移
    pub row_offsets: Vec<usize>,
    /// 当前停在正文内容的第几个逻辑行（0 基），用于记住进度
    pub card_line: usize,
    /// 当前行在视口顶部的偏移（ScrollView content-y 语义，向下滚动为负）
    pub scroll_y: f32,
    /// 拖动时记录：按下时的鼠标坐标与窗口坐标
    pub drag_origin: Option<((f32, f32), (f32, f32))>,
}

/// 打开阅读窗口所需参数
pub struct ShowRequest {
    pub book_index: usize,
    pub chapter_index: i64,
    pub chapter_title: String,
    pub text: String,
    pub start_line: usize,
}

/// 生成「折行后的行号 -> 原文里的字符偏移」映射，末尾额外补一个总长度做结尾哨兵。
pub fn build_row_offsets(text: &str, chars_per_row: usize) -> Vec<usize> {
    let per_row = chars_per_row.max(1);
    // 每个字符起始的字节偏移，末尾补总长度做哨兵
    let starts: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut offsets = vec![0usize];
    let mut col = 0usize;
    for (n, (i, ch)) in text.char_indices().enumerate() {
        if ch == '\n' {
            offsets.push(i + ch.len_utf8());
            col = 0;
            continue;
        }
        col += 1;
        if col >= per_row {
            // 这个字符之后换到下一显示行
            offsets.push(starts[n + 1]);
            col = 0;
        }
    }
    offsets.push(text.len());
    offsets
}

/// 给定当前屏的位置，算出按 PgUp / PgDn 之后应该停在哪个逻辑行。
///
/// 规则是「上一屏最后一行成为下一屏第一行」：一屏能显示 `rows` 个显示行
/// （下标 `p..=p+rows-1`），最后一行是 `p + rows - 1`；下一屏就从那一行开始。
/// 于是每按一次 PgDn，新屏顶部前进 `rows - 1` 个显示行，PgUp 反着来。
pub fn plan_page_move(
    card_line: usize,
    card_offsets: &[usize],
    row_offsets: &[usize],
    rows_per_screen: usize,
    direction: i32,
) -> usize {
    let step = rows_per_screen.saturating_sub(1).max(1);
    let total_rows = row_offsets.len().saturating_sub(1);
    let anchors: Vec<usize> = card_offsets
        .iter()
        .map(|off| {
            row_offsets
                .partition_point(|&o| o <= *off)
                .saturating_sub(1)
        })
        .collect();
    let last = anchors.len().saturating_sub(1);

    // 当前逻辑行在折行行号里的位置；末屏顶部被夹在 total_row - 1 之内，
    // 保证最后一行仍落在可视区内。
    let start = anchors.get(card_line).copied().unwrap_or(0);
    let max_top = total_rows.saturating_sub(1);
    let target = if direction >= 0 {
        start.saturating_add(step).min(max_top)
    } else {
        start.saturating_sub(step)
    };

    // 落到哪个逻辑行：取锚点行号不超过 target 的最后一行
    let mut line = 0usize;
    for (i, &a) in anchors.iter().enumerate() {
        if a <= target {
            line = i;
        } else {
            break;
        }
    }
    line.min(last)
}

/// 打开阅读窗口并呈现整章正文。
pub fn show_reader(
    app: &AppWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    cfg: &crate::config::Config,
    req: ShowRequest,
) -> anyhow::Result<ReaderWindow> {
    let reader = ReaderWindow::new()?;
    activate(app, &reader, state, cfg, req);
    Ok(reader)
}

/// 把一份内容装进已有（或新建）的阅读窗口：重置状态、铺正文、按行号定位、接线回调。
///
/// 新建窗口与"刷新同一个窗口"走的是同一条路径：只要给了 `start_line`，
/// 正文会整体重铺并按该行重新定位，不需要区分首次打开还是重开。
pub fn activate(
    app: &AppWindow,
    reader: &ReaderWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    cfg: &crate::config::Config,
    req: ShowRequest,
) {
    let ShowRequest {
        book_index,
        chapter_index,
        chapter_title,
        text,
        start_line,
    } = req;
    {
        let mut s = state.borrow_mut();
        s.visible = true;
        s.book_index = book_index;
        s.chapter_index = chapter_index;
        s.chapter_title = chapter_title.clone();
        s.text = text;
        s.card_line = 0;
        s.scroll_y = 0.0;
        s.drag_origin = None;
    }

    // 内容变了，折行表得重建（按当前字号）—— apply_style 里会顺带做掉
    apply_style(reader, cfg, state);
    reader.set_title_text(chapter_title.clone().into());
    reader.set_page_info("".into());
    reader.set_full_text(state.borrow().text.clone().into());
    reader.set_scroll_offset(0.0);
    render(reader, state);
    wire_reader(reader, app, state);
    locate(reader, state, start_line);
}

/// 滚到正文的第 `card_line` 个逻辑行，并同步状态与页脚。
///
/// 定位一律用视口的**实际行高**换算，不依赖视图报出来的可见行数：
/// 首帧还没布局时那个值可能是 0，靠它会跳错位置。
pub fn locate(
    reader: &ReaderWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    card_line: usize,
) {
    let line_height = (reader.get_font_size() * reader.get_line_height_factor()).max(1.0);
    {
        let mut s = state.borrow_mut();
        let total_rows = s.row_offsets.len().saturating_sub(1);
        s.card_line = card_line.min(total_rows);
        let row = row_of(&s, s.card_line);
        // content-y 向下为负
        s.scroll_y = -(row as f32 * line_height);
    }
    reader.set_scroll_offset(state.borrow().scroll_y);
    render(reader, state);
}

/// 某个逻辑行落在折行后的第几个显示行（对外也用于定位 / 测试）
pub fn row_of(s: &ReaderState, card_line: usize) -> usize {
    let offsets = card_line_offsets(&s.text);
    offsets
        .get(card_line)
        .map(|off| {
            s.row_offsets
                .partition_point(|&o| o <= *off)
                .saturating_sub(1)
        })
        .unwrap_or(0)
}

/// 把配置里的颜色 / 字号应用到阅读窗口。
///
/// 字号 / 行高一变，折行表和滚动坐标就全变了，所以顺手按当前逻辑行重新定位一次，
/// 免得"改个字号就跳回开头"。
pub fn apply_style(
    reader: &ReaderWindow,
    cfg: &crate::config::Config,
    state: &Rc<std::cell::RefCell<ReaderState>>,
) {
    reader.set_bg(parse_color(&cfg.read_bg));
    reader.set_fg(parse_color(&cfg.read_fg));
    reader.set_font_size(cfg.read_font_size as f32);
    reader.set_line_height_factor(cfg.read_line_height);

    let line = state.borrow().card_line;
    let text = state.borrow().text.clone();
    state.borrow_mut().row_offsets =
        build_row_offsets(&text, line_chars_per_row(cfg.read_font_size) as usize);
    locate(reader, state, line);
}

/// 按字号估算每屏能放几行正文
fn line_chars_per_row(font_size: i32) -> f32 {
    (LINE_CHARS_PER_ROW * 20.0 / font_size.max(6) as f32).max(8.0)
}

/// 一屏可显示的正文行数
///
/// 两个输入都能拿到实际值：正文可视高度来自 ScrollView 的 visible-height，
/// 行高 = 字号 × 行高倍数。首帧还没布局完时 height 可能是 0，按 1 行兜底。
pub fn rows_per_screen(body_height: f32, font_size: f32, line_height_factor: f32) -> usize {
    let line_height = (font_size * line_height_factor).max(1.0);
    ((body_height.max(line_height)) / line_height)
        .floor()
        .max(1.0) as usize
}

fn render(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>) {
    let s = state.borrow();
    let total_rows = s.row_offsets.len().saturating_sub(1).max(1);
    // 当前逻辑行在折行后的行号
    let row = row_of(&s, s.card_line);
    // 距离正文末尾还有几行
    let remain = total_rows.saturating_sub(row);
    let pct = if total_rows == 0 {
        100
    } else {
        (((total_rows - remain) as f32 / total_rows as f32) * 100.0).round() as i32
    };
    reader.set_page_info(format!("{}% · 第 {} 行", pct.max(0), s.card_line + 1).into());
}

/// 正文里每个逻辑行的字符偏移（以 `\n` 分段）
pub fn card_line_offsets(text: &str) -> Vec<usize> {
    let mut offs = vec![0usize];
    for (idx, b) in text.bytes().enumerate() {
        if b == b'\n' {
            offs.push(idx + 1);
        }
    }
    offs
}

fn step(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>, direction: i32) {
    let rows = rows_per_screen(
        reader.get_body_height(),
        reader.get_font_size(),
        reader.get_line_height_factor(),
    );
    let line_height = (reader.get_font_size() * reader.get_line_height_factor()).max(1.0);
    {
        let mut s = state.borrow_mut();
        let card_offsets = card_line_offsets(&s.text);
        let row_offsets = s.row_offsets.clone();
        s.card_line = plan_page_move(
            s.card_line,
            &card_offsets,
            &row_offsets,
            rows.max(2),
            direction,
        );
        s.scroll_y = -(row_of(&s, s.card_line) as f32 * line_height);
    }
    let y = state.borrow().scroll_y;
    reader.set_scroll_offset(y);
    render(reader, state);
}

fn wire_reader(
    reader: &ReaderWindow,
    app: &AppWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
) {
    let weak_scroll = reader.as_weak();
    let st = state.clone();
    reader.on_scroll_page(move |direction| {
        if let Some(r) = weak_scroll.upgrade() {
            step(&r, &st, direction);
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
                    s.card_line,
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
        if let Some((mx, my)) = crate::platform::mouse_pos()
            && let Some(r) = weak_drag.upgrade()
        {
            let p = r.window().position();
            st_drag.borrow_mut().drag_origin = Some(((mx, my), (p.x as f32, p.y as f32)));
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
