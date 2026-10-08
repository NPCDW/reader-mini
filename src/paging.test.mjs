// 分页是阅读窗口最容易出错的地方（翻页漏行、重复行、末屏留白），单测锁住行为。
// 用 node --test 直接跑：src/paging.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import { pageOfRow, pageTops, rowsPerScreen } from "./paging.js";

test("一屏几行：向下取整，宁可底部空一点也不切半行", () => {
  assert.equal(rowsPerScreen(300, 30), 10);
  // 一屏 13.9 行只放 13 行
  assert.equal(rowsPerScreen(417, 30), 13);
  // 高度还没量出来时至少 1 行
  assert.equal(rowsPerScreen(0, 30), 1);
  assert.equal(rowsPerScreen(20, 30), 1);
});

test("装得下就是一页", () => {
  assert.deepEqual(pageTops(5, 10), [0]);
  assert.deepEqual(pageTops(0, 10), [0]);
});

test("相邻两屏首尾相接：正文每行至少出现一次，不跳字", () => {
  for (const [total, rows] of [[60, 17], [61, 17], [200, 17], [40, 13]]) {
    const seen = new Set();
    for (const top of pageTops(total, rows)) {
      for (let r = top; r < Math.min(top + rows, total); r += 1) seen.add(r);
    }
    assert.equal(seen.size, total, `${total} 行 / 一屏 ${rows} 行时不能漏行`);
  }
});

test("只有最后一屏会与上一屏重叠，重叠量正好是「收尾」所需的行数", () => {
  const total = 60;
  const rows = 17;
  const tops = pageTops(total, rows); // [0, 17, 34, 43]
  assert.deepEqual(tops, [0, 17, 34, 43]);
  // 末屏从 43 起，覆盖 43..59，与上一屏重叠 34+17-43 = 8 行
  assert.equal(tops[tops.length - 1] + rows, total);
});

test("最后一屏往回收：末行贴着底部，不留整屏空白", () => {
  const total = 60;
  const rows = 17;
  const tops = pageTops(total, rows);
  const last = tops[tops.length - 1];
  assert.equal(last, total - rows);
  assert.ok(last + rows === total, "末屏最后一行正好是全文最后一行");
});

test("相邻两屏最多错开一屏：中间几屏都是整屏推进", () => {
  const tops = pageTops(200, 17);
  for (let i = 1; i < tops.length - 1; i += 1) {
    assert.equal(tops[i] - tops[i - 1], 17, `第 ${i} 屏应整屏推进 17 行`);
  }
});

test("行 -> 屏：取顶部不超过它的最后一屏", () => {
  const tops = pageTops(60, 17); // [0, 17, 34, 43]
  assert.equal(pageOfRow(tops, 0), 0);
  assert.equal(pageOfRow(tops, 16), 0);
  assert.equal(pageOfRow(tops, 17), 1);
  assert.equal(pageOfRow(tops, 42), 2);
  assert.equal(pageOfRow(tops, 43), 3);
  assert.equal(pageOfRow(tops, 999), 3);
});

test("空分页表不炸", () => {
  assert.equal(pageOfRow([], 5), 0);
});
