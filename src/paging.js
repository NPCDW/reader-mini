// 阅读窗口的分页：一行几个字由窗口宽度定、一屏几行由窗口高度定。
//
// 分页只认浏览器实测出来的行位置，不按字号估算 —— 估算出来的行数一旦不准，
// 屏幕下沿就会切出「半行字」。

/** 一行占多高。量 N 行再除以 N，压掉长度取整带来的误差 */
export function lineHeight(heights) {
  if (heights.length < 2) return 0;
  return (heights[heights.length - 1] - heights[0]) / (heights.length - 1);
}

/**
 * 一屏能完整放几行。
 *
 * 放得下的条件是**这一屏里每一行的底边都不越出可视区**，也就是
 * `行数 × 行高 ≤ 可视高度`。取 `floor` 就够了，但还有一条更要紧的：
 * 屏幕下沿的那条缝不能塞得下下一行的任何一部分 —— 只要缝高 ≥ 行高，
 * 下一行的顶边就落在屏里，露出来的就是那半行字。
 *
 * 行高是浏览器排出来的小数（字号 × 行高倍数不一定是整数像素），缝总归会有；
 * 所以调用方会把可视区裁到「整行高的整数倍」（见 `clipHeight`），
 * 缝恒等于 0，这里只需要保证末行完整。
 */
export function rowsPerScreen(bodyHeight, lineHeight) {
  const lh = Math.max(lineHeight, 1);
  const h = Math.max(bodyHeight, 1);
  return Math.max(Math.floor(h / lh), 1);
}

/**
 * 把可视高度裁到整行高的整数倍。
 *
 * 不裁的话，可视区总会有「行高整数倍之外」的那点零头 —— 它够高的话，
 * 正好露出下一行的上半截，那就是读者在屏幕下沿看到的半行字。
 * 裁到整行高的整数倍之后，下沿就是某一行的底边，一个字都不会多露。
 */
export function clipHeight(bodyHeight, lineHeight) {
  const lh = Math.max(lineHeight, 1);
  const h = Math.max(bodyHeight, 1);
  return Math.max(Math.floor(h / lh) * lh, lh);
}

/**
 * 每屏的首行行号，长度就是本章页数。
 *
 * 前面几屏从正文开头按「一屏 `step` 行」整整推进，**只有最后一屏可以不满**：
 * 它从上一屏的末行接上，一路显示到正文末尾。这样每屏最后一行都完整，
 * 下一屏的第一行就是上一屏末行的下一行。
 *
 * 最后一屏从哪儿起，取决于滚不到的问题：`scrollTop` 到内容末尾就停了，
 * 最多滚到「内容高度 − 可视高度」。按网格排出来的末屏有时比这条线更靠下，
 * 滚不到位、末行跌出视口，下沿就切出半行字。
 *
 * 规则（`bottomRow` 是滚到底时还能当作屏顶的最下面的行）：
 * - 网格末屏滚得到 → 就用网格，末屏只显示余数那几行；
 * - 滚不到 → 末屏往后让到能滚到的最后一个整屏位置
 *   （`floor(bottomRow / step) × step`），它仍然是从整屏位置上起步，
 *   而且因为滚得到，能一路显示到正文末尾。
 *
 * 传 `null` 表示不分页（内容装得下整章）。
 */
export function pageFirstRows(totalRows, rows, bottomRow = null) {
  const step = Math.max(rows, 1);
  const n = Math.max(totalRows, 1);
  if (n <= step) return [0];

  // 网格末屏：最后一个整屏位置，它负责把正文收尾
  const gridLast = Math.floor((n - 1) / step) * step;
  let last = gridLast;
  if (bottomRow !== null && gridLast > Math.max(bottomRow, 0)) {
    // 网格末屏滚不到：退到滚得到的最后一个整屏位置；退不动就只剩一屏
    last = Math.floor(Math.max(bottomRow, 0) / step) * step;
  }

  const firsts = [];
  for (let first = 0; first <= last; first += step) firsts.push(first);
  return firsts.length ? firsts : [0];
}

/** 某一屏显示到第几行（下标不含）——最后一屏会一路显示到正文末尾 */
export function pageEnds(totalRows, firsts, rows) {
  const n = Math.max(totalRows, 1);
  return firsts.map((first, i) =>
    i + 1 < firsts.length ? Math.min(firsts[i + 1], n) : n,
  );
}

/** 某个显示行落在第几屏（取顶部不超过它的最后一屏） */
export function pageOfRow(tops, row) {
  if (!tops.length) return 0;
  let lo = 0;
  let hi = tops.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (tops[mid] <= row) lo = mid + 1;
    else hi = mid;
  }
  return Math.max(lo - 1, 0);
}

/** 用不可见的探针元素量「这段文本在当前宽度下有多高」 */
export function measureHeight(probe, text) {
  probe.textContent = text;
  return probe.getBoundingClientRect().height;
}

/**
 * 量一行的真实行高。
 *
 * 用「N 行的高度 − 1 行的高度」再除以 `N - 1`：探针和正文一样带顶部内边距，
 * 直接拿 N 行高度除以 N 会把这层内边距摊进每一行（20px 字号 × 1.5 的 30px
 * 会量成 30.6px），行高偏大之后分页就会少算行数、下沿切出半行字。
 * 先减掉一行的基准高度，内边距和取整误差都抵消掉了。
 *
 * 量不出高度（比如没有中文字体）返回 0，调用方退回按字号估。
 */
export function lineHeightOf(probe, rows = 10) {
  if (rows < 2) return 0;
  const one = measureHeight(probe, "字");
  if (!(one > 0)) return 0;
  const many = measureHeight(probe, new Array(rows).fill("字").join("\n"));
  const lh = (many - one) / (rows - 1);
  return lh > 0 ? lh : 0;
}

/**
 * 逐段量出每一行顶边的 y 坐标（相对正文顶部）。
 *
 * 逐段量：把探针里前 k 段一起量一次高度，相邻两次的差除以行高就是第 k 段
 * 占了几行，段内每行按行高等距铺开（中文单栏排版里段内行高是均匀的）。
 *
 * 为什么不用「整章总高度 ÷ 行高」反推行数：那个数字带一点零头，
 * 凑整后多出来的那一行会在屏幕下沿被切成半行字。
 *
 * 量不出高度（探针返回 0 或整章只有一行）返回 `null`，调用方退回估行。
 */
export function measureLines(probe, text, lineHeightValue, maxRows = 2000) {
  const segs = text.split("\n");
  if (!segs.length || !(lineHeightValue > 0)) return null;
  // 整章量不出高度，说明探针拿不到排版结果（没有字体、还没挂上 DOM 等）
  const whole = measureHeight(probe, text);
  if (!(whole > 0)) return null;

  const offsets = [];
  let rows = 0;
  let prev = measureHeight(probe, "");
  for (let k = 0; k < segs.length; k += 1) {
    const h = measureHeight(probe, segs.slice(0, k + 1).join("\n"));
    rows += Math.max(Math.round((h - prev) / lineHeightValue), 1);
    if (rows > maxRows) return null;
    prev = h;
  }
  for (let i = 0; i < rows; i += 1) offsets.push(i * lineHeightValue);
  if (offsets.length < 2) return null;
  return offsets;
}
