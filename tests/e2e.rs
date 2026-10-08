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

    // 不切页：整章正文全在窗口里
    assert_eq!(reader.get_full_text().to_string(), text);
    assert_eq!(reader.get_scroll_offset(), 0.0);
    // 折行行号表 + 分页表都已建好
    assert!(state.borrow().row_offsets.len() > 1);
    let pages = state.borrow().page_tops.len();
    assert!(pages > 1, "整章应按窗口大小分成若干页, got {pages}");
    // 页脚把「第几页 / 共几页」写出来
    let info = reader.get_page_info().to_string();
    assert!(info.contains("页"), "页脚应显示页码: {info}");
    assert!(info.starts_with("第 1/"), "应从第一页开始: {info}");

    reader.invoke_closed();
    assert!(!state.borrow().visible);
}

/// 分页：相邻两屏首尾相接（不重叠、不跳行），最后一屏把本章末行顶到底部。
#[test]
fn page_tops_are_contiguous_and_cover_the_end() {
    // 一屏装得下全文 -> 只有一页
    assert_eq!(reader_mini::reader_view::page_tops(8, 10), vec![0]);

    // 一屏 10 行、共 100 行：顶部 0/10/…/90，最后一屏末行正好是第 99 行
    let tops = reader_mini::reader_view::page_tops(100, 10);
    assert_eq!(tops.len(), 10);
    for (i, w) in tops.windows(2).enumerate() {
        assert_eq!(w[1] - w[0], 10, "第 {i} 屏应整整前进 10 行，不重叠也不跳行");
    }
    assert_eq!(*tops.last().unwrap() + 10, 100, "最后一屏应把末行顶到底部");

    // 正文长度不是整数屏时，最后一屏往回收，不留一屏空白
    let tops = reader_mini::reader_view::page_tops(21, 10);
    assert_eq!(tops, vec![0, 10, 11]);
    assert_eq!(*tops.last().unwrap() + 10, 21);

    // 显示行 -> 第几屏
    let tops = reader_mini::reader_view::page_tops(21, 10);
    assert_eq!(reader_mini::reader_view::page_of_row(&tops, 0), 0);
    assert_eq!(reader_mini::reader_view::page_of_row(&tops, 9), 0);
    assert_eq!(reader_mini::reader_view::page_of_row(&tops, 10), 1);
    assert_eq!(reader_mini::reader_view::page_of_row(&tops, 20), 2);
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

/// 通过真实窗口验证：连续按 PgDn，每一屏都整整前进一屏（不重复上一屏的末行），
/// 且一屏的行都完整落在正文区里；翻到本章最后一屏时，末行正好落在屏幕底部。
#[test]
fn screen_step_moves_one_whole_screen() {
    i_slint_backend_testing::init_no_event_loop();

    // 每行都短于一行能放的字数，保证「一个逻辑行 == 一个显示行」，便于核对
    let text: String = (1..=60).map(|i| format!("第{i}行\n")).collect();
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

    let (line_height, rows, total_rows) = {
        let s = state.borrow();
        (s.line_height, s.rows_per_page, s.total_rows)
    };
    // 行高必须是 Slint 量出来的真实值（自然行高 × 倍数），不是字号 × 倍数
    assert!(
        line_height > reader.get_font_size() * reader.get_line_height_factor(),
        "行高应按字体的自然行高算, got {line_height}"
    );
    assert!(rows > 2, "测试环境应能算出多于一屏的行数, got {rows}");
    // 一屏的行必须整行放得下：底部不能露出半行
    assert!(
        (rows as f32) * line_height <= reader.get_body_height() - reader.get_body_top_pad(),
        "一屏 {rows} 行应完整落在正文区里"
    );

    // 一屏几行由窗口高度定，页数由正文总行数定
    let pages = state.borrow().page_tops.len();
    assert_eq!(
        pages,
        reader_mini::reader_view::page_tops(total_rows, rows).len()
    );
    assert!(pages > 1, "60 行应该分成好几页, got {pages}");
    assert_eq!(state.borrow().page_index, 0);

    let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
    let screen_at = |offset: f32| -> Vec<&str> {
        let top = (-offset / line_height).round().max(0.0) as usize;
        lines.iter().copied().skip(top).take(rows).collect()
    };

    // 每翻一屏，屏顶整整前进 rows 行：不会重复上一屏的末行（否则就看到半行字）
    let mut expected_top = 0usize;
    for step in 0..3 {
        let top = (-reader.get_scroll_offset() / line_height).round() as usize;
        assert_eq!(top, expected_top, "第 {step} 屏应停在第 {expected_top} 行");
        assert_eq!(
            *screen_at(reader.get_scroll_offset()).first().unwrap(),
            lines[expected_top],
            "第 {step} 屏首行"
        );
        expected_top += rows;
        reader.invoke_scroll_page(1);
    }

    // 一路翻到最后一屏
    while state.borrow().page_index + 1 < pages {
        let before = state.borrow().page_index;
        reader.invoke_scroll_page(1);
        assert_eq!(state.borrow().page_index, before + 1, "PgDn 应只前进一屏");
    }

    // 最后一屏：本章末行正好落在屏幕底部
    assert_eq!(state.borrow().page_index, pages - 1);
    let last_screen = screen_at(reader.get_scroll_offset());
    assert_eq!(*last_screen.last().unwrap(), "第60行");

    // PgUp 能回到开头
    for _ in 0..(pages - 1) {
        reader.invoke_scroll_page(-1);
    }
    assert_eq!(reader.get_scroll_offset(), 0.0);
    assert_eq!(state.borrow().card_line, 0);
    // 已经在第一页，再按 PgUp 也不会退到负偏移
    reader.invoke_scroll_page(-1);
    assert_eq!(reader.get_scroll_offset(), 0.0);

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

    let (line_height, rows) = {
        let s = state.borrow();
        (s.line_height, s.rows_per_page)
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

    assert_eq!(state.borrow().card_line, 0, "起始应在第一屏");

    send(WindowEvent::KeyPressed {
        text: slint::platform::Key::PageDown.into(),
    });

    assert!(reader.get_scroll_offset() < 0.0, "PgDn 应向下滚动");
    assert!(state.borrow().card_line > 0, "PgDn 应推进当前行");
    // 整整翻过一屏 rows 行：偏移正好落在一整行的边界上，不会把一行切成两半
    let moved = -reader.get_scroll_offset() / line_height;
    assert_eq!(
        moved.round() as usize,
        rows,
        "PgDn 应整整前进一屏 {rows} 行, 实际 {moved} 行"
    );

    // 再按一次 PgUp 回到开头
    send(WindowEvent::KeyPressed {
        text: slint::platform::Key::PageUp.into(),
    });
    assert_eq!(reader.get_scroll_offset(), 0.0);
    assert_eq!(state.borrow().card_line, 0);
}

/// 本章最后一页再按 PgDn -> 要下一章；第一页再按 PgUp -> 要上一章。
#[test]
fn paging_past_the_last_page_asks_for_the_next_chapter() {
    i_slint_backend_testing::init_no_event_loop();

    let text: String = (1..=40).map(|i| format!("第{i}行\n")).collect();
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
            chapter_index: 3,
            chapter_title: "第4章".into(),
            text: text.clone(),
            start_line: 0,
        },
    )
    .unwrap();

    let pages = state.borrow().page_tops.len();
    assert!(pages > 1, "这一章应该有多页, got {pages}");

    // 装一个只记方向的翻章钩子（真正的取章动作在 main 里）
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<i32>::new()));
    let sink = asked.clone();
    reader_mini::reader_view::set_chapter_hook(std::rc::Rc::new(move |d: i32| {
        sink.borrow_mut().push(d);
    }));

    // 一路翻到本章最后一页
    while state.borrow().page_index + 1 < pages {
        reader.invoke_scroll_page(1);
    }
    // 再按一次：本章没有下一屏了，应该去要下一章
    reader.invoke_scroll_page(1);
    assert_eq!(*asked.borrow(), vec![1], "最后一页再 PgDn 应请求下一章");
    assert_eq!(state.borrow().page_index, pages - 1, "请求翻章时不该动位置");

    // 回到第一页再按 PgUp -> 要上一章
    while state.borrow().page_index > 0 {
        reader.invoke_scroll_page(-1);
    }
    reader.invoke_scroll_page(-1);
    assert_eq!(*asked.borrow(), vec![1, -1], "第一页再 PgUp 应请求上一章");
    assert_eq!(state.borrow().page_index, 0);

    reader_mini::reader_view::clear_chapter_hook();
}

