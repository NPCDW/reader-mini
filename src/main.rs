use reader_mini::app::{AppWindow, BookItem, ChapterItem};
use reader_mini::reader_win::ReaderWindow;
use reader_mini::{api, config, platform, progress, reader_view};

use anyhow::Result;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use slint::ComponentHandle;

fn parse_hotkey(spec: &str) -> HotKey {
    let mut mods = Modifiers::empty();
    let mut code = Code::KeyR;
    for part in spec.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "win" => mods |= Modifiers::SUPER,
            other => {
                if let Some(c) = other.chars().next() {
                    code = match c {
                        'a'..='z' => letter_code(c),
                        '0'..='9' => digit_code(c),
                        _ => Code::KeyR,
                    };
                }
            }
        }
    }
    HotKey::new(Some(mods), code)
}

fn letter_code(c: char) -> Code {
    match c {
        'a' => Code::KeyA,
        'b' => Code::KeyB,
        'c' => Code::KeyC,
        'd' => Code::KeyD,
        'e' => Code::KeyE,
        'f' => Code::KeyF,
        'g' => Code::KeyG,
        'h' => Code::KeyH,
        'i' => Code::KeyI,
        'j' => Code::KeyJ,
        'k' => Code::KeyK,
        'l' => Code::KeyL,
        'm' => Code::KeyM,
        'n' => Code::KeyN,
        'o' => Code::KeyO,
        'p' => Code::KeyP,
        'q' => Code::KeyQ,
        'r' => Code::KeyR,
        's' => Code::KeyS,
        't' => Code::KeyT,
        'u' => Code::KeyU,
        'v' => Code::KeyV,
        'w' => Code::KeyW,
        'x' => Code::KeyX,
        'y' => Code::KeyY,
        _ => Code::KeyZ,
    }
}

fn digit_code(c: char) -> Code {
    match c {
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        _ => Code::Digit9,
    }
}

enum HotkeyMsg {
    Toggle,
}

