//! 配置：baseUrl、阅读样式（含字体 / 字重 / 字距）、快捷键、阅读窗口尺寸、失焦是否自动收起。
//!
//! 落在系统配置目录（`~/.config/reader-mini/config.json`，Windows 在 `%APPDATA%`），
//! 不进仓库、不进安装包。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 一套字体主题：名字 + 阅读窗口要的那几样样式。
///
/// 设置页里可以存好几套，「一个圆圈 + 圆圈底下的名字」就是它在界面上的样子
/// （圆圈的背景是这个 `bg`，圈里那个「字」用的是 `fg` 与这套字体）。
/// 读者选中哪一套，阅读窗口就照哪一套排 —— 见 `Config::active_theme`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Theme {
    pub id: String,
    pub name: String,
    /// 阅读页面的底色
    pub bg: String,
    /// 正文字色
    pub fg: String,
    pub font_size: i32,
    /// CSS `font-family` 的值，如 `system-ui, sans-serif`
    pub font_family: String,
    /// CSS `font-weight`：100..900 或 normal / bold
    pub font_weight: String,
    /// CSS `letter-spacing` 的值，如 `0` / `1px` / `0.05em`
    pub letter_spacing: String,
    /// 行高倍数（相对字号）
    pub line_height: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "默认主题".into(),
            bg: "#181818".into(),
            fg: "#bdbdbd".into(),
            font_size: 20,
            font_family: "system-ui".into(),
            font_weight: "normal".into(),
            letter_spacing: "0".into(),
            line_height: 1.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub base_url: String,
    /// 字体主题：可以有好几套，`read_theme_id` 指的那一套是「正在用的」
    pub themes: Vec<Theme>,
    /// 正在用的主题的 id；指不到人时（主题被删了）退回第一套
    pub read_theme_id: String,
    /// 旧配置里那几项散着的样式。留着是为了读得进老配置（`normalize` 会把它们
    /// 包成「默认主题」），往后读样式一律走 `active_theme`，别再直接读这几个
    pub read_bg: String,
    pub read_fg: String,
    pub read_font_size: i32,
    /// 正文字体族（CSS `font-family` 的值，如 `system-ui, sans-serif`）
    pub read_font_family: String,
    /// 正文字重（CSS `font-weight`：100..900 或 normal / bold）
    pub read_font_weight: String,
    /// 正文文字间距（CSS `letter-spacing` 的值，如 `0` / `1px` / `0.05em`）
    pub read_letter_spacing: String,
    /// 正文行高倍数（相对字号）
    pub read_line_height: f32,
    /// 阅读窗口内的三个快捷键：隐藏窗口 / 下一页 / 上一页。
    /// 只写主键与修饰键的配置串（如 `Esc`、`PgDn`），阅读窗口照它认键；
    /// 除这三个之外的一概不理（含鼠标滚轮与右键）
    pub key_hide: String,
    pub key_next: String,
    pub key_prev: String,
    pub hotkey: String,
    pub reader_width: f32,
    pub reader_height: f32,
    /// 阅读窗口失去焦点时自动收起。默认关：点一下别处窗口就没了，容易误伤读者
    pub close_on_blur: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:1122".into(),
            themes: Vec::new(),
            read_theme_id: String::new(),
            read_bg: "#181818".into(),
            read_fg: "#bdbdbd".into(),
            read_font_size: 20,
            read_font_family: "system-ui".into(),
            read_font_weight: "normal".into(),
            read_letter_spacing: "0".into(),
            read_line_height: 1.5,
            key_hide: "Esc".into(),
            key_next: "PgDn".into(),
            key_prev: "PgUp".into(),
            hotkey: "Alt+PgDn".into(),
            reader_width: 460.0,
            reader_height: 560.0,
            close_on_blur: false,
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
        let mut cfg: Self = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        cfg.normalize();
        cfg
    }

    /// 把主题这一块整理成能用的样子。
    ///
    /// 两种情形要救：老配置里没有 `themes`（那几项散着的样式就是「默认主题」），
    /// 以及主题被删空 / `readThemeId` 指不到人 —— 阅读窗口总得有一套样式可读。
    /// 前端 `theme.js` 的 `migrate` 是同一套规则，两边能互相看懂。
    pub fn normalize(&mut self) {
        if self.themes.is_empty() {
            self.themes.push(Theme {
                id: "default".into(),
                bg: self.read_bg.clone(),
                fg: self.read_fg.clone(),
                font_size: self.read_font_size,
                font_family: self.read_font_family.clone(),
                font_weight: self.read_font_weight.clone(),
                letter_spacing: self.read_letter_spacing.clone(),
                line_height: self.read_line_height,
                ..Theme::default()
            });
        }
        for (i, t) in self.themes.iter_mut().enumerate() {
            if t.id.is_empty() {
                t.id = format!("t{i}");
            }
            if t.name.trim().is_empty() {
                t.name = if i == 0 {
                    "默认主题".into()
                } else {
                    format!("主题 {}", i + 1)
                };
            }
            if t.font_size <= 0 {
                t.font_size = Theme::default().font_size;
            }
            if t.line_height <= 0.0 {
                t.line_height = Theme::default().line_height;
            }
            if t.bg.is_empty() {
                t.bg = Theme::default().bg;
            }
            if t.fg.is_empty() {
                t.fg = Theme::default().fg;
            }
            if t.font_family.trim().is_empty() {
                t.font_family = Theme::default().font_family;
            }
            if t.font_weight.trim().is_empty() {
                t.font_weight = Theme::default().font_weight;
            }
            if t.letter_spacing.trim().is_empty() {
                t.letter_spacing = Theme::default().letter_spacing;
            }
        }
        if !self.themes.iter().any(|t| t.id == self.read_theme_id) {
            self.read_theme_id = self.themes[0].id.clone();
        }
    }

    /// 正在用的那一套主题。
    ///
    /// 阅读窗口读样式的唯一入口：`themes` 空着（比如刚 `Default::default()`）
    /// 就现给一套默认的，绝不返回「没有样式」。
    pub fn active_theme(&self) -> Theme {
        self.themes
            .iter()
            .find(|t| t.id == self.read_theme_id)
            .or_else(|| self.themes.first())
            .cloned()
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
