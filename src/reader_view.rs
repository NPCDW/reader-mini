use slint::{ComponentHandle, ModelRc, VecModel};

use crate::app::{AppWindow, BookItem, ChapterItem};
use crate::reader_win::ReaderWindow;
use std::cell::RefCell;
use std::rc::Rc;

/// 窗口最小尺寸（逻辑像素），别被拉成一条线
pub const MIN_WINDOW_WIDTH: f32 = 260.0;
pub const MIN_WINDOW_HEIGHT: f32 = 200.0;
/// 兜底窗口尺寸：配置里没记过（或记了脏值）时用它
const DEFAULT_WINDOW_WIDTH: f32 = 460.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 560.0;
/// 正文之外的固定高度：顶栏 26px + 底栏 24px
const WINDOW_CHROME_HEIGHT: f32 = 50.0;
/// 正文左右内边距（各 18px）：从窗口宽度估算每行字数时要扣掉
const BODY_SIDE_CHROME: f32 = 36.0;
/// 正文顶部内边距：正文从这里开始画，算「一屏几行」时也要先扣掉。
/// 会写进 UI 的 `body-top-pad`，两边始终用同一个值。
const BODY_TOP_PAD: f32 = 4.0;
/// 量行高时一次量几行：量 N 行再除以 N，压掉长度取整带来的误差
const LINE_PROBE_ROWS: usize = 10;
/// 量不到真实行高时的兜底：字体的自然行高（ascent + descent）≈ 字号的 1.25 倍
const FALLBACK_NATURAL_LINE_RATIO: f32 = 1.25;

/// 拉伸窗口时记下的初始快照
#[derive(Debug, Clone, Copy)]
pub struct ResizeOrigin {
    /// 按下时的全局鼠标坐标
    pub mouse: (f32, f32),
    /// 按下时的窗口位置（物理像素）
    pub pos: (f32, f32),
    /// 按下时的窗口尺寸（物理像素）
    pub size: (f32, f32),
    /// 被拉的边：-1 = 左 / 上，0 = 这一维不动，1 = 右 / 下
    pub edge_x: i32,
    pub edge_y: i32,
}

/// 关闭阅读窗口时的额外钩子：(book_index, chapter_index, chapter_title, 停在第几行)
pub type CloseHook = dyn Fn(usize, i64, String, usize);
/// 本章翻到头的钩子：1 = 要下一章，-1 = 要上一章
pub type ChapterHook = dyn Fn(i32);

thread_local! {
    static CLOSE_HOOK: RefCell<Option<std::rc::Rc<CloseHook>>> = RefCell::new(None);
    static CHAPTER_HOOK: RefCell<Option<std::rc::Rc<ChapterHook>>> = RefCell::new(None);
}

pub fn set_close_hook(hook: std::rc::Rc<CloseHook>) {
    CLOSE_HOOK.with(|c| *c.borrow_mut() = Some(hook));
}

pub fn set_chapter_hook(hook: std::rc::Rc<ChapterHook>) {
    CHAPTER_HOOK.with(|c| *c.borrow_mut() = Some(hook));
}

pub fn clear_chapter_hook() {
    CHAPTER_HOOK.with(|c| *c.borrow_mut() = None);
}

#[derive(Default)]
pub struct ReaderState {
    pub visible: bool,
    pub book_index: usize,
    pub chapter_index: i64,
    pub chapter_title: String,
    /// 整章正文（一次铺满，按屏翻动，不做内容切分）
    pub text: String,
    /// 正文按显示宽度折行后的行号 -> 各行的字符偏移（估算，只用于「显示行 -> 第几段」）
    pub row_offsets: Vec<usize>,
    /// 每一屏顶部所在的显示行；**长度就是本章的页数**
    pub page_tops: Vec<usize>,
    /// 一行的真实高度（像素）：由 Slint 量出来，翻页就按它换算偏移
    pub line_height: f32,
    /// 一屏能放几行
    pub rows_per_page: usize,
    /// 整章折行后一共有几显示行
    pub total_rows: usize,
    /// 当前停在第几屏（0 基）
    pub page_index: usize,
    /// 当前停在正文内容的第几个逻辑行（0 基），用于记住进度
    pub card_line: usize,
    /// 当前屏顶那一行相对正文可视区上沿的偏移（向下滚动为负）
    pub scroll_y: f32,
    /// 拖动时记录：按下时的鼠标坐标与窗口坐标
    pub drag_origin: Option<((f32, f32), (f32, f32))>,
    /// 拉伸时记录：按下时的鼠标坐标、窗口位置、窗口尺寸与被拉的边
    pub resize_origin: Option<ResizeOrigin>,
}

