// 阅读窗口的分页：一行几个字由窗口宽度定、一屏几行由窗口高度定。
//
// 分页只认浏览器实测出来的行位置，不按字号估算。「一屏几行」是数出来的，
// 不是算出来的：从屏幕底边看过去，被切开的下一行就是下一页的第一行。

/**
 * 一行占多高。
 *
 * 量 N 行再除以 N，压掉长度取整带来的误差。
 */
export function lineHeight(heights) {
  if (heights.length < 2) return 0;
  return (heights[heights.length - 1] - heights[0]) / (heights.length - 1);
}

/**
 * 一屏的顶边和底边在哪（正文坐标）。
 *
 * 正文容器的 `scrollTop` 只能停在「内容高度 - 可视高度」处，再往下就是空白。
 * 所以这里给的 `bottom` 是它实际能滚到的最远位置，往前推出来的分页表也以它收尾。
 */
export function pageWindow(scrollHeight, bodyHeight, scrollTop) {
  const max = Math.max(scrollHeight - bodyHeight, 0);
  return { top: Math.min(Math.max(scrollTop, 0), max), bottom: max };
}

/**
 * 每屏的顶部位置（正文 y）。
 *
 * 从给定的位置起，一屏一屏往下推：下一屏的顶部 = 上一屏那点露在屏幕下面
 * 的下一行的 y，这样翻过去的第一行才是完整的（不是那半行字）。
 * 推到内容末尾停住 —— 正文容器的 `scrollTop` 就到此为止，再往下是空白。
 */
export function pageTops(offsets, bodyHeight, scrollHeight, scrollTop = 0) {
  const { top: t0, bottom: hardEnd } = pageWindow(
    scrollHeight,
    bodyHeight,
    scrollTop,
  );
  const tops = [t0];
  let top = t0;
  while (top < hardEnd) {
    const next = nextTop(offsets, top, bodyHeight, hardEnd);
    if (!(next > top)) break;
    tops.push(next);
    top = next;
  }
  return tops;
}

/** 从 `from` 再往下推一屏该停在哪；最多停到内容末尾（`hardEnd`） */
function nextTop(offsets, from, bodyHeight, hardEnd) {
  const straddler = straddlerBelow(offsets, from, from + bodyHeight);
  const next = Math.min(straddler, hardEnd);
  return next > from ? next : hardEnd;
}

/**
 * 从 `from` 往下看，屏幕下沿那一刀切在哪些行上。
 *
 * 返回被切开的那些行里、最靠下的那一行的后面一行 —— 也就是「下一屏该从哪开始」。
 * 被切开的行就是「半行字」的出处：它整行躺不进这一屏，就得整个留给下一屏。
 * 没有行被切到（正文到屏幕下沿之前就完了）返回 `Infinity`。
 */
function straddlerBelow(offsets, from, limit) {
  let crossing = null;
  for (const y of offsets) {
    if (y > from && y < limit) crossing = y;
  }
  if (crossing === null) return Infinity;
  const idx = offsets.indexOf(crossing);
  if (idx < 0 || idx + 1 >= offsets.length) return Infinity;
  return offsets[idx + 1];
}

/**
 * 按真实行位置算出每个位置该滚到哪、能装下哪几行。
 *
 * 关键是这里不需要「整章一共几行」这个数字 —— 那是算出来的、会错；
 * 每屏的顶部（`tops`）与装到的行数（`pages`）都直接落在量出来的行上。
 */
export function paginate(offsets, bodyHeight, scrollHeight, scrollTop = 0) {
  const tops = pageTops(offsets, bodyHeight, scrollHeight, scrollTop);
  const pages = tops.map((top, i) => {
    const bottom = i + 1 < tops.length ? tops[i + 1] : Infinity;
    let end = 0;
    for (let k = 0; k < offsets.length; k += 1) {
      if (offsets[k] < bottom) end = k + 1;
    }
    return { top, end };
  });
  return { tops, pages };
}

/** 某个位置落在第几页（取顶部不超过它的最后一页） */
export function pageOfTop(tops, top) {
  if (!tops.length) return 0;
  let lo = 0;
  let hi = tops.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (tops[mid] <= top + 0.5) lo = mid + 1;
    else hi = mid;
  }
  return Math.max(lo - 1, 0);
}

/** 某个正文行落在第几页（取最后一页满足「行顶没越过它的底边」） */
export function pageOfRow(pages, row) {
  let found = 0;
  for (let i = 0; i < pages.length; i += 1) {
    if (row >= pages[i].end) found = i + 1;
  }
  return Math.min(found, Math.max(pages.length - 1, 0));
}

/**
 * 量一行的真实行高：量 N 行再除以 N，压掉长度取整带来的误差。
 */
export function lineHeightOf(probe, rows = 10) {
  if (rows < 2) return 0;
  const h = measureHeight(probe, new Array(rows).fill("字").join("\n"));
  return h > 0 ? h / rows : 0;
}

/** 用不可见的探针元素量「这段文本在当前宽度下有多高」 */
export function measureHeight(probe, text) {
  probe.textContent = text;
  return probe.getBoundingClientRect().height;
}

/**
 * 量出正文每一行顶边的 y 坐标（相对正文顶部）。
 *
 * 逐段量：把探针里的前 k 段一起量一次高度，相邻两次的差除以行高就是第 k 段
 * 占了几行，段内每行按行高等距铺开（中文单栏排版里段内行高是均匀的）。
 *
 * 为什么不用「整章总高度 ÷ 行高」反推行数：那个数字会带一点零头，凑整后
 * 多出来的那一行在末屏就被顶出半行字。这里让浏览器逐段说「这一段几行」，
 * 行位置全部落在实测高度上。
 *
 * 量不出高度（比如没有中文字体）返回 `null`，调用方退回估行。
 */
export function measureLines(probe, text, lineHeightValue, maxRows = 2000) {
  const segs = text.split("\n");
  if (!segs.length || !(lineHeightValue > 0)) return null;
  const offsets = [];
  let prev = measureHeight(probe, "");
  for (let k = 0; k < segs.length; k += 1) {
    const h = measureHeight(probe, segs.slice(0, k + 1).join("\n"));
    const rows = Math.max(Math.round((h - prev) / lineHeightValue), 1);
    if (offsets.length + rows > maxRows) return null;
    for (let i = 0; i < rows; i += 1) offsets.push(0);
    prev = h;
  }
  if (offsets.length < 2) return null;
  return offsets.map((_, i) => i * lineHeightValue);
}