fn main() -> Result<()> {
    let cfg = Rc::new(RefCell::new(config::Config::load()));
    let books: Rc<RefCell<Vec<api::Book>>> = Rc::new(RefCell::new(Vec::new()));
    let chapters: Rc<RefCell<Vec<api::Chapter>>> = Rc::new(RefCell::new(Vec::new()));
    let reader_state: Rc<RefCell<reader_view::ReaderState>> =
        Rc::new(RefCell::new(Default::default()));
    let current_book: Rc<RefCell<i64>> = Rc::new(RefCell::new(-1));

    let rt = tokio::runtime::Runtime::new()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let ui = AppWindow::new()?;
    {
        let c = cfg.borrow();
        ui.set_base_url(c.base_url.clone().into());
        ui.set_read_bg(c.read_bg.clone().into());
        ui.set_read_fg(c.read_fg.clone().into());
        ui.set_read_font_size(c.read_font_size);
        ui.set_hotkey(c.hotkey.clone().into());
    }
    ui.set_books(ModelRc::new(VecModel::from(Vec::<BookItem>::new())));
    ui.set_chapters(ModelRc::new(VecModel::from(Vec::<ChapterItem>::new())));

    // 窗口/快捷键 -> 主循环通信
    let (tx, rx) = mpsc::channel::<HotkeyMsg>();
    let _hotkey_manager = {
        let spec = cfg.borrow().hotkey.clone();
        match GlobalHotKeyManager::new() {
            Ok(m) => {
                let hk = parse_hotkey(&spec);
                if let Err(e) = m.register(hk) {
                    eprintln!("注册全局快捷键 {spec} 失败: {e}");
                }
                let tx = tx.clone();
                std::thread::spawn(move || loop {
                    if let Ok(ev) = GlobalHotKeyEvent::receiver().recv() {
                        let _ = tx.send(HotkeyMsg::Toggle);
                        let _ = ev;
                    }
                });
                Some(m)
            }
            Err(e) => {
                eprintln!("初始化全局快捷键失败: {e}");
                None
            }
        }
    };

    // 设置页保存
    {
        let cfg = cfg.clone();
        let weak = ui.as_weak();
        ui.on_save_settings(move |base, bg, fg, size| {
            let mut c = cfg.borrow_mut();
            c.base_url = base.to_string();
            c.read_bg = bg.to_string();
            c.read_fg = fg.to_string();
            c.read_font_size = size;
            let msg = match c.save() {
                Ok(_) => "已保存".to_string(),
                Err(e) => format!("保存失败: {e}"),
            };
            if let Some(app) = weak.upgrade() {
                app.set_status(msg.into());
            }
        });
    }

    // 刷新书架
    {
        let weak = ui.as_weak();
        let books = books.clone();
        let cfg = cfg.clone();
        let client = client.clone();
        let rt_handle = rt.handle().clone();
        let ui_refresh = {
            let weak = weak.clone();
            let books = books.clone();
            move || {
                let app = weak.upgrade().unwrap();
                app.set_loading(true);
                let c = cfg.borrow().clone();
                match rt_handle.block_on(api::get_bookshelf(&client, &c.base_url)) {
                    Ok(list) => {
                        app.set_books(reader_view::books_model(&list));
                        *books.borrow_mut() = list;
                        app.set_loading(false);
                        app.set_status("书架已更新".into());
                    }
                    Err(e) => {
                        app.set_loading(false);
                        app.set_status(format!("获取书架失败: {e}").into());
                    }
                }
            }
        };
        let r = ui_refresh.clone();
        ui.on_refresh(r);

        // 首次加载：主循环启动后触发一次
        let r3 = ui_refresh.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(200), r3);
    }

    // 目录：拉取章节列表
    {
        let weak = ui.as_weak();
        let books = books.clone();
        let chapters = chapters.clone();
        let cfg = cfg.clone();
        let client = client.clone();
        let rt_handle = rt.handle().clone();
        ui.on_open_toc(move |idx| {
            let app = match weak.upgrade() {
                Some(a) => a,
                None => return,
            };
            let i = idx as usize;
            let bl = books.borrow();
            let Some(book) = bl.get(i).cloned() else {
                return;
            };
            drop(bl);
            app.set_loading(true);
            let c = cfg.borrow().clone();
            match rt_handle.block_on(api::get_chapter_list(&client, &c.base_url, &book.book_url)) {
                Ok(list) => {
                    app.set_chapters(reader_view::chapters_model(&list));
                    *chapters.borrow_mut() = list;
                    app.set_loading(false);
                }
                Err(e) => {
                    app.set_loading(false);
                    app.set_status(format!("获取目录失败: {e}").into());
                }
            }
        });
    }

    // 快捷键轮询（在主循环定时器里处理）
    let ui_weak = ui.as_weak();
    let state = reader_state.clone();
    let cfg2 = cfg.clone();
    let book_list = books.clone();
    let client2 = client.clone();
    let current = current_book.clone();
    let rt_handle = rt.handle().clone();
    // 当前阅读窗口 + 原生句柄
    let active: Rc<RefCell<Option<(ReaderWindow, platform::Handle)>>> = Rc::new(RefCell::new(None));

    // 统一打开逻辑：拉正文 -> 建窗 -> 应用上次进度
    type OpenReader = Rc<dyn Fn(&AppWindow, usize, i64, String)>;
    let open_reader: OpenReader = {
        let book_list = book_list.clone();
        let state = state.clone();
        let cfg = cfg2.clone();
        let client = client2.clone();
        let rt_handle = rt_handle.clone();
        let active = active.clone();
        Rc::new(
            move |app: &AppWindow, i: usize, chapter_index: i64, title: String| {
                let bl = book_list.borrow();
                let Some(book) = bl.get(i).cloned() else {
                    return;
                };
                drop(bl);
                let c = cfg.borrow().clone();
                let Ok(text) = rt_handle.block_on(api::get_book_content(
                    &client,
                    &c.base_url,
                    &book.book_url,
                    chapter_index,
                )) else {
                    app.set_status("获取正文失败".into());
                    return;
                };
                // 恢复上次读到的页
                let start_page = match progress::recall(i) {
                    Some(rec) if rec.chapter_index == chapter_index => rec.page,
                    _ => 0,
                };
                let req = reader_view::ShowRequest {
                    book_index: i,
                    chapter_index,
                    chapter_title: title,
                    text,
                    start_page,
                };
                match reader_view::show_reader(app, &state, &c, req) {
                    Ok(r) => {
                        let _ = r.show();
                        let h = platform::handle_of(r.window());
                        *active.borrow_mut() = Some((r, h));
                    }
                    Err(e) => {
                        app.set_status(format!("打开阅读窗口失败: {e}").into());
                    }
                }
            },
        )
    };

    // 快捷键轮询 + 失焦自动关闭
    let timer = slint::Timer::default();
    {
        let open_reader = open_reader.clone();
        let state = state.clone();
        let active = active.clone();
        let current = current.clone();
        let book_list = book_list.clone();
        let ui_weak = ui_weak.clone();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(80),
            move || {
                // 1) 快捷键切换
                while let Ok(HotkeyMsg::Toggle) = rx.try_recv() {
                    let visible = state.borrow().visible;
                    if visible {
                        if let Some((r, _)) = active.borrow_mut().take() {
                            let _ = r.hide();
                        }
                        state.borrow_mut().visible = false;
                        continue;
                    }
                    let Some(app) = ui_weak.upgrade() else { return };
                    let idx = *current.borrow();
                    let bl = book_list.borrow();
                    let (i, chapter_index, title) = if idx >= 0 && (idx as usize) < bl.len() {
                        let b = &bl[idx as usize];
                        let rec = progress::recall(idx as usize);
                        match rec {
                            Some(r) => (idx as usize, r.chapter_index, r.chapter_title),
                            None => (
                                idx as usize,
                                b.dur_chapter_index,
                                b.dur_chapter_title.clone(),
                            ),
                        }
                    } else if let Some(b) = bl.first() {
                        (0usize, b.dur_chapter_index, b.dur_chapter_title.clone())
                    } else {
                        continue;
                    };
                    drop(bl);
                    let title = if title.is_empty() {
                        "继续阅读".into()
                    } else {
                        title
                    };
                    open_reader(&app, i, chapter_index, title);
                }

                // 2) 失焦即关闭
                if state.borrow().visible {
                    let focused = {
                        let b = active.borrow();
                        match b.as_ref() {
                            Some((_, h)) => platform::is_focused(*h),
                            None => true,
                        }
                    };
                    if !focused {
                        // 拖动过程中鼠标仍按下时不要关闭
                        let dragging = state.borrow().drag_origin.is_some();
                        if !dragging && !platform::left_button_down() {
                            if let Some((r, _)) = active.borrow_mut().take() {
                                r.invoke_closed();
                            }
                            state.borrow_mut().visible = false;
                        }
                    }
                }
            },
        );
    }

    // 关闭阅读窗口时，把进度同步给服务端
    {
        let books_hook = books.clone();
        let cfg_hook = cfg.clone();
        let client_hook = client.clone();
        let rt_hook = rt.handle().clone();
        reader_view::set_close_hook(Rc::new(move |book_index, chapter_index, chapter_title| {
            let bl = books_hook.borrow();
            let Some(book) = bl.get(book_index).cloned() else {
                return;
            };
            drop(bl);
            let c = cfg_hook.borrow().clone();
            let _ = rt_hook.block_on(api::save_book_progress(
                &client_hook,
                &c.base_url,
                &book,
                chapter_index,
                0,
                &chapter_title,
            ));
        }));
    }

    // 书架页“阅读”按钮：走统一打开逻辑
    {
        let open_reader = open_reader.clone();
        let weak = ui.as_weak();
        let current = current.clone();
        let books = books.clone();
        ui.on_open_book(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let i = idx as usize;
            *current.borrow_mut() = idx as i64;
            let bl = books.borrow();
            let Some(book) = bl.get(i).cloned() else {
                return;
            };
            drop(bl);
            let (chapter_index, title) = match progress::recall(i) {
                Some(rec) => (rec.chapter_index, rec.chapter_title),
                None => (book.dur_chapter_index, book.dur_chapter_title.clone()),
            };
            let title = if title.is_empty() {
                book.name.clone()
            } else {
                title
            };
            open_reader(&app, i, chapter_index, title);
        });
    }

    ui.run()?;
    Ok(())
}
