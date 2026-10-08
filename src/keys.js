// 按键录制：把一次 KeyboardEvent 变成 `Ctrl+Alt+R` 这种配置串。
// 与 Rust 侧 hotkey::normalize 保持同一套写法，两边能互相看懂。

const NAMED = {
  " ": "Space",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Home: "Home",
  End: "End",
  Insert: "Insert",
  Escape: "Esc",
  Tab: "Tab",
  Enter: "Enter",
};

const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta"];

export function specFromEvent(event) {
  // 只按下修饰键不算一次录制，等真正的主键
  if (MODIFIER_KEYS.includes(event.key)) return null;

  const parts = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");

  const key = NAMED[event.key] ?? (event.key.length === 1 ? event.key.toUpperCase() : event.key);
  if (!key || MODIFIER_KEYS.includes(key)) return null;
  parts.push(key);
  return parts.join("+");
}
