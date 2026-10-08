//! 全局快捷键：解析配置里的字符串，并支持运行时换键。

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

/// 配置串的规范写法，解析与生成共用，避免两边变形。
pub const DEFAULT_SPEC: &str = "Ctrl+Alt+R";

/// 把配置串规范化成 `Ctrl+Alt+R` 这种写法：修饰键名统一大小写，主键字母转大写。
///
/// 设置页是照着用户真实按下的键拼串的，字母可能是小写（Shift 组合尤其容易），
/// 落盘前统一一次，免得同一套快捷键出现两种写法。
pub fn normalize(spec: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in spec.split('+') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let lower = p.to_ascii_lowercase();
        let named = match lower.as_str() {
            "ctrl" | "control" => Some("Ctrl"),
            "alt" | "option" => Some("Alt"),
            "shift" => Some("Shift"),
            "super" | "meta" | "win" | "cmd" => Some("Super"),
            _ => None,
        };
        out.push(match named {
            Some(n) => n.to_string(),
            None if p.chars().count() == 1 && p.chars().all(|c| c.is_ascii_alphabetic()) => {
                p.to_ascii_uppercase()
            }
            None => p.to_string(),
        });
    }
    out.join("+")
}

/// 解析 "Ctrl+Alt+R" 这类写法。
///
/// 支持 ctrl / control / alt / shift / super / meta / win 作为修饰键，
/// 字母、数字、F1~F12、空格、方向键、PgUp / PgDn / Home / End 作为主键。
pub fn parse(spec: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in spec.split('+') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "win" | "cmd" => mods |= Modifiers::SUPER,
            other => code = key_code(other),
        }
    }
    Some(HotKey::new(Some(mods), code?))
}