/// 回归：翻到本章最后一页再按 PgDn 时，翻章钩子是在「翻页回调还在跑」的时候
/// 就地刷新同一个窗口的（main 就是这么干的）。
/// 以前刷新时会重新接一遍回调，Slint 直接 panic（Callback Handler set while called），
/// 程序就退出了；现在回调只在新建窗口时接一次。
#[test]
fn switching_chapter_inside_the_page_callback_does_not_panic() {
    i_slint_backend_testing::init_no_event_loop();
    use slint::ComponentHandle;

    let cfg = std::rc::Rc::new(reader_mini::config::Config::default());
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));
    let app = std::rc::Rc::new(reader_mini::app::AppWindow::new().unwrap());
    let text: String = (1..=40).map(|i| format!("第{i}行\n")).collect();
    let reader = reader_mini::reader_view::show_reader(
        &app,
        &state,
        &cfg,
        reader_mini::reader_view::ShowRequest {
            book_index: 0,
            chapter_index: 3,
            chapter_title: "第4章".into(),
            text,
            start_line: 0,
        },
    )
    .unwrap();
    let pages = state.borrow().page_tops.len();

    // 和 main 里的翻章钩子一样：拿到下一章正文后，就地把这同一个窗口刷掉
    let weak = reader.as_weak();
    let st = state.clone();
    let a = app.clone();
    let c = cfg.clone();
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<i32>::new()));
    let sink = asked.clone();
    reader_mini::reader_view::set_chapter_hook(std::rc::Rc::new(move |d: i32| {
        sink.borrow_mut().push(d);
        let Some(r) = weak.upgrade() else { return };
        let next: String = (1..=30).map(|i| format!("下一章第{i}行\n")).collect();
        let chapter = st.borrow().chapter_index + d as i64;
        reader_mini::reader_view::activate(
            &a,
            &r,
            &st,
            &c,
            reader_mini::reader_view::ShowRequest {
                book_index: 0,
                chapter_index: chapter,
                chapter_title: format!("第{}章", chapter + 1),
                text: next,
                start_line: 0,
            },
        );
        // 往回翻：落在上一章的最后一页（main 里也是这么接的）
        if d < 0 {
            reader_mini::reader_view::goto_last_page(&r, &st);
        }
    }));

    while state.borrow().page_index + 1 < pages {
        reader.invoke_scroll_page(1);
    }
    // 最后一页再按 PgDn：翻章钩子会在翻页回调里刷新窗口，以前这里直接 panic
    reader.invoke_scroll_page(1);
    assert_eq!(*asked.borrow(), vec![1], "应请求下一章");
    assert_eq!(state.borrow().chapter_index, 4, "应翻到下一章");
    assert_eq!(state.borrow().page_index, 0, "新章从头开始");
    assert!(reader.get_full_text().to_string().contains("下一章第1行"));

    // 往回翻：第一页再按 PgUp -> 上一章，并停在那章的最后一页
    reader.invoke_scroll_page(-1);
    assert_eq!(*asked.borrow(), vec![1, -1]);
    assert_eq!(state.borrow().chapter_index, 3);
    assert_eq!(
        state.borrow().page_index,
        state.borrow().page_tops.len() - 1,
        "往回翻应停在上一章的最后一页"
    );

    reader_mini::reader_view::clear_chapter_hook();
}

