// 分页是阅读窗口最容易出错的地方（翻页漏行、重复行、半行字），单测锁住行为。
// 用 node --test 直接跑：src/paging.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import {
  lineHeight,
  measureLines,
  pageOfRow,
  pageOfTop,
  pageTops,
  paginate,
} from "./paging.js";

/**
 * 假探针：每 10 个字符折一行、行高 20，空文本高度 0。
 * 替掉真实浏览器，单测里才跑得动量行逻辑。
 */
function fakeProbe(lineHeightValue = 20, charsPerRow = 10) {
  return {
    _text: "",
    set textContent(v) {
      this._text = v;
    },
    get textContent() {
      return this._text;
    },
    getBoundingClientRect() {
      if (this._text === "") return { height: 0 };
      const rows = this._text
        .split("\n")
        .reduce(
          (n, seg) => n + (seg ? Math.ceil(seg.length / charsPerRow) : 1),
          0,
        );
      return { height: rows * lineHeightValue };
    },
  };
}

/** 造一批等距行位置：n 行、行高 lh */
const grid = (n, lh = 30) => Array.from({ length: n }, (_, i) => i * lh);

test("行高按末行与首行摊出来，不受长度取整影响", () => {
  assert.equal(lineHeight([]), 0);
  assert.equal(lineHeight([300]), 0);
  // 量 10 行得到 10 个高度，末行比首行多 301px：摊到 9 段上
  const heights = Array.from({ length: 10 }, (_, i) => (i * 301) / 9);
  assert.equal(lineHeight(heights), 301 / 9);
});

test("下一页从「被切开的下一行」接上，不切半行", () => {
  const rows = grid(20);
  const h = 390;
  const scrollHeight = 600;
  const tops = pageTops(rows, h, scrollHeight);
  // 屏幕下沿切在第 13 行（y=360）上，下一页就从第 14 行（y=390）开始，
  // 但它已经越过内容末尾（600 - 390 = 210），所以就停在末尾
  assert.deepEqual(tops, [0, 210]);
  // 正文够长时下一页才真的落在被切开的下一行
  assert.deepEqual(pageTops(grid(40), h, 40 * 30), [0, 390, 780, 810]);
  // 屏幕高度变一点，接上的那一行也跟着变，永远是完整的一行
  assert.equal(pageTops(grid(40), 389, 40 * 30)[1], 390);
  assert.equal(pageTops(grid(40), 375, 40 * 30)[1], 390);
  assert.equal(pageTops(grid(40), 370, 40 * 30)[1], 390);
});

test("相邻两屏首尾相接：正文每行至少出现一次，不跳字", () => {
  // 内容末尾把最后一屏挤短时，短屏会与上一屏重叠几行 —— 宁可让读者多看
  // 一遍，也不能漏掉任何一行
  for (const [n, h] of [
    [60, 510],
    [61, 510],
    [200, 510],
    [40, 390],
    [7, 240],
    [1, 240],
  ]) {
    const rows = grid(n);
    const { tops, pages } = paginate(rows, h, n * 30);
    const seen = new Set();
    for (const p of pages) {
      for (let r = 0; r < p.end; r += 1) seen.add(r);
    }
    assert.equal(seen.size, n, `${n} 行 / 一屏 ${h}px 时不能漏行`);
    assert.equal(tops.length, pages.length);
  }
});

test("每屏整屏推进，只有末屏可能短一点", () => {
  const tops = pageTops(grid(60), 510, 60 * 30);
  // 17 行一屏（510 = 17×30），第 4 屏只剩 9 行，就落在内容末尾
  assert.deepEqual(tops, [0, 510, 1020, 1290]);
  for (let i = 1; i < tops.length - 1; i += 1) {
    assert.equal(tops[i] - tops[i - 1], 510, `第 ${i} 屏应整屏推进`);
  }
  assert.ok(tops[tops.length - 1] < tops[tops.length - 2] + 510);
});

test("位置停在中间时，以它自己那一屏为准", () => {
  // 兜底位置不能顶到内容末尾之外，否则读者会看到一屏空白
  const tops = pageTops(grid(60), 510, 60 * 30, 700);
  assert.ok(tops[0] >= 700, "第一屏不能落在给定位置之前");
  assert.ok(tops[0] < 700 + 510, "第一屏也不该跳过给定位置");
  assert.ok(tops.every((t) => t <= 60 * 30 - 510));
});

test("正文装不满一屏时只有一屏", () => {
  assert.deepEqual(pageTops(grid(5), 510, 150), [0]);
  assert.deepEqual(pageTops([], 510, 0), [0]);
});

test("短尾屏让位：末屏不越过内容末尾", () => {
  // 60 行正文装 4 屏（510×4=2040，内容高 1800），末屏只能停在 1290
  const { tops } = paginate(grid(60), 510, 60 * 30);
  assert.deepEqual(tops, [0, 510, 1020, 1290]);
  assert.equal(tops[tops.length - 1], 60 * 30 - 510);
});

