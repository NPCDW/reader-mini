<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  closeReader,
  getConfig,
  on,
  send,
  saveReaderSize,
  switchChapter,
  takePending,
} from "./bridge";
import { lineHeightOf, measureHeight, pageTops, rowsPerScreen } from "./paging";

/** 正文上下左右的留白；顶部还要额外让出 BODY_TOP_PAD，分页时必须扣掉 */
const SIDE_PAD = 18;
const BODY_TOP_PAD = 6;
const HEAD_H = 26;
const FOOT_H = 24;

const bg = ref("#f5f0e1");
const fg = ref("#333333");
const fontSize = ref(20);
const lineHeightFactor = ref(1.5);

const title = ref("");
const text = ref("");
const chapterIndex = ref(0);
/** 往回翻章时后端给 usize::MAX，表示落在本章末尾 */
const pendingLastPage = ref(false);

/** 是否正在拖动标题条 */
const dragging = ref(false);

const scrollerEl = ref(null);
const bodyEl = ref(null);
const probeEl = ref(null);

const bodyWidth = ref(0);
const bodyHeight = ref(0);
const lineHeight = ref(0);
const totalRows = ref(1);
/** 每屏顶部所在行号，长度即页数 */
const tops = ref([0]);
const pageIndex = ref(0);

const pages = computed(() => Math.max(tops.value.length, 1));

const pageInfo = computed(() => {
  const total = Math.max(totalRows.value, 1);
  const top = tops.value[pageIndex.value] ?? 0;
  const bottom = Math.min(top + rowsPerScreen(bodyHeight.value, lineHeight.value), total);
  const pct = Math.round((bottom / total) * 100);
  return `第 ${pageIndex.value + 1}/${pages.value} 页 · ${Math.min(Math.max(pct, 0), 100)}%`;
});

/** 量不到真实行高时的兜底：自然行高 ≈ 字号 × 1.25 × 行高倍数 */
const fallbackLineHeight = () =>
  Math.max(fontSize.value * lineHeightFactor.value * 1.25, 1);

/** 正文可视区尺寸跟着窗口走 */
function syncBodySize() {
  bodyWidth.value = window.innerWidth;
  bodyHeight.value = Math.max(
    window.innerHeight - HEAD_H - FOOT_H - BODY_TOP_PAD,
    1,
  );
}

/**
 * 按当前窗口尺寸与字号重建行高、整章行数与分页表。
 *
 * 一屏几行 = 正文可视高度 ÷ 实测行高，整章几行 = 实测全文高度 ÷ 实测行高，
 * 所以改字号、换章、拉伸窗口之后都要重算一遍。
 * 重建后尽量停在原来那一屏对应的行上。
 */
function rebuild(keepRow = null) {
  if (!probeEl.value) return;
  const anchor = keepRow ?? tops.value[pageIndex.value] ?? 0;

  probeEl.value.style.width = `${Math.max(bodyWidth.value - SIDE_PAD * 2, 80)}px`;
  probeEl.value.style.fontSize = `${fontSize.value}px`;
  probeEl.value.style.lineHeight = String(lineHeightFactor.value);

  const lh = lineHeightOf(probeEl.value, 10);
  lineHeight.value = lh > 0 ? lh : fallbackLineHeight();

  const h = measureHeight(probeEl.value, text.value);
  totalRows.value =
    h > lineHeight.value / 2 ? Math.max(Math.round(h / lineHeight.value), 1) : 1;

  tops.value = pageTops(totalRows.value, rowsPerScreen(bodyHeight.value, lineHeight.value));
  const idx = tops.value.findIndex((t) => t >= anchor);
  pageIndex.value = idx < 0 ? tops.value.length - 1 : idx;
  applyPage();
}

/** 当前屏顶部贴着正文可视区上沿 */
function applyPage() {
  if (!scrollerEl.value) return;
  scrollerEl.value.scrollTop = (tops.value[pageIndex.value] ?? 0) * lineHeight.value;
}

function gotoPage(index) {
  pageIndex.value = Math.min(Math.max(index, 0), pages.value - 1);
  applyPage();
  reportPosition();
}

/**
 * 当前读到正文的第几个逻辑行（按 `\n` 分段），用来记住进度。
 *
 * 显示行 -> 逻辑行靠「每段占几个显示行」累加反推：正文是中文单栏排版，
 * 这个换算只在记进度时用，差一行不影响续读的体感。
 */
