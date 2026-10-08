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
