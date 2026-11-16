import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createRenderer, nextTick } from "vue";
import { compileScript, parse } from "vue/compiler-sfc";

// 用 Vue 自己的编译器和轻量渲染器跑真实组件，不另加 DOM 测试依赖。
const source = await readFile(new URL("./components/SettingsPage.vue", import.meta.url), "utf8");
const { descriptor } = parse(source);
const compiled = compileScript(descriptor, { id: "settings-keys-test", inlineTemplate: true });
const code = compiled.content
  .replaceAll('from "vue"', `from ${JSON.stringify(import.meta.resolve("vue"))}`)
  .replace('from "../keys"', `from ${JSON.stringify(new URL("./keys.js", import.meta.url).href)}`)
  .replace('from "../theme"', `from ${JSON.stringify(new URL("./theme.js", import.meta.url).href)}`);
const { default: SettingsPage } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);

function node(type, text = "") {
  const el = Object.assign(new EventTarget(), {
    type, text, tagName: type.toUpperCase(), children: [], props: {}, parent: null,
  });
  Object.defineProperty(el, "options", {
    get: () => el.children.filter((child) => child.type === "option"),
  });
  return el;
}

const renderer = createRenderer({
  createElement: (type) => node(type),
  createText: (text) => node("text", text),
  createComment: (text) => node("comment", text),
  setText: (el, text) => { el.text = text; },
  setElementText: (el, text) => { el.text = text; el.children = []; },
  patchProp: (el, key, previous, value) => { el.props[key] = value; },
  parentNode: (el) => el.parent,
  nextSibling: (el) => el.parent?.children[el.parent.children.indexOf(el) + 1] ?? null,
  insert(el, parent, anchor = null) {
    if (el.parent) el.parent.children.splice(el.parent.children.indexOf(el), 1);
    const at = anchor ? parent.children.indexOf(anchor) : parent.children.length;
    parent.children.splice(at, 0, el);
    el.parent = parent;
  },
  remove(el) {
    el.parent.children.splice(el.parent.children.indexOf(el), 1);
    el.parent = null;
  },
});

function find(root, predicate) {
  if (predicate(root)) return root;
  for (const child of root.children) {
    const found = find(child, predicate);
    if (found) return found;
  }
  return null;
}

function textOf(el) {
  return el.text + el.children.filter((child) => child.type !== "comment").map(textOf).join("");
}

function keyEvent(key, code = key, extra = {}) {
  return {
    key, code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false,
    defaultPrevented: false,
    preventDefault() { this.defaultPrevented = true; },
    ...extra,
  };
}

function mountSettings(t, config = {}) {
  const root = node("root");
  const dirty = [];
  const saved = [];
  const app = renderer.createApp(SettingsPage, {
    config,
    onSave: async (value) => { saved.push(value); },
    onDirty: (value) => dirty.push(value),
  });
  const vm = app.mount(root);
  t.after(() => app.unmount());
  return {
    root, vm, dirty, saved,
    button(label) {
      const field = find(root, (el) => el.props.class === "field" && el.children.some(
        (child) => child.type === "span" && textOf(child) === label,
      ));
      assert.ok(field, `找得到「${label}」设置项`);
      return find(field, (el) => el.type === "button");
    },
  };
}

async function record(button, event) {
  button.props.onClick();
  await nextTick();
  assert.equal(textOf(button).trim(), "请按下按键…");
  button.props.onKeydown(event);
  await nextTick();
}