test("行 -> 屏：取最后一屏「行顶没越过它底边」", () => {
  // 16 行一屏（510 = 17×30，末行 y=480 的底边 510 正好装满）时，第 17 行才翻页
  const { pages } = paginate(grid(60, 30), 510, 60 * 30);
  assert.equal(pageOfRow([{ top: 0, end: 17 }], 0), 0);
  assert.equal(pageOfRow([{ top: 0, end: 17 }], 16), 0);
  // 第 17 行的 y 正是 510 = 第 1 屏的顶部，归第 1 屏
  assert.equal(pageOfRow(pages, 17), 1); // pageTops(grid(60))[1] === 510
  assert.equal(pageOfRow(pages, 59), pages.length - 1);
  assert.equal(pageOfRow([], 5), 0);
  // 短尾屏：末屏比上一屏只多几行时，夹在两屏之间的行归末屏
  assert.equal(pageOfRow(pages, 43), pages.length - 1);
});

test("位置 -> 屏：取顶部不超过它的最后一屏", () => {
  const { tops } = paginate(grid(60), 510, 60 * 30); // [0, 510, 1020, 1290]
  assert.equal(pageOfTop(tops, 0), 0);
  assert.equal(pageOfTop(tops, 509), 0);
  assert.equal(pageOfTop(tops, 510), 1);
  assert.equal(pageOfTop(tops, 999999), tops.length - 1);
});

test("分页表为空不炸", () => {
  assert.deepEqual(paginate([], 510, 0), {
    tops: [0],
    pages: [{ top: 0, end: 0 }],
  });
});

test("回归：翻页不会在屏幕下沿留半个字", () => {
  // 用户报的场景：460×560 的窗口、字号 20、行高 1.5 → 行高 30，
  // 正文可视高度 504。
  //
  // 老写法按「504 ÷ 30 = 16.8 → 16 行」切，把第 16 行（y=480~510）算进
  // 这一屏：480~504 这半个字就露在屏幕下沿，翻页时被读者看见了。
  //
  // 新写法只认浏览器量出来的行位置：一屏装到「底边还在屏幕里」的最后一行为止，
  // 被屏幕下沿切开的下一行从下一页的开头开始。
  const lh = 30;
  const h = 504;
  const rows = grid(40, lh);
  const contentHeight = 40 * lh; // 1200
  const tops = pageTops(rows, h, contentHeight);

  for (const top of tops) {
    const limit = top + h;
    // 屏幕里看得见的行：顶边在屏幕内的那些
    const visible = rows.filter((y) => y >= top && y < limit);
    const firstClipped = visible.find((y) => y + lh > limit);
    // 最多只能有一行被切（下一行必须整个在屏幕外）
    assert.ok(
      !firstClipped || visible.indexOf(firstClipped) === visible.length - 1,
      `第 ${tops.indexOf(top)} 屏下沿切了不止一行`,
    );
    // 被切开的那一行，必须是下一屏的第一行 —— 翻过去就看不到半行字了
    if (firstClipped !== undefined && tops.includes(firstClipped)) {
      const at = tops.indexOf(firstClipped);
      assert.equal(at, tops.indexOf(top) + 1, "被切开的行没接在下一屏开头");
    }
  }
  // 老写法给的位置（第二屏停在 480）不成立：480 处第 16 行还会再露一次
  assert.notEqual(tops[1], 480);
  assert.equal(tops[1], 510, "下一页从被切开的下一行开始");
});

test("逐段量行：段落高度差 ÷ 行高就是这一段几行", () => {
  const probe = fakeProbe(20, 10);
  // 25 字 = 3 行
  assert.deepEqual(measureLines(probe, "a".repeat(25), 20), [0, 20, 40]);
  // 25 字（3 行）+ 1 字（1 行）= 4 行
  assert.deepEqual(
    measureLines(probe, "a".repeat(25) + "\nb", 20),
    [0, 20, 40, 60],
  );
  // 25 字（3 行）+ 5 字（1 行）+ 10 字（1 行）= 5 行
  assert.deepEqual(
    measureLines(probe, "a".repeat(25) + "\nbbbbb\ncccccccccc", 20),
    [0, 20, 40, 60, 80],
  );
});

test("逐段量行：量不出来就交回 null，让调用方估", () => {
  const zero = {
    textContent: "",
    getBoundingClientRect: () => ({ height: 0 }),
  };
  assert.equal(measureLines(zero, "正文", 20), null);
  assert.equal(measureLines(fakeProbe(), "", 20), null);
  assert.equal(measureLines(fakeProbe(), "正文", 0), null);
});

test("逐段量行：行数多到离谱就认输", () => {
  const probe = fakeProbe(20, 10);
  assert.equal(measureLines(probe, "a".repeat(500), 20, 10), null);
});