/// 拉伸窗口：只动被拉的那条边；拉左 / 上边时窗口位置要跟着走。
#[test]
fn resize_rect_only_moves_dragged_edge() {
    let base = reader_mini::reader_view::ResizeOrigin {
        mouse: (100.0, 100.0),
        pos: (200.0, 150.0),
        size: (460.0, 560.0),
        edge_x: 1,
        edge_y: 0,
    };

    // 右边缘往右 40px：位置不动，宽度 +40
    let rect = reader_mini::reader_view::resize_rect(base, 140.0, 100.0, 260.0, 200.0);
    assert_eq!(rect, (200.0, 150.0, 500.0, 560.0));

    // 左边缘往左 20px：宽度 +20，左边跟着往左挪 20
    let left = reader_mini::reader_view::ResizeOrigin { edge_x: -1, ..base };
    let rect = reader_mini::reader_view::resize_rect(left, 80.0, 100.0, 260.0, 200.0);
    assert_eq!(rect, (180.0, 150.0, 480.0, 560.0));

    // 右上角：宽 +30、高 -30，顶边下移 30
    let corner = reader_mini::reader_view::ResizeOrigin {
        edge_x: 1,
        edge_y: -1,
        ..base
    };
    let rect = reader_mini::reader_view::resize_rect(corner, 130.0, 130.0, 260.0, 200.0);
    assert_eq!(rect, (200.0, 180.0, 490.0, 530.0));

    // 往回拖过头：不会小于下限
    let rect = reader_mini::reader_view::resize_rect(base, -900.0, -900.0, 260.0, 200.0);
    assert_eq!(rect, (200.0, 150.0, 260.0, 560.0));
}