test("三个阅读设置按下控制键立即结束，后续主键不会覆盖", async (t) => {
  const page = mountSettings(t);
  for (const [label, key, code, expected, flag] of [
    ["隐藏窗口", "Control", "ControlLeft", "Ctrl", "ctrlKey"],
    ["上一页", "Shift", "ShiftRight", "Shift", "shiftKey"],
    ["下一页", "Alt", "AltRight", "Alt", "altKey"],
  ]) {
    const button = page.button(label);
    const event = keyEvent(key, code, { [flag]: true });
    await record(button, event);
    assert.equal(event.defaultPrevented, true);
    assert.equal(textOf(button).trim(), expected);
    assert.equal(find(page.root, (el) => el.type === "button" && textOf(el) === "取消录制"), null);
    button.props.onKeydown(keyEvent("p", "KeyP", { [flag]: true }));
    await nextTick();
    assert.equal(textOf(button).trim(), expected);
    assert.equal(page.dirty.at(-1), true);
    button.props.onKeyup(keyEvent(key, code));
  }
  await page.vm.submit();
  assert.equal(page.saved.length, 1);
  assert.equal(page.saved[0].keyHide, "Ctrl");
  assert.equal(page.saved[0].keyPrev, "Shift");
  assert.equal(page.saved[0].keyNext, "Alt");
  assert.equal(page.saved[0].hotkey, "Alt+PgDn");
  assert.equal(page.dirty.at(-1), false);
  const reopened = mountSettings(t, page.saved[0]);
  assert.equal(textOf(reopened.button("隐藏窗口")).trim(), "Ctrl");
  assert.equal(textOf(reopened.button("上一页")).trim(), "Shift");
  assert.equal(textOf(reopened.button("下一页")).trim(), "Alt");
});

test("Esc、Tab、Enter、空格和符号只录一键，长按和松键抑制默认行为", async (t) => {
  const page = mountSettings(t);
  const button = page.button("隐藏窗口");
  for (const [key, code, expected] of [
    ["Escape", "Escape", "Esc"],
    ["Tab", "Tab", "Tab"],
    ["Enter", "Enter", "Enter"],
    [" ", "Space", "Space"],
    ["+", "NumpadAdd", "+"],
    ["Meta", "MetaLeft", "Super"],
    ["AltGraph", "AltRight", "AltGraph"],
  ]) {
    const event = keyEvent(key, code);
    await record(button, event);
    assert.equal(event.defaultPrevented, true);
    assert.equal(textOf(button).trim(), expected);
    const repeat = keyEvent(key, code, { repeat: true });
    button.props.onKeydown(repeat);
    assert.equal(repeat.defaultPrevented, true);
    const release = keyEvent(key, code);
    button.props.onKeyup(release);
    assert.equal(release.defaultPrevented, true);
    await nextTick();
    assert.equal(textOf(button).trim(), expected);
  }
  button.props.onClick();
  button.props.onKeydown(keyEvent("Enter", "Enter", { repeat: true }));
  await nextTick();
  assert.equal(textOf(button).trim(), "请按下按键…");
  button.props.onKeydown(keyEvent("Enter"));
  await nextTick();
  assert.equal(textOf(button).trim(), "Enter");
});

test("全局快捷键仍等待主键并录制组合", async (t) => {
  const page = mountSettings(t);
  const button = page.button("呼出 / 收起阅读窗口（保存后立即生效）");
  button.props.onClick();
  button.props.onKeydown(keyEvent("Control", "ControlLeft", { ctrlKey: true }));
  await nextTick();
  assert.equal(textOf(button).trim(), "请按下组合键…");
  button.props.onKeydown(keyEvent("p", "KeyP", { ctrlKey: true }));
  await nextTick();
  assert.equal(textOf(button).trim(), "Ctrl+P");
  await page.vm.submit();
  assert.equal(page.saved[0].hotkey, "Ctrl+P");
});

test("保存规范化后检查重键，旧组合键提示重新录制且不静默改写", async (t) => {
  for (const config of [
    { keyHide: "Escape", keyNext: "Esc" },
    { keyHide: "Control", keyNext: "Ctrl" },
    { keyHide: "Meta", keyNext: "Super" },
    { keyHide: "a", keyNext: "A" },
  ]) {
    const page = mountSettings(t, config);
    await page.vm.submit();
    await nextTick();
    assert.equal(page.saved.length, 0);
    assert.ok(find(page.root, (el) => el.props.class === "error" && textOf(el).includes("不能设成同一个键")));
  }
  const page = mountSettings(t, { keyHide: "Shift+A" });
  await page.vm.submit();
  await nextTick();
  assert.equal(page.saved.length, 0);
  assert.equal(textOf(page.button("隐藏窗口")).trim(), "Shift+A");
  assert.ok(find(page.root, (el) => el.props.class === "error" && textOf(el).includes("请重新录制")));
  await record(page.button("隐藏窗口"), keyEvent("+", "NumpadAdd"));
  await page.vm.submit();
  assert.equal(page.saved[0].keyHide, "+");
});
