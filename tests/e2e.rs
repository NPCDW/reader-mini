//! 端到端冒烟测试：懒加载行装填 + 4 个接口对接 mock 服务。

use std::io::{Read, Write};
use std::net::TcpListener;

fn spawn_mock() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let body = if path.starts_with("/getBookshelf") {
                serde_json::json!({"data":[{
                    "author":"刘慈欣","bookUrl":"https://example.org/book/39020.htm",
                    "durChapterIndex":1,"durChapterPos":0,"durChapterTitle":"第2章",
                    "intro":"三体简介","latestChapterTitle":"第35章","name":"三体",
                    "totalChapterNum":35,"wordCount":"133万字","kind":"科幻"
                }],"errorMsg":"","isSuccess":true})
                .to_string()
            } else if path.starts_with("/getChapterList") {
                serde_json::json!({"data":[
                    {"title":"第1章","url":"https://example.org/txt/1"},
                    {"title":"第2章","url":"https://example.org/txt/2"}
                ],"errorMsg":"","isSuccess":true})
                .to_string()
            } else if path.starts_with("/getBookContent") {
                let mut text = String::new();
                for i in 0..50 {
                    text.push_str(&format!("第{}行：正文内容用于验证滚动翻屏逻辑。\n", i + 1));
                }
                serde_json::json!({"data": text, "errorMsg":"","isSuccess":true}).to_string()
            } else if path.starts_with("/saveBookProgress") {
                serde_json::json!({"data":"","errorMsg":"","isSuccess":true}).to_string()
            } else {
                "{}".to_string()
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
        }
    });
    port
}

#[test]
fn query_params_are_percent_encoded() {
    let q = reader_mini::api::encode_query(&[("url", "https://e.org/a b?c=1"), ("index", "3")]);
    assert_eq!(q, "url=https%3A%2F%2Fe.org%2Fa%20b%3Fc%3D1&index=3");
}

#[test]
fn api_roundtrip_against_mock() {
    let port = spawn_mock();
    let base = format!("http://127.0.0.1:{port}");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = reader_mini::net::client().unwrap();

    rt.block_on(async {
        let books = reader_mini::api::get_bookshelf(&client, &base)
            .await
            .unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].name, "三体");

        let chapters = reader_mini::api::get_chapter_list(&client, &base, &books[0].book_url)
            .await
            .unwrap();
        assert_eq!(chapters.len(), 2);

        let text = reader_mini::api::get_book_content(&client, &base, &books[0].book_url, 1)
            .await
            .unwrap();
        assert!(text.contains("第1行"));

        reader_mini::api::save_book_progress(&client, &base, &books[0], 2, 0, "第3章")
            .await
            .unwrap();
    });
}

/// 全文一次性展示：窗口里只有一段文本，滚动偏移初始为 0。
#[test]
fn reader_shows_whole_text_at_once() {
    i_slint_backend_testing::init_no_event_loop();

    let port = spawn_mock();
    let base = format!("http://127.0.0.1:{port}");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = reader_mini::net::client().unwrap();
    let text = rt
        .block_on(reader_mini::api::get_book_content(&client, &base, "x", 1))
        .unwrap();

    let cfg = reader_mini::config::Config::default();
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));

    let app = reader_mini::app::AppWindow::new().unwrap();
    let reader = reader_mini::reader_view::show_reader(
        &app,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 0,
            chapter_index: 1,
            chapter_title: "第2章".into(),
            text: text.clone(),
            start_line: 0,
        },
    )
    .unwrap();

    // 不分页：整章正文全在窗口里
    assert_eq!(reader.get_full_text().to_string(), text);
    assert_eq!(reader.get_scroll_offset(), 0.0);
    // 折行行号表已建好
    assert!(state.borrow().row_offsets.len() > 1);

    reader.invoke_closed();
    assert!(!state.borrow().visible);
}