const currentLine = computed(() => {
  const topRows = tops.value[pageIndex.value] ?? 0;
  const perRow = Math.max(
    Math.floor((bodyWidth.value - SIDE_PAD * 2) / Math.max(fontSize.value, 6)),
    8,
  );
  const lines = text.value.split("\n");
  let rows = 0;
  for (let i = 0; i < lines.length; i += 1) {
    rows += Math.max(Math.ceil((lines[i].length + 1) / perRow), 1);
    if (rows > topRows) return i;
  }
  return Math.max(lines.length - 1, 0);
});

/**
 * 把「现在读到哪」告诉主窗口。
 *
 * 托盘菜单的「退出」要先收掉阅读窗口才能把进度写回去，而那时它手上
 * 只有主窗口，所以位置得随时同步过去。
 */
function reportPosition() {
  send("reader://position", {
    chapterIndex: chapterIndex.value,
    title: title.value,
    line: currentLine.value,
  }).catch(() => {});
}

async function load(payload) {
  if (!payload) return;
  chapterIndex.value = payload.chapterIndex ?? 0;
  title.value = payload.chapterTitle || "";
  text.value = payload.text || "";
  pendingLastPage.value = (payload.startLine ?? 0) > 1_000_000;
  await nextTick();
  rebuild();
  gotoPage(pendingLastPage.value ? pages.value - 1 : 0);
}

/** PgUp / PgDn：本章内翻一屏，翻到本章头尾就换到相邻一章 */
async function step(direction) {
  const atEnd = direction > 0 && pageIndex.value + 1 >= pages.value;
  const atStart = direction < 0 && pageIndex.value === 0;
  if (!atEnd && !atStart) {
    gotoPage(pageIndex.value + direction);
    return;
  }
  try {
    await switchChapter({ currentIndex: chapterIndex.value, direction });
  } catch (e) {
    console.warn(String(e));
  }
}

/** 正在拖动窗口：这时失焦是拖拽过程的一部分，不该把窗口关掉 */
function interacting() {
  return dragging.value;
}

/** 关窗：进度写回服务端 + 本地记行号，然后把自己藏起来 */
async function close() {
  if (interacting()) return;
  try {
    await closeReader({
      chapterIndex: chapterIndex.value,
      chapterTitle: title.value,
      line: currentLine.value,
    });
  } catch (e) {
    console.warn(String(e));
  }
}

async function onKeydown(event) {
  const handled = {
    PageDown: () => step(1),
    PageUp: () => step(-1),
    Escape: () => close(),
    Home: () => {
      gotoPage(0);
      return Promise.resolve();
    },
    End: () => step(1),
  }[event.key];
  if (!handled) return;
  event.preventDefault();
  await handled();
}

// ---- 拖动：按住顶部标题条把窗口挪走 ----
//
// 按下时记一次窗口位置，之后每次移动都按「起始位置 + 累计位移」算。
// 每次移动都去问一次窗口在哪，会把上一步的误差累积进去，拖起来会飘。
let dragOrigin = null;

async function onDragStart(event) {
  if (event.button !== 0) return;
  const win = getCurrentWindow();
  const [pos, scale] = await Promise.all([win.outerPosition(), win.scaleFactor()]);
  dragOrigin = {
    pointer: { x: event.screenX, y: event.screenY },
    pos: { x: pos.x, y: pos.y },
    scale,
  };
  dragging.value = true;
}

async function onDragMove(event) {
  if (!dragOrigin) return;
  const { pointer, pos, scale } = dragOrigin;
  const win = getCurrentWindow();
  await win.setPosition({
    type: "Physical",
    x: Math.round(pos.x + (event.screenX - pointer.x) * scale),
    y: Math.round(pos.y + (event.screenY - pointer.y) * scale),
  });
}

function onDragEnd() {
  dragOrigin = null;
  dragging.value = false;
}

let resizeTimer = null;

/**
 * 窗口尺寸变了就重排，并防抖落盘。
 *
 * 拉伸交给窗口系统自己处理（Tauri 的 resizable），这边只负责
 * 跟着新尺寸重算分页 —— 自己算一遍鼠标位移再 setSize 会和系统抢，
 * 拉起来一顿一顿的。
 */
function schedulePersist() {
  if (resizeTimer) clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    resizeTimer = null;
    persistSize();
  }, 400);
}

/** 把当前窗口尺寸记进配置，下次打开还是这个大小 */
async function persistSize() {
  const win = getCurrentWindow();
  const [size, scale] = await Promise.all([win.innerSize(), win.scaleFactor()]);
  await saveReaderSize(size.width / scale, size.height / scale).catch((e) =>
    console.warn(`记录窗口尺寸失败: ${e}`),
  );
}

