<script setup>
import { onMounted, onUnmounted, reactive, ref } from "vue";
import {
  call,
  getBookshelf,
  getChapterList,
  cacheBooks,
  getConfig,
  on,
  openReader,
  refreshReaderStyle,
  saveConfig,
  setCurrentBook,
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

// 与 Rust 侧 Config::default() 同一套默认值
const DEFAULT_CONFIG = {
  baseUrl: "http://127.0.0.1:1122",
  readBg: "#181818",
  readFg: "#bdbdbd",
  readFontSize: 20,
  readLineHeight: 1.5,
  hotkey: "Alt+PgDn",
  readerWidth: 460,
  readerHeight: 560,
  // 阅读窗口失焦即收起是个容易误伤的行为（点一下别处窗口就没了），默认关掉
  closeOnBlur: false,
};

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

/** 托盘「继续阅读」/ 全局快捷键：同一套续读规则 */
async function continueReading() {
  if (!books.value.length) {
    status.value = "书架还没加载出来";
    return;
  }
  // 认书只认 bookUrl，下标只在眼前这份列表里临时算一下。
  // 记住的下标会在书架被重排后指到别的书上 —— 那正是「两本书轮流出现」的来源
  await read(continueIndex(books.value, currentBookUrl.value));
}

async function handleToggle(name) {
  if (name === "reader") {
    await continueReading();
    return;
  }
  // 「收起」与「退出」都要先把阅读窗口的进度收干净：
  // 快捷键收起直接叫窗口自己收（它手上有准确的章节与行号），
  // 退出时窗口可能已经没了，就用手上这份位置兜底
  if (name === "close" || name === "quit") {
    // 阅读窗口自己收起时会先把进度写完、再叫后端藏窗口，到这里通常已经收干净了；
    // 这一次是兜底：窗口可能已经没了（退出前先被带走），手上这份位置就是唯一线索
    await call("close_reader", {
      chapterIndex: readerState.chapterIndex,
      chapterTitle: readerState.title,
      line: readerState.line,
      pos: readerState.pos ?? 0,
      hide: true,
    }).catch(() => {});
  }
  if (name === "quit") await call("quit_app").catch(() => {});
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
});

onUnmounted(() => {
  stopTick?.();
  stopPos?.();
  stopClosed?.();
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<template>
  <div class="shell">
    <header class="tabs">
      <button :class="['tab', { on: tab === 'shelf' }]" @click="tab = 'shelf'">
        书架
      </button>
      <button
        :class="['tab', { on: tab === 'settings' }]"
        @click="tab = 'settings'"
      >
        设置
      </button>
      <span class="spacer" />
      <button class="tab ghost" :disabled="loading" @click="refresh()">
        {{ loading ? "加载中…" : "刷新" }}
      </button>
    </header>

    <main v-if="tab === 'shelf'" class="shelf">
      <template v-if="page === 'books'">
        <article
          v-for="(book, i) in books"
          :key="book.bookUrl || i"
          class="book"
        >
          <div class="info">
            <h3>{{ book.name }}</h3>
            <p class="meta">
              {{ book.author }} · 最新 {{ book.latestChapterTitle }}
            </p>
            <p class="progress">
              读至 {{ book.durChapterIndex }} 章 · {{ book.durChapterTitle }}
            </p>
            <p class="intro">{{ book.intro }}</p>
          </div>
          <div class="actions">
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
              : "书架为空：请先在「设置」里填写 baseUrl，再点刷新"
          }}
        </p>
      </template>

      <template v-else>
        <div class="toc-head">
          <button class="plain" @click="page = 'books'">← 返回</button>
          <strong>目录</strong>
        </div>
        <ul class="toc">
          <li v-for="(ch, i) in chapters" :key="i" @click="openChapter(i)">
            {{ ch.title }}
          </li>
        </ul>
        <p v-if="!chapters.length" class="empty">暂无目录</p>
      </template>
    </main>

    <SettingsPage v-else :config="config" :on-save="saveSettings" />

    <footer class="status">{{ status }}</footer>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.tabs {
  display: flex;
  gap: 6px;
  padding: 6px;
  align-items: center;
}

.tab {
  width: 88px;
  height: 36px;
  border: none;
  border-radius: 6px;
  background: #2a2d34;
  color: #c9cdd4;
  cursor: pointer;
  font-size: 14px;
}

.tab:hover:not(:disabled) {
  background: #33373f;
}

.tab:disabled {
  opacity: 0.5;
  cursor: default;
}

.tab.on {
  background: #4a6fa5;
  color: #fff;
}

.tab.ghost {
  margin-left: auto;
}

.spacer {
  flex: 1;
}

.shelf {
  flex: 1;
  /* flex 项默认不肯矮过内容（`min-height: auto`），不给 0 就会把外壳顶高，
     外面那条滚动条就是它顶出来的 */
  min-height: 0;
  overflow-y: auto;
  padding: 10px;
}

.book {
  display: flex;
  gap: 10px;
  padding: 10px;
  margin-bottom: 8px;
  background: #212429;
  border: 1px solid #2f333a;
  border-radius: 8px;
}

.info {
  flex: 1;
  min-width: 0;
}

.info h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #e9ebee;
}

.meta,
.intro,
.progress {
  margin: 2px 0 0;
  font-size: 12px;
}

.meta {
  color: #868c96;
}

.progress {
  color: #7ea6dd;
}

.intro {
  color: #9aa0aa;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 130px;
}

button.primary,
button.plain {
  height: 34px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
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

.toc-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}

.toc-head button {
  width: 88px;
  height: 32px;
}

.toc {
  margin: 0;
  padding: 0;
  list-style: none;
}

.toc li {
  padding: 9px 10px;
  margin-bottom: 4px;
  background: #212429;
  border-radius: 6px;
  cursor: pointer;
}

.toc li:hover {
  background: #2b3037;
}

.empty {
  text-align: center;
  color: #767c86;
}

.status {
  height: 26px;
  line-height: 26px;
  padding: 0 10px;
  color: #7ea6dd;
  font-size: 12px;
  border-top: 1px solid #2f333a;
}
</style>