impl ReaderState {
    /// 正在拖动 / 拉伸窗口：这时不要因为“失去焦点”就把窗口关掉
    pub fn interacting(&self) -> bool {
        self.drag_origin.is_some() || self.resize_origin.is_some()
    }
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

/// 按「一屏 `rows_per_screen` 行」把整章切成若干屏。
///
/// 返回**每一屏顶部所在的显示行**下标，长度就是本章的页数：
/// 相邻两屏首尾相接，错开整整 `rows` 行 —— 既不重复一行（否则翻页时会看到
/// 上一屏剩下的半行字），也不会漏掉一行。
/// 最后一屏会往回收一点（顶部取 `总行数 - 一屏行数`），让本章最后一行贴着
/// 屏幕底部，而不是留一屏空白。
pub fn page_tops(total_rows: usize, rows_per_screen: usize) -> Vec<usize> {
    let rows = rows_per_screen.max(1);
    let n = total_rows.max(1);
    if n <= rows {
        return vec![0];
    }
    // 最后一屏的顶部：从这一行开始，最后一行正好落在屏幕底部
    let last_top = n - rows;
    let mut tops = vec![0usize];
    let mut top = 0usize;
    while top < last_top {
        top = (top + rows).min(last_top);
        tops.push(top);
    }
    tops
}

/// 某个显示行落在第几屏（取顶部不超过它的最后一屏）
pub fn page_of_row(page_tops: &[usize], row: usize) -> usize {
    if page_tops.is_empty() {
        return 0;
    }
    page_tops.partition_point(|&t| t <= row).saturating_sub(1)
}

/// 打开阅读窗口并呈现整章正文。
pub fn show_reader(
    app: &AppWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    cfg: &crate::config::Config,
    req: ShowRequest,
) -> anyhow::Result<ReaderWindow> {
    let reader = ReaderWindow::new()?;
    // 上次拉好的尺寸下次打开还生效；只在新建窗口时做，
    // 刷新已有窗口时不该把用户刚拉好的尺寸打回去。
    restore_window_size(&reader, cfg);
    // 回调只在这里接一次：翻到本章最后一页再按 PgDn 时，是在「正在翻页」的回调里
    // 就地刷新同一个窗口的，那时再重设回调会被 Slint 当成
    // 「Callback Handler set while called」直接 panic，程序就退出了。
    wire_reader(&reader, app, state);
    activate(app, &reader, state, cfg, req);
    Ok(reader)
}

/// 把一份内容装进已有（或新建）的阅读窗口：重置状态、铺正文、按行号定位。
///
/// 新建窗口与"刷新同一个窗口"走的是同一条路径：只要给了 `start_line`，
/// 正文会整体重铺并按该行重新定位，不需要区分首次打开还是重开。
/// 回调**不在这里接**：翻章时是在翻页回调里走到这里的，重设回调会 panic。
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
        s.resize_origin = None;
    }

    // 正文可视区跟着窗口尺寸走（新建窗口的尺寸已由 restore_window_size 写入）
    sync_body_metrics(reader);
    // 内容变了：折行表 + 分页表都得重建（按当前字号 + 正文尺寸）—— apply_style 里顺带做掉
    apply_style(reader, cfg, state);
    reader.set_title_text(chapter_title.clone().into());
    reader.set_full_text(state.borrow().text.clone().into());
    reader.set_scroll_offset(0.0);
    // 定位到指定行（顺带算出停在第几屏、刷新页脚）
    locate(reader, state, start_line);
    let _ = app;
}

/// 滚到正文的第 `card_line` 个逻辑行，并同步状态与页脚。
///
/// 定位一律用**量出来的实际行高**换算，不依赖视图报出来的可见行数：
/// 首帧还没布局时那个值可能是 0，靠它会跳错位置。
pub fn locate(
    reader: &ReaderWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    card_line: usize,
) {
    let (card, row, line_height) = {
        let s = state.borrow();
        let max_card = card_line_offsets(&s.text).len().saturating_sub(1);
        let card = card_line.min(max_card);
        (
            card,
            row_of_card_line(reader, &s, card),
            s.line_height.max(1.0),
        )
    };
    {
        let mut s = state.borrow_mut();
        s.card_line = card;
        s.page_index = page_of_row(&s.page_tops, row);
        // content-y 向下为负
        s.scroll_y = -(row as f32 * line_height);
    }
    reader.set_scroll_offset(state.borrow().scroll_y);
    render(reader, state);
}

