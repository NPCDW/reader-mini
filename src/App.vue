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

const tab = ref("shelf");
const page = ref("books");
const books = ref([]);
const chapters = ref([]);
const currentBook = ref(-1);
const loading = ref(false);
const status = ref("");

const config = ref(null);
/** 设置保存后递增，让已开着的阅读窗口重新读一遍样式 */
const styleTick = ref(0);
/** 阅读窗口最近一次回传的位置，收起 / 退出时用它把进度收干净 */
const readerState = reactive({ chapterIndex: 0, title: "", line: 0, pos: 0 });
let pollTimer = null;
let stopTick = null;
let stopPos = null;

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
};

async function bootstrap() {
  try {
    config.value = { ...DEFAULT_CONFIG, ...(await getConfig()) };
  } catch {
    config.value = { ...DEFAULT_CONFIG };
  }
  await refresh();
}

async function refresh() {
  loading.value = true;
  try {
    const list = await getBookshelf();
    books.value = list;
    await cacheBooks(list);
    status.value = "书架已更新";
  } catch (e) {
    status.value = `获取书架失败: ${e}`;
  } finally {
    loading.value = false;
  }
}

/** 打开某本书：带上这本书 + 上次读到的章节与行号 */
async function read(idx) {
  if (idx < 0 || idx >= books.value.length) return;
  currentBook.value = idx;
  await setCurrentBook(idx);
  const book = books.value[idx];
  const point = await call("resume_point", { bookIndex: idx }).catch(
    () => null,
  );
  const chapterIndex = point?.chapterIndex ?? book.durChapterIndex ?? 0;
  const title = point?.chapterTitle || book.durChapterTitle || book.name;
  status.value = `正在阅读《${book.name}》`;
  try {
    // 返回 false = 窗口本来就开着，这次按键是「收起」，不再拿内容去刷它
    const shown = await openReader({
      bookIndex: idx,
      chapterIndex,
      chapterTitle: title,
      startLine: point?.startLine ?? 0,
      styleTick: styleTick.value,
    });
    if (!shown) status.value = `已收起《${book.name}》`;
  } catch (e) {
    status.value = `打开阅读窗口失败: ${e}`;
  }
}

/** 目录里点某章：从该章正文最开始读起 */
async function openChapter(chapterIndex) {
  const idx = currentBook.value;
  if (idx < 0) {
    status.value = "请先从书架打开一本书的目录";
    return;
  }
  const ch = chapters.value[chapterIndex];
  // 目录下标就是 getBookContent 的 index，不能因为空标题重新编号
  const title = ch?.title || `第${chapterIndex + 1}章`;
  page.value = "books";
  status.value = `正在阅读《${books.value[idx].name}》`;
  try {
    // 从目录进是按章节读，阅读窗口开着就带上新样式一起刷新
    await openReader({
      bookIndex: idx,
      chapterIndex,
      chapterTitle: title,
      startLine: 0,
      styleTick: styleTick.value,
    });
  } catch (e) {
    status.value = `打开阅读窗口失败: ${e}`;
  }
}

async function openToc(idx) {
  currentBook.value = idx;
  await setCurrentBook(idx);
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
  await read(currentBook.value >= 0 ? currentBook.value : 0);
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
});

onUnmounted(() => {
  stopTick?.();
  stopPos?.();
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
      <button class="tab ghost" :disabled="loading" @click="refresh">
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
            <button class="primary" @click="read(i)">阅读</button>
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
  background: #e3e6ea;
  color: #444;
  cursor: pointer;
  font-size: 14px;
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
  overflow-y: auto;
  padding: 10px;
}

.book {
  display: flex;
  gap: 10px;
  padding: 10px;
  margin-bottom: 8px;
  background: #fff;
  border: 1px solid #e0e0e0;
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
}

.meta,
.intro,
.progress {
  margin: 2px 0 0;
  font-size: 12px;
}

.meta {
  color: #888;
}

.progress {
  color: #4a6fa5;
}

.intro {
  color: #666;
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
  background: #e3e6ea;
  color: #333;
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
  background: #fff;
  border-radius: 6px;
  cursor: pointer;
}

.toc li:hover {
  background: #eef2f8;
}

.empty {
  text-align: center;
  color: #999;
}

.status {
  height: 26px;
  line-height: 26px;
  padding: 0 10px;
  color: #4a6fa5;
  font-size: 12px;
  border-top: 1px solid #e3e6ea;
}
</style>
