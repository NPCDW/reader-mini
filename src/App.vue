<script setup>
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import {
  call,
  getBookshelf,
  getChapterList,
  cacheBooks,
  getConfig,
  hideReader,
  on,
  openReader,
  refreshReaderStyle,
  saveConfig,
  send,
  setCurrentBook,
  showReader,
} from "./bridge";
import SettingsPage from "./components/SettingsPage.vue";
import { continueIndex, findBook } from "./bookshelf";

const tab = ref("shelf");
const page = ref("books");
const books = ref([]);
const chapters = ref([]);
/**
 * 最后一本读过的书，按 `bookUrl` 记 —— 不能记下标。
 *
 * 书架会被远端按阅读时间重排，收起阅读窗口后又会悄悄刷一次书架，
 * 手上那个下标早就指到别的书上去了。快捷键续读要是照下标取，
 * 就会「打开 A、收起、打开 B」轮流出现（见 ISSUE #12）。
 */
const currentBookUrl = ref("");
const loading = ref(false);
/** 正在打开的书的 bookUrl：后端要先联网取正文，这一下不是抬手就有的 */
const openingUrl = ref("");
const status = ref("");

const config = ref(null);
/** 设置保存后递增，让已开着的阅读窗口重新读一遍样式 */
const styleTick = ref(0);
/** 阅读窗口最近一次回传的位置，收起 / 退出时用它把进度收干净 */
const readerState = reactive({
  bookUrl: "",
  chapterIndex: 0,
  title: "",
  line: 0,
  pos: 0,
});
let pollTimer = null;
let stopTick = null;
let stopPos = null;
let stopClosed = null;
let stopFailed = null;

// 与 Rust 侧 Config::default() 同一套默认值
const DEFAULT_CONFIG = {
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
  readerWidth: 460,
  readerHeight: 560,
  // 阅读窗口失焦即收起是个容易误伤的行为（点一下别处窗口就没了），默认关掉
  closeOnBlur: false,
};

/** 版本号：侧栏底下那一行小字，跟 `package.json` / `tauri.conf.json` 同一个数 */
const version = "0.6.4";

/** 侧栏上「正在阅读」的小灯：有书正在读就亮，点一下把阅读窗口叫回来 */
const reading = computed(
  () => readerState.bookUrl !== "" || currentBookUrl.value !== "",
);

/** 当前目录属于哪本书（目录页标题旁那行小字） */
const currentBookName = computed(
  () => findBook(books.value, currentBookUrl.value)?.name ?? "",
);

async function bootstrap() {
  try {
    config.value = { ...DEFAULT_CONFIG, ...(await getConfig()) };
  } catch {
    config.value = { ...DEFAULT_CONFIG };
  }
  await refresh();
}

/**
 * 拉一次书架。
 *
 * `quiet` 是收起阅读窗口后自动刷的那一次：它不写状态栏 —— 那是后台刷新，
 * 读者要看到的还是「已收起《…》」，不是「书架已更新」。
 */
async function refresh(quiet = false) {
  loading.value = true;
  try {
    const list = await getBookshelf();
    books.value = list;
    await cacheBooks(list);
    if (!quiet) status.value = "书架已更新";
  } catch (e) {
    if (!quiet) status.value = `获取书架失败: ${e}`;
  } finally {
    loading.value = false;
  }
}

