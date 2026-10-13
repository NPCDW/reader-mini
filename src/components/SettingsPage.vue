<script setup>
import { computed, reactive, ref, watch } from "vue";
import { hasModifier, normalizeSpec, specFromEvent } from "../keys";
import { newThemeId, normalizeTheme, themePatch, THEME_DEFAULTS } from "../theme";

const props = defineProps({
  config: { type: Object, required: true },
  // 用回调 prop 而不是事件：保存结果（成功与否、失败原因）要能带回来，
  // `emit()` 拿不到父组件的返回值。
  onSave: { type: Function, required: true },
});

const emit = defineEmits(["dirty"]);

// 与 Rust 侧 Config::default() 同一套默认值：回退到默认时不能回退成另一套
const DEFAULTS = {
  baseUrl: "http://127.0.0.1:1122",
  readBg: "#181818",
  readFg: "#bdbdbd",
  readFontSize: 20,
  readFontFamily: "system-ui",
  readFontWeight: "normal",
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
/**
 * 主题列表与当前主题。不复用 `form.readBg` 那一堆：表单里那份是正在编辑的
 * 「草稿」，主题列表是「存着的几套」—— 读者点一下别的圆圈就该整份换过来，
 * 两者混在一起会让「改了一半再切主题」变得说不清。
 */
const themes = ref([]);
const activeId = ref("");
const capturing = ref("");
const hotkeyError = ref("");
/** 正要删的那一套主题的 id：非空时确认框亮着 */
const pendingRemoveId = ref("");

/** 正在编辑的那一套；id 指不到人时退回第一套（后端也是这么兜的） */
const activeTheme = computed(
  () => themes.value.find((t) => t.id === activeId.value) ?? themes.value[0],
);

/** 主题里的圆圈：背景是这套主题的底色，圈里那个「字」用的是这套的字 */
function swatchStyle(theme) {
  return {
    background: theme.bg,
    color: theme.fg,
    fontFamily: theme.fontFamily,
    fontWeight: theme.fontWeight,
  };
}

/** 改了主题里的一项：写回草稿，并让顶栏那颗「保存」亮起来 */
function touch() {
  emit("dirty", true);
}

function updateTheme(key, value) {
  if (!activeTheme.value) return;
  activeTheme.value[key] = value;
  touch();
}

function selectTheme(id) {
  activeId.value = id;
  touch();
}

/**
 * 新增：照当前这套复制一份，改名、换新 id。
 * 复制而不是从零开始 —— 读者多半是想「在这套基础上再改一点」。
 */
function addTheme() {
  const base = activeTheme.value ?? THEME_DEFAULTS;
  const theme = normalizeTheme(
    { ...base, id: newThemeId(), name: `主题 ${themes.value.length + 1}` },
    `主题 ${themes.value.length + 1}`,
  );
  themes.value.push(theme);
  activeId.value = theme.id;
  touch();
}

/**
 * 点「删除这个主题」：只是摆出确认框，真正的删在 `confirmRemoveTheme`。
 *
 * 删主题是「点错就没有了」的动作，直接删太容易手滑，先问一句。
 * 只剩一套时连问都不问 —— 本来就删不了。
 */
function askRemoveTheme() {
  if (!activeTheme.value) return;
  if (themes.value.length <= 1) {
    hotkeyError.value = "至少留一套字体主题";
    return;
  }
  pendingRemoveId.value = activeTheme.value.id;
}

/** 确认删除：至少留一套。 */
function confirmRemoveTheme() {
  const id = pendingRemoveId.value;
  pendingRemoveId.value = "";
  if (!id) return;
  if (themes.value.length <= 1) {
    hotkeyError.value = "至少留一套字体主题";
    return;
  }
  // 删掉的正好是当前这套时，就近选一套（删的是最后一个就往前一个），
  // 不能让「选中项」空着 —— 阅读窗口要照它排
  const at = themes.value.findIndex((t) => t.id === id);
  if (at < 0) return;
  themes.value.splice(at, 1);
  if (activeId.value === id) {
    activeId.value = themes.value[Math.min(at, themes.value.length - 1)].id;
  }
  hotkeyError.value = "";
  touch();
}

/** 要删的那一套（确认框里显示它的名字）；空串表示没在确认 */
const pendingRemoveName = computed(
  () => themes.value.find((t) => t.id === pendingRemoveId.value)?.name ?? "",
);

watch(
  () => props.config,
  (c) => {
    if (!c) return;
    form.baseUrl = c.baseUrl ?? DEFAULTS.baseUrl;
    form.keyHide = c.keyHide || DEFAULTS.keyHide;
    form.keyNext = c.keyNext || DEFAULTS.keyNext;
    form.keyPrev = c.keyPrev || DEFAULTS.keyPrev;
    form.hotkey = c.hotkey || DEFAULTS.hotkey;
    // 后端可能还没有这一项（旧配置），没有就按默认关掉
    form.closeOnBlur = c.closeOnBlur === true;

    const list = Array.isArray(c.themes) && c.themes.length ? c.themes : [];
    themes.value = list.map((t, i) => normalizeTheme(t, i === 0 ? "默认主题" : `主题 ${i + 1}`));
    if (!themes.value.length) themes.value.push(normalizeTheme({ id: "default" }));
    const wanted = themes.value.some((t) => t.id === c.readThemeId);
    activeId.value = wanted ? c.readThemeId : themes.value[0].id;
    emit("dirty", false);
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
  if (!hasModifier(form.hotkey)) {
    hotkeyError.value = "全局快捷键需要至少一个修饰键（如 Ctrl / Alt）";
    return;
  }
  // 阅读窗口的三个键只放行裸键：带修饰键的组合会跟全局快捷键撞车
  // （同一次按键走两条路，窗口关掉又自己打开）
  for (const slot of KEY_SLOTS) {
    if (hasModifier(form[slot.name])) {
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
  if (!themes.value.length) {
    hotkeyError.value = "至少留一套字体主题";
    return;
  }
  // 名字不能空也不能重：圆圈底下就那一行字，两套同名读者分不出点的是哪套
  const names = themes.value.map((t) => t.name.trim());
  if (names.some((n) => !n)) {
    hotkeyError.value = "字体主题要有名字";
    return;
  }
  if (new Set(names).size !== names.length) {
    hotkeyError.value = "字体主题不要重名";
    return;
  }
  // 每一套都要查，不只当前这套：存下去的是整份主题列表
  for (const theme of themes.value) {
    const name = theme.name.trim();
    if (
      !/#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(theme.bg.trim()) ||
      !/#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(theme.fg.trim())
    ) {
      hotkeyError.value = `主题「${name}」的颜色要写成十六进制，如 #181818`;
      return;
    }
  }
  // 存下去的是整理过的那份：名字去了空白、缺项按默认补齐
  const savedThemes = themes.value.map((t) =>
    normalizeTheme({ ...t, name: t.name.trim() }),
  );
  const message = await props.onSave({
    baseUrl: form.baseUrl.trim(),
    themes: savedThemes,
    readThemeId: savedThemes.some((t) => t.id === activeId.value)
      ? activeId.value
      : savedThemes[0].id,
    // 正在用的那一套照旧摊到那几个字段上：阅读窗口从配置里读的就是它们，
    // 主题是给设置页看的组织方式
    ...themePatch(
      savedThemes.find((t) => t.id === activeId.value) ?? savedThemes[0],
    ),
    // 存之前统一写法：`Escape` -> `Esc`、字母转大写，落盘的串是规范的那一种
    keyHide: normalizeSpec(form.keyHide) || DEFAULTS.keyHide,
    keyNext: normalizeSpec(form.keyNext) || DEFAULTS.keyNext,
    keyPrev: normalizeSpec(form.keyPrev) || DEFAULTS.keyPrev,
    hotkey: normalizeSpec(form.hotkey) || DEFAULTS.hotkey,
    closeOnBlur: form.closeOnBlur === true,
  });
  hotkeyError.value = message ? String(message) : "";
  if (!message) emit("dirty", false);
}

/**
 * 顶栏那颗「保存」按的就是这里。
 *
 * 走同一套校验与同一个 `onSave`，返回值是给状态栏的那句话（成功是空串）——
 * 保存这件事只留一条路，顶栏与页内那颗按钮不会给出不一样的结论。
 */
defineExpose({ submit: save });
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
      <h2>字体主题</h2>
      <p class="hint">
        一套主题就是一套阅读样式：底色、字色、字体、字号、字重、行高。
        点圆圈选中它，被选中的那套就是阅读页面的样式；一条圆圈一套，圆圈的底是
        这套的底色，圈里那个「字」用的是这套的字与字色
      </p>

      <div class="themes">
        <div
          v-for="theme in themes"
          :key="theme.id"
          :class="['theme', { on: theme.id === activeId }]"
          role="button"
          :aria-pressed="theme.id === activeId"
          :title="theme.name"
          @click="selectTheme(theme.id)"
        >
          <span class="swatch" :style="swatchStyle(theme)">字</span>
          <span class="tname">{{ theme.name }}</span>
        </div>
        <div class="theme add" role="button" title="新增主题" @click="addTheme">
          <span class="swatch plus">＋</span>
          <span class="tname">新增</span>
        </div>
      </div>

      <template v-if="activeTheme">
        <div class="grid">
          <label class="field name">
            <span>主题名称</span>
            <input
              :value="activeTheme.name"
              type="text"
              spellcheck="false"
              @input="updateTheme('name', $event.target.value)"
            />
          </label>
          <span class="grow" />
          <button
            class="plain danger"
            :disabled="themes.length <= 1"
            @click="askRemoveTheme"
          >
            删除这个主题
          </button>
        </div>
        <div class="grid">
          <label class="field">
            <span>背景色</span>
            <input
              :value="activeTheme.bg"
              type="text"
              spellcheck="false"
              @input="updateTheme('bg', $event.target.value)"
            />
          </label>
          <label class="field">
            <span>字体色</span>
            <input
              :value="activeTheme.fg"
              type="text"
              spellcheck="false"
              @input="updateTheme('fg', $event.target.value)"
            />
          </label>
          <label class="field">
            <span>字号</span>
            <input
              :value="activeTheme.fontSize"
              type="number"
              min="8"
              max="72"
              @input="updateTheme('fontSize', Number($event.target.value))"
            />
          </label>
          <label class="field">
            <span>行高倍数</span>
            <input
              :value="activeTheme.lineHeight"
              type="number"
              step="0.1"
              min="1"
              @input="updateTheme('lineHeight', Number($event.target.value))"
            />
          </label>
        </div>
        <div class="grid">
          <label class="field wide">
            <span>字体</span>
            <input
              :value="activeTheme.fontFamily"
              type="text"
              spellcheck="false"
              placeholder="system-ui、Microsoft YaHei、serif…"
              @input="updateTheme('fontFamily', $event.target.value)"
            />
          </label>
          <label class="field">
            <span>字重</span>
            <select
              :value="activeTheme.fontWeight"
              @change="updateTheme('fontWeight', $event.target.value)"
            >
              <option v-for="w in FONT_WEIGHTS" :key="w.value" :value="w.value">
                {{ w.label }}
              </option>
            </select>
          </label>
        </div>
        <p class="preview" :style="{ background: activeTheme.bg, color: activeTheme.fg }">
          <span
            :style="{
              fontFamily: activeTheme.fontFamily,
              fontWeight: activeTheme.fontWeight,
              fontSize: `${activeTheme.fontSize}px`,
              lineHeight: activeTheme.lineHeight,
            }"
          >
            这段字就是阅读页面的样子。
          </span>
        </p>
      </template>
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

    <!-- 保存只留右上角那一颗（App.vue 的顶栏），这里不再放第二颗：
         同一个动作两处入口，读者总会怀疑它们是不是一样 -->
    <div v-if="capturing" class="actions">
      <button class="plain" @click="startCapture('')">取消录制</button>
    </div>

    <!-- 删除主题的确认框：删掉就没了的动作，先问一句 -->
    <div v-if="pendingRemoveId" class="mask" @click.self="pendingRemoveId = ''">
      <div class="dialog" role="alertdialog" aria-modal="true">
        <h3>删除字体主题</h3>
        <p>
          确定删除「{{ pendingRemoveName }}」吗？删掉之后就不在列表里了。
          这一套的样式设置会跟着没，且不能撤销。
        </p>
        <div class="dialog-actions">
          <button class="plain" @click="pendingRemoveId = ''">取消</button>
          <button class="plain danger" @click="confirmRemoveTheme">删除</button>
        </div>
      </div>
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

/* ---- 字体主题：一条圆圈一套，「选中的那套」用一圈边框标出来 ---- */
.themes {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: 14px;
}

.theme {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  width: 62px;
  cursor: pointer;
}

/* 圆圈：底是这套主题的底色，圈里那个「字」用的就是这套的字与字色 */
.swatch {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  border: 1px solid #3a3f47;
  font-size: 18px;
  line-height: 1;
  user-select: none;
}

.theme.on .swatch {
  /* 选中：留一圈呼吸的缝，再套一层亮边 —— 圈与圈之间不挤在一起才看得清 */
  box-shadow: 0 0 0 3px #1e2026, 0 0 0 5px #4a6fa5;
}

.theme.add .swatch {
  background: #212429;
  color: #838994;
  border-style: dashed;
}

.theme:hover .swatch {
  border-color: #4a6fa5;
}

.tname {
  max-width: 100%;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  font-size: 12px;
  color: #a2a9b4;
}

.theme.on .tname {
  color: #7ea6dd;
}

/* 主题名那一行：左边输入框，右边「删除这个主题」 */
.grid .grow {
  flex: 1;
}

.grid .field.name {
  flex: 0 1 220px;
  max-width: 220px;
}

.plain.danger {
  align-self: flex-end;
  color: #e0685a;
  border: 1px solid #4a2f2c;
  background: #2a1f1e;
}

.plain.danger:disabled {
  opacity: 0.45;
  cursor: default;
}

/* 小样：直接拿这套主题的色与字排一段，选之前先看见 */
.preview {
  margin: 0;
  padding: 12px;
  border-radius: 8px;
  border: 1px solid #2b2f37;
  overflow: hidden;
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
.plain {
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
.plain:hover {
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

.mask {
  position: fixed;
  inset: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.5);
}

.dialog {
  width: min(320px, calc(100% - 48px));
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  background: #1e2026;
  border: 1px solid #2f333b;
  border-radius: 10px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
}

.dialog h3 {
  margin: 0;
  font-size: 14px;
  color: #dfe3ea;
}

.dialog p {
  margin: 0;
  font-size: 12px;
  line-height: 1.6;
  color: #8f97a3;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}
</style>
