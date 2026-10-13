import assert from "node:assert/strict";
import test from "node:test";
import { migrate, themePatch, THEME_DEFAULTS } from "./theme.js";

test("旧配置：散着的 readXxx 变成默认主题", () => {
  const { themes, id } = migrate({
    readBg: "#123456",
    readFg: "#654321",
    readFontSize: 24,
    readLineHeight: 1.8,
    readFontFamily: "serif",
    readFontWeight: "bold",
  });
  assert.equal(themes.length, 1);
  assert.equal(themes[0].id, id, "选中项要落在主题里");
  assert.equal(themes[0].name, "默认主题");
  assert.equal(themes[0].bg, "#123456");
  assert.equal(themes[0].fontSize, 24);
});

test("新配置：主题照原样留着，选中项也跟着", () => {
  const { themes, id } = migrate({
    themes: [
      { id: "a", name: "甲", bg: "#111", fg: "#eee", fontSize: 18 },
      { id: "b", name: "乙", bg: "#fff", fg: "#000", fontSize: 22 },
    ],
    readThemeId: "b",
  });
  assert.deepEqual(
    themes.map((t) => t.name),
    ["甲", "乙"],
  );
  assert.equal(id, "b");
  // 缺的项按默认补齐，不让阅读窗口读到 undefined
  assert.equal(themes[0].lineHeight, THEME_DEFAULTS.lineHeight);
});

test("选中项指向一个已经不在的主题：退回第一个，而不是没有选中", () => {
  const { themes, id } = migrate({
    themes: [{ id: "a", name: "甲" }],
    readThemeId: "gone",
  });
  assert.equal(id, themes[0].id);
});

test("主题被删空了：留一个默认主题，别让阅读窗口没样式可读", () => {
  const { themes, id } = migrate({ themes: [], readBg: "#202020" });
  assert.equal(themes.length, 1);
  assert.equal(themes[0].bg, "#202020");
  assert.equal(id, themes[0].id);
});

test("themePatch：摊平成阅读窗口读的那几个字段", () => {
  const patch = themePatch({
    bg: "#111",
    fg: "#222",
    fontSize: 21,
    lineHeight: 1.7,
    fontFamily: "serif",
    fontWeight: "bold",
  });
  assert.deepEqual(patch, {
    readBg: "#111",
    readFg: "#222",
    readFontSize: 21,
    readLineHeight: 1.7,
    readFontFamily: "serif",
    readFontWeight: "bold",
  });
});
