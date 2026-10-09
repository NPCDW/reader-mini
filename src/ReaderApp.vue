<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  closeReader,
  getConfig,
  on,
  send,
  saveProgress,
  saveReaderSize,
  switchChapter,
  takePending,
} from "./bridge";
import {
  clipHeight,
  lineHeightOf,
  measureRows,
  pageEnds,
  pageFirstRows,
  pageOfRow,
  rowsPerScreen,
} from "./paging";

/** 正文左右留白，与 scoped 样式里的 .body padding 是同一个值 */
const SIDE_PAD = 18;
/** 标题条与正文之间的缝（.bar 的 margin-bottom），分页时算进可用高度 */
const TOP_GAP = 6;
/** 整章最多认多少行；超过就当量坏了，退回估行 */
const MAX_ROWS_PER_CHAPTER = 2000;
/** 收起时全局快捷键可能还在按住，这段时间里不接受失焦自动收起（仅配置项打开时才走） */
const CLOSE_GUARD_MS = 600;
/**
 * 失焦后先缓这么久再收：Windows 上拉伸窗口一定会发一次失焦、拖拽也常发，
 * 焦点随即又回来（Linux 上没有这种抖动）。立刻收的话读者伸手去拉窗口，窗口就没了。
 */
const BLUR_CLOSE_DELAY_MS = 400;
/** 鼠标还按着、或窗口尺寸刚变过，都算「正在摆弄窗口」，这期间的失焦不作数 */
const INTERACT_GRACE_MS = 800;
/** 鼠标按下的状态靠鼠标事件维持；太久没再来事件（比如在窗口外松的手）就别再当真 */
const BUTTON_HELD_MAX_MS = 1500;
/** 拿鼠标按下状态要听的这几个事件；捕获阶段挂，免得被别的处理拦掉 */
const MOUSE_TYPES = ["mousedown", "mousemove", "mouseup"];
/**
 * 翻页上报进度的节流：连着翻好几屏也只报最后停住的那一屏。
 *
 * 5 秒是「读者停下来」的量级 —— 比这更密的上报只是在给服务端刷请求，
 * 而读者翻页时中间那些屏本来就不算读过。真要精确，靠收起时的收尾上报。
 */
const SAVE_THROTTLE_MS = 5000;
/** 窗口刚打开 / 刚换章时报一次，稍等一下 —— 免得「呼出来」被当成「读到这儿」 */
const SHOW_DELAY_MS = 1200;

const bg = ref("#181818");
const fg = ref("#bdbdbd");
const fontSize = ref(20);
const lineHeightFactor = ref(1.5);
/** 失焦即收起。这是配置项，默认关 —— 点一下别处窗口就没了太容易误伤 */
const closeOnBlur = ref(false);

const title = ref("");
const text = ref("");
const chapterIndex = ref(0);
/** 当前这本书在书架里的下标，上报进度要用；-1 表示没书 */
const bookIndex = ref(-1);
/** 服务端记的上次读到的正文位置（以字数计）；换章时为 0 —— 新章从头读起 */
const resumePos = ref(0);
/** 手上这份内容对应的样式版本，用来判断「配置是不是比正文新」 */
const styleTick = ref(0);
/** 往回翻章时后端给 usize::MAX，表示落在本章末尾 */
const pendingLastPage = ref(false);

/** 是否正在拖动标题条 */
const dragging = ref(false);

/** 滚动容器，可视高度会被裁到整行高的整数倍 */
const scrollerEl = ref(null);
/** 页头（标题条）与页脚，用来算正文还剩多少可用高度 */
const barEl = ref(null);
const footEl = ref(null);
/** 正文段落，整章都在里面，高度就是整章的高度 */
const textEl = ref(null);
/** 量高度用的探针，不可见 */
const probeEl = ref(null);