async function applyStyle(cfg = {}) {
  bg.value = cfg.readBg || bg.value;
  fg.value = cfg.readFg || fg.value;
  fontSize.value = cfg.readFontSize || fontSize.value;
  lineHeightFactor.value = cfg.readLineHeight || lineHeightFactor.value;
  document.documentElement.style.setProperty("--read-bg", bg.value);
  document.documentElement.style.setProperty("--read-fg", fg.value);
}

let stopLoad = null;
let observer = null;

onMounted(async () => {
  await applyStyle(await getConfig().catch(() => ({})));
  syncBodySize();
  // 窗口尺寸变了就按新尺寸重算分页；正文本身高度变化也会触发，rebuild 是幂等的
  observer = new ResizeObserver(() => {
    const prevW = bodyWidth.value;
    const prevH = bodyHeight.value;
    syncBodySize();
    if (prevW === bodyWidth.value && prevH === bodyHeight.value) return;
    rebuild();
    schedulePersist();
  });
  observer.observe(document.documentElement);
  if (bodyEl.value) observer.observe(bodyEl.value);

  // 先取一次：窗口是异步起来的，后端建窗口时发的那个事件我们可能没赶上
  const initial = await takePending().catch(() => null);
  if (initial) await load(initial);
  // 首次装载也记一次：窗口被系统缩放（HiDPI）或用户从不拉伸时，
  // 配置里也该有真实尺寸，下次打开才不会大小跳一下
  await persistSize();
  stopLoad = await on("reader://load", (event) => load(event.payload));
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("blur", close);
  window.addEventListener("mouseup", onDragEnd);
});

onUnmounted(() => {
  stopLoad?.();
  observer?.disconnect();
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("blur", close);
  window.removeEventListener("mouseup", onDragEnd);
  if (resizeTimer) clearTimeout(resizeTimer);
});

watch([fontSize, lineHeightFactor], () => rebuild());
</script>

<template>
  <div class="frame" @mousemove="onDragMove($event)">
    <header class="bar" @mousedown="onDragStart" @mouseup="onDragEnd">
      <span class="grip">⠿ {{ title }}</span>
      <button class="close" @click="close">✕</button>
    </header>

    <!-- 整章正文一次铺满，没有可见滚动条；翻页靠 scrollTop 对到行号 -->
    <div ref="scrollerEl" class="body" :style="{ '--fs': `${fontSize}px`, '--lh': lineHeightFactor }">
      <p ref="bodyEl" class="text">{{ text }}</p>
    </div>

    <footer class="foot">
      <button @click="step(-1)">PgUp</button>
      <button @click="step(1)">PgDn</button>
      <span class="spacer" />
      <span class="info">{{ pageInfo }}</span>
    </footer>

    <!-- 量高度用的探针：不可见，只为拿浏览器排版出来的真实高度 -->
    <p ref="probeEl" class="probe" aria-hidden="true" />

  </div>
</template>

<style scoped>
.frame {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--read-bg, #f5f0e1);
  color: var(--read-fg, #333);
}

.bar {
  display: flex;
  align-items: center;
  flex: 0 0 26px;
  height: 26px;
  cursor: grab;
}

.grip {
  flex: 1;
  padding-left: 6px;
  font-size: 12px;
  opacity: 0.35;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.close {
  width: 28px;
  height: 26px;
  border: none;
  background: transparent;
  color: inherit;
  opacity: 0.45;
  font-size: 13px;
  cursor: pointer;
}

.body {
  flex: 1;
  overflow: auto;
  padding: 0 18px;
  scrollbar-width: none;
}

.body::-webkit-scrollbar {
  display: none;
}

.text {
  margin: 0;
  /* 顶部内边距与 BODY_TOP_PAD 是同一个值 */
  padding-top: 6px;
  font-size: var(--fs, 20px);
  line-height: var(--lh, 1.5);
  white-space: pre-wrap;
  word-break: break-word;
}

.foot {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 24px;
  height: 24px;
}

.foot button {
  width: 60px;
  height: 24px;
  border: none;
  background: transparent;
  color: inherit;
  opacity: 0.5;
  font-size: 11px;
  cursor: pointer;
}

.spacer {
  flex: 1;
}

.info {
  padding-right: 8px;
  font-size: 11px;
  opacity: 0.45;
}

.probe {
  position: absolute;
  top: 0;
  left: -99999px;
  margin: 0;
  visibility: hidden;
  white-space: pre-wrap;
  word-break: break-word;
}

</style>