/** 打开某本书：带上这本书 + 上次读到的章节与行号 */
async function read(idx) {
  if (idx < 0 || idx >= books.value.length) return;
  // 认书一律用 bookUrl：远端可能刚把书架重排过，这个下标指的是眼前这份列表，
  // 等它传到后端时可能已经指到别的书上了
  const book = books.value[idx];
  if (openingUrl.value) return;
  currentBookUrl.value = book.bookUrl;
  // 这几步都要走网络（问续读点、取正文），等的时候按钮上给个「加载中」
  openingUrl.value = book.bookUrl;
  status.value = `正在打开《${book.name}》…`;
  try {
    await setCurrentBook(book.bookUrl);
    const point = await call("resume_point", { bookUrl: book.bookUrl }).catch(
      () => null,
    );
    const chapterIndex = point?.chapterIndex ?? book.durChapterIndex ?? 0;
    const title = point?.chapterTitle || book.durChapterTitle || book.name;
    // 「阅读」不是一个开关：窗口关着就打开，开着就把这本交到开着的那一个上，
    // 不会把正在读的窗口收掉（收起是快捷键 / 托盘那一下的事，见 `handleToggle`）
    await openReader({
      bookUrl: book.bookUrl,
      chapterIndex,
      chapterTitle: title,
      startLine: point?.startLine ?? 0,
      styleTick: styleTick.value,
    });
    status.value = `正在阅读《${book.name}》`;
  } catch (e) {
    status.value = `打开阅读窗口失败: ${e}`;
  } finally {
    openingUrl.value = "";
  }
}

/** 目录里点某章：从该章正文最开始读起 */
async function openChapter(chapterIndex) {
  // 目录属于哪本书同样按 bookUrl 认：这份目录是打开时拉的，
  // 期间书架可能已经被重排过
  const book = findBook(books.value, currentBookUrl.value);
  if (!book) {
    status.value = "请先从书架打开一本书的目录";
    return;
  }
  const ch = chapters.value[chapterIndex];
  // 目录下标就是 getBookContent 的 index，不能因为空标题重新编号
  const title = ch?.title || `第${chapterIndex + 1}章`;
  page.value = "books";
  // 后端要联网取这一章的正文，等的时候目录上给个「加载中」
  openingUrl.value = book.bookUrl;
  status.value = `正在打开《${book.name}》…`;
  try {
    // 从目录进是按章节读，阅读窗口开着就带上新样式一起刷新
    await openReader({
      bookUrl: book.bookUrl,
      chapterIndex,
      chapterTitle: title,
      startLine: 0,
      styleTick: styleTick.value,
    });
    status.value = `正在阅读《${book.name}》`;
  } catch (e) {
    status.value = `打开阅读窗口失败: ${e}`;
  } finally {
    openingUrl.value = "";
  }
}

async function openToc(idx) {
  currentBookUrl.value = books.value[idx].bookUrl;
  await setCurrentBook(books.value[idx].bookUrl);
  page.value = "toc";
  loading.value = true;
  try {
    chapters.value = await getChapterList(books.value[idx].bookUrl);
  } catch (e) {
    status.value = `获取目录失败: ${e}`;
    chapters.value = [];
  } finally {
    loading.value = false;
  }
}

/**
 * 托盘「继续阅读」/ 全局快捷键：同一套续读规则。
 *
 * 窗口只是被收起了（收起是 `hide`，内容还在它自己那里）时直接把它叫出来：
 * 不再走 `open_reader` —— 那一路要联网取正文（窗口得先摆「加载中」），
 * 还会按服务端记的位置把读者挪走，而读者要的就是刚才那一屏。
 */
async function continueReading() {
  if (await showReader().catch(() => false)) {
    status.value = "已呼出阅读窗口";
    return;
  }
  if (!books.value.length) {
    status.value = "书架还没加载出来";
    return;
  }
  // 认书只认 bookUrl，下标只在眼前这份列表里临时算一下。
  // 记住的下标会在书架被重排后指到别的书上 —— 那正是「两本书轮流出现」的来源
  await read(continueIndex(books.value, currentBookUrl.value));
}

/** 点顶栏那颗「正在阅读」小灯：窗口开着就收起，收着就叫出来 */
async function toggleReader() {
  const name = readerState.bookUrl ? "close" : "reader";
  await handleToggle(name);
}