/// 翻一屏：新一屏第一行 == 上一屏最后一行。
#[test]
fn page_move_keeps_last_line_as_next_first() {
    // 20 个逻辑行，每行正好占 1 个显示行
    let text: String = (0..20).map(|i| format!("行{i}\n")).collect();
    let rows = reader_mini::reader_view::build_row_offsets(&text, 100);
    let cards = reader_mini::reader_view::card_line_offsets(&text);
    assert_eq!(cards.len(), 21, "20 行 + 结尾哨兵");

    // 一屏 10 行 -> 可视为 0..=9，新一屏从第 9 行开始
    let next = reader_mini::reader_view::plan_page_move(0, &cards, &rows, 10, 1);
    assert_eq!(next, 9, "新屏首行应为旧屏末行（下标 9）");
    let new_top = text[cards[next]..].lines().next().unwrap();
    let old_bottom = text[cards[9]..].lines().next().unwrap();
    assert_eq!(new_top, old_bottom);

    let next2 = reader_mini::reader_view::plan_page_move(next, &cards, &rows, 10, 1);
    assert_eq!(next2, 18);

    // 回翻回到 9
    let prev = reader_mini::reader_view::plan_page_move(next2, &cards, &rows, 10, -1);
    assert_eq!(prev, 9);

    // 到底后停在能看到最后一行的位置，不越界
    let last = reader_mini::reader_view::plan_page_move(19, &cards, &rows, 10, 1);
    assert_eq!(last, 19);
}

/// 长行会先折行再算行号，翻屏时依旧保留末行。
#[test]
fn long_lines_are_wrapped_before_paging() {
    // 每行放 4 个字，一个中文字占 3 字节
    let text = "字".repeat(10);
    let rows = reader_mini::reader_view::build_row_offsets(&text, 4);
    // 3 个显示行 + 结尾哨兵
    assert_eq!(rows.len(), 4, "offset 表 = 行首偏移 + 末尾哨兵");
    assert_eq!(rows[0], 0);
    assert_eq!(rows[1], 12);
    assert_eq!(rows[2], 24);
    assert_eq!(*rows.last().unwrap(), text.len());

    // 换行符是硬断行，一个 `\n` 起一行（哪怕没到列宽）
    let mixed = "ab\ncd";
    let rows = reader_mini::reader_view::build_row_offsets(mixed, 100);
    assert_eq!(rows, vec![0, 3, 5]);
}

/// 通过真实窗口验证：连续按 PgDn，每一屏的首行都是上一屏的末行。
#[test]
fn screen_step_keeps_last_row_at_top() {
    i_slint_backend_testing::init_no_event_loop();

    let text: String = (1..=60)
        .map(|i| format!("第{i}行：这是一段用于验证不分页滚动阅读的正文内容。\n"))
        .collect();
    let cfg = reader_mini::config::Config::default();
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));
    let app = reader_mini::app::AppWindow::new().unwrap();
    let reader = reader_mini::reader_view::show_reader(
        &app,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 0,
            chapter_index: 0,
            chapter_title: "第1章".into(),
            text: text.clone(),
            start_line: 0,
        },
    )
    .unwrap();

    // 整章一次性铺满，没有分页痕迹
    assert_eq!(reader.get_full_text().to_string(), text);
    assert_eq!(reader.get_scroll_offset(), 0.0);

    let line_height = reader.get_font_size() * reader.get_line_height_factor();
    let rows = reader_mini::reader_view::rows_per_screen(
        reader.get_body_height(),
        reader.get_font_size(),
        reader.get_line_height_factor(),
    );
    assert!(rows > 2, "测试环境应能算出多于一屏的行数, got {rows}");

    let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
    let screen_at = |offset: f32| -> Vec<&str> {
        let top = (-offset / line_height).round().max(0.0) as usize;
        lines.iter().copied().skip(top).take(rows).collect()
    };

    let mut previous_bottom: Option<String> = None;
    for step in 0..3 {
        let screen = screen_at(reader.get_scroll_offset());
        let top = *screen.first().unwrap();
        let bottom = *screen.last().unwrap();
        if let Some(prev) = &previous_bottom {
            assert_eq!(top, prev.as_str(), "第 {step} 屏首行应为上一屏末行");
        }
        previous_bottom = Some(bottom.to_string());
        reader.invoke_scroll_page(1);
    }

    // PgUp 能回到开头
    reader.invoke_scroll_page(-1);
    reader.invoke_scroll_page(-1);
    reader.invoke_scroll_page(-1);
    reader.invoke_scroll_page(-1);
    assert_eq!(reader.get_scroll_offset(), 0.0);
    assert_eq!(state.borrow().card_line, 0);

    reader.invoke_closed();
    assert!(!state.borrow().visible);
}