const bodyWidth = ref(0);
/** 滚动容器能拿到的可视高度（原始值，随时可能变） */
const rawHeight = ref(0);
/** 裁到整行高倍数之后的有效视口高度；分页与翻页都以它为准 */
const bodyHeight = ref(0);
const lineHeight = ref(0);
/** 每一行顶边的 y 坐标（相对正文顶部），分页的依据 */
const offsets = ref([]);
/** 每一行顶边落在正文的第几个字，与 `offsets` 一一对应 */
const rowChars = ref([]);
/** 每屏顶部所在的 y 坐标（正文坐标），长度即页数 */
const tops = ref([0]);
/** 每屏首行的行号，长度与 tops 相同 —— 两者是同一条分页表的两种单位 */
const rowOfPage = ref([0]);
/** 每屏显示到第几行（下标不含）；最后一屏一路显示到正文末尾 */
const rowEndOfPage = ref([0]);
const pageIndex = ref(0);

const pages = computed(() => Math.max(tops.value.length, 1));

const pageInfo = computed(() => {
  const total = Math.max(offsets.value.length, 1);
  const shown = rowEndOfPage.value[pageIndex.value] ?? 0;
  const pct = Math.round((shown / total) * 100);
  return `第 ${pageIndex.value + 1}/${pages.value} 页 · ${Math.min(Math.max(pct, 0), 100)}%`;
});

/** 量不到真实行高时的兜底：自然行高 ≈ 字号 × 1.25 × 行高倍数 */
const fallbackLineHeight = () =>
  Math.max(fontSize.value * lineHeightFactor.value * 1.25, 1);

/**
 * 正文可视区尺寸：直接问浏览器。
 *
 * 量的是**滚动容器**的可视高度，不是正文段落的高度 —— 段落高度是整章的高度，
 * 拿它当可视区，一屏就把整章都算进去了。也不按页头页脚去减：
 * 自己减多减少一点，正好就是一行字。
 *
 * 量到的是「容器还剩多少高度可用」（页头页脚之间的那块），
 * 会再被裁到整行高的整数倍（见 `applyViewport`）——
 * 屏幕上沿那点零头必须消掉，不然它会露出下一行。
 */
function syncBodySize() {
  bodyWidth.value = window.innerWidth;
  // 容器高度已经被我们裁过（`applyViewport` 里设的），所以不能再问它要可用高度：
  // 那会越裁越小。可用高度 = 窗口高度 − 页头 − 页头下的缝 − 页脚。
  const available = Math.max(
    window.innerHeight -
      (barEl.value?.offsetHeight ?? 0) -
      TOP_GAP -
      (footEl.value?.offsetHeight ?? 0),
    1,
  );
  if (available !== rawHeight.value) {
    rawHeight.value = available;
  }
  applyViewport();
}

/**
 * 把滚动容器的可视高度裁到整行高的整数倍。
 *
 * 容器原本占满页头页脚之间的空间，那个高度一般不是行高的整数倍，
 * 多出来的零头正好露出下一行的上半截。裁到整行高的整数倍之后，
 * 下沿就是某一行的底边，一点字都不会多露。
 */
function applyViewport() {
  const lh = lineHeight.value > 0 ? lineHeight.value : 0;
  const clipped = lh > 0 ? clipHeight(rawHeight.value, lh) : rawHeight.value;
  bodyHeight.value = clipped;
  if (scrollerEl.value) scrollerEl.value.style.height = `${clipped}px`;
}

/**
 * 按当前窗口尺寸与字号重建行位置与分页表。
 *
 * 行位置由浏览器逐段量出来（`measureRows`，量不到就退回按行高估行）：
 * 「整章总高度 ÷ 行高」反推出来的行数只要多一行，屏幕下沿就会被切成半行字。
 * 每屏顶部落在量出来的行上，翻页就不会出现半行字。
 *
 * `keepY` 是重建后要停住的那个位置（正文 y），不给就停在当前屏顶部。
 */
