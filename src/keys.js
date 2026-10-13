// 按键录制：把一次 KeyboardEvent 变成 `Alt+PgDn` 这种配置串。
// 与 Rust 侧 hotkey::normalize 保持同一套写法，两边能互相看懂。

// 只有这几个才算修饰键：Ctrl / Alt / Shift / Win（Mac 的 Command 也是 Super）。
// 别的键 —— 哪怕是 `+` `-` `*` `/` 这种符号 —— 都是普通主键，按下去就该记成
// 它本身，不能被当成修饰键拼进前缀里。
const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta", "OS", "AltGraph"];

/** 主键名的统一写法：两处命名对齐，配置串才能互认 */
const NAMED = {
  " ": "Space",
  PageUp: "PgUp",
  PageDown: "PgDn",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Escape: "Esc",
  Esc: "Esc",
};

/** 字母 / 数字键：只认这些，其余长度 1 的都可当符号主键 */
const isAlpha = (s) => /^[a-zA-Z]$/.test(s);
const isDigit = (s) => /^[0-9]$/.test(s);

/**
 * `event` -> 主键名（不含修饰键）；认不出来返回 null。
 *
 * 字母与数字走 `code`（`KeyA` / `Digit1`），这样主键盘与小键盘、Shift+2 打出来的
 * `@` 都能认出到底是哪个物理键。其余单字符 —— `+` `-` `*` `/` `,` 这些 —— 直接
 * 用 `event.key`：符号才是读者想按的那个键，按 `code` 反推会把 `*` 认成 `8`。
 */
function nameOf(event) {
  const code = event.code || "";
  const key = event.key ?? "";

  if (NAMED[code]) return NAMED[code];
  if (NAMED[key]) return NAMED[key];
  // 单字符符号（`+` `-` `*` `/` …）先认：`*` 在有些键盘上是 Shift+Digit8，
  // 按 `code` 会认成 `8`，可是读者按的是那颗 `*`
  if (key.length === 1 && !isAlpha(key) && !isDigit(key)) return key;
  if (code.startsWith("Key") && isAlpha(code.slice(3))) return code.slice(3).toUpperCase();
  if (code.startsWith("Digit") && isDigit(code.slice(5))) return code.slice(5);
  // 字母 / 数字但 `code` 没给出来（少数环境）：退回 `key`
  if (key.length === 1) return key.toUpperCase();
  if (MODIFIER_KEYS.includes(key)) return null;
  // F1..F12、Home、End 之类：`key` 本身就是名字
  if (/^F\d{1,2}$/.test(key)) return key;
  if (key.length > 1) return key;
  return null;
}

export function specFromEvent(event) {
  // 只按下修饰键不算一次录制，等真正的主键
  if (!event || MODIFIER_KEYS.includes(event.key)) return null;

  const key = nameOf(event);
  if (!key) return null;

  const parts = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  // 符号键（`+` `*` 这些）在某些键盘上要按住 Shift 才打得出来，那不是「用了 Shift
  // 这个修饰键」，就不该写成 `Shift+…` —— 否则设置页会把它当成带修饰键的组合拒掉
  const isSymbol = key.length === 1 && !isAlpha(key) && !isDigit(key);
  if (event.shiftKey && !isSymbol) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  parts.push(key);
  return parts.join("+");
}

/**
 * 把配置串切成一项一项。数组里最后一项是主键，前面是修饰键。
 *
 * `+` 本身就是主键，跟分隔符撞个满怀：整串只有它一个 `+` 时，那就是「加号键」，
 * 别把它拆没了（`normalizeSpec("+")` 从前会变成空串，加号键就永远对不上）。
 */
function splitSpec(spec) {
  const raw = String(spec ?? "").trim();
  if (!raw) return [];
  if (raw === "+") return ["+"]; // 加号键：整串就是它
  return raw
    .split("+")
    .map((p) => p.trim())
    .filter(Boolean);
}

/** 修饰键写法归一：`control` / `ctrl` -> `Ctrl`，`win` / `meta` -> `Super` */
function normalizeModifier(part) {
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
    command: "Super",
  }[part.toLowerCase()];
  return named ?? null;
}

/** 主键写法归一：`esc` -> `Esc`，`escape` -> `Esc`，字母转大写 */
function normalizeKey(part) {
  const named = NAMED[part] ?? NAMED[part[0]?.toUpperCase() + part.slice(1)];
  if (named) return named;
  if (part.length === 1 && isAlpha(part)) return part.toUpperCase();
  return part;
}

/**
 * 配置串规范化：修饰键名统一大小写、主键名统一写法、字母主键转大写。
 *
 * 设置页是照着用户真实按下的键拼串的（`specFromEvent`），也可能手改配置文件，
 * 两边写法统一之后再比，才不会出现 `ctrl+p` 与 `Ctrl+P`、`Esc` 与 `Escape`
 * 对不上的情况。
 */
export function normalizeSpec(spec) {
  const parts = splitSpec(spec);
  if (!parts.length) return "";
  return parts.map((p) => normalizeModifier(p) ?? normalizeKey(p)).join("+");
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
  const got = normalizeSpec(specFromEvent(event));
  return want === got;
}

/** 显示名：把配置串写成读者看得懂的样子（`PgDn` 之类由调用方拼） */
export function displaySpec(spec) {
  const named = { PgUp: "PgUp", PgDn: "PgDn" };
  return splitSpec(normalizeSpec(spec))
    .map((p) => named[p] ?? p)
    .join(" + ");
}

/** 修饰键名的集合（归一化之后）：`Ctrl` `Alt` `Shift` `Super` */
const MODIFIER_NAMES = new Set(["Ctrl", "Alt", "Shift", "Super"]);

/**
 * 配置串里有没有修饰键。
 *
 * 只有在 `+` 前面那些项里认出 `Ctrl` / `Alt` / `Shift` / `Super` 才算 ——
 * `+` `-` `*` `/` 这些是普通主键，不能因为它们里有 `+` 或长得像符号就当修饰键，
 * 否则读者把加号设成快捷键时会被一句「不要带修饰键」拦下来。
 */
export function hasModifier(spec) {
  return splitSpec(spec).some((p) => MODIFIER_NAMES.has(normalizeModifier(p)));
}
