import assert from "node:assert/strict";
import test from "node:test";
import { displaySpec, hasModifier, matchSpec, normalizeSpec, specFromEvent } from "./keys.js";

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

test("displaySpec：写成人看得懂的样子", () => {
  assert.equal(displaySpec("Ctrl+P"), "Ctrl + P");
  assert.equal(displaySpec("Esc"), "Esc");
});
