// 认书按下标就会串书（见 ISSUE #12），这里把行为锁住。
import assert from "node:assert/strict";
import test from "node:test";
import { continueIndex, findBook, indexOfBook } from "./bookshelf.js";

const A = { bookUrl: "https://example.org/a", name: "甲" };
const B = { bookUrl: "https://example.org/b", name: "乙" };

test("按 bookUrl 找书，不按下标", () => {
  assert.equal(indexOfBook([A, B], B.bookUrl), 1);
  assert.equal(findBook([A, B], B.bookUrl)?.name, "乙");
  assert.equal(findBook([A, B], "https://example.org/gone"), null);
  assert.equal(indexOfBook([A, B], ""), -1);
});

test("书架重排后仍续到同一本书", () => {
  // 读者最后读的是「甲」，收起时书架被远端重排成 [乙, 甲]
  const reordered = [B, A];
  assert.equal(continueIndex(reordered, A.bookUrl), 1);
  assert.equal(reordered[continueIndex(reordered, A.bookUrl)].name, "甲");
});

test("记住的那本被移出书架时退回第一本", () => {
  assert.equal(continueIndex([A, B], "https://example.org/gone"), 0);
  assert.equal(continueIndex([], A.bookUrl), -1);
});

test("还没读过任何书时打开第一本", () => {
  assert.equal(continueIndex([A, B], ""), 0);
});
