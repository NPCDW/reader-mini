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

test("末行下面留半个行高：下一行的半个字才不会露出来", () => {
  // 一屏 13 行的位置、行高 30、空档 15：只能放 12 行，末行底边离下沿还有 15px
  assert.equal(rowsPerScreen(405, 30, 15), 13);
  assert.equal(rowsPerScreen(390, 30, 15), 12);
  // 不算空档就会把第 13 行放进去，它的上半截露在视口里
  assert.equal(rowsPerScreen(390, 30), 13);
  // 高度不够一行时仍然留一行，别算出 0
  assert.equal(rowsPerScreen(20, 30, 15), 1);
});

test("装得下也是一页", () => {
  assert.deepEqual(pageTops(5, 10), [0]);
  assert.deepEqual(pageTops(0, 10), [0]);
});

test("相邻两屏首尾相接：正文每行至少出现一次，不跳字", () => {
  for (const [total, rows] of [
    [60, 17],
    [61, 17],
    [200, 17],
    [40, 13],
  ]) {
    const seen = new Set();
    for (const top of pageTops(total, rows)) {
      for (let r = top; r < Math.min(top + rows, total); r += 1) seen.add(r);
    }
    assert.equal(seen.size, total, `${total} 行 / 一屏 ${rows} 行时不能漏行`);
  }
});

test("只有最后一屏会不满，其它屏都是整屏推进", () => {
  const tops = pageTops(60, 17);
  assert.deepEqual(tops, [0, 17, 34, 51]);
  for (let i = 1; i < tops.length; i += 1) {
    assert.equal(tops[i] - tops[i - 1], 17, `第 ${i} 屏应整屏推进 17 行`);
  }
});

test("一屏放得下整章时只有一页", () => {
  assert.deepEqual(pageTops(17, 17), [0]);
  assert.deepEqual(pageTops(3, 17), [0]);
});

test("末屏从上一屏的下一行开始，不重复上一屏的行", () => {
  const tops = pageTops(60, 17); // [0, 17, 34, 51]
  assert.equal(tops[tops.length - 1], 51, "末屏首行就是上一屏末行的下一行");
});

test("行 -> 屏：取顶部不超过它的最后一屏", () => {
  const tops = pageTops(60, 17); // [0, 17, 34, 51]
  assert.equal(pageOfRow(tops, 0), 0);
  assert.equal(pageOfRow(tops, 16), 0);
  assert.equal(pageOfRow(tops, 17), 1);
  assert.equal(pageOfRow(tops, 51), 3);
  assert.equal(pageOfRow(tops, 999), 3);
});

test("空分页表不炸", () => {
  assert.equal(pageOfRow([], 5), 0);
});
