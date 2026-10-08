//! 快捷键：把 `Ctrl+Alt+R` 这种写法转成 Tauri 加速键，并统一写法。

/// 主键名与 Tauri 加速键的对应。Tauri 用 W3C 键盘事件的 `code`，
/// 字母是 `KeyX`、数字是 `DigitN` —— 和 Ctrl/Alt 组合在一起时尤其要写对。
fn code_of(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    if name.chars().count() == 1 {
        let c = name.chars().next()?;
        if c.is_ascii_alphabetic() {
            return Some(format!("Key{}", c.to_ascii_uppercase()));
        }
        if c.is_ascii_digit() {
            return Some(format!("Digit{c}"));
        }
    }
    let named = match upper.as_str() {
        "SPACE" | "空格" => "Space",
        "UP" | "ARROWUP" => "ArrowUp",
        "DOWN" | "ARROWDOWN" => "ArrowDown",
        "LEFT" | "ARROWLEFT" => "ArrowLeft",
        "RIGHT" | "ARROWRIGHT" => "ArrowRight",
        "PAGEUP" | "PGUP" => "PageUp",
        "PAGEDOWN" | "PGDN" => "PageDown",
        "HOME" => "Home",
        "END" => "End",
        "INSERT" | "INS" => "Insert",
        "ENTER" | "RETURN" => "Enter",
        "ESCAPE" | "ESC" => "Escape",
        "TAB" => "Tab",
        "F1" => "F1",
        "F2" => "F2",
        "F3" => "F3",
        "F4" => "F4",
        "F5" => "F5",
        "F6" => "F6",
        "F7" => "F7",
        "F8" => "F8",
        "F9" => "F9",
        "F10" => "F10",
        "F11" => "F11",
        "F12" => "F12",
        _ => return None,
    };
    Some(named.to_string())
}

/// 配置串 -> Tauri 加速键。认不出来返回 `None`。
pub fn to_accelerator(spec: &str) -> Option<String> {
    let mut mods: Vec<&str> = Vec::new();
    let mut code: Option<String> = None;
    for part in spec.split('+') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.push("Ctrl"),
            "alt" | "option" => mods.push("Alt"),
            "shift" => mods.push("Shift"),
            "super" | "meta" | "win" | "cmd" => mods.push("Super"),
            other => code = code_of(other),
        }
    }
    let code = code?;
    mods.push(&code);
    Some(mods.join("+"))
}

/// 规范化：修饰键名统一大小写，字母主键转大写。
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
        let named = match p.to_ascii_lowercase().as_str() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_specs() {
        assert_eq!(to_accelerator("Ctrl+Alt+R").unwrap(), "Ctrl+Alt+KeyR");
        assert_eq!(to_accelerator("ctrl+shift+f12").unwrap(), "Ctrl+Shift+F12");
        assert_eq!(to_accelerator("Super+Space").unwrap(), "Super+Space");
        assert_eq!(to_accelerator("Ctrl+Alt+PgUp").unwrap(), "Ctrl+Alt+PageUp");
    }

    #[test]
    fn rejects_garbage() {
        assert!(to_accelerator("Ctrl+Alt+").is_none());
        assert!(to_accelerator("Ctrl+Alt+NotAKey").is_none());
    }

    #[test]
    fn normalizes_case() {
        assert_eq!(normalize("ctrl+alt+k"), "Ctrl+Alt+K");
        assert_eq!(normalize("CTRL + ALT + F12"), "Ctrl+Alt+F12");
        assert!(to_accelerator(&normalize("ctrl+alt+k")).is_some());
    }
}
