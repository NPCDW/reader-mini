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
  readFontFamily: "system-ui",
  readFontWeight: "normal",
  readLetterSpacing: "0",
  readLineHeight: 1.5,
  keyHide: "Esc",
  keyNext: "PgDn",
  keyPrev: "PgUp",
  hotkey: "Alt+PgDn",
  closeOnBlur: false,
};

/** 正文字重：下拉比手填稳 —— 字重不是任意数都有效，写成 350 有的字体压根不认 */
const FONT_WEIGHTS = [
  { value: "300", label: "细体 300" },
  { value: "normal", label: "常规 400" },
  { value: "500", label: "中等 500" },
  { value: "600", label: "半粗 600" },
  { value: "bold", label: "加粗 700" },
  { value: "800", label: "特粗 800" },
];

/** 阅读窗口那三个快捷键，照这个顺序录 */
const KEY_SLOTS = [
  { name: "keyHide", label: "隐藏窗口" },
  { name: "keyNext", label: "下一页" },
  { name: "keyPrev", label: "上一页" },
];

const form = reactive({ ...DEFAULTS });

const capturing = ref("");
const hotkeyError = ref("");

watch(
  () => props.config,
  (c) => {
    if (!c) return;
    form.baseUrl = c.baseUrl ?? DEFAULTS.baseUrl;
    form.readBg = c.readBg || DEFAULTS.readBg;
    form.readFg = c.readFg || DEFAULTS.readFg;
    form.readFontSize = c.readFontSize ?? DEFAULTS.readFontSize;
    form.readFontFamily = c.readFontFamily || DEFAULTS.readFontFamily;
    form.readFontWeight = c.readFontWeight || DEFAULTS.readFontWeight;
    form.readLetterSpacing = c.readLetterSpacing ?? DEFAULTS.readLetterSpacing;
    form.readLineHeight = c.readLineHeight ?? DEFAULTS.readLineHeight;
    form.keyHide = c.keyHide || DEFAULTS.keyHide;
    form.keyNext = c.keyNext || DEFAULTS.keyNext;
    form.keyPrev = c.keyPrev || DEFAULTS.keyPrev;
    form.hotkey = c.hotkey || DEFAULTS.hotkey;
    // 后端可能还没有这一项（旧配置），没有就按默认关掉
    form.closeOnBlur = c.closeOnBlur === true;
  },
  { immediate: true, deep: true },
);

/** 录制某个槽位：点一下按钮进入录制，之后直接按下组合键即可 */
function startCapture(name) {
  capturing.value = name;
  hotkeyError.value = "";
}

function captureKey(event, name) {
  if (capturing.value !== name) return;
  event.preventDefault();
  const spec = specFromEvent(event);
  if (!spec) return; // 只按了修饰键，继续等
  form[name] = spec;
  capturing.value = "";
}

/** 快捷键至少要有一个修饰键，否则会把整个键盘都吃掉 */
async function save() {
  if (!/[+]/.test(form.hotkey)) {
    hotkeyError.value = "全局快捷键需要至少一个修饰键（如 Ctrl / Alt）";
    return;
  }
  // 阅读窗口的三个键只放行裸键：带修饰键的组合会跟全局快捷键撞车
  // （同一次按键走两条路，窗口关掉又自己打开）
  for (const slot of KEY_SLOTS) {
    if (/[+]/.test(form[slot.name])) {
      hotkeyError.value = `阅读快捷键「${slot.label}」不要带修饰键`;
      return;
    }
  }
  // 三个键不能重：重了就只有前一个生效，读者按下去看不到预期的动作
  const picked = KEY_SLOTS.map((s) => form[s.name]).map((v) => String(v).trim());
  if (new Set(picked).size !== picked.length) {
    hotkeyError.value = "阅读快捷键三个不能设成同一个键";
    return;
  }
  const bg = form.readBg.trim();
  const fg = form.readFg.trim();
  if (
    !bg ||
    !fg ||
    !/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(bg) ||
    !/#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(fg)
  ) {
    hotkeyError.value = "颜色要写成十六进制，如 #181818";
    return;
  }
  const letterSpacing = form.readLetterSpacing.trim() || DEFAULTS.readLetterSpacing;
  if (!/^(0|[+-]?(\d+(\.\d+)?)(px|em|rem|%)?)$/.test(letterSpacing)) {
    hotkeyError.value = "字距要写成 0、1px 或 0.05em 这样";
    return;
  }
  const message = await props.onSave({
    baseUrl: form.baseUrl.trim(),
    readBg: bg,
    readFg: fg,
    readFontSize: Number(form.readFontSize) || DEFAULTS.readFontSize,
    readFontFamily: form.readFontFamily.trim() || DEFAULTS.readFontFamily,
    readFontWeight: form.readFontWeight,
    readLetterSpacing: letterSpacing,
    readLineHeight: Number(form.readLineHeight) || DEFAULTS.readLineHeight,
    keyHide: form.keyHide.trim() || DEFAULTS.keyHide,
    keyNext: form.keyNext.trim() || DEFAULTS.keyNext,
    keyPrev: form.keyPrev.trim() || DEFAULTS.keyPrev,
    hotkey: form.hotkey,
    closeOnBlur: form.closeOnBlur === true,
  });
  hotkeyError.value = message ? String(message) : "";
}
</script>