/// 翻到第 `index` 屏：屏顶那一行贴着正文可视区上沿，当前行同步成那一行。
pub fn goto_page(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>, index: usize) {
    {
        let mut s = state.borrow_mut();
        if s.page_tops.is_empty() {
            s.page_tops = vec![0];
        }
        let idx = index.min(s.page_tops.len() - 1);
        let top = s.page_tops[idx];
        let card = card_line_at_row(&s, top);
        s.page_index = idx;
        s.card_line = card;
        // 屏顶那一行贴着正文可视区上沿（content-y 向下为负）
        s.scroll_y = -(top as f32 * s.line_height.max(1.0));
    }
    reader.set_scroll_offset(state.borrow().scroll_y);
    render(reader, state);
}

/// 停到本章最后一屏（翻到上一章时用：从上一章末尾接着往回读）
pub fn goto_last_page(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>) {
    let last = state.borrow().page_tops.len().saturating_sub(1);
    goto_page(reader, state, last);
}

/// 某个逻辑行落在折行后的第几个显示行（对外也用于定位 / 测试）
pub fn row_of(s: &ReaderState, card_line: usize) -> usize {
    let offsets = card_line_offsets(&s.text);
    offsets
        .get(card_line)
        .map(|off| row_of_offset(&s.row_offsets, *off))
        .unwrap_or(0)
}

/// 第 `card` 个逻辑行落在折行后的第几个显示行。
///
/// 折行是 Slint 做的，所以最准的办法是让它量一下「这一行之前的正文」有多高：
/// 高度 ÷ 行高就是行号（量的是到行首为止，结尾的 `\n` 不算一行，先去掉）。
/// 量不出来（没有可用字体 / 还没布局）才退回按字数估算的折行表。
fn row_of_card_line(reader: &ReaderWindow, s: &ReaderState, card: usize) -> usize {
    let line_height = s.line_height;
    let Some(&offset) = card_line_offsets(&s.text).get(card) else {
        return 0;
    };
    if offset == 0 {
        return 0;
    }
    if line_height > 1.0 {
        let prefix = &s.text[..offset.min(s.text.len())];
        let prefix = prefix.strip_suffix('\n').unwrap_or(prefix);
        // 空文本也要算一行，量出来是行高而不是 0，所以空前缀直接走估算
        if !prefix.is_empty() {
            let h = measure_text_height(reader, prefix);
            if h > 0.0 {
                return (h / line_height).round() as usize;
            }
        }
    }
    row_of_offset(&s.row_offsets, offset)
}

/// 让 Slint 量一下「这段文本在正文宽度下有多高」。
///
/// 探针 Text 不显示（`opacity: 0`），只为把 Slint 排版出来的真实高度读回 Rust：
/// 行高、整章行数、某一行在第几行都由此而来，不靠字号估算。
fn measure_text_height(reader: &ReaderWindow, text: &str) -> f32 {
    reader.set_measure_text(text.into());
    reader.get_measured_height().max(0.0)
}

/// 一行的真实高度（像素）：让 Slint 量 `LINE_PROBE_ROWS` 行再除以行数。
///
/// Slint 的行高 = 字体自然行高（ascent + descent）× `line-height-factor`，
/// **不是**字号 × 倍数 —— 按字号算会小掉三分之一，一屏就会塞进太多行，
/// 每屏底部都被切掉半行（翻页时看着就像"字被劈成两半"）。
/// 量不到时按「自然行高 ≈ 1.25 倍字号」兜底，宁可少放一行也不切字。
pub fn line_height_of(reader: &ReaderWindow) -> f32 {
    let probe = ["字"; LINE_PROBE_ROWS].join("\n");
    let h = measure_text_height(reader, &probe);
    if h > LINE_PROBE_ROWS as f32 {
        h / LINE_PROBE_ROWS as f32
    } else {
        (reader.get_font_size() * reader.get_line_height_factor() * FALLBACK_NATURAL_LINE_RATIO)
            .max(1.0)
    }
}

/// 整章折行后一共有几显示行：量出来的全文高度 ÷ 行高；量不到就用估算的折行表
fn content_rows_of(reader: &ReaderWindow, text: &str, line_height: f32, fallback: usize) -> usize {
    let h = measure_text_height(reader, text);
    if h > line_height * 0.5 {
        (h / line_height).round().max(1.0) as usize
    } else {
        fallback.max(1)
    }
}

