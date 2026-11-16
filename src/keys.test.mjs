import assert from "node:assert/strict";
import test from "node:test";
import {
  displaySpec, hasModifier, isSingleKeySpec, keyFromEvent,
  matchSingleKey, matchSpec, normalizeSpec, specFromEvent,
} from "./keys.js";

/** 造一个只带这几个字段的假事件 —— 够 keys.js 用 */
function ev(key, code, mods = {}) {
  return {
    key,
    code,
    ctrlKey: !!mods.ctrl,
    altKey: !!mods.alt,
    shiftKey: !!mods.shift,
    metaKey: !!mods.meta,
  };
}

test("Esc 认得出：Escape / Esc 两种写法都是同一个键", () => {
  assert.equal(specFromEvent(ev("Escape", "Escape")), "Esc");
  assert.equal(matchSpec("Esc", ev("Escape", "Escape")), true);
  assert.equal(matchSpec("Escape", ev("Escape", "Escape")), true);
  assert.equal(matchSpec("Esc", ev("Esc", "Escape")), true);
});

test("+ - * / 是普通主键，不是修饰键", () => {
  // 加号键的配置串就是 `+` 本身，split 之后不能变成空串
  assert.equal(normalizeSpec("+"), "+");
  assert.equal(specFromEvent(ev("+", "Equal", { shift: true })), "+");
  assert.equal(matchSpec("+", ev("+", "Equal", { shift: true })), true);
  assert.equal(hasModifier("+"), false);

  assert.equal(matchSpec("-", ev("-", "Minus")), true);
  assert.equal(hasModifier("-"), false);

  // `*` 在有些键盘上是 Shift+Digit8：按 code 会认成 `8`，但读者按的是 `*`
  assert.equal(matchSpec("*", ev("*", "Digit8", { shift: true })), true);
  assert.equal(hasModifier("*"), false);

  assert.equal(matchSpec("/", ev("/", "Slash")), true);
  assert.equal(hasModifier("/"), false);
});

test("符号键不因为要按住 Shift 就被记成 Shift 组合", () => {
  assert.equal(specFromEvent(ev("+", "Equal", { shift: true })), "+");
  assert.equal(specFromEvent(ev("*", "Digit8", { shift: true })), "*");
  // 字母 / 数字上的 Shift 是真修饰键，照记
  assert.equal(specFromEvent(ev("P", "KeyP", { shift: true })), "Shift+P");
});

test("只有 Ctrl / Alt / Shift / Super 算修饰键", () => {
  assert.equal(hasModifier("Ctrl+P"), true);
  assert.equal(hasModifier("Alt+PgDn"), true);
  assert.equal(hasModifier("Shift+A"), true);
  assert.equal(hasModifier("Super+K"), true);
  assert.equal(hasModifier("Esc"), false);
  assert.equal(hasModifier("PgDn"), false);
});

test("修饰键写法归一：两边写法不同也能互认", () => {
  assert.equal(normalizeSpec("ctrl+p"), "Ctrl+P");
  assert.equal(normalizeSpec("win+k"), "Super+K");
  assert.equal(normalizeSpec("escape"), "Esc");
  assert.equal(matchSpec("Ctrl+P", ev("p", "KeyP", { ctrl: true })), true);
});

test("多看一个修饰键就不是同一个键", () => {
  assert.equal(matchSpec("Esc", ev("Escape", "Escape", { shift: true })), false);
  assert.equal(matchSpec("PgDn", ev("PageDown", "PageDown", { ctrl: true })), false);
});

test("只按修饰键不算一次录制", () => {
  assert.equal(specFromEvent(ev("Control", "ControlLeft", { ctrl: true })), null);
  assert.equal(specFromEvent(ev("Shift", "ShiftLeft", { shift: true })), null);
});

test("阅读单键录制支持左右控制键，不等待组合中的主键", () => {
  const modifiers = [
    ["Control", "Control", "Ctrl", { ctrl: true }],
    ["Shift", "Shift", "Shift", { shift: true }],
    ["Alt", "Alt", "Alt", { alt: true }],
    ["Meta", "Meta", "Super", { meta: true }],
    ["OS", "Meta", "Super", { meta: true }],
    ["AltGraph", "Alt", "AltGraph", { ctrl: true, alt: true }],
  ];
  for (const [key, code, expected, mods] of modifiers) {
    for (const side of ["Left", "Right"]) {
      const event = ev(key, `${code}${side}`, mods);
      assert.equal(keyFromEvent(event), expected);
      assert.equal(isSingleKeySpec(expected), true);
      assert.equal(matchSingleKey(expected, event), true);
      // 全局快捷键的录制还是要等真正的主键。
      assert.equal(specFromEvent(event), null);
    }
  }
});