function rebuild(keepY = null) {
  if (!textEl.value || !scrollerEl.value) return;
  const anchor = keepY ?? tops.value[pageIndex.value] ?? 0;

  const lh = probeLineHeight();
  lineHeight.value = lh > 0 ? lh : fallbackLineHeight();
  applyViewport();

  const measured = measuredRows();
  offsets.value = measured?.offsets ?? estimatedOffsets();
  rowChars.value = measured?.rowChars ?? estimatedRowChars(offsets.value.length);

  // 同一条分页表的两种单位：行号用来算百分比 / 记进度，y 用来滚到位置。
  // 位置必须落在实测出来的 y 上 —— 浏览器取整出来的行高会差一两个像素，
  // 按「行号 × 行高」去滚，翻几屏就会偏出半行。
  const perPage = rowsPerScreen(bodyHeight.value, lineHeight.value);
  // 相邻两屏重叠一行：下一屏的第一行就是上一屏看得见的最后一行，
  // 且同样贴着视口上沿 —— 翻页时上下文不断，也不会露出上一屏的残行。
  const stride = Math.max(perPage - 1, 1);
  rowOfPage.value = pageFirstRows(
    offsets.value.length,
    stride,
    lastReachableRow(),
  );
  rowEndOfPage.value = pageEnds(offsets.value.length, rowOfPage.value, perPage);
  tops.value = rowOfPage.value.map(
    (row) => offsets.value[Math.min(row, offsets.value.length - 1)] ?? 0,
  );
  const idx = tops.value.findIndex((t) => t >= anchor - 0.5);
  pageIndex.value = idx < 0 ? tops.value.length - 1 : idx;
  applyPage();
}

/**
 * 滚到底时还能当作屏顶的最下面的行。
 *
 * `scrollTop` 到内容末尾就停了，最多滚到「整章高度 − 可视高度」。末屏从更靠下的
 * 行开始就滚不到位，会被夹回来、末行跌出视口。把这条线换算成行号交给
 * `pageFirstRows`，让它决定末屏站哪儿。
 *
 * 内容装得下整章（`maxScroll <= 0`）时返回 `null`：不分页，也就没有这个限制。
 */
function lastReachableRow() {
  const scroller = scrollerEl.value;
  const lh = lineHeight.value;
  if (!scroller || !(lh > 0)) return null;
  const maxScroll = scroller.scrollHeight - bodyHeight.value;
  if (!(maxScroll > 0)) return null;
  return Math.floor(maxScroll / lh);
}

/** 探针量出的行高（量不到返回 0） */
function probeLineHeight() {
  const el = probeEl.value;
  if (!el) return 0;
  syncProbe();
  return lineHeightOf(el, 10);
}

/**
 * 量出每行顶边的 y，以及每行落在正文的第几个字。
 *
 * 优先逐段实测（浏览器说这一段占几行），量不出来再退回「整章高度 ÷ 行高」——
 * 后者的零头会凑出多一行，屏幕下沿就会切出半行字。
 */
function measuredRows() {
  const el = probeEl.value;
  if (!el) return null;
  syncProbe();
  return measureRows(el, text.value, lineHeight.value, MAX_ROWS_PER_CHAPTER);
}

/** 量不到逐行高度时的兜底：按行高铺满整章 */
function estimatedOffsets() {
  const lh = Math.max(lineHeight.value, 1);
  const h = Math.max(textEl.value?.offsetHeight ?? lh, lh);
  const n = Math.max(Math.round(h / lh), 1);
  return Array.from({ length: n }, (_, i) => i * lh);
}

/** 量不到逐行高度时的兜底：把整章字数摊到估出来的每一行 */
function estimatedRowChars(count) {
  const len = text.value.length;
  const per = Math.max(Math.ceil(len / Math.max(count, 1)), 1);
  return Array.from({ length: Math.max(count, 1) }, (_, i) =>
    Math.min(i * per, len),
  );
}

/** 探针调到与正文一模一样的排版：同宽、同字号、同行高 */
function syncProbe() {
  const el = probeEl.value;
  if (!el) return;
  el.style.width = `${Math.max(bodyWidth.value - SIDE_PAD * 2, 80)}px`;
  el.style.fontSize = `${fontSize.value}px`;
  el.style.lineHeight = String(lineHeightFactor.value);
}

