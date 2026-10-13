// 封面缓存只按 `bookUrl` 认书（书架会被远端重排），同书同址才算命中。
import assert from "node:assert/strict";
import test from "node:test";
import {
  coverEntry,
  coverSrc,
  emptyCache,
  fetchCoverDataUrl,
  isCoverCached,
  loadCache,
  pendingCovers,
  saveCache,
} from "./cover.js";

const A = "https://example.org/book/39020.htm";
const B = "https://example.org/book/25234.htm";
const coverA = "https://example.org/files/39020s.jpg";
const coverB = "https://example.org/files/25234s.jpg";

/** 一个够用的 localStorage 替身，用来验缓存落盘这件事 */
function fakeStorage(seed = {}) {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
    dump: () => Object.fromEntries(map),
  };
}

test("缺书、缺封面、缺图的记录都不留", () => {
  assert.equal(coverEntry(A, coverA, ""), null);
  assert.equal(coverEntry("", coverA, "data:,"), null);
  assert.equal(coverEntry(A, "", "data:,"), null);
  assert.equal(coverEntry(A, coverA, "data:,").url, coverA);
});

test("同书同址算命中，换址不算", () => {
  const entry = coverEntry(A, coverA, "data:,");
  assert.ok(isCoverCached(A, coverA, entry));
  assert.ok(!isCoverCached(A, "https://example.org/files/new.jpg", entry));
  // 按 bookUrl 查出来的记录才对得上：拿别人那条来比不算命中
  assert.ok(!isCoverCached(B, coverA, coverEntry(B, coverB, "data:,")));
});

test("没取到过封面的书才要去取", () => {
  const books = [
    { bookUrl: A, coverUrl: coverA },
    { bookUrl: B, coverUrl: coverB },
    { bookUrl: "https://example.org/book/none.htm" },
  ];
  const todo = pendingCovers(books, { [A]: coverEntry(A, coverA, "data:,") });
  assert.deepEqual(
    todo.map((t) => t.bookUrl),
    [B],
  );
});

test("封面地址变了要重新取", () => {
  const books = [{ bookUrl: A, coverUrl: "https://example.org/files/new.jpg" }];
  const todo = pendingCovers(books, { [A]: coverEntry(A, coverA, "data:,") });
  assert.equal(todo.length, 1);
  assert.equal(todo[0].coverUrl, "https://example.org/files/new.jpg");
});

test("全命中时一次请求都不发", () => {
  const books = [
    { bookUrl: A, coverUrl: coverA },
    { bookUrl: B, coverUrl: coverB },
  ];
  const cache = {
    [A]: coverEntry(A, coverA, "data:a"),
    [B]: coverEntry(B, coverB, "data:b"),
  };
  assert.deepEqual(pendingCovers(books, cache), []);
});

test("缓存读写能过一轮，读回是同一份", () => {
  const storage = fakeStorage();
  const cache = { [A]: coverEntry(A, coverA, "data:a") };
  assert.ok(saveCache(cache, storage));
  assert.deepEqual(loadCache(storage), cache);
});

test("缓存坏了不当真，书架照常开", () => {
  const storage = fakeStorage({ "reader-mini.cover-cache.v1": "{ 不是 json" });
  assert.deepEqual(loadCache(storage), emptyCache());
});

test("渲染用缓存里的图，没缓存才回落到远端", () => {
  const cache = { [A]: coverEntry(A, coverA, "data:a") };
  assert.equal(coverSrc(A, coverA, cache), "data:a");
  assert.equal(coverSrc(B, coverB, cache), coverB);
  assert.equal(coverSrc(A, "", cache), "");
});

test("封面取到了才缓存：失败不入库", async () => {
  const blob = { size: 1 };
  const ok = fetchCoverDataUrl(coverA, async () => ({ ok: true, blob: async () => blob }), async () =>
    "data:image/png;base64,AAA",
  );
  assert.equal(await ok, "data:image/png;base64,AAA");
  // 转换这一步自己失败（FileReader 报错）同样不入库
  assert.equal(
    await fetchCoverDataUrl(coverA, async () => ({ ok: true, blob: async () => blob }), async () => ""),
    "",
  );
  assert.equal(await fetchCoverDataUrl(coverA, async () => ({ ok: false })), "");
  assert.equal(
    await fetchCoverDataUrl(coverA, async () => {
      throw new Error("图床挂了");
    }),
    "",
  );
});
