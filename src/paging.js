// 阅读窗口的分页：一行几个字由窗口宽度定、一屏几行由窗口高度定。
//
// 行高与整章行数都由浏览器实测，不按字号估算 —— 估算会小掉一截，
// 一屏塞进太多行，每屏底部就会被切成「半行字」。

/**
 * 一屏能放几行。
 *
 * 放得下的前提是最后一行的底边不越出可视区，同时它下面还要留出半行（`gap`）
 * —— 只按 `h / 行高` 取整时，末行贴着底边，下一行的上半截正好露在下面，
 * 就成了翻页时那「半行字」。所以按 `(可视高度 - gap) / 行高` 取整。
 *
 * `gap` 取行高的一半：再多就是白白少放一行字。
 */
export function rowsPerScreen(bodyHeight, lineHeight, gap = 0) {
  const lh = Math.max(lineHeight, 1);
  const h = Math.max(bodyHeight, 1);
  return Math.max(Math.floor((h - gap) / lh), 1);
}

/**
 * 每屏顶部所在的显示行号，长度就是本章页数。
 *
 * 每屏整整推进一屏行数：既不重复一行，也不漏一行。
 * 这就是阅读器翻页的样子 —— 上一屏的最后一行不会又被下一屏翻出来，
 * 下一屏也从下一行接上，不从半行字开始。
 */
export function pageTops(totalRows, rows) {
  const step = Math.max(rows, 1);
  const n = Math.max(totalRows, 1);
  const tops = [];
  for (let top = 0; top < n; top += step) tops.push(top);
  return tops.length ? tops : [0];
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
