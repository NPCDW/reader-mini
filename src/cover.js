// 封面缓存：`coverUrl` 取到就记下来，之后不再取。
//
// 服务端的 `coverUrl` 指向第三方图床，每次刷新书架都重新拉一遍封面图，
// 一是白费流量，二是图床抽风时书架上一排裂图。封面这东西一旦拿到手就不会变，
// 所以按 `bookUrl`（不是下标，书架会被远端重排）缓存进 localStorage，
// 命中就不再发请求，图挂了也有上次那张顶着。

const KEY = "reader-mini.cover-cache.v1";

/** 缓存的形：`{ [bookUrl]: { url, bytes } }`，`bytes` 是 dataURL，<img> 直接能用 */
export function emptyCache() {
  return {};
}

/** 从 localStorage 读缓存；读坏了（格式不对）就当空的，别让书架打不开 */
export function loadCache(storage = safeStorage()) {
  if (!storage) return emptyCache();
  try {
    const raw = JSON.parse(storage.getItem(KEY) || "{}");
    return raw && typeof raw === "object" && !Array.isArray(raw) ? raw : emptyCache();
  } catch {
    return emptyCache();
  }
}

/** 写回缓存；写失败（隐私模式 / 配额满）就只是不缓存，不该影响书架 */
export function saveCache(cache, storage = safeStorage()) {
  if (!storage) return false;
  try {
    storage.setItem(KEY, JSON.stringify(cache));
    return true;
  } catch {
    return false;
  }
}

/** 一条缓存记录；缺任何一个都当没这一条 */
export function coverEntry(bookUrl, coverUrl, bytes) {
  if (!bookUrl || !coverUrl || !bytes) return null;
  return { url: coverUrl, bytes };
}

/**
 * 缓存命中的判据：这本书取到过、封面地址也没变。
 *
 * 换过封面地址的书（换源站、书换了封面）要重新取一次，不能拿旧图顶着。
 * 调用方传进来的 `entry` 是按 `bookUrl` 查出来的那一条，所以这里只比地址。
 */
export function isCoverCached(bookUrl, coverUrl, entry) {
  if (!bookUrl || !coverUrl || !entry) return false;
  return entry.url === coverUrl;
}

/**
 * 这批书里哪些书需要去取封面。
 *
 * `cache` 是「已按 bookUrl 取到封面」的表。返回 `{ bookUrl, coverUrl }` 列表，
 * 没有就是 `[]` —— 全命中的话一次请求都不发。
 */
export function pendingCovers(books = [], cache = {}) {
  return books
    .map((b) => ({ bookUrl: b?.bookUrl, coverUrl: b?.coverUrl }))
    .filter(
      ({ bookUrl, coverUrl }) =>
        bookUrl && coverUrl && !isCoverCached(bookUrl, coverUrl, cache[bookUrl]),
    );
}

/** 渲染时要的那一份：有缓存用缓存，没有才回落到远端 `coverUrl` */
export function coverSrc(bookUrl, coverUrl, cache = {}) {
  if (!coverUrl) return "";
  const entry = cache[bookUrl];
  return isCoverCached(bookUrl, coverUrl, entry) ? entry.bytes : coverUrl;
}

/**
 * 取一张封面，变成 `<img>` 能直接用的 dataURL；失败返回空串（不缓存失败）。
 *
 * 转换这一步单独交给 `toDataUrl`：`FileReader` 只有浏览器里有，
 * 单测里塞一个假的进去就能跑，不必起个无头浏览器。
 */
export async function fetchCoverDataUrl(coverUrl, fetchImpl = fetch, toDataUrl = blobToDataUrl) {
  if (!coverUrl) return "";
  try {
    const res = await fetchImpl(coverUrl);
    if (!res.ok) return "";
    return await toDataUrl(await res.blob());
  } catch {
    return "";
  }
}

function blobToDataUrl(blob) {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => resolve("");
    reader.readAsDataURL(blob);
  });
}

function safeStorage() {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}