/// 字符偏移落在折行后的第几个显示行
pub fn row_of_offset(row_offsets: &[usize], offset: usize) -> usize {
    row_offsets
        .partition_point(|&o| o <= offset)
        .saturating_sub(1)
}

/// 某个显示行上是第几个逻辑行（行号超出正文时取最后一行）
pub fn card_line_at_row(s: &ReaderState, row: usize) -> usize {
    card_line_offsets(&s.text)
        .iter()
        .enumerate()
        .take_while(|(_, off)| row_of_offset(&s.row_offsets, **off) <= row)
        .last()
        .map(|(i, _)| i)
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
    rebuild_pages(reader, state);
    locate(reader, state, line);
}

/// 按当前窗口尺寸 / 字号重建行高、折行表与分页表。
///
/// 一屏几行 = （正文可视高度 - 顶部内边距）÷ **量出来的行高**，
/// 整章几行 = Slint 量出来的全文高度 ÷ 行高；
/// 一行几个字 = 正文可视宽度 ÷ 字号（只用来把显示行换回第几段）。
/// 三者都跟着窗口走，所以改字号、换章、拉伸窗口之后都要重算一遍。
pub fn rebuild_pages(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>) {
    let line_height = line_height_of(reader);
    let rows = rows_per_screen(reader.get_body_height(), line_height);
    let per_row = line_chars_per_row(body_width_of(reader), reader.get_font_size());
    let text = state.borrow().text.clone();
    let mut s = state.borrow_mut();
    s.line_height = line_height;
    s.rows_per_page = rows;
    s.row_offsets = build_row_offsets(&text, per_row.round().max(1.0) as usize);
    s.total_rows = content_rows_of(
        reader,
        &text,
        line_height,
        s.row_offsets.len().saturating_sub(1),
    );
    s.page_tops = page_tops(s.total_rows, rows);
}

/// 按正文宽度与字号估算每行能放几个字（中文按一个字占一个字号宽算）
fn line_chars_per_row(body_width: f32, font_size: f32) -> f32 {
    let usable = (body_width - BODY_SIDE_CHROME).max(font_size * 4.0);
    (usable / font_size.max(6.0)).max(8.0)
}

/// 正文可视宽度：ScrollView 还没上报就退回窗口宽度，再不行用默认宽度
fn body_width_of(reader: &ReaderWindow) -> f32 {
    let w = reader.get_body_width();
    if w > 1.0 {
        return w;
    }
    let (ww, _) = logical_window_size(reader);
    if ww > 1.0 { ww } else { DEFAULT_WINDOW_WIDTH }
}

/// 窗口尺寸（逻辑像素）
fn logical_window_size(reader: &ReaderWindow) -> (f32, f32) {
    let sf = reader.window().scale_factor().max(0.1);
    let s = reader.window().size();
    (s.width as f32 / sf, s.height as f32 / sf)
}

/// 让「正文可视区尺寸」与当前窗口尺寸对齐。
///
/// 窗口已经有真实尺寸（比如被拉伸过）就用它；新建的窗口还没 show，
/// 尺寸要等 show 之后才知道，这时保留 ScrollView 自己报的值。
fn sync_body_metrics(reader: &ReaderWindow) {
    let (w, h) = (reader.get_win_width(), reader.get_win_height());
    if w > 1.0 && h > 1.0 {
        reader.set_body_width(w);
        reader.set_body_height((h - WINDOW_CHROME_HEIGHT).max(1.0));
    }
}

/// 按配置恢复阅读窗口尺寸（逻辑像素）。
///
/// 改的是 UI 上的 `win-width` / `win-height`：它们同时是布局约束，
/// 所以窗口一创建就是配置里的尺寸，不会被系统的“首选尺寸”拽回 460×560。
pub fn restore_window_size(reader: &ReaderWindow, cfg: &crate::config::Config) {
    let w = if cfg.reader_width >= MIN_WINDOW_WIDTH {
        cfg.reader_width
    } else {
        DEFAULT_WINDOW_WIDTH
    };
    let h = if cfg.reader_height >= MIN_WINDOW_HEIGHT {
        cfg.reader_height
    } else {
        DEFAULT_WINDOW_HEIGHT
    };
    reader.set_win_width(w);
    reader.set_win_height(h);
    reader.set_body_width(w);
    reader.set_body_height((h - WINDOW_CHROME_HEIGHT).max(1.0));
    // 正文的上内边距只有 Rust 这一份定义，写进 UI 免得两边算不到一起
    reader.set_body_top_pad(BODY_TOP_PAD);
}

