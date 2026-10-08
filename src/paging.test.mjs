// 分页是阅读窗口最容易出错的地方（翻页漏行、重复行、半行字），单测锁住行为。
// 用 node --test 直接跑：src/paging.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import {
  clipHeight,
  lineHeight,
  lineHeightOf,
  measureLines,
  pageEnds,
  pageFirstRows,
  pageOfRow,
  rowsPerScreen,
} from "./paging.js";

/**
 * 假探针：每 charsPerRow 个字符折一行、行高 lineHeightValue，空文本高度 0。
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

test("一屏几行：向下取整，末行完整才算放得下", () => {
  assert.equal(rowsPerScreen(300, 30), 10);
  // 504px 装 16 行整（16×30=480），第 17 行怎么也塞不进去
  assert.equal(rowsPerScreen(504, 30), 16);
  // 一屏 16.9 行只放 16 行
  assert.equal(rowsPerScreen(507, 30), 16);
  // 高度还没量出来时至少 1 行
  assert.equal(rowsPerScreen(0, 30), 1);
  assert.equal(rowsPerScreen(20, 30), 1);
});

test("末行完整：屏幕下沿不会切到下一行的上半截", () => {
  const lh = 30;
  const h = 504;
  const rows = rowsPerScreen(h, lh);
  // 末行底边（16×30 = 480）必须还在可视区里，第 17 行（顶边 480）整个在区外
  assert.ok(rows * lh <= h, "末行底边不能越出可视区");
  assert.ok((rows + 1) * lh > h, "再多一行就会被切");
});

test("装得下也是一页", () => {
  assert.deepEqual(pageFirstRows(5, 10), [0]);
  assert.deepEqual(pageFirstRows(0, 10), [0]);
  assert.deepEqual(pageFirstRows(17, 17), [0]);
});

test("相邻两屏首尾相接：正文每行至少出现一次，不跳字", () => {
  for (const [total, rows] of [
    [60, 17],
    [61, 17],
    [200, 17],
    [40, 16],
    [40, 13],
  ]) {
    const firsts = pageFirstRows(total, rows);
    const ends = pageEnds(total, firsts, rows);
    const seen = new Set();
    firsts.forEach((first, i) => {
      for (let r = first; r < ends[i]; r += 1) seen.add(r);
    });
    assert.equal(seen.size, total, `${total} 行 / 一屏 ${rows} 行时不能漏行`);
  }
});

test("只有最后一屏可能不满，其它屏都是整屏推进", () => {
  const firsts = pageFirstRows(60, 17);
  assert.deepEqual(firsts, [0, 17, 34, 51]);
  for (let i = 1; i < firsts.length; i += 1) {
    assert.equal(firsts[i] - firsts[i - 1], 17, `第 ${i} 屏应整屏推进 17 行`);
  }
  const ends = pageEnds(60, firsts, 17);
  assert.deepEqual(ends, [17, 34, 51, 60]);
  // 末屏 51 起、显示到 60（9 行），是唯一不满的那一屏
  assert.equal(ends[ends.length - 1], 60);
  assert.ok(ends[ends.length - 1] - firsts[firsts.length - 1] <= 17);
});

test("第二页的第一行就是第一页末行的下一行", () => {
  const steps = [
    [60, 16],
    [60, 17],
    [80, 12],
    [43, 16],
    [200, 13],
  ];
  for (const [total, rows] of steps) {
    const firsts = pageFirstRows(total, rows);
    const ends = pageEnds(total, firsts, rows);
    for (let i = 1; i < firsts.length; i += 1) {
      assert.equal(
        firsts[i],
        ends[i - 1],
        `${total} 行 / 一屏 ${rows} 行：第 ${i + 1} 屏应接在第 ${i} 屏末行之后`,
      );
    }
    assert.equal(ends[ends.length - 1], total, "末屏必须显示到正文末尾");
  }
});

test("行 -> 屏：取顶部不超过它的最后一屏", () => {
  const firsts = pageFirstRows(60, 17); // [0, 17, 34, 51]
  assert.equal(pageOfRow(firsts, 0), 0);
  assert.equal(pageOfRow(firsts, 16), 0);
  assert.equal(pageOfRow(firsts, 17), 1);
  assert.equal(pageOfRow(firsts, 51), 3);
  assert.equal(pageOfRow(firsts, 999), 3);
  assert.equal(pageOfRow([], 5), 0);
});

test("回归：460×560 窗口翻页不留半行字，第二屏第一行接在上一屏末行之后", () => {
  // 用户实机场景：窗口 460×560、字号 20、行高 1.5 → 行高 30，
  // 正文可视高度 504。
  const lh = 30;
  const h = 504;
  const total = 43;
  const rows = rowsPerScreen(h, lh); // 16
  const firsts = pageFirstRows(total, rows);
  const ends = pageEnds(total, firsts, rows);

  assert.equal(rows, 16);
  // 43 行：前面整屏推进到 16、32，末屏从 32 起显示到 43（只有 11 行）
  assert.deepEqual(firsts, [0, 16, 32]);
  assert.deepEqual(ends, [16, 32, 43]);
  for (let i = 0; i < firsts.length; i += 1) {
    const top = firsts[i];
    const isLast = i === firsts.length - 1;
    // 这一屏能完整显示的行：顶边在可视区里，且底边没越出下沿
    const visible = [];
    for (let r = top; r < ends[i]; r += 1) {
      if ((r - top + 1) * lh <= h) visible.push(r);
    }
    const last = visible[visible.length - 1];
    if (isLast) {
      // 末屏滚到底，正文末尾那行必须看得见
      assert.equal(ends[i], total, "末屏必须显示到正文末尾");
      continue;
    }
    // 非末屏：下一行必须整个在可视区之外 —— 下沿不会留半行字
    assert.ok((last - top + 2) * lh > h, `第 ${i + 1} 屏露了半行字`);
  }
  // 第二屏的第一行 = 第一屏末行的下一行（16），翻过去是完整的一行
  assert.equal(firsts[1], ends[0]);
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

test("行高：量不到就返回 0，交给调用方按字号估", () => {
  assert.equal(lineHeight([]), 0);
  assert.equal(lineHeight([300]), 0);
  assert.equal(lineHeight([0, 0, 0]), 0);
  // 量 10 行得到 10 个高度，末行比首行多 301px：摊到 9 段上
  const heights = Array.from({ length: 10 }, (_, i) => (i * 301) / 9);
  assert.equal(lineHeight(heights), 301 / 9);
  assert.equal(lineHeightOf(fakeProbe(20), 10), 20);
  assert.equal(
    lineHeightOf({ getBoundingClientRect: () => ({ height: 0 }) }, 10),
    0,
  );
});

test("可视高度裁到整行高的整数倍：屏幕下沿那点零头必须消掉", () => {
  // 行高 30.6（20px 字号 × 1.5 排出来的），可视区 387px：
  // 387 ÷ 30.6 = 12.6…，只能放 12 行，剩的 0.6 行（19.8px）正好够
  // 露出第 13 行的上半截 —— 就是那半行字
  assert.equal(
    Number(clipHeight(387, 30.6).toFixed(2)),
    Number((12 * 30.6).toFixed(2)),
  );
  assert.equal(clipHeight(387, 30.6), 12 * 30.6);
  // 裁完之后：末行底边正好贴下沿，下一行的顶边也在下沿之外
  const clipped = clipHeight(387, 30.6);
  const rows = rowsPerScreen(387, 30.6);
  assert.equal(rows * 30.6, clipped);
  assert.ok(rows * 30.6 <= clipped + 1e-9, "末行完整");
  assert.ok(rows * 30.6 >= clipped - 1e-9, "下一行的顶边不落在可视区里");
  // 整行装得下时不动它
  assert.equal(clipHeight(300, 30), 300);
  // 一行都装不下时至少留一行
  assert.equal(clipHeight(20, 30), 30);
});

test("页与页之间不重叠不漏行：每屏首行 = 上一屏末行的下一行", () => {
  for (const [total, rows] of [
    [13, 3],
    [24, 3],
    [80, 12],
    [81, 12],
    [200, 17],
    [25, 16],
  ]) {
    const firsts = pageFirstRows(total, rows, total - rows);
    const ends = pageEnds(total, firsts, rows);
    // 首行连续
    for (let i = 1; i < firsts.length; i += 1) {
      assert.equal(
        firsts[i],
        ends[i - 1],
        `${total} 行 / 一屏 ${rows} 行时第 ${i + 1} 屏没接上`,
      );
    }
    // 每行都出现
    const seen = new Set();
    firsts.forEach((f, i) => {
      for (let r = f; r < ends[i]; r += 1) seen.add(r);
    });
    assert.equal(seen.size, total, `${total} 行 / 一屏 ${rows} 行时不能漏行`);
    // 末屏显示到正文末尾
    assert.equal(ends[ends.length - 1], total);
  }
});

test("末屏跟着正文末尾走：不硬凑一个装不满又滚不到的短屏", () => {
  // 80 行 / 一屏 12 行：网格会排到 72，但滚到底最多站到 68 行。
  // 末屏直接站到 68，68~79 一屏收尾；60~67 由 60 那屏（显示 60~71）接住，
  // 两屏之间不会漏行
  assert.deepEqual(pageFirstRows(80, 12, 68), [0, 12, 24, 36, 48, 60, 68]);
  assert.deepEqual(
    pageEnds(80, pageFirstRows(80, 12, 68), 12),
    [12, 24, 36, 48, 60, 68, 80],
  );
  // 72 滚得到，末屏就落在网格上，正文正好收尾
  assert.deepEqual(pageFirstRows(80, 12, 72), [0, 12, 24, 36, 48, 60, 72]);
  // 正文正好是一屏行数的整数倍时，末屏就落在网格上，不多不少
  assert.deepEqual(pageFirstRows(84, 12, 72), [0, 12, 24, 36, 48, 60, 72]);
  assert.deepEqual(
    pageEnds(84, pageFirstRows(84, 12, 72), 12),
    [12, 24, 36, 48, 60, 72, 84],
  );
});

test("末屏站在滚动极限行上：上一屏与末屏之间不漏行", () => {
  // 100 行 / 一屏 16 行：网格末屏 96 滚不到（极限 70），末屏站到 70；
  // 64 那屏显示 64~79，接住 70 之前的行
  assert.deepEqual(pageFirstRows(100, 16, 70), [0, 16, 32, 48, 64, 70]);
  assert.deepEqual(
    pageEnds(100, pageFirstRows(100, 16, 70), 16),
    [16, 32, 48, 64, 70, 100],
  );
  // 84 也滚不到 96，末屏站到 84
  assert.deepEqual(pageFirstRows(100, 16, 84), [0, 16, 32, 48, 64, 80, 84]);

  // 90 行 / 一屏 16 行：网格末屏 80 滚不到（极限 74），末屏站到 74
  assert.deepEqual(pageFirstRows(90, 16, 74), [0, 16, 32, 48, 64, 74]);
  assert.deepEqual(
    pageEnds(90, pageFirstRows(90, 16, 74), 16),
    [16, 32, 48, 64, 74, 90],
  );
});

test("重叠翻页：下一屏首行 = 上一屏末行（步进 = 每屏行数 - 1）", () => {
  // 一屏 16 行、步进 15 行：第二屏从第 15 行起，正是第一屏看得见的最后一行，
  // 且同样贴着视口上沿。正文 43 行、滚动极限 27 行（43 - 16）
  const perPage = 16;
  const stride = perPage - 1;
  const total = 43;
  const bottomRow = total - perPage;
  const firsts = pageFirstRows(total, stride, bottomRow);
  const ends = pageEnds(total, firsts, perPage);

  assert.deepEqual(firsts, [0, 15, 27]);
  assert.equal(firsts[1], perPage - 1, "第二屏首行应是第一屏的末行");
  // 每屏在 16 行的窗口里末行都完整：首行顶边贴上沿、末行底边不出下沿
  const lh = 30;
  const viewport = perPage * lh;
  for (let i = 0; i < firsts.length; i += 1) {
    const top = firsts[i];
    const lastVisible = Math.min(top + perPage, total) - 1;
    assert.ok(
      (lastVisible - top + 1) * lh <= viewport,
      `第 ${i + 1} 屏末行不完整`,
    );
  }
  assert.equal(ends[ends.length - 1], total, "末屏必须显示到正文末尾");
});