async function handleToggle(name) {
  if (name === "reader") {
    await continueReading();
    return;
  }
  // 收起只是把窗口藏起来：窗口手上的书、章、第几屏全都还在，再按一下就是原样。
  // 不走 `close_reader`：那一路要联网写进度、写完再让主窗口刷一次书架，
  // 读者按完收起还得等这一圈。翻页本来就上报过，还攒着的那一次让窗口自己发掉
  if (name === "close") {
    send("reader://flush").catch(() => {});
    await hideReader().catch(() => {});
    return;
  }
  // 退出要把进度收干净：这时窗口可能已经没了，手上这份位置就是唯一线索
  if (name === "quit") {
    await call("close_reader", {
      chapterIndex: readerState.chapterIndex,
      chapterTitle: readerState.title,
      line: readerState.line,
      pos: readerState.pos ?? 0,
      hide: true,
    }).catch(() => {});
    await call("quit_app").catch(() => {});
  }
}

async function saveSettings(next) {
  try {
    config.value = {
      ...DEFAULT_CONFIG,
      ...(await saveConfig({ ...config.value, ...next })),
    };
    // 背景色 / 字色 / 字号 / 行高改了，已经开着的阅读窗口也得换上新样式：
    // 递增标记 + 重新呼出一次，阅读窗口收到就原地重排
    styleTick.value += 1;
    await refreshReaderStyle(styleTick.value).catch(() => {});
    status.value = `设置已保存并已应用；快捷键已生效：${config.value.hotkey}`;
    return "";
  } catch (e) {
    const msg = String(e);
    status.value = msg;
    return msg;
  }
}

onMounted(async () => {
  await bootstrap();
  // 托盘菜单与全局快捷键的信号都落在同一条队列里。
  // 后端在入队时会发一个 `reader://tick` 提示，但定时轮询才是取走信号的地方 ——
  // 只靠事件会漏（窗口还没起来时发的就丢了），只靠轮询又慢，
  // 所以两个都留着，轮询兜底。
  pollTimer = setInterval(async () => {
    const name = await call("poll_toggle").catch(() => null);
    if (name) await handleToggle(name);
  }, 300);
  stopTick = await on("reader://tick", async () => {
    const name = await call("poll_toggle").catch(() => null);
    if (name) await handleToggle(name);
  });
  // 阅读窗口每次定位都回传一下「读到哪」，退出时才有东西可写
  stopPos = await on("reader://position", (event) => {
    Object.assign(readerState, event.payload ?? {});
  });
  // 阅读窗口收起、后端把进度写进服务端之后才发这个：这时拉一次书架，
  // 书架上「读至第几章」才是刚读到的那一章
  stopClosed = await on("reader://closed", () => {
    refresh(true);
  });
  // 窗口是立刻叫出来的、正文在后台取：取不到由后端回头说一声，写进状态栏
  stopFailed = await on("reader://failed", (event) => {
    status.value = String(event.payload ?? "取正文失败");
  });
});

