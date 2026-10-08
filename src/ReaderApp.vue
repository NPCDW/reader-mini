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
import { lineHeightOf, measureLines, pageOfTop, paginate } from "./paging";

/** 正文左右留白，与 scoped 样式里的 .body padding 是同一个值 */
const SIDE_PAD = 18;
/** 整章最多认多少行；超过就当量坏了，退回估行（一本书的一章不会有这么多） */
const MAX_ROWS_PER_CHAPTER = 2000;

const bg = ref("#181818");
const fg = ref("#bdbdbd");
const fontSize = ref(20);
const lineHeightFactor = ref(1.5);

const title = ref("");
const text = ref("");
const chapterIndex = ref(0);
/** 是否正在拖动标题条 */
const dragging = ref(false);

const scrollerEl = ref(null);
const bodyEl = ref(null);
const probeEl = ref(null);

const bodyWidth = ref(0);
const bodyHeight = ref(0);
const lineHeight = ref(0);
/** 每一行顶边的 y 坐标（相对正文顶部），分页/百分比的依据 */
const offsets = ref([]);
/** 每屏顶部所在的 y 坐标，长度即页数 */
const tops = ref([0]);
/** 每页装到第几行（下标不含） */
const rowsInPage = ref([0]);
/** 每页顶部对应的正文行，用来记进度 */
const rowsOfTop = ref([0]);
const pageIndex = ref(0);

const pages = computed(() => Math.max(tops.value.length, 1));

const pageInfo = computed(() => {
  const total = Math.max(offsets.value.length, 1);
  const end = rowsInPage.value[pageIndex.value] ?? 0;
  const pct = Math.round((end / total) * 100);
  return `第 ${pageIndex.value + 1}/${pages.value} 页 · ${Math.min(Math.max(pct, 0), 100)}%`;
});

/** 正文可视区尺寸：直接问浏览器，不按页头页脚去减（多减一点就是一行字） */
function syncBodySize() {
  bodyWidth.value = window.innerWidth;
  bodyHeight.value = Math.max(bodyEl.value?.clientHeight ?? 1, 1);
}

/** 量不到真实行高时的兜底：自然行高 ≈ 字号 × 1.25 × 行高倍数 */
const fallbackLineHeight = () =>
  Math.max(fontSize.value * lineHeightFactor.value * 1.25, 1);

/** 探针调到与正文一模一样的排版：同宽、同字号、同行高、同 padding 补偿 */
function syncProbe() {
  const el = probeEl.value;
  if (!el) return;
  el.style.width = `${Math.max(bodyWidth.value - SIDE_PAD * 2, 80)}px`;
  el.style.fontSize = `${fontSize.value}px`;
  el.style.lineHeight = String(lineHeightFactor.value);
}

/**
 * 重建行位置与分页表。
 *
 * 行位置由浏览器逐段量出来（`measureLines` 量不到就退回按行高估行）：
 * 「整章总高度 ÷ 行高」反推出来的行数只要多一行，最后一屏就会被顶出半行字，
 * 分页只按量出来的 y 算，一屏装到「底边还在屏幕里」的最后一行为止。
 *
 * `keepTop` 是重建后要停住的那个位置（正文 y），不给就停在当前页顶部。
 */
function rebuild(keepTop = null) {
  if (!probeEl.value || !bodyEl.value) return;
  const anchor = keepTop ?? tops.value[pageIndex.value] ?? 0;

  syncProbe();
  const lh = lineHeightOf(probeEl.value, 10);
  lineHeight.value = lh > 0 ? lh : fallbackLineHeight();

  const measured = measureLines(
    probeEl.value,
    text.value,
    lineHeight.value,
    MAX_ROWS_PER_CHAPTER,
  );
  offsets.value = measured ?? estimatedOffsets();

  const { tops: nextTops, pages: nextPages } = paginate(
    offsets.value,
    bodyHeight.value,
    bodyEl.value.scrollHeight,
    bodyEl.value.scrollTop,
  );
  tops.value = nextTops;
  rowsInPage.value = nextPages.map((p) => p.end);
  rowsOfTop.value = nextPages.map((p) =>
    offsets.value.findIndex((y) => y >= p.top - 0.5),
  );
  pageIndex.value = pageOfTop(tops.value, anchor);
  applyPage();
}

/** 量不到逐行高度时的兜底：按行高铺满整章 */
function estimatedOffsets() {
  const lh = Math.max(lineHeight.value, 1);
  const h = Math.max(bodyEl.value?.scrollHeight ?? lh, lh);
  const n = Math.max(Math.round(h / lh), 1);
  return new Array(n).fill(0).map((_, i) => i * lh);
}

/** 当前屏顶部贴着正文可视区上沿；位置是正文 y，不是行号 */
function applyPage() {
  if (!scrollerEl.value) return;
  scrollerEl.value.scrollTop = tops.value[pageIndex.value] ?? 0;
}

function gotoPage(index) {
  pageIndex.value = Math.min(Math.max(index, 0), pages.value - 1);
  applyPage();
  reportPosition();
}