/// 拉伸完重新折行，仍停在原来读到的那一行。
#[test]
fn resizing_keeps_reading_position() {
    i_slint_backend_testing::init_no_event_loop();

    let cfg = reader_mini::config::Config::default();
    let state = std::rc::Rc::new(std::cell::RefCell::new(
        reader_mini::reader_view::ReaderState::default(),
    ));
    let app = reader_mini::app::AppWindow::new().unwrap();
    let text: String = (1..=80)
        .map(|i| format!("第{i}行：这是一段用于验证拉伸窗口后不丢阅读位置的正文内容。\n"))
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

    // 拉宽：每一行能放更多字，折行表变短，但读到的行号不该变
    let before = state.borrow().row_offsets.len();
    let scroll_before = reader.get_scroll_offset();
    reader_mini::reader_view::finish_resize(&reader, &state, 900.0, 700.0);
    assert_eq!(state.borrow().card_line, 20, "拉伸不该丢掉阅读位置");
    assert!(
        state.borrow().row_offsets.len() < before,
        "窗口变宽后折行应该变少"
    );
    assert_ne!(reader.get_scroll_offset(), scroll_before);
    assert!(reader.get_scroll_offset() < 0.0);

    // 拉窄：折行变多，位置依旧不动
    reader_mini::reader_view::finish_resize(&reader, &state, 320.0, 400.0);
    assert_eq!(state.borrow().card_line, 20);
    assert!(state.borrow().row_offsets.len() > before);
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
    // 偏移必须落在某一整行的边界上：多一行 / 少一行都会把那一行切成两半
    let line_height = state.borrow().line_height;
    let rows_down = -reader.get_scroll_offset() / line_height;
    assert_eq!(
        rows_down,
        rows_down.round(),
        "滚动偏移应正好是整数行, got {rows_down}"
    );
    assert!(rows_down > 0.0, "应往下滚到第 12 行附近");
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
