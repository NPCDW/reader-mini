// 书架上认书：一律按 `bookUrl`，不按下标。
//
// 服务端会按阅读时间重排书架，收起阅读窗口后又会悄悄刷一次，
// 两件事叠在一起，手上那个下标很快就指到别的书上去了 ——
// 「打开 A、收起、打开 B」两本书轮流出现就是这么来的。

/** 按 `bookUrl` 找书在眼前这份列表里的位置；找不到返回 -1 */
export function indexOfBook(books, bookUrl) {
  if (!bookUrl) return -1;
  return books.findIndex((b) => b.bookUrl === bookUrl);
}

/** 按 `bookUrl` 取书；找不到返回 null */
export function findBook(books, bookUrl) {
  const idx = indexOfBook(books, bookUrl);
  return idx < 0 ? null : books[idx];
}

/**
 * 快捷键 / 托盘续读该打开哪本书的下标。
 *
 * 记住的那本还在这份列表里就打开它，否则退回第一本。
 * 返回 -1 表示书架是空的，没得开。
 */
export function continueIndex(books, bookUrl) {
  if (!books.length) return -1;
  const idx = indexOfBook(books, bookUrl);
  return idx >= 0 ? idx : 0;
}