<template>
  <div class="settings">
    <section class="card">
      <h2>服务</h2>
      <label class="field">
        <span>服务地址 baseUrl</span>
        <input v-model="form.baseUrl" type="text" spellcheck="false" />
      </label>
    </section>

    <section class="card">
      <h2>阅读样式</h2>
      <p class="hint">保存后立刻作用到已打开的阅读窗口，不用把它关掉重开</p>
      <div class="grid">
        <label class="field">
          <span>背景色</span>
          <input v-model="form.readBg" type="text" spellcheck="false" />
        </label>
        <label class="field">
          <span>字体色</span>
          <input v-model="form.readFg" type="text" spellcheck="false" />
        </label>
        <label class="field">
          <span>字号</span>
          <input
            v-model.number="form.readFontSize"
            type="number"
            min="8"
            max="72"
          />
        </label>
        <label class="field">
          <span>行高倍数</span>
          <input
            v-model.number="form.readLineHeight"
            type="number"
            step="0.1"
            min="1"
          />
        </label>
      </div>
      <div class="grid">
        <label class="field wide">
          <span>字体</span>
          <input
            v-model="form.readFontFamily"
            type="text"
            spellcheck="false"
            placeholder="system-ui、Microsoft YaHei、serif…"
          />
        </label>
        <label class="field">
          <span>字重</span>
          <select v-model="form.readFontWeight">
            <option v-for="w in FONT_WEIGHTS" :key="w.value" :value="w.value">
              {{ w.label }}
            </option>
          </select>
        </label>
        <label class="field">
          <span>字距</span>
          <input
            v-model="form.readLetterSpacing"
            type="text"
            spellcheck="false"
            placeholder="0、1px、0.05em"
          />
        </label>
      </div>
    </section>

    <section class="card">
      <h2>阅读快捷键</h2>
      <p class="hint">
        阅读窗口只认这三个键，其余按键（含鼠标滚轮和右键）一律屏蔽。
        不要带修饰键：带修饰键的组合会跟全局快捷键撞车
      </p>
      <div class="grid keys">
        <div v-for="slot in KEY_SLOTS" :key="slot.name" class="field">
          <span>{{ slot.label }}</span>
          <button
            :class="['capture', { on: capturing === slot.name }]"
            @click="startCapture(slot.name)"
            @keydown="captureKey($event, slot.name)"
          >
            {{
              capturing === slot.name
                ? "请按下按键…"
                : form[slot.name] || "点击这里，再按下按键"
            }}
          </button>
        </div>
      </div>
    </section>

    <section class="card">
      <h2>全局快捷键</h2>
      <label class="field">
        <span>呼出 / 收起阅读窗口（保存后立即生效）</span>
        <button
          :class="['capture', { on: capturing === 'hotkey' }]"
          @click="startCapture('hotkey')"
          @keydown="captureKey($event, 'hotkey')"
        >
          {{
            capturing === "hotkey"
              ? "请按下组合键…"
              : form.hotkey || "点击这里，再按下组合键"
          }}
        </button>
      </label>
      <p class="hint">当前生效：{{ config.hotkey }}</p>
    </section>

    <section class="card">
      <h2>窗口行为</h2>
      <label class="check">
        <input v-model="form.closeOnBlur" type="checkbox" />
        <span>阅读窗口失去焦点后自动关闭</span>
      </label>
      <p class="hint">
        默认关闭：开着它时点一下别处窗口就没了。不选的话只按
        <code>Esc</code>（或你设的隐藏键）/ 右上角 ✕ / 同一个全局快捷键收起
      </p>
      <p class="hint">
        拖拽、拉伸窗口不会触发关闭 —— 焦点回来或松手之后才算真的切走
      </p>
    </section>

    <p v-if="hotkeyError" class="error">{{ hotkeyError }}</p>

    <div class="actions">
      <button class="primary" @click="save">保存设置</button>
      <button class="plain" @click="startCapture('')">取消录制</button>
    </div>
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
  gap: 12px;
  align-items: stretch;
}

.card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  background: #1e2026;
  border: 1px solid #2b2f37;
  border-radius: 10px;
}

.card h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: #8f97a3;
  text-transform: uppercase;
}

.grid {
  display: flex;
  gap: 10px;
  width: 100%;
}

.grid.keys {
  flex-wrap: wrap;
}

.grid.keys .field {
  flex: 1;
  min-width: 140px;
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
  font-size: 13px;
}

.field-label {
  margin-top: 4px;
}

.row {
  display: flex;
  gap: 10px;
  width: 100%;
}

input,
select {
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

select {
  cursor: pointer;
}

input:focus,
select:focus {
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
  font-size: 13px;
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
  font-family: ui-monospace, "SF Mono", Consolas, monospace;
}

.capture.on {
  border-color: #4a6fa5;
  color: #7ea6dd;
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

.actions {
  display: flex;
  gap: 10px;
}

.hint,
.error {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
}

.hint {
  color: #838994;
}

.error {
  color: #e0685a;
}
</style>
