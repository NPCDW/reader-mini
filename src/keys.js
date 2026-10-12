// 按键录制：把一次 KeyboardEvent 变成 `Alt+PgDn` 这种配置串。
// 与 Rust 侧 hotkey::normalize 保持同一套写法，两边能互相看懂。

// `event.key` 是「按下哪个键的结果」，与 KeyboardEvent.code 不同：主键盘数字
// 与数字小键盘都是 `0`..`9`，Shift+2 是 `@`。按 code 取才能真正区分按键。
const NAMED = {
  " ": "Space",
  PageUp: "PgUp",
  PageDown: "PgDn",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
};

const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta"];

/** `code` -> 配置串里的主键名；认不出来返回 null */
function nameOf(event) {
  const code = event.code || "";
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  if (NAMED[code]) return NAMED[code];
  if (NAMED[event.key]) return NAMED[event.key];
  if (event.key.length === 1) return event.key.toUpperCase();
  if (MODIFIER_KEYS.includes(event.key)) return null;
  return event.key;
}

export function specFromEvent(event) {
  // 只按下修饰键不算一次录制，等真正的主键
  if (event.key && MODIFIER_KEYS.includes(event.key)) return null;

  const key = nameOf(event);
  if (!key || MODIFIER_KEYS.includes(key)) return null;

  const parts = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  parts.push(key);
  return parts.join("+");
}

/**
 * 配置串规范化：修饰键名统一大小写、字母主键转大写。
 *
 * 设置页是照着用户真实按下的键拼串的（`specFromEvent`），也可能手改配置文件，
 * 两边写法统一之后再比，才不会出现 `ctrl+p` 与 `Ctrl+P` 对不上的情况。
 */
export function normalizeSpec(spec) {
  return String(spec ?? "")
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => {
      const named = {
        ctrl: "Ctrl",
        control: "Ctrl",
        alt: "Alt",
        option: "Alt",
        shift: "Shift",
        super: "Super",
        meta: "Super",
        win: "Super",
        cmd: "Super",
      }[p.toLowerCase()];
      if (named) return named;
      // 字母主键一律大写：Shift 组合拼出来的常是小写
      if (p.length === 1 && /[a-z]/i.test(p)) return p.toUpperCase();
      return p;
    })
    .join("+");
}

/**
 * 这次按键是不是配置串说的那一个。
 *
 * 认的是整串相等（含修饰键）：配置成 `Esc` 时 `Shift+Esc` 不算 ——
 * 阅读窗口只放行配置的那三个，多一点修饰键就不是同一个键了。
 */
export function matchSpec(spec, event) {
  const want = normalizeSpec(spec);
  if (!want) return false;
  return want === normalizeSpec(specFromEvent(event));
}

/** 显示名：把配置串写成读者看得懂的样子（`PgDn` -> `PgDn 下一页` 之类由调用方拼） */
export function displaySpec(spec) {
  const parts = normalizeSpec(spec).split("+").filter(Boolean);
  const named = { PgUp: "PgUp", PgDn: "PgDn" };
  return parts.map((p) => named[p] ?? p).join(" + ");
}