/// 按「拉的是哪条边 + 鼠标位移」算出新的窗口矩形 `(x, y, w, h)`。
///
/// 拉左边 / 上边时被拉的那一角要跟着鼠标走，所以位置 = 原位置 + (原尺寸 - 新尺寸)。
/// 尺寸夹在 `[min, 原位置 + 原尺寸]` 之间：既不小于下限，也不会被拖到屏幕左 / 上之外。
pub fn resize_rect(
    o: ResizeOrigin,
    mx: f32,
    my: f32,
    min_w: f32,
    min_h: f32,
) -> (f32, f32, f32, f32) {
    let dx = mx - o.mouse.0;
    let dy = my - o.mouse.1;
    let max_w = (o.pos.0 + o.size.0).max(min_w);
    let max_h = (o.pos.1 + o.size.1).max(min_h);
    let w = match o.edge_x {
        -1 => (o.size.0 - dx).clamp(min_w, max_w),
        1 => (o.size.0 + dx).max(min_w),
        _ => o.size.0,
    };
    let h = match o.edge_y {
        -1 => (o.size.1 - dy).clamp(min_h, max_h),
        1 => (o.size.1 + dy).max(min_h),
        _ => o.size.1,
    };
    let x = if o.edge_x < 0 {
        o.pos.0 + (o.size.0 - w)
    } else {
        o.pos.0
    };
    let y = if o.edge_y < 0 {
        o.pos.1 + (o.size.1 - h)
    } else {
        o.pos.1
    };
    (x, y, w, h)
}

/// 把窗口拉到鼠标当前位置对应的尺寸，返回拉伸后的窗口尺寸（逻辑像素）。
///
/// 只改窗口本身，不动正文：一次拉伸里鼠标会移动几十次，每移动一次都重排整章正文太贵，
/// 折行表放到松手时（`finish_resize`）再算一次。
fn apply_resize(reader: &ReaderWindow, o: ResizeOrigin) -> Option<(f32, f32)> {
    let (mx, my) = crate::platform::mouse_pos()?;
    let sf = reader.window().scale_factor().max(0.1);
    let (x, y, w, h) = resize_rect(o, mx, my, MIN_WINDOW_WIDTH * sf, MIN_WINDOW_HEIGHT * sf);
    let (lw, lh) = (w / sf, h / sf);
    // 尺寸写在 UI 属性上：它同时是布局约束（min == max），系统侧不会再把它拽回去
    reader.set_win_width(lw);
    reader.set_win_height(lh);
    reader.window().set_size(slint::PhysicalSize::new(
        w.round().max(1.0) as u32,
        h.round().max(1.0) as u32,
    ));
    // 拉左边 / 上边时窗口的左上角也要跟着走
    if o.edge_x < 0 || o.edge_y < 0 {
        reader.window().set_position(slint::PhysicalPosition::new(
            x.round() as i32,
            y.round() as i32,
        ));
    }
    Some((lw, lh))
}

/// 拉伸结束：按新尺寸重算正文可视区与折行表，并留在原来读到的那一行。
pub fn finish_resize(
    reader: &ReaderWindow,
    state: &Rc<std::cell::RefCell<ReaderState>>,
    width: f32,
    height: f32,
) {
    reader.set_body_width(width.max(1.0));
    reader.set_body_height((height - WINDOW_CHROME_HEIGHT).max(1.0));
    // 窗口变了：一屏几行 / 一行几字都变，页数也得重算
    let line = state.borrow().card_line;
    rebuild_pages(reader, state);
    locate(reader, state, line);
    save_size(width, height);
}

/// 一屏能放几行正文：正文可视高度扣掉顶部内边距，再除以**真实行高**。
///
/// 用 `floor` 是刻意的：宁可底部空一点，也不能让最后一行只露出一半。
pub fn rows_per_screen(body_height: f32, line_height: f32) -> usize {
    let lh = line_height.max(1.0);
    let usable = (body_height - BODY_TOP_PAD).max(lh);
    (usable / lh).floor().max(1.0) as usize
}