/// PgUp / PgDn 通过窗口的按键事件分发进入，行为与鼠标点击一致。
#[test]
fn page_down_key_scrolls_one_screen() {
    i_slint_backend_testing::init_no_event_loop();

    use slint::ComponentHandle;
    use slint::platform::WindowEvent;

    let text: String = (1..=60)
        .map(|i| format!("第{i}行：这是一段用于验证不分页滚动阅读的正文内容。\n"))
        .collect();
    let cfg = reader_mini::config::Config::default();
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));
    let app = reader_mini::app::AppWindow::new().unwrap();
    let reader = reader_mini::reader_view::show_reader(
        &app,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 0,
            chapter_index: 0,
            chapter_title: "第1章".into(),
            text: text.clone(),
            start_line: 0,
        },
    )
    .unwrap();

    let line_height = reader.get_font_size() * reader.get_line_height_factor();
    let rows = reader_mini::reader_view::rows_per_screen(
        reader.get_body_height(),
        reader.get_font_size(),
        reader.get_line_height_factor(),
    );
    let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
    let screen_at = |offset: f32| -> Vec<&str> {
        let top = (-offset / line_height).round().max(0.0) as usize;
        lines.iter().copied().skip(top).take(rows).collect()
    };

    let send = |event: WindowEvent| {
        reader.window().dispatch_event(event);
    };

    // 先点一下窗口，让 FocusScope 拿到键盘焦点（真实使用里点窗口就这个效果）
    send(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(10.0, 40.0),
        button: slint::platform::PointerEventButton::Left,
    });
    send(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(10.0, 40.0),
        button: slint::platform::PointerEventButton::Left,
    });

    // PgDn：新一屏首行 == 上一屏末行
    let first_screen = screen_at(0.0);
    let first_bottom = first_screen.last().unwrap().to_string();
    assert_eq!(state.borrow().card_line, 0, "起始应在第一屏");

    send(WindowEvent::KeyPressed {
        text: slint::platform::Key::PageDown.into(),
    });

    assert!(reader.get_scroll_offset() < 0.0, "PgDn 应向下滚动");
    assert!(state.borrow().card_line > 0, "PgDn 应推进当前行");
    let new_top = *screen_at(reader.get_scroll_offset()).first().unwrap();
    assert_eq!(
        new_top,
        first_bottom.as_str(),
        "PgDn 后新一屏首行应为上一屏末行"
    );

    // 再按一次 PgUp 回到开头
    send(WindowEvent::KeyPressed {
        text: slint::platform::Key::PageUp.into(),
    });
    assert_eq!(reader.get_scroll_offset(), 0.0);
    assert_eq!(state.borrow().card_line, 0);
}

/// 「阅读」按钮：同一个内容可以按给定位置反复刷新，窗口被重新定位。
#[test]
fn activate_republishes_content_at_requested_line() {
    i_slint_backend_testing::init_no_event_loop();

    let cfg = reader_mini::config::Config::default();
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));
    let app = reader_mini::app::AppWindow::new().unwrap();

    let text: String = (1..=60)
        .map(|i| format!("第{i}行：这是一段用于验证刷新阅读页的正文内容。\n"))
        .collect();
    let reader = reader_mini::reader_view::show_reader(
        &app,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 0,
            chapter_index: 0,
            chapter_title: "第1章".into(),
            text: text.clone(),
            start_line: 0,
        },
    )
    .unwrap();
    assert_eq!(state.borrow().book_index, 0);
    assert_eq!(state.borrow().card_line, 0);
    assert_eq!(reader.get_scroll_offset(), 0.0);

    // 换成另一本书的第 5 章，并指定从第 12 行开始读
    let other: String = (1..=40)
        .map(|i| format!("另一本书的第{i}行正文。\n"))
        .collect();
    reader_mini::reader_view::activate(
        &app,
        &reader,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 3,
            chapter_index: 5,
            chapter_title: "第5章".into(),
            text: other.clone(),
            start_line: 12,
        },
    );

    // 窗口内容被整体替换
    assert_eq!(reader.get_full_text().to_string(), other);
    assert_eq!(reader.get_title_text().to_string(), "第5章");
    // 状态同步到新的书 / 章 / 行
    assert_eq!(state.borrow().book_index, 3);
    assert_eq!(state.borrow().chapter_index, 5);
    assert_eq!(state.borrow().card_line, 12);
    // 滚动位置跟着新位置走，而不是停在开头
    assert!(reader.get_scroll_offset() < 0.0, "应滚动到指定行");
    let line_height = reader.get_font_size() * reader.get_line_height_factor();
    let expected = -((reader_mini::reader_view::row_of(&state.borrow(), 12)) as f32 * line_height);
    assert_eq!(reader.get_scroll_offset(), expected);
}