onUnmounted(() => {
  stopTick?.();
  stopPos?.();
  stopClosed?.();
  stopFailed?.();
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<template>
  <div class="shell">
    <aside class="rail">
      <div class="brand">
        <span class="mark" aria-hidden="true">📖</span>
        <span class="name">reader-mini</span>
      </div>
      <nav class="nav">
        <button
          :class="['nav-item', { on: tab === 'shelf' }]"
          @click="tab = 'shelf'"
        >
          <span class="ico" aria-hidden="true">▤</span>
          <span>书架</span>
          <span class="badge">{{ books.length }}</span>
        </button>
        <button
          :class="['nav-item', { on: tab === 'settings' }]"
          @click="tab = 'settings'"
        >
          <span class="ico" aria-hidden="true">⚙</span>
          <span>设置</span>
        </button>
      </nav>
      <div class="rail-foot">
        <button class="ghost" :disabled="loading" @click="refresh()">
          {{ loading ? "刷新中…" : "刷新书架" }}
        </button>
        <p class="ver">v{{ version }}</p>
      </div>
    </aside>

    <div class="body">
      <header class="topbar">
        <h1>{{ tab === "shelf" ? "我的书架" : "设置" }}</h1>
        <p v-if="tab === 'shelf'" class="crumb">
          {{ page === "books" ? `${books.length} 本书` : currentBookName }}
        </p>
        <span class="grow" />
        <span v-if="reading" class="chip" @click="toggleReader">
          <i class="dot" aria-hidden="true" />正在阅读
        </span>
      </header>

      <main v-if="tab === 'shelf'" class="content">
        <template v-if="page === 'books'">
          <article v-for="(book, i) in books" :key="book.bookUrl || i" class="book">
            <div class="cover" aria-hidden="true">
              {{ (book.name || "?").slice(0, 1) }}
            </div>
            <div class="info">
              <h3>{{ book.name }}</h3>
              <p class="meta">{{ book.author }}</p>
              <p class="progress">
                <span>读至 {{ book.durChapterIndex }} 章</span>
                <span class="sep">·</span>
                <span class="ell">{{ book.durChapterTitle }}</span>
              </p>
              <p class="intro">{{ book.intro }}</p>
            </div>
            <div class="ops">
              <button
                class="primary"
                :disabled="openingUrl !== ''"
                @click="read(i)"
              >
                <span
                  v-if="openingUrl === book.bookUrl"
                  class="spinner"
                  aria-hidden="true"
                />
                {{ openingUrl === book.bookUrl ? "加载中…" : "阅读" }}
              </button>
              <button class="plain" @click="openToc(i)">目录</button>
            </div>
          </article>
          <p v-if="!books.length" class="empty">
            {{
              loading
                ? "正在获取书架…"
                : "书架为空：请先在「设置」里填写 baseUrl，再刷新"
            }}
          </p>
        </template>

        <template v-else>
          <div class="toc-head">
            <button class="plain" @click="page = 'books'">← 返回书架</button>
            <strong>目录</strong>
            <span class="grow" />
            <span class="crumb">{{ chapters.length }} 章</span>
          </div>
          <ul class="toc">
            <li v-for="(ch, i) in chapters" :key="i" @click="openChapter(i)">
              <span class="no">{{ i + 1 }}</span>
              <span class="t">{{ ch.title }}</span>
            </li>
          </ul>
          <p v-if="!chapters.length" class="empty">暂无目录</p>
        </template>
      </main>

      <SettingsPage v-else :config="config" :on-save="saveSettings" />

      <footer class="statusbar">
        <span :class="['status', { busy: loading }]">{{ status || "就绪" }}</span>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  height: 100%;
  /* 主窗口是两栏：左边窄导航，右边内容。整窗不滚，只有内容区滚 */
  overflow: hidden;
}

/* ---- 左侧导航栏 ---- */
.rail {
  display: flex;
  flex-direction: column;
  flex: 0 0 200px;
  padding: 16px 12px;
  background: #1a1c21;
  border-right: 1px solid #2b2f37;
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 6px 16px;
}

.mark {
  font-size: 18px;
}

.name {
  font-size: 15px;
  font-weight: 600;
  color: #e9ebee;
  letter-spacing: 0.02em;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 38px;
  padding: 0 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: #a2a9b4;
  font: inherit;
  font-size: 14px;
  cursor: pointer;
  text-align: left;
}

.nav-item:hover {
  background: #22252b;
  color: #d6d9de;
}

.nav-item.on {
  background: #2b3a52;
  color: #fff;
}

.ico {
  width: 18px;
  text-align: center;
  opacity: 0.8;
}

.badge {
  margin-left: auto;
  padding: 0 6px;
  height: 18px;
  line-height: 18px;
  border-radius: 9px;
  background: #2a2d34;
  color: #838994;
  font-size: 11px;
}

.nav-item.on .badge {
  background: #3d5a80;
  color: #dbe7f5;
}

.rail-foot {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.ghost {
  height: 34px;
  border: 1px solid #33383f;
  border-radius: 8px;
  background: transparent;
  color: #a2a9b4;
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.ghost:hover:not(:disabled) {
  background: #22252b;
  color: #d6d9de;
}

.ghost:disabled {
  opacity: 0.6;
  cursor: default;
}

.ver {
  margin: 0;
  padding-left: 2px;
  font-size: 11px;
  color: #5d626d;
}

/* ---- 右侧内容 ---- */
.body {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}

.topbar {
  display: flex;
  align-items: baseline;
  gap: 12px;
  padding: 16px 20px 12px;
  border-bottom: 1px solid #24272d;
}

.topbar h1 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #e9ebee;
}

.crumb {
  margin: 0;
  font-size: 12px;
  color: #6f757f;
}

.grow {
  flex: 1;
}

/* 「正在阅读」小灯：点一下把收起来的阅读窗口叫回来 */
.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 10px;
  border-radius: 12px;
  background: #23303f;
  color: #7ea6dd;
  font-size: 12px;
  cursor: pointer;
}

.chip:hover {
  background: #2b3a52;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #7ea6dd;
}

.content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 16px 20px;
}

