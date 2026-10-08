// 阅读窗口的分页：一行几个字由窗口宽度定、一屏几行由窗口高度定。
//
// 行高与整章行数都由浏览器实测，不按字号估算 —— 估算会小掉一截，
// 一屏塞进太多行，每屏底部就会被切成「半行字」。

/** 一屏能放几行：正文可视高度 ÷ 实测行高，向下取整 */
export function rowsPerScreen(bodyHeight, lineHeight) {
  const lh = Math.max(lineHeight, 1);
  return Math.max(Math.floor(Math.max(bodyHeight, lh) / lh), 1);
}

/**
 * 每屏顶部所在的显示行号，长度就是本章页数。
 *
 * 相邻两屏错开整整一屏行数：既不重复一行，也不漏一行。
 * 最后一屏往回收一点，让本章最后一行贴着底部，而不是留一屏空白。
 */
export function pageTops(totalRows, rows) {
  const step = Math.max(rows, 1);
  const n = Math.max(totalRows, 1);
  if (n <= step) return [0];
  const lastTop = n - step;
  const tops = [0];
  let top = 0;
  while (top < lastTop) {
    top = Math.min(top + step, lastTop);
    tops.push(top);
  }
  return tops;
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

/** 量一行的真实行高：量 N 行再除以 N，压掉长度取整带来的误差 */
export function lineHeightOf(probe, rows = 10) {
  const h = measureHeight(probe, new Array(rows).fill("字").join("\n"));
  return h > rows ? h / rows : 0;
}
