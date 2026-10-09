<script setup>
import { reactive, ref, watch } from "vue";
import { specFromEvent } from "../keys";

const props = defineProps({
  config: { type: Object, required: true },
  // 用回调 prop 而不是事件：保存结果（成功与否、失败原因）要能带回来，
  // `emit()` 拿不到父组件的返回值。
  onSave: { type: Function, required: true },
});

// 与 Rust 侧 Config::default() 同一套默认值：回退到默认时不能回退成另一套
const DEFAULTS = {
  baseUrl: "http://127.0.0.1:1122",
  readBg: "#181818",
  readFg: "#bdbdbd",
  readFontSize: 20,
  readLineHeight: 1.5,
  hotkey: "Alt+PgDn",
  closeOnBlur: false,
};

const form = reactive({ ...DEFAULTS });

const capturing = ref(false);
const hotkeyError = ref("");

watch(
  () => props.config,
  (c) => {
    if (!c) return;
    form.baseUrl = c.baseUrl ?? DEFAULTS.baseUrl;
    form.readBg = c.readBg || DEFAULTS.readBg;
    form.readFg = c.readFg || DEFAULTS.readFg;
    form.readFontSize = c.readFontSize ?? DEFAULTS.readFontSize;
    form.readLineHeight = c.readLineHeight ?? DEFAULTS.readLineHeight;
    form.hotkey = c.hotkey || DEFAULTS.hotkey;
    // 后端可能还没有这一项（旧配置），没有就按默认关掉
    form.closeOnBlur = c.closeOnBlur === true;
  },
  { immediate: true, deep: true },
);

function captureKey(event) {
  if (!capturing.value) return;
  event.preventDefault();
  const spec = specFromEvent(event);
  if (!spec) return; // 只按了修饰键，继续等
  form.hotkey = spec;
  capturing.value = false;
}

/** 点一下输入框进入录制，之后直接按下组合键即可 */
function startCapture() {
  capturing.value = true;
  hotkeyError.value = "";
}

/** 快捷键至少要有一个修饰键，否则会把整个键盘都吃掉 */
async function save() {
  if (!/[+]/.test(form.hotkey)) {
    hotkeyError.value = "快捷键需要至少一个修饰键（如 Ctrl / Alt）";
    return;
  }
  const bg = form.readBg.trim();
  const fg = form.readFg.trim();
  if (
    !bg ||
    !fg ||
    !/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(bg) ||
    !/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(fg)
  ) {
    hotkeyError.value = "颜色要写成十六进制，如 #181818";
    return;
  }
  const message = await props.onSave({
    baseUrl: form.baseUrl.trim(),
    readBg: bg,
    readFg: fg,
    readFontSize: Number(form.readFontSize) || DEFAULTS.readFontSize,
    readLineHeight: Number(form.readLineHeight) || DEFAULTS.readLineHeight,
    hotkey: form.hotkey,
    closeOnBlur: form.closeOnBlur === true,
  });
  hotkeyError.value = message ? String(message) : "";
}
</script>

<template>
  <div class="settings">
    <label class="field">
      <span>服务地址 baseUrl</span>
      <input v-model="form.baseUrl" type="text" spellcheck="false" />
    </label>

    <span class="field-label">阅读窗口：背景色 / 字体色 / 字号</span>
    <div class="row">
      <input v-model="form.readBg" type="text" spellcheck="false" />
      <input v-model="form.readFg" type="text" spellcheck="false" />
      <input
        v-model.number="form.readFontSize"
        type="number"
        min="8"
        max="72"
      />
    </div>

    <label class="field">
      <span>行高倍数（相对字号，默认 1.5）</span>
      <input
        v-model.number="form.readLineHeight"
        type="number"
        step="0.1"
        min="1"
      />
    </label>

    <span class="field-label"
      >全局快捷键（呼出 / 关闭阅读窗口，保存后立即生效）</span
    >
    <div class="row">
      <button
        :class="['capture', { on: capturing }]"
        @click="startCapture"
        @keydown="captureKey"
      >
        {{
          capturing ? "请按下组合键…" : form.hotkey || "点击这里，再按下组合键"
        }}
      </button>
      <button class="plain" @click="form.hotkey = DEFAULTS.hotkey">
        恢复默认
      </button>
    </div>
    <p class="hint">当前生效：{{ config.hotkey }}</p>
    <p class="hint">保存后立刻作用到已打开的阅读窗口，不用把它关掉重开</p>
    <p class="hint">
      状态栏会说清楚生效没生效；万一阅读窗口没赶上，收起再呼出一次就一定读得到新设置
    </p>

    <label class="check">
      <input v-model="form.closeOnBlur" type="checkbox" />
      <span>阅读窗口失去焦点后自动关闭</span>
    </label>
    <p class="hint">
      默认关闭：开着它时点一下别处窗口就没了。不选的话只按
      <code>Esc</code> / 右上角 ✕ / 同一个全局快捷键收起
    </p>
    <p class="hint">
      拖拽、拉伸窗口不会触发关闭 —— 焦点回来或松手之后才算真的切走
    </p>

    <p v-if="hotkeyError" class="error">{{ hotkeyError }}</p>

    <button class="primary" @click="save">保存设置</button>
  </div>
</template>

<style scoped>
.settings {
  flex: 1;
  /* 与书架那一块同理：flex 项默认不肯矮过内容，不给 0 会把外壳顶出滚动条 */
  min-height: 0;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  align-items: flex-start;
}

.field,
.field-label {
  width: 100%;
  color: #c9cdd4;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  margin-top: 4px;
}

.row {
  display: flex;
  gap: 10px;
  width: 100%;
}

input {
  width: 100%;
  height: 38px;
  padding: 0 8px;
  font: inherit;
  color: inherit;
  background: #212429;
  border: 1px solid #3a3f47;
  border-radius: 6px;
  outline: none;
}

input:focus {
  border-color: #4a6fa5;
}

/* 勾选项不吃上面那条「输入框撑满一行」的规则 */
input[type="checkbox"] {
  width: 16px;
  height: 16px;
  flex: 0 0 auto;
  margin: 0;
  accent-color: #4a6fa5;
  cursor: pointer;
}

.check {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  color: #c9cdd4;
  cursor: pointer;
}

.check code {
  padding: 0 4px;
  background: #2a2d34;
  border-radius: 4px;
  font-size: 12px;
}

.capture,
.plain,
.primary {
  height: 38px;
  border: 1px solid #3a3f47;
  border-radius: 6px;
  background: #212429;
  color: #c9cdd4;
  font: inherit;
  cursor: pointer;
  text-align: left;
  padding: 0 10px;
}

.capture:hover,
.plain:hover,
.primary:hover {
  filter: brightness(1.15);
}

.capture {
  flex: 1;
}

.capture.on {
  border-color: #4a6fa5;
  color: #4a6fa5;
}

.plain {
  width: 110px;
  background: #2a2d34;
  border-color: transparent;
  text-align: center;
}

.primary {
  width: 140px;
  background: #4a6fa5;
  border-color: transparent;
  color: #fff;
  text-align: center;
}

.hint,
.error {
  margin: 0;
  font-size: 12px;
}

.hint {
  color: #838994;
}

.error {
  color: #e0685a;
}
</style>