/** 当前读到正文的第几个逻辑行（按 `\n` 分段），用来记住进度 */
const currentLine = computed(() =>
  Math.max(rowsOfTop.value[pageIndex.value] ?? 0, 0),
);

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
  const sameChapter = chapterIndex.value === (payload.chapterIndex ?? 0);
  chapterIndex.value = payload.chapterIndex ?? 0;
  title.value = payload.chapterTitle || "";
  text.value = payload.text || "";
  // 往回翻章时后端给 usize::MAX，表示落在本章末尾
  const toEnd = (payload.startLine ?? 0) > 1_000_000;
  // 换章 / 续读要跳到新位置；同一章原地刷新（保存设置、拉伸窗口）就停在原地，
  // 别把读者踢回页首
  const keepTop =
    sameChapter && !toEnd ? (tops.value[pageIndex.value] ?? 0) : null;

  // 每次展示都重新读一遍配置：设置页保存后重新交一次手，这里就是样式生效的地方
  applyStyle(await getConfig().catch(() => ({})));
  await nextTick();
  rebuild(keepTop);
  if (keepTop === null) gotoPage(toEnd ? pages.value - 1 : 0);
}

/** PgUp / PgDn：本章内翻一屏，翻到本章头尾就换到相邻一章 */
async function step(direction) {
  if (!text.value) return;
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

/**
 * 收起：让主窗口把进度收干净（写回服务端 + 本地记行号），然后藏起自己。
 *
 * 全局快捷键按第二下、托盘「收起」都落到这里，所以加个闸：
 * 收起过程中又按了一下，不能两条路径同时去写进度。
 */
let closing = null;

async function close() {
  if (closing) return closing;
  closing = doClose().finally(() => {
    closing = null;
  });
  return closing;
}

async function doClose() {
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
  // 呼出用的全局快捷键在窗口里再按一次就是收起，和托盘「继续阅读 / 收起」同一个语义
  if (event.key === "PageDown" && event.altKey) {
    event.preventDefault();
    await close();
    return;
  }
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
  const [pos, scale] = await Promise.all([
    win.outerPosition(),
    win.scaleFactor(),
  ]);
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

function applyStyle(cfg = {}) {
  bg.value = cfg.readBg || bg.value;
  fg.value = cfg.readFg || fg.value;
  fontSize.value = cfg.readFontSize || fontSize.value;
  lineHeightFactor.value = cfg.readLineHeight || lineHeightFactor.value;
}

let stopLoad = null;
let stopToggle = null;
let stopStyle = null;
let observer = null;

onMounted(async () => {
  // 先挂监听再取内容：全局快捷键呼出时后端发的 reader://load 就在这一瞬间，
  // 内容先拿在手上再谈排版，不然初次打开会是一片空白
  stopLoad = await on("reader://load", (event) => load(event.payload));
  // 同一个快捷键的第二下是「关闭」：后端发这个事件让窗口自己收干净
  stopToggle = await on("reader://toggle", () => close());
  // 手上没有正文时（还没读过书），样式改了只重读配置
  stopStyle = await on("reader://style", async (event) => {
    applyStyle(await getConfig().catch(() => ({})));
    await nextTick();
    rebuild();
  });

  applyStyle(await getConfig().catch(() => ({})));
  // 先取一次：窗口是异步起来的，后端可能在页面开始监听前就发过事件了。
  // 取完不给它 clear —— 它同时是「当前该显示的那份内容」，保存设置后后端
  // 要拿它再交一次手
  const initial = await takePending().catch(() => null);
  if (initial) await load(initial);

  syncBodySize();
  rebuild();
  // 窗口尺寸变了（或正文自己换了行数）就按新尺寸重排，并防抖落盘。
  // 尺寸以浏览器给的为准：正文容器的高度多减/少减一点，正好就是一行字。
  observer = new ResizeObserver(() => {
    syncBodySize();
    rebuild();
    schedulePersist();
  });
  observer.observe(document.body);
  if (bodyEl.value) observer.observe(bodyEl.value);
  // 首次装载也记一次：窗口被系统缩放（HiDPI）或用户从不拉伸时，
  // 配置里也该有真实尺寸，下次打开才不会大小跳一下
  await persistSize();
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("blur", close);
  window.addEventListener("mouseup", onDragEnd);
});

onUnmounted(() => {
  stopLoad?.();
  stopToggle?.();
  stopStyle?.();
  observer?.disconnect();
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("blur", close);
  window.removeEventListener("mouseup", onDragEnd);
  if (resizeTimer) clearTimeout(resizeTimer);
});

// 字号与行高倍数直接决定行高与整章行数，变了必须重排；颜色只走 CSS 变量，不用动排版
watch([fontSize, lineHeightFactor], () => {
  nextTick(() => rebuild());
});

// 窗口被挪到屏幕外时再按 PgDn，光改 scrollTop 浏览器不会去排新内容 ——
// 那一屏就会停在半行字上。用焦点把窗口托回可见区（只在窗口不可见时才做，
// 免得平时翻页平白抢焦点）
watch(pageIndex, async () => {
  await nextTick();
  const win = getCurrentWindow();
  const visible = await win.isVisible().catch(() => true);
  if (!visible) await win.setFocus().catch(() => {});
});
</script>

<template>
  <div class="frame" @mousemove="onDragMove($event)">
    <header class="bar" @mousedown="onDragStart" @mouseup="onDragEnd">
      <span class="grip">⠿ {{ title }}</span>
      <button class="close" @click="close">✕</button>
    </header>

    <!-- 整章正文一次铺满，没有可见滚动条；翻页靠 scrollTop 对到行号 -->
    <div
      ref="scrollerEl"
      class="body"
      :style="{ '--fs': `${fontSize}px`, '--lh': lineHeightFactor }"
    >
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
  background: var(--read-bg, #181818);
  color: var(--read-fg, #bdbdbd);
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