/**
 * 当前屏顶部贴着正文可视区上沿。
 *
 * 位置是正文 y，不是行号 —— 浏览器取整出来的行高会差那么一两个像素，
 * 按行号 × 行高去滚，滚几屏就会偏出半行。
 *
 * 末屏例外：它要显示到正文末尾，得滚到能滚的最下面，否则末尾那几行看不到。
 * `scrollTop` 会自动夹到上限，这里直接把目标设成内容末尾就好。
 */
function applyPage() {
  if (!scrollerEl.value) return;
  const atLast = pageIndex.value >= pages.value - 1;
  scrollerEl.value.scrollTop = atLast
    ? scrollerEl.value.scrollHeight
    : (tops.value[pageIndex.value] ?? 0);
}

function gotoPage(index) {
  pageIndex.value = Math.min(Math.max(index, 0), pages.value - 1);
  applyPage();
  reportPosition();
  // 翻页就是读者的进度：停下这一屏就把它报给服务端。节流在前头，
  // 连着翻十屏只会发最后那一屏；真要能精确定位，靠的是收起时的收尾调用
  if (ready) scheduleSave(SAVE_THROTTLE_MS);
}

/**
 * 当前读到正文的第几个逻辑行（按 `\n` 分段），用来记住进度。
 *
 * 显示行 -> 逻辑行靠「每段占几个显示行」累加反推：正文是中文单栏排版，
 * 这个换算只在记进度时用，差一行不影响续读的体感。
 */
const currentLine = computed(() => {
  const topRows = rowOfPage.value[pageIndex.value] ?? 0;
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
 * 正在阅读的正文位置，以字数计 —— 也就是上报给服务端的 `durChapterPos`。
 *
 * 当前屏首行落在正文的第几个字就是它：第一屏是 0，第一屏有 20 个字，
 * 翻到第二屏就是 20。翻页时它跟着变，上报的才是读者真正停下的地方。
 */
const durChapterPos = computed(() => {
  const row = rowOfPage.value[pageIndex.value] ?? 0;
  return rowChars.value[row] ?? 0;
});

/**
 * 把「现在读到哪」告诉主窗口。
 *
 * 托盘菜单的「退出」要先收掉阅读窗口才能把进度写回去，而那时它手上
 * 只有主窗口，所以位置得随时同步过去。
 */
function reportPosition() {
  send("reader://position", {
    bookIndex: bookIndex.value,
    chapterIndex: chapterIndex.value,
    title: title.value,
    line: currentLine.value,
    pos: durChapterPos.value,
  }).catch(() => {});
}

/** 手上这本书读到哪了；还没拿到书（窗口刚起来）就不上报 */
function progressArgs() {
  if (bookIndex.value < 0 || !text.value) return null;
  return {
    bookIndex: bookIndex.value,
    chapterIndex: chapterIndex.value,
    chapterTitle: title.value,
    line: currentLine.value,
    pos: durChapterPos.value,
  };
}

let saveTimer = null;
/** 装载完成之前别上报：那时行号还是空的，报上去就是把进度抹了 */
let ready = false;

/** 攒一下再报：翻页是连续动作，报得太勤就是给服务端刷无效请求 */
function scheduleSave(delay = SAVE_THROTTLE_MS) {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    saveProgressNow();
  }, delay);
}

function saveProgressNow() {
  if (saveTimer) {
    clearTimeout(saveTimer);
    saveTimer = null;
  }
  const args = progressArgs();
  if (!args) return Promise.resolve();
  return saveProgress(args).catch((e) => console.warn(`保存进度失败: ${e}`));
}

/**
 * 这一章从哪一屏读起。
 *
 * 换章都是从头（第 0 屏）；其余按服务端给的那个字数（`durChapterPos`）
 * 找到 pos 所在的那个显示行，落到它所在那一屏 —— 存的是字数而不是行号，
 * 换字号、换窗口大小也不会对不上。每次打开（含同一章再点一次「阅读」）
 * 都会走一遍这里，服务端的位置变了就跟着变。
 */
