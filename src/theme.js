// 字体主题：一套阅读样式（背景 / 字体色 / 字号 / 字体 / 字重 / 字距 / 行高）。
//
// 设置页里可以存好几套，读者点一下圆圈就换一套；阅读窗口只认被选中的那一套
// （配置里的 `readThemeId`）。旧配置里没有主题这一项，那些散着的 readBg /
// readFg / readFontSize… 就是「默认主题」—— 见 `migrate`。

/** 主题 id 的前缀：时间戳加一点随机，够用且不用后端配合发号 */
export function newThemeId(now = Date.now()) {
  return `t${now.toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/** 与 Rust 侧 Config::default() 同一套默认值 */
export const THEME_DEFAULTS = {
  bg: "#181818",
  fg: "#bdbdbd",
  fontSize: 20,
  // 行高写 1.5，不是配置里的 f32 1.5 —— 同一个数，写成更好认的那个
  lineHeight: 1.5,
  fontFamily: "system-ui",
  fontWeight: "normal",
  letterSpacing: "0",
};

export const DEFAULT_THEME_NAME = "默认主题";

/** 一个主题要能在阅读窗口里排版得出来，缺项都按默认补上 */
export function normalizeTheme(raw = {}, fallbackName = DEFAULT_THEME_NAME) {
  const name = String(raw.name ?? "").trim() || fallbackName;
  return {
    id: raw.id || newThemeId(),
    name,
    bg: raw.bg || THEME_DEFAULTS.bg,
    fg: raw.fg || THEME_DEFAULTS.fg,
    fontSize: Number(raw.fontSize) > 0 ? Number(raw.fontSize) : THEME_DEFAULTS.fontSize,
    lineHeight: Number(raw.lineHeight) > 0 ? Number(raw.lineHeight) : THEME_DEFAULTS.lineHeight,
    fontFamily: String(raw.fontFamily ?? "").trim() || THEME_DEFAULTS.fontFamily,
    fontWeight: String(raw.fontWeight ?? "").trim() || THEME_DEFAULTS.fontWeight,
    letterSpacing: String(raw.letterSpacing ?? "").trim() || THEME_DEFAULTS.letterSpacing,
  };
}

/** 配置里那些散着的 readXxx：默认主题就是它们 */
export function legacyTheme(config = {}) {
  return normalizeTheme({
    id: "default",
    name: DEFAULT_THEME_NAME,
    bg: config.readBg,
    fg: config.readFg,
    fontSize: config.readFontSize,
    lineHeight: config.readLineHeight,
    fontFamily: config.readFontFamily,
    fontWeight: config.readFontWeight,
    letterSpacing: config.readLetterSpacing,
  });
}

/**
 * 把一份配置整理成「有主题」的样子，返回 `{ themes, id }`。
 *
 * 两种要整理的情形：旧配置（只有那些散着的 readXxx）—— 就地产出一个默认主题；
 * 新配置但主题被删空了 —— 至少留一个，不然阅读窗口没有样式可读。
 * `id` 一定要落在 `themes` 里：指不到人的选中项等于没有选中。
 */
export function migrate(config = {}) {
  const raw = Array.isArray(config.themes) ? config.themes : [];
  const themes = raw.map((t, i) => normalizeTheme(t, i === 0 ? DEFAULT_THEME_NAME : `主题 ${i + 1}`));
  if (!themes.length) themes.push(legacyTheme(config));
  const wanted = themes.some((t) => t.id === config.readThemeId);
  const id = wanted ? config.readThemeId : themes[0].id;
  return { themes, id };
}

/** 主题摊平成阅读窗口要的那几个字段（与 Config 里的 readXxx 同名） */
export function themePatch(theme) {
  return {
    readBg: theme.bg,
    readFg: theme.fg,
    readFontSize: theme.fontSize,
    readLineHeight: theme.lineHeight,
    readFontFamily: theme.fontFamily,
    readFontWeight: theme.fontWeight,
    readLetterSpacing: theme.letterSpacing,
  };
}