/* ---- 书架卡片 ---- */
.book {
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 14px;
  margin-bottom: 10px;
  background: #1e2026;
  border: 1px solid #2b2f37;
  border-radius: 10px;
  transition: border-color 0.15s ease;
}

.book:hover {
  border-color: #3d5a80;
}

/* 书名首字当封面：没有封面图，一个字比空白像个书 */
.cover {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 44px;
  height: 58px;
  border-radius: 6px;
  background: linear-gradient(160deg, #3d5a80, #2b3a52);
  color: #dbe7f5;
  font-size: 22px;
  font-weight: 600;
}

.info {
  flex: 1;
  min-width: 0;
}

.info h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: #e9ebee;
}

.meta,
.progress,
.intro {
  margin: 4px 0 0;
  font-size: 12px;
}

.meta {
  color: #868c96;
}

.progress {
  display: flex;
  align-items: center;
  gap: 6px;
  color: #7ea6dd;
}

.sep {
  color: #4a4e57;
}

.ell {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.intro {
  color: #9aa0aa;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.ops {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 96px;
  flex: 0 0 auto;
}

button.primary,
button.plain {
  height: 32px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}

button.primary {
  background: #4a6fa5;
  color: #fff;
}

button.plain {
  background: #2a2d34;
  color: #c9cdd4;
}

button.primary:hover:not(:disabled),
button.plain:hover:not(:disabled) {
  filter: brightness(1.15);
}

button.primary:disabled {
  opacity: 0.75;
  cursor: default;
}

/* 按钮里那个转圈：取正文要联网，等的时候让按钮自己说在忙 */
button.primary .spinner {
  display: inline-block;
  width: 11px;
  height: 11px;
  margin-right: 6px;
  vertical-align: -1px;
  border: 2px solid currentColor;
  /* 留一道缺口，转起来才看得出在动 */
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* ---- 目录 ---- */
.toc-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.toc-head button {
  width: 100px;
  height: 30px;
  border: none;
  border-radius: 6px;
  background: #2a2d34;
  color: #c9cdd4;
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.toc-head button:hover {
  filter: brightness(1.15);
}

.toc-head strong {
  color: #e9ebee;
}

.toc {
  margin: 0;
  padding: 0;
  list-style: none;
}

.toc li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  margin-bottom: 4px;
  background: #1e2026;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  color: #c9cdd4;
}

.toc li:hover {
  background: #2b3037;
  color: #e9ebee;
}

.toc .no {
  flex: 0 0 28px;
  color: #5d626d;
  font-size: 11px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.toc .t {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.empty {
  padding: 48px 0;
  text-align: center;
  color: #767c86;
  font-size: 13px;
}

/* ---- 状态栏 ---- */
.statusbar {
  display: flex;
  align-items: center;
  height: 28px;
  padding: 0 20px;
  border-top: 1px solid #24272d;
  background: #1a1c21;
}

.status {
  font-size: 12px;
  color: #7ea6dd;
}

.status.busy {
  color: #d8b46a;
}
</style>
