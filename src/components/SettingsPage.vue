<script setup>
import { reactive, ref, watch } from "vue";
import { specFromEvent } from "../keys";

const props = defineProps({
  config: { type: Object, required: true },
  // 用回调 prop 而不是事件：保存结果（成功与否、失败原因）要能带回来，
  // `emit()` 拿不到父组件的返回值。
  onSave: { type: Function, required: true },
});

const DEFAULT_HOTKEY = "Ctrl+Alt+R";

const form = reactive({
  baseUrl: "",
  readBg: "",
  readFg: "",
  readFontSize: 20,
  readLineHeight: 1.5,
  hotkey: DEFAULT_HOTKEY,
});

const capturing = ref(false);
const hotkeyError = ref("");

watch(
  () => props.config,
  (c) => {
    if (!c) return;
    form.baseUrl = c.baseUrl ?? "";
    form.readBg = c.readBg ?? "";
    form.readFg = c.readFg ?? "";
    form.readFontSize = c.readFontSize ?? 20;
    form.readLineHeight = c.readLineHeight ?? 1.5;
    form.hotkey = c.hotkey || DEFAULT_HOTKEY;
  },
  { immediate: true, deep: true },
);

/** 点一下进入录制，之后直接按下组合键即可 */
function startCapture() {
  capturing.value = true;
  hotkeyError.value = "";
}

function captureKey(event) {
  if (!capturing.value) return;
  event.preventDefault();
  const spec = specFromEvent(event);
  if (!spec) return; // 只按了修饰键，继续等
  form.hotkey = spec;
  capturing.value = false;
}

/** 快捷键至少要有一个修饰键，否则会把整个键盘都吃掉 */
async function save() {
  if (!/[+]/.test(form.hotkey)) {
    hotkeyError.value = "快捷键需要至少一个修饰键（如 Ctrl / Alt）";
    return;
  }
  const message = await props.onSave({
    baseUrl: form.baseUrl.trim(),
    readBg: form.readBg.trim(),
    readFg: form.readFg.trim(),
    readFontSize: Number(form.readFontSize) || 20,
    readLineHeight: Number(form.readLineHeight) || 1.5,
    hotkey: form.hotkey,
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
      <input v-model.number="form.readFontSize" type="number" min="8" max="72" />
    </div>

    <label class="field">
      <span>行高倍数（相对字号，默认 1.5）</span>
      <input v-model.number="form.readLineHeight" type="number" step="0.1" min="1" />
    </label>

    <span class="field-label">全局快捷键（呼出 / 关闭阅读窗口，保存后立即生效）</span>
    <div class="row">
      <button
        :class="['capture', { on: capturing }]"
        @click="startCapture"
        @keydown="captureKey"
      >
        {{ capturing ? "请按下组合键…" : form.hotkey || "点击这里，再按下组合键" }}
      </button>
      <button class="plain" @click="form.hotkey = DEFAULT_HOTKEY">恢复默认</button>
    </div>
    <p class="hint">当前生效：{{ config.hotkey }}</p>
    <p v-if="hotkeyError" class="error">{{ hotkeyError }}</p>

    <button class="primary" @click="save">保存设置</button>
  </div>
</template>

<style scoped>
.settings {
  flex: 1;
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
  color: #333;
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
  background: #fff;
  border: 1px solid #c8ced8;
  border-radius: 6px;
  outline: none;
}

input:focus {
  border-color: #4a6fa5;
}

.capture,
.plain,
.primary {
  height: 38px;
  border: 1px solid #c8ced8;
  border-radius: 6px;
  background: #fff;
  color: #333;
  font: inherit;
  cursor: pointer;
  text-align: left;
  padding: 0 10px;
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
  background: #e3e6ea;
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
  color: #999;
}

.error {
  color: #c0392b;
}
</style>
