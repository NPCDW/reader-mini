//! 阅读进度：记录每本书读到哪一章、哪一页，下次打开可续读。

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
pub fn remember(book_index: usize, chapter_index: i64, chapter_title: String, page: usize) {
    let key = book_index.to_string();
    with_cache(|m| {
        m.insert(
            key,
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
pub fn recall(book_index: usize) -> Option<Record> {
    with_cache(|m| m.get(&book_index.to_string()).cloned())
}

fn flush() {
    let data = with_cache(|m| serde_json::to_string_pretty(m).unwrap_or_default());
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, data);
}