/// 页脚：第几页 / 共几页 + 读到百分之多少（按本章的折行总行数算）
fn render(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>) {
    let info = {
        let s = state.borrow();
        let total_rows = s.total_rows.max(1);
        let pages = s.page_tops.len().max(1);
        let page = s.page_index.min(pages - 1);
        // 本屏最后一行就是读完的地方
        let bottom = s.page_tops.get(page).copied().unwrap_or(0) + s.rows_per_page.max(1);
        let pct = ((bottom.min(total_rows) as f32 / total_rows as f32) * 100.0).round() as i32;
        format!("第 {}/{} 页 · {}%", page + 1, pages, pct.clamp(0, 100))
    };
    reader.set_page_info(info.into());
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

/// PgDn / PgUp：在本章内翻一屏；翻到本章最后一页 / 第一页还按，就翻到下一章 / 上一章。
fn step(reader: &ReaderWindow, state: &Rc<std::cell::RefCell<ReaderState>>, direction: i32) {
    let (page, pages) = {
        let s = state.borrow();
        let pages = s.page_tops.len().max(1);
        (s.page_index.min(pages - 1), pages)
    };
    if direction >= 0 {
        if page + 1 < pages {
            goto_page(reader, state, page + 1);
        } else {
            request_chapter(1);
        }
    } else if page > 0 {
        goto_page(reader, state, page - 1);
    } else {
        request_chapter(-1);
    }
}

/// 本章翻到头：取上一章 / 下一章这件事交给 main（它手上有目录和接口）
fn request_chapter(direction: i32) {
    if let Some(cb) = CHAPTER_HOOK.with(|c| c.borrow().clone()) {
        cb(direction);
    }
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
            // 通过回调把进度同步到服务端并落到本地（由 main.rs 提供实现）：
            // 记哪一章、哪一行都交给 main，它手上有 bookUrl 能当键
            if let Some(cb) = CLOSE_HOOK.with(|c| c.borrow().clone()) {
                cb(snap.0, snap.1, snap.2, snap.3);
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

    // 拉伸：和拖动一样用系统鼠标位置换算。按下时记下「鼠标 / 窗口位置 / 窗口尺寸 /
    // 拉的是哪条边」，之后每次移动都按初始快照重算，避免误差累积。
    let weak_rs = reader.as_weak();
    let st_rs = state.clone();
    reader.on_resize_start(move |edge_x, edge_y| {
        let Some((mx, my)) = crate::platform::mouse_pos() else {
            return;
        };
        if let Some(r) = weak_rs.upgrade() {
            let p = r.window().position();
            let s = r.window().size();
            st_rs.borrow_mut().resize_origin = Some(ResizeOrigin {
                mouse: (mx, my),
                pos: (p.x as f32, p.y as f32),
                size: (s.width as f32, s.height as f32),
                edge_x,
                edge_y,
            });
        }
    });

    let weak_rm = reader.as_weak();
    let st_rm = state.clone();
    reader.on_resizing(move || {
        let Some(o) = st_rm.borrow().resize_origin else {
            return;
        };
        if let Some(r) = weak_rm.upgrade() {
            let _ = apply_resize(&r, o);
        }
    });

    let weak_re = reader.as_weak();
    let st_re = state.clone();
    reader.on_resize_end(move || {
        let Some(o) = st_re.borrow().resize_origin else {
            return;
        };
        st_re.borrow_mut().resize_origin = None;
        if let Some(r) = weak_re.upgrade()
            && let Some((w, h)) = apply_resize(&r, o)
        {
            finish_resize(&r, &st_re, w, h);
        }
    });

    let _ = app;
}

/// 窗口尺寸写回配置
pub fn save_window_size(reader: &ReaderWindow) {
    save_size(reader.get_win_width(), reader.get_win_height());
}

/// 把一组窗口尺寸（逻辑像素）写回配置：太小的不入盘，免得下次开成一条线
pub fn save_size(width: f32, height: f32) {
    if width < MIN_WINDOW_WIDTH || height < MIN_WINDOW_HEIGHT {
        return;
    }
    let mut cfg = crate::config::Config::load();
    cfg.reader_width = width;
    cfg.reader_height = height;
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

/// 目录模型：`index` 必须是服务端目录里的原始下标（`getBookContent` 用它取正文），
/// 所以过滤空标题时不能重新编号。
pub fn chapters_model(chapters: &[crate::api::Chapter]) -> ModelRc<ChapterItem> {
    let items: Vec<ChapterItem> = chapters
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.title.is_empty())
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