fn key_code(name: &str) -> Option<Code> {
    // 主键名大小写不敏感：UI 捕获时可能给出小写字母
    let upper = name.to_ascii_uppercase();
    // 单个字母
    if let Some(c) = name.chars().next()
        && name.chars().count() == 1
    {
        let letter = match c.to_ascii_lowercase() {
            'a'..='z' => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        if let Some(l) = letter {
            return LETTERS.iter().find(|(ch, _)| *ch == l).map(|(_, c)| *c);
        }
        if c.is_ascii_digit() {
            return DIGITS
                .iter()
                .find(|(ch, _)| *ch == c)
                .map(|(_, code)| *code);
        }
    }
    match upper.as_str() {
        "SPACE" | "空格" => Some(Code::Space),
        "UP" | "ARROWUP" => Some(Code::ArrowUp),
        "DOWN" | "ARROWDOWN" => Some(Code::ArrowDown),
        "LEFT" | "ARROWLEFT" => Some(Code::ArrowLeft),
        "RIGHT" | "ARROWRIGHT" => Some(Code::ArrowRight),
        "PAGEUP" | "PGUP" => Some(Code::PageUp),
        "PAGEDOWN" | "PGDN" => Some(Code::PageDown),
        "HOME" => Some(Code::Home),
        "END" => Some(Code::End),
        "INSERT" | "INS" => Some(Code::Insert),
        "ENTER" | "RETURN" => Some(Code::Enter),
        "ESCAPE" | "ESC" => Some(Code::Escape),
        "TAB" => Some(Code::Tab),
        "F1" => Some(Code::F1),
        "F2" => Some(Code::F2),
        "F3" => Some(Code::F3),
        "F4" => Some(Code::F4),
        "F5" => Some(Code::F5),
        "F6" => Some(Code::F6),
        "F7" => Some(Code::F7),
        "F8" => Some(Code::F8),
        "F9" => Some(Code::F9),
        "F10" => Some(Code::F10),
        "F11" => Some(Code::F11),
        "F12" => Some(Code::F12),
        _ => None,
    }
}

static LETTERS: [(char, Code); 26] = [
    ('a', Code::KeyA),
    ('b', Code::KeyB),
    ('c', Code::KeyC),
    ('d', Code::KeyD),
    ('e', Code::KeyE),
    ('f', Code::KeyF),
    ('g', Code::KeyG),
    ('h', Code::KeyH),
    ('i', Code::KeyI),
    ('j', Code::KeyJ),
    ('k', Code::KeyK),
    ('l', Code::KeyL),
    ('m', Code::KeyM),
    ('n', Code::KeyN),
    ('o', Code::KeyO),
    ('p', Code::KeyP),
    ('q', Code::KeyQ),
    ('r', Code::KeyR),
    ('s', Code::KeyS),
    ('t', Code::KeyT),
    ('u', Code::KeyU),
    ('v', Code::KeyV),
    ('w', Code::KeyW),
    ('x', Code::KeyX),
    ('y', Code::KeyY),
    ('z', Code::KeyZ),
];

static DIGITS: [(char, Code); 10] = [
    ('0', Code::Digit0),
    ('1', Code::Digit1),
    ('2', Code::Digit2),
    ('3', Code::Digit3),
    ('4', Code::Digit4),
    ('5', Code::Digit5),
    ('6', Code::Digit6),
    ('7', Code::Digit7),
    ('8', Code::Digit8),
    ('9', Code::Digit9),
];

/// 全局快捷键管理器：设置页改了键就重新注册，不用重启。
pub struct Hotkeys {
    manager: Option<GlobalHotKeyManager>,
    registered: Option<HotKey>,
}

impl Hotkeys {
    pub fn new(initial: &str, on_toggle: impl Fn() + Send + 'static) -> Self {
        let manager = GlobalHotKeyManager::new().ok();
        if manager.is_some() {
            // 独立线程收事件，主循环轮询状态即可，避免跨线程碰 UI
            std::thread::spawn(move || {
                while GlobalHotKeyEvent::receiver().recv().is_ok() {
                    on_toggle();
                }
            });
        }
        let mut me = Self {
            manager,
            registered: None,
        };
        match me.apply(initial) {
            Ok(()) => {}
            Err(e) => eprintln!("{e}"),
        }
        me
    }

    /// 把当前快捷键换成新的；解析失败或注册失败会保留原来的键。
    pub fn apply(&mut self, spec: &str) -> Result<(), String> {
        let Some(manager) = self.manager.as_ref() else {
            return Err("当前系统不支持全局快捷键".into());
        };
        let Some(target) = parse(spec) else {
            return Err(format!("快捷键「{spec}」无法识别"));
        };
        if self.registered == Some(target) {
            return Ok(());
        }
        if let Some(old) = self.registered
            && let Err(e) = manager.unregister(old)
        {
            eprintln!("注销旧快捷键失败: {e}");
        }
        match manager.register(target) {
            Ok(()) => {
                self.registered = Some(target);
                Ok(())
            }
            Err(e) => {
                // 注册失败就把旧键装回去，尽量别让用户失去呼出手段
                if let Some(old) = self.registered
                    && let Err(e2) = manager.register(old)
                {
                    eprintln!("恢复旧快捷键失败: {e2}");
                }
                Err(format!("注册快捷键「{spec}」失败: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_specs() {
        for spec in [
            "Ctrl+Alt+R",
            "ctrl+shift+r",
            "Super+Space",
            "Ctrl+F12",
            "Alt+Ctrl+PgUp",
        ] {
            assert!(parse(spec).is_some(), "{spec} 应当能解析");
        }
    }

    #[test]
    fn normalizes_specs() {
        assert_eq!(normalize("ctrl+alt+k"), "Ctrl+Alt+K");
        assert_eq!(normalize("CTRL + ALT + F12"), "Ctrl+Alt+F12");
        // 命名键 `space` 与 `Space` 等价，主键名保持解析器认识的写法
        assert!(parse(&normalize("Super+space")).is_some());
        // 规范化后仍然能被解析
        for spec in ["ctrl+alt+k", "shift+pgup", "super+space"] {
            assert!(parse(&normalize(spec)).is_some(), "{spec} 规范化后应可解析");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("Ctrl+Alt+").is_none());
        assert!(parse("Ctrl+Alt+NotAKey").is_none());
    }
}
