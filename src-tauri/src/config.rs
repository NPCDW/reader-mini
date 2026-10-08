//! 配置：baseUrl、阅读样式、快捷键、阅读窗口尺寸。
//!
//! 落在系统配置目录（`~/.config/reader-mini/config.json`，Windows 在 `%APPDATA%`），
//! 不进仓库、不进安装包。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub base_url: String,
    pub read_bg: String,
    pub read_fg: String,
    pub read_font_size: i32,
    /// 正文行高倍数（相对字号）
    pub read_line_height: f32,
    pub hotkey: String,
    pub reader_width: f32,
    pub reader_height: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:1122".into(),
            read_bg: "#f5f0e1".into(),
            read_fg: "#333333".into(),
            read_font_size: 20,
            read_line_height: 1.5,
            hotkey: "Ctrl+Alt+R".into(),
            reader_width: 460.0,
            reader_height: 560.0,
        }
    }
}

fn dir() -> PathBuf {
    directories::ProjectDirs::from("io", "readermini", "reader-mini")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
