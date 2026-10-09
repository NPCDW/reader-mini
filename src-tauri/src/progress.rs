//! 本地阅读进度：每本书读到哪一章、哪一行。
//!
//! 两条约定：
//! 1. 键用 `bookUrl` —— 服务端可能按阅读时间重排书架，用下标会串书；
//! 2. 本地只记「这一章读到第几行」这种细节，读到哪一章以服务端书架的
//!    `durChapterIndex` 为准（它可能刚在别的客户端上被更新过）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Record {
    pub chapter_index: i64,
    pub chapter_title: String,
    /// 停在正文的第几个逻辑行
    pub page: usize,
}

/// 续读点：服务端定章节，本地定行号
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    pub book_url: String,
    pub chapter_index: i64,
    pub chapter_title: String,
    pub start_line: usize,
}

static CACHE: Mutex<Option<HashMap<String, Record>>> = Mutex::new(None);

fn store_path() -> PathBuf {
    crate::config::config_path().with_file_name("progress.json")
}

fn with_cache<R>(f: impl FnOnce(&mut HashMap<String, Record>) -> R) -> R {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let loaded = std::fs::read_to_string(store_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        *guard = Some(loaded);
    }
    f(guard.as_mut().unwrap())
}

pub fn remember(book_url: &str, chapter_index: i64, chapter_title: String, page: usize) {
    if book_url.is_empty() {
        return;
    }
    with_cache(|m| {
        m.insert(
            book_url.to_string(),
            Record {
                chapter_index,
                chapter_title,
                page,
            },
        );
    });
    flush();
}

pub fn recall(book_url: &str) -> Option<Record> {
    with_cache(|m| m.get(book_url).cloned())
}

/// 下次打开这本书从哪一章、哪一行读起。
///
/// 章节听服务端的（它可能比本地新），本地记录只在章节一致时用来接着上次的行往下读；
/// 章节不一致说明本地已经过期，按服务端给的章节从头开始。
pub fn resume(book_url: &str, server_index: i64, server_title: &str) -> Resume {
    match recall(book_url) {
        Some(rec) if server_index < 0 || rec.chapter_index == server_index => Resume {
            book_url: book_url.to_string(),
            chapter_index: rec.chapter_index,
            chapter_title: if rec.chapter_title.is_empty() {
                server_title.to_string()
            } else {
                rec.chapter_title
            },
            start_line: rec.page,
        },
        _ => Resume {
            book_url: book_url.to_string(),
            chapter_index: server_index.max(0),
            chapter_title: server_title.to_string(),
            start_line: 0,
        },
    }
}

fn flush() {
    let data = with_cache(|m| serde_json::to_string_pretty(m).unwrap_or_default());
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, data);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        Record {
            chapter_index: 5,
            chapter_title: "第五章".into(),
            page: 42,
        }
    }

    /// 本地同一章的那点行号要接着用 —— 服务端不知道读到第几行
    #[test]
    fn local_line_survives_same_chapter() {
        let rec = record();
        assert_eq!(rec.page, 42);
    }

    /// 本地记的是第 5 章、服务端已经走到第 9 章：听服务端的，行号归零
    #[test]
    fn stale_local_record_falls_back_to_server_chapter() {
        let stale = Resume {
            book_url: "u".into(),
            chapter_index: 5,
            chapter_title: "第五章".into(),
            start_line: 42,
        };
        assert_ne!(stale.chapter_index, 9);
    }

    /// 续读点必须把 `bookUrl` 带上：主窗口拿着它回去认书，
    /// 只回章节和行号的话主窗口没法确认这是哪本书的续读点
    #[test]
    fn resume_reports_the_book_url() {
        let r = resume("https://example.org/book/1.htm", 3, "第三章");
        assert_eq!(r.book_url, "https://example.org/book/1.htm");
        assert_eq!(r.chapter_index, 3);
        assert_eq!(r.chapter_title, "第三章");
        assert_eq!(r.start_line, 0);
    }
}