function startPage() {
  if (pendingLastPage.value) return pages.value - 1;
  const pos = resumePos.value;
  if (!(pos > 0)) return 0;
  const chars = rowChars.value;
  // 位置落在正文之外（比如章节内容改短了）就从头读起，别把读者直接扔到末屏
  if (!chars.length || pos > chars[chars.length - 1]) return 0;
  // 先找 pos 落在哪个显示行：顶部不超过 pos 的最后一行（不是第一行不早于它的，
  // 那会是下一行的开头，边界上要多跳一屏）。再把它映射到那一屏
  return pageOfRow(rowOfPage.value, pageOfRow(chars, pos));
}

async function load(payload) {
  if (!payload) return;
  bookIndex.value = payload.bookIndex ?? -1;
  chapterIndex.value = payload.chapterIndex ?? 0;
  title.value = payload.chapterTitle || "";
  resumePos.value = payload.durChapterPos ?? 0;
  const keepPage = pageIndex.value;
  const sameText = styleTick.value > 0 && payload.styleTick === styleTick.value;
  text.value = payload.text || "";
  styleTick.value = payload.styleTick ?? 0;
  pendingLastPage.value = (payload.startLine ?? 0) > 1_000_000;

  // 每次展示都重新读一遍配置：设置页改完样式会重新交一次手，这里就是生效的地方
  applyStyle(await getConfig().catch(() => ({})));
  await nextTick();
  // 只是重刷样式（正文没换，也不是要重新打开）：留在原来那一屏，别把读者踢回页首。
  // 同一本同一章再点一次「阅读」也算重新打开 —— 服务端记的位置可能已经在别处
  // 改过了，那一次要按新位置重新定页（见 `startPage`）
  if (sameText && !payload.reposition) {
    rebuild();
    pageIndex.value = Math.min(keepPage, pages.value - 1);
    applyPage();
    return;
  }
  rebuild();
  gotoPage(startPage());
  // 窗口刚打开 / 刚换章：读者停在这一屏就算进度。延迟一点报，
  // 免得刚弹出还没看就被记成「读到这儿」—— 真读起来后的那次翻页会把它盖掉
  ready = true;
  scheduleSave(SHOW_DELAY_MS);
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

/** 鼠标按下的键位掩码（事件上的 `buttons`），0 表示没按 */
let heldButtons = 0;
/** 上一次看见「有键按着」的时刻 */
let heldAt = 0;
/** 上一次窗口尺寸真的变了的时刻 */
let resizedAt = 0;

/** 鼠标事件上带着按下键的位掩码，顺手记一笔 —— 比自己数 mousedown / mouseup 稳：
 *  在窗口外松手收不到 mouseup，但接下来总会有 mousemove 把状态带回来 */
function noteButtons(event) {
  if (typeof event?.buttons !== "number") return;
  heldButtons = event.buttons;
  if (event.buttons) heldAt = performance.now();
}

/** 尺寸变了就记一笔：拉伸窗口的那几秒里，系统发的失焦不能当真 */
function onWindowResize() {
  resizedAt = performance.now();
}

/**
 * 正在摆弄窗口：这时失焦是拖拽 / 拉伸过程的一部分，不该把窗口关掉。
 *
 * Windows 上拉伸窗口一定会发一次失焦、拖拽也常发，只看失焦会把读者的窗口收没，
 * 所以「还按着鼠标」「尺寸刚变过」「正在拖标题条」都算进去。
 */
function interacting() {
  const now = performance.now();
  if (dragging.value) return true;
  if (heldButtons && now - heldAt < BUTTON_HELD_MAX_MS) return true;
  return now - resizedAt < INTERACT_GRACE_MS;
}

/**
 * 收起：让主窗口把进度收干净（写回服务端 + 本地记行号），然后藏起自己。
 *
 * 全局快捷键按第二下、托盘「收起」都落到这里，所以加个闸：
 * 收起过程中又按了一下，不能两条路径同时去写进度。
 */
let closing = null;
/** 刚刚收起过。收起后全局快捷键可能还按着，这段时间里不接受失焦自动收起 */
let closedAt = 0;

async function close() {
  if (closing) return closing;
  closing = doClose()
    .then((done) => {
      if (done) closedAt = performance.now();
      return done;
    })
    .finally(() => {
      closing = null;
    });
  return closing;
}

/**
 * 真正收起：成功返回 true。
 *
 * 收起前先把手上这屏的进度同步过去（`hide: false`）—— 翻页上报是节流的，
 * 读者可能刚翻两屏就按 Esc，最后那两屏还在攒着没发。等服务端收下了再叫它把窗口藏了
 * （`hide: true`）：反过来的话「藏」是同步的、写进度是异步的，可能还没写完就被托盘退出带走。
 * 正在拖窗口时不上报也不收：拖拽过程中要经过失焦。
 */
async function doClose() {
  if (interacting()) return false;
  const args = progressArgs();
  const payload = {
    chapterIndex: args?.chapterIndex ?? chapterIndex.value,
    chapterTitle: args?.chapterTitle ?? title.value,
    line: args?.line ?? currentLine.value,
    pos: durChapterPos.value,
  };
  try {
    // 先去重掉还在攒着的那次翻页上报（这一下会一起写掉），再同步 + 收起
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = null;
    await closeReader({ ...payload, hide: false });
    await closeReader({ ...payload, hide: true });
    return true;
  } catch (e) {
    console.warn(String(e));
    return false;
  }
}

/** 待执行的失焦收起 */
let blurTimer = null;

function cancelBlurClose() {
  if (blurTimer) clearTimeout(blurTimer);
  blurTimer = null;
}

/** 焦点回来了：刚才那次失焦是拖拽 / 拉伸抖出来的，不收 */
function onFocus() {
  cancelBlurClose();
}

/** 到点了再看一眼：还在摆弄窗口就再缓一轮，等松手、尺寸稳下来才真收 */
function checkBlurClose() {
  blurTimer = null;
  // 焦点已经回来了（没派发 focus 事件的情形）也算数
  if (document.hasFocus()) return;
  if (interacting()) {
    blurTimer = setTimeout(checkBlurClose, BLUR_CLOSE_DELAY_MS);
    return;
  }
  close();
}

/**
 * 失焦收起。
 *
 * 有些窗口管理器在隐藏窗口之后再补一次失焦，这时不能当成读者主动关闭；
 * 拖拽 / 拉伸窗口时系统也会发一次失焦，所以不立刻收，先缓一缓：
 * 焦点回来了就取消，到点还在摆弄窗口就继续等。
 */
function blurClose() {
  if (performance.now() - closedAt < CLOSE_GUARD_MS) return;
  if (blurTimer) return;
  blurTimer = setTimeout(checkBlurClose, BLUR_CLOSE_DELAY_MS);
}

// 失焦收起是配置项，开关可能中途被改（设置页保存后阅读窗口会重读配置），
// 所以跟着 `closeOnBlur` 挂 / 摘监听，而不是只在装载时挂一次
watch(
  closeOnBlur,
  (on) => {
    if (on) {
      window.addEventListener("blur", blurClose);
      window.addEventListener("focus", onFocus);
    } else {
      cancelBlurClose();
      window.removeEventListener("blur", blurClose);
      window.removeEventListener("focus", onFocus);
    }
  },
  { immediate: true },
);

async function onKeydown(event) {
  // 带修饰键的组合（默认 Alt+PgDn 就是那个呼出 / 收起的全局快捷键）不在这里动：
  // 它由后端的全局快捷键统一开关接管。这里再处理一次的话，同一次按键会走两条路 ——
  // 一条把窗口收起，另一条看见窗口已经收起又把它呼出来，就成了「关掉又自己打开」。
  if (event.ctrlKey || event.altKey || event.metaKey) return;
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
  // 先记上「正在拖」：问窗口位置是异步的，那一下的失焦可能比回答先到
  dragging.value = true;
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
}

async function onDragMove(event) {
  if (!dragOrigin) return;
  // 在窗口外松手收不到 mouseup，等指针回来时顺手把拖动收了 ——
  // 不然 `dragging` 一直挂着，之后就再也不会失焦自动收起了
  if (!event.buttons) return onDragEnd();
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
  // 旧配置里没有这一项（`undefined`）：按默认关掉，别让它变成真
  closeOnBlur.value = cfg.closeOnBlur === true;
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
  // 手上没有正文时，样式改了只重读配置
  stopStyle = await on("reader://style", async (event) => {
    styleTick.value = event.payload ?? styleTick.value;
    applyStyle(await getConfig().catch(() => ({})));
    await nextTick();
    rebuild();
  });

  applyStyle(await getConfig().catch(() => ({})));
  syncBodySize();
  // 窗口尺寸变了就按新尺寸重算分页；正文本身高度变化也会触发，rebuild 是幂等的
  observer = new ResizeObserver(() => {
    const prevW = bodyWidth.value;
    const prevH = rawHeight.value;
    syncBodySize();
    if (prevW === bodyWidth.value && prevH === rawHeight.value) return;
    rebuild();
    schedulePersist();
  });
  observer.observe(document.documentElement);
  if (textEl.value) observer.observe(textEl.value);

  // 先取一次：窗口是异步起来的，后端建窗口时发的那个事件我们可能没赶上
  const initial = await takePending().catch(() => null);
  if (initial) await load(initial);
  // 首次装载也记一次：窗口被系统缩放（HiDPI）或用户从不拉伸时，
  // 配置里也该有真实尺寸，下次打开才不会大小跳一下
  await persistSize();
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("mouseup", onDragEnd);
  window.addEventListener("resize", onWindowResize);
  for (const type of MOUSE_TYPES) {
    window.addEventListener(type, noteButtons, { capture: true });
  }
});