test("阅读单键录制沿用普通按键名，不拼修饰前缀", () => {
  const keys = [
    ["Escape", "Escape", "Esc"],
    ["Tab", "Tab", "Tab"],
    ["Enter", "Enter", "Enter"],
    [" ", "Space", "Space"],
    ["Backspace", "Backspace", "Backspace"],
    ["Delete", "Delete", "Delete"],
    ["PageUp", "PageUp", "PgUp"],
    ["PageDown", "PageDown", "PgDn"],
    ["ArrowLeft", "ArrowLeft", "Left"],
    ["F12", "F12", "F12"],
    ["CapsLock", "CapsLock", "CapsLock"],
    ["ContextMenu", "ContextMenu", "ContextMenu"],
    ["AudioVolumeUp", "AudioVolumeUp", "AudioVolumeUp"],
    ["p", "KeyP", "P"],
    ["1", "Digit1", "1"],
    ["+", "NumpadAdd", "+"],
    ["-", "Minus", "-"],
    ["*", "NumpadMultiply", "*"],
    ["/", "Slash", "/"],
  ];
  for (const [key, code, expected] of keys) {
    assert.equal(keyFromEvent(ev(key, code)), expected);
    assert.equal(matchSingleKey(expected, ev(key, code)), true);
    assert.equal(keyFromEvent(ev(key, code, { ctrl: true, shift: true })), expected);
  }
  assert.equal(keyFromEvent(ev("+", "Equal", { shift: true })), "+");
  assert.equal(keyFromEvent(ev("*", "Digit8", { shift: true })), "*");
  assert.equal(keyFromEvent(null), null);
  assert.equal(keyFromEvent(ev("", "")), null);
});

test("单键校验允许控制键和加号，不接受旧组合键", () => {
  for (const spec of ["Ctrl", "Shift", "Alt", "Super", "AltGraph", "+", "Esc", "a"]) {
    assert.equal(isSingleKeySpec(spec), true);
  }
  for (const spec of ["", " ", null, "Ctrl+P", "Shift+A", "Alt+PgDn", "Ctrl++", "A+", "++"]) {
    assert.equal(isSingleKeySpec(spec), false);
  }
  assert.equal(normalizeSpec("Control"), "Ctrl");
  assert.equal(normalizeSpec("Meta"), "Super");
  assert.equal(normalizeSpec("OS"), "Super");
  assert.equal(normalizeSpec("altgraph"), "AltGraph");
  assert.equal(matchSingleKey("control", ev("Control", "ControlLeft", { ctrl: true })), true);
  assert.equal(matchSingleKey("Meta", ev("OS", "MetaLeft", { meta: true })), true);
});

test("阅读单键匹配不把组合键或后续主键认作控制键", () => {
  assert.equal(matchSingleKey("Ctrl", ev("p", "KeyP", { ctrl: true })), false);
  assert.equal(matchSingleKey("Ctrl", ev("+", "NumpadAdd", { ctrl: true })), false);
  assert.equal(matchSingleKey("Alt", ev("+", "NumpadAdd", { alt: true })), false);
  assert.equal(matchSingleKey("Super", ev("+", "NumpadAdd", { meta: true })), false);
  assert.equal(matchSingleKey("Shift", ev("P", "KeyP", { shift: true })), false);
  assert.equal(matchSingleKey("Ctrl", ev("Control", "ControlLeft", { ctrl: true, shift: true })), false);
  assert.equal(matchSingleKey("Alt", ev("AltGraph", "AltRight", { ctrl: true, alt: true })), false);
  assert.equal(matchSingleKey("AltGraph", ev("AltGraph", "AltRight", { ctrl: true, alt: true, shift: true })), false);
  assert.equal(matchSingleKey("PgDn", ev("PageDown", "PageDown", { alt: true })), false);
  assert.equal(matchSingleKey("Esc", ev("Escape", "Escape", { shift: true })), false);
  assert.equal(matchSingleKey("Ctrl+P", ev("p", "KeyP", { ctrl: true })), false);
  assert.equal(matchSingleKey("Shift+A", ev("A", "KeyA", { shift: true })), false);
  assert.equal(matchSingleKey("+", ev("+", "Equal", { shift: true })), true);
  assert.equal(matchSingleKey("*", ev("*", "Digit8", { shift: true })), true);
  assert.equal(matchSingleKey("+", ev("+", "NumpadAdd", { ctrl: true })), false);
  assert.equal(matchSingleKey("Ctrl", null), false);
});

test("displaySpec：写成人看得懂的样子", () => {
  assert.equal(displaySpec("Ctrl+P"), "Ctrl + P");
  assert.equal(displaySpec("Esc"), "Esc");
});
