//! 阅读进度：记录每本书读到哪一章、哪一页，下次打开可续读。
//!
//! 两条约定：
//! 1. 键用 `bookUrl`，不用书架下标 —— 服务端可能按阅读时间重排书架，下标会串书；
//! 2. 本地只记"这一章读到第几行"这种**细节**，读到哪一章以服务端书架的
//!    `durChapterIndex` 为准（它可能刚在别的客户端上被更新过）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Record {
    pub chapter_index: i64,
    pub chapter_title: String,
    /// 停在正文的第几个逻辑行（旧版本这里存的是页码）
    pub page: usize,
}

static CACHE: Mutex<Option<HashMap<String, Record>>> = Mutex::new(None);

fn store_path() -> PathBuf {
    let dir = directories::ProjectDirs::from("io", "readermini", "reader-mini")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join("progress.json")
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

/// 记录进度（内存 + 落盘）
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

/// 读取某本书的进度
pub fn recall(book_url: &str) -> Option<Record> {
    with_cache(|m| m.get(book_url).cloned())
}

/// 下次打开这本书该从哪一章、哪一行读起。
///
/// `server_index` / `server_title` 来自书架接口（即 `durChapterIndex` /
/// `durChapterTitle`，"正在阅读的章节"），它决定**读哪一章**，因为服务端可能
/// 比本地更新（在别处读过、或书架刚刷新过）。
/// 本地记录只有在章节一致时才生效，用来接着上次的**行**往下读；章节不一致
/// 就说明本地已经过期，按服务端给的章节从头开始。
pub fn resume(book_url: &str, server_index: i64, server_title: &str) -> (i64, String, usize) {
    match recall(book_url) {
        // 本地记得的正是服务端这一章：接着上次的行读
        Some(rec) if server_index < 0 || rec.chapter_index == server_index => {
            let title = if rec.chapter_title.is_empty() {
                server_title.to_string()
            } else {
                rec.chapter_title
            };
            (rec.chapter_index, title, rec.page)
        }
        // 没有本地记录，或本地记录已经过期：以服务端章节为准
        _ => (server_index.max(0), server_title.to_string(), 0),
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