onUnmounted(() => {
  stopLoad?.();
  stopToggle?.();
  stopStyle?.();
  observer?.disconnect();
  cancelBlurClose();
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("blur", blurClose);
  window.removeEventListener("focus", onFocus);
  window.removeEventListener("mouseup", onDragEnd);
  window.removeEventListener("resize", onWindowResize);
  for (const type of MOUSE_TYPES) {
    window.removeEventListener(type, noteButtons, { capture: true });
  }
  if (resizeTimer) clearTimeout(resizeTimer);
  if (saveTimer) clearTimeout(saveTimer);
});

// 字号与行高倍数直接决定行高与整章行数，变了必须重排；颜色只走 CSS 变量，不用动排版
watch([fontSize, lineHeightFactor], () => {
  nextTick(() => rebuild());
});
</script>

<template>
  <div class="frame" @mousemove="onDragMove($event)">
    <header
      ref="barEl"
      class="bar"
      @mousedown="onDragStart"
      @mouseup="onDragEnd"
    >
      <span class="grip">⠿ {{ title }}</span>
      <button class="close" @click="close">✕</button>
    </header>

    <!-- 整章正文一次铺满，没有可见滚动条；翻页靠 scrollTop 对到行号 -->
    <div
      ref="scrollerEl"
      class="body"
      :style="{ '--fs': `${fontSize}px`, '--lh': lineHeightFactor }"
    >
      <p ref="textEl" class="text">{{ text }}</p>
    </div>

    <footer ref="footEl" class="foot">
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
  /* 标题条与正文之间的缝用 margin 留（TOP_GAP），不进正文内边距 ——
     内边距会让「scrollTop = 行顶坐标」对不上，末行被裁、下一屏露残行 */
  margin-bottom: 6px;
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
  /* 高度由脚本按整行高裁出来（applyViewport），这里不参与 flex 分配 ——
     让 flex 去撑的话，容器会比「整行高的整数倍」高出一截，
     多出来的那点正好露出下一行的上半截 */
  flex: 0 0 auto;
  overflow: auto;
  padding: 0 18px;
  scrollbar-width: none;
}

.body::-webkit-scrollbar {
  display: none;
}

.text {
  margin: 0;
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
  /* 正文高度裁到整行倍数后，100vh 里难免剩一点零头：吸到窗口底部，
     状态栏位置就不跟着正文高度晃 */
  margin-top: auto;
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