/// 改字号后仍停在原来读到的地方，不会被甩回开头。
#[test]
fn apply_style_keeps_reading_position() {
    i_slint_backend_testing::init_no_event_loop();

    let cfg = reader_mini::config::Config::default();
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));
    let app = reader_mini::app::AppWindow::new().unwrap();
    let text: String = (1..=60)
        .map(|i| format!("第{i}行：改字号不要跳回开头。\n"))
        .collect();
    let reader = reader_mini::reader_view::show_reader(
        &app,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 0,
            chapter_index: 0,
            chapter_title: "第1章".into(),
            text: text.clone(),
            start_line: 20,
        },
    )
    .unwrap();
    assert_eq!(state.borrow().card_line, 20);
    let before = reader.get_scroll_offset();

    let mut bigger = cfg.clone();
    bigger.read_font_size = 28;
    reader_mini::reader_view::apply_style(&reader, &bigger, &state);

    assert_eq!(reader.get_font_size(), 28.0);
    assert_eq!(state.borrow().card_line, 20, "改字号不应丢掉阅读位置");
    assert_ne!(reader.get_scroll_offset(), before, "行高变了，偏移要重算");
}

/// 快捷键输入框不接收文本，直接听键盘事件拼出配置串。
#[test]
fn hotkey_input_comes_from_key_events() {
    i_slint_backend_testing::init_no_event_loop();

    use slint::ComponentHandle;
    use slint::platform::{Key, WindowEvent};

    let app = reader_mini::app::AppWindow::new().unwrap();
    // 设置页
    app.set_tab(1);
    let window = app.window();
    // 布局一次，让捕获框拿到位置
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1.0, 1.0),
    });

    let send = |event: WindowEvent| window.dispatch_event(event);

    // 点捕获框开始录制
    let mut clicked = false;
    for y in [1.0f32, 300.0, 340.0, 380.0, 420.0, 500.0, 520.0] {
        if clicked {
            break;
        }
        send(WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(60.0, y),
            button: slint::platform::PointerEventButton::Left,
        });
        send(WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(60.0, y),
            button: slint::platform::PointerEventButton::Left,
        });
        clicked = app.get_capturing();
    }
    assert!(clicked, "点击快捷键框应进入录制状态");
    assert_eq!(app.get_hotkey_input().to_string(), "");

    // 按下 Ctrl+Alt+K：分三次按下，输入框不会把字符当文本收进去
    send(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    send(WindowEvent::KeyPressed {
        text: Key::Alt.into(),
    });
    send(WindowEvent::KeyPressed {
        text: Key::K.into(),
    });
    let spec = app.get_hotkey_input().to_string();
    assert_eq!(
        spec.to_ascii_uppercase(),
        "CTRL+ALT+K",
        "按键应被拼成配置串"
    );
    // 拼出来的串必须是快捷键解析器认识的写法
    assert!(
        reader_mini::hotkey::parse(&spec).is_some(),
        "{spec} 应当能被解析成全局快捷键"
    );

    // 单独按修饰键不算一次完整输入，不会把已有键清掉
    assert!(!app.get_capturing());
}
