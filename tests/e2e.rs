//! 端到端冒烟测试：分页行为 + 4 个接口对接 mock 服务。

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
                    text.push_str(&format!("第{}行：正文内容用于验证分页逻辑。\n", i + 1));
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
fn paginate_keeps_last_line_as_next_first() {
    let text = "第一行\n第二行\n第三行\n第四行内容比较长一些\n第五行";
    let pages = reader_mini::paginate_for_test(text, 12);
    assert!(pages.len() >= 2, "应被切成多页: {pages:?}");
    let p1_last = pages[0].lines().last().unwrap().to_string();
    let p2_first = pages[1].lines().next().unwrap().to_string();
    assert_eq!(p1_last, p2_first, "翻页需保留上一页最后一行");
}

#[test]
fn api_roundtrip_against_mock() {
    let port = spawn_mock();
    let base = format!("http://127.0.0.1:{port}");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = reqwest::Client::new();

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

#[test]
fn reader_window_creates_and_pages() {
    i_slint_backend_testing::init_no_event_loop();

    let port = spawn_mock();
    let base = format!("http://127.0.0.1:{port}");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = reqwest::Client::new();
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
            text,
            start_page: 0,
        },
    )
    .unwrap();

    // 初始在第 1 页
    assert_eq!(state.borrow().page, 0);
    assert!(reader.get_page_info().to_string().starts_with("1/"));
    let first = reader.get_page_text().to_string();

    // 翻下一页
    reader.invoke_next_page();
    assert_eq!(state.borrow().page, 1);
    let second = reader.get_page_text().to_string();
    assert_ne!(first, second);

    // 新页首行 == 旧页末行
    let old_last = first.lines().last().unwrap();
    let new_first = second.lines().next().unwrap();
    assert_eq!(old_last, new_first, "翻页应保留上一页最后一行");

    // 往回翻
    reader.invoke_prev_page();
    assert_eq!(state.borrow().page, 0);
    assert_eq!(reader.get_page_text().to_string(), first);

    // 关闭
    reader.invoke_closed();
    assert!(!state.borrow().visible);
}
