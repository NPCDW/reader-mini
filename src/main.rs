use reader_mini::app::{AppWindow, BookItem, ChapterItem};
use reader_mini::reader_win::ReaderWindow;
use reader_mini::{api, config, hotkey, net, platform, progress, reader_view};

use anyhow::Result;
use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

use slint::ComponentHandle;

/// 快捷键线程 -> 主循环的信号
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
    let client = net::client()?;

    let ui = AppWindow::new()?;
    {
        let c = cfg.borrow();
        ui.set_base_url(c.base_url.clone().into());
        ui.set_read_bg(c.read_bg.clone().into());
        ui.set_read_fg(c.read_fg.clone().into());
        ui.set_read_font_size(c.read_font_size);
        ui.set_hotkey(c.hotkey.clone().into());
        ui.set_hotkey_input(c.hotkey.clone().into());
        ui.set_line_height_text(format!("{}", c.read_line_height).into());
    }
    ui.set_books(ModelRc::new(VecModel::from(Vec::<BookItem>::new())));
    ui.set_chapters(ModelRc::new(VecModel::from(Vec::<ChapterItem>::new())));

    // 窗口/快捷键 -> 主循环通信
    let (tx, rx) = mpsc::channel::<HotkeyMsg>();
    let hotkeys = Arc::new(std::sync::Mutex::new({
        let tx = tx.clone();
        hotkey::Hotkeys::new(&cfg.borrow().hotkey, move || {
            let _ = tx.send(HotkeyMsg::Toggle);
        })
    }));

    // 当前阅读窗口（供设置页改样式时热更新）
    let style_target: Rc<RefCell<slint::Weak<ReaderWindow>>> =
        Rc::new(RefCell::new(slint::Weak::default()));

    // 设置页保存：顺手把新快捷键注册上，不用重启
    {
        let cfg = cfg.clone();
        let weak = ui.as_weak();
        let hotkeys = hotkeys.clone();
        let style_target = style_target.clone();
        let reader_state_for_style = reader_state.clone();
        ui.on_save_settings(move |base, bg, fg, size, spec, line_height| {
            let spec = hotkey::normalize(&spec);
            let mut c = cfg.borrow_mut();
            c.base_url = base.to_string();
            c.read_bg = bg.to_string();
            c.read_fg = fg.to_string();
            c.read_font_size = size;
            c.read_line_height = line_height;
            c.hotkey = spec.clone();
            let saved = c.save();
            drop(c);

            // 换键：成功才认为设置生效
            let hotkey_msg = match hotkeys.lock() {
                Ok(mut h) => match h.apply(&spec) {
                    Ok(()) => format!("快捷键已生效：{spec}"),
                    Err(e) => e,
                },
                Err(_) => "快捷键管理器不可用".to_string(),
            };
            let msg = match saved {
                Ok(_) => format!("设置已保存；{hotkey_msg}"),
                Err(e) => format!("配置落盘失败: {e}；{hotkey_msg}"),
            };
            if let Some(app) = weak.upgrade() {
                app.set_status(msg.into());
                app.set_hotkey(spec.clone().into());
                app.set_hotkey_input(spec.clone().into());
            }
            // 字号 / 颜色改了，已开着的阅读窗口立刻跟着变（并保持当前读到的地方）
            if let Some(reader) = style_target.borrow().upgrade() {
                let c = cfg.borrow().clone();
                reader_view::apply_style(&reader, &c, &reader_state_for_style);
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

    // 统一打开/刷新逻辑：拉正文 -> 复用或新建窗口 -> 定位到指定行并呈现
    type OpenReader = Rc<dyn Fn(&AppWindow, usize, i64, String, usize)>;
    let open_reader: OpenReader = {
        let book_list = book_list.clone();
        let state = state.clone();
        let cfg = cfg2.clone();
        let client = client2.clone();
        let rt_handle = rt_handle.clone();
        let active = active.clone();
        let style_target = style_target.clone();
        Rc::new(
            move |app: &AppWindow,
                  i: usize,
                  chapter_index: i64,
                  title: String,
                  start_line: usize| {
                let bl = book_list.borrow();
                let Some(book) = bl.get(i).cloned() else {
                    app.set_status("书架上找不到这本书".into());
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
                let req = reader_view::ShowRequest {
                    book_index: i,
                    chapter_index,
                    chapter_title: title,
                    text,
                    start_line,
                };
                // 已经有阅读窗口就原地刷新（正文 + 定位到 start_line），
                // 没有才新建；两条路径都走同一个 activate。
                let existing = active.borrow_mut().take();
                match existing {
                    Some((r, h)) => {
                        reader_view::activate(app, &r, &state, &c, req);
                        let _ = r.show();
                        *style_target.borrow_mut() = r.as_weak();
                        *active.borrow_mut() = Some((r, h));
                    }
                    None => match reader_view::show_reader(app, &state, &c, req) {
                        Ok(r) => {
                            let _ = r.show();
                            let h = platform::handle_of(r.window());
                            *style_target.borrow_mut() = r.as_weak();
                            *active.borrow_mut() = Some((r, h));
                        }
                        Err(e) => {
                            app.set_status(format!("打开阅读窗口失败: {e}").into());
                            return;
                        }
                    },
                }
                app.set_status(format!("正在阅读《{}》", book.name).into());
            },
        )
    };

    // 目录页点章节：打开阅读窗口，并从该章正文最开始处读起
    {
        let open_reader = open_reader.clone();
        let weak = ui.as_weak();
        let chapters = chapters.clone();
        let current = current.clone();
        ui.on_open_chapter(move |idx| {
            let Some(app) = weak.upgrade() else { return };
            let book_idx = app.get_current_book();
            if book_idx < 0 {
                app.set_status("请先从书架打开一本书的目录".into());
                return;
            }
            let cl = chapters.borrow();
            let Some(ch) = cl.get(idx as usize).cloned() else {
                return;
            };
            drop(cl);
            // 这本书成为"当前书"，之后按快捷键继续读也是它
            *current.borrow_mut() = book_idx as i64;
            let title = if ch.title.is_empty() {
                "当前章节".into()
            } else {
                ch.title.clone()
            };
            // start_line = 0：定位到正文最开始处
            open_reader(&app, book_idx as usize, idx as i64, title, 0);
        });
    }

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
                    // 与「阅读」按钮同一套续读规则：章节听服务端的，行号听本地的
                    let (i, chapter_index, title, line) = if idx >= 0 && (idx as usize) < bl.len() {
                        let b = &bl[idx as usize];
                        let r =
                            progress::resume(&b.book_url, b.dur_chapter_index, &b.dur_chapter_title);
                        (idx as usize, r.0, r.1, r.2)
                    } else if let Some(b) = bl.first() {
                        let r =
                            progress::resume(&b.book_url, b.dur_chapter_index, &b.dur_chapter_title);
                        (0usize, r.0, r.1, r.2)
                    } else {
                        continue;
                    };
                    drop(bl);
                    let title = if title.is_empty() {
                        "继续阅读".into()
                    } else {
                        title
                    };
                    open_reader(&app, i, chapter_index, title, line);
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

    // 关闭阅读窗口时，把进度同步给服务端，同时更新内存里的书架
    {
        let books_hook = books.clone();
        let cfg_hook = cfg.clone();
        let client_hook = client.clone();
        let rt_hook = rt.handle().clone();
        reader_view::set_close_hook(Rc::new(
            move |book_index, chapter_index, chapter_title, line| {
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
                // 本地只记「这一章读到第几行」，键用 bookUrl，书架重排也不会串书
                progress::remember(&book.book_url, chapter_index, chapter_title.clone(), line);
                // 刚写回服务端的就是最新进度：同步到内存书架，
                // 免得下次点「阅读」又被刷新前的旧值拽回去
                if let Some(b) = books_hook.borrow_mut().get_mut(book_index) {
                    b.dur_chapter_index = chapter_index;
                    b.dur_chapter_title = chapter_title;
                }
            },
        ));
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
            // 读服务端给的「正在阅读的章节」；本地只补上同一章里读到第几行
            let (chapter_index, title, line) = progress::resume(
                &book.book_url,
                book.dur_chapter_index,
                &book.dur_chapter_title,
            );
            let title = if title.is_empty() {
                book.name.clone()
            } else {
                title
            };
            // 带上"这本书 + 上次读到的位置"，已有窗口会被刷新到该位置
            open_reader(&app, i, chapter_index, title, line);
        });
    }

    ui.run()?;
    Ok(())
}
