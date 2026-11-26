// Windowed lists: only the rows near the visible part of a scroll area are in the page.
// A list of thousands of chat messages or task cards stays a few dozen elements, so it
// opens and scrolls quickly. Rows above and below the window are stood in for by padding
// sized from measured heights (and an average for rows never shown).
//
// Each row is a wrapper with a role ("article" in a feed, "listitem" in a list) and
// aria-posinset/aria-setsize, so a screen reader still hears where it is in the whole list.
// Keyboard: Tab past the last rendered card brings in the next one (lists); Page Down and
// Page Up move between articles (feeds). The focused row is kept while it is near the window.
"use strict";

/**
 * opts:
 *   scroller  the element that scrolls (the list may sit anywhere inside it)
 *   count     number of rows
 *   render(i) the row's content (a Node)
 *   sig(i)    optional: a string that changes when row i must be drawn again
 *   key(i)    optional: what row heights are remembered by (defaults to i)
 *   heights   optional Map kept by the caller, so a redraw keeps what was measured
 *   estimate  height in pixels assumed for a row never measured, until some are
 *   role      "feed" or "list"
 *   label     the list's accessible name
 *   anchor    keep the row at the top of the view still when heights above it change,
 *             and a list scrolled to its end at the end
 */
function vlist(opts) {
  const OVERSCAN = 600; // pixels drawn beyond each edge of the view
  const KEEP = 60; // a focused row this many rows outside the window stays drawn
  const sc = opts.scroller;
  const heights = opts.heights || new Map();
  const key = opts.key || ((i) => i);
  const feed = opts.role === "feed";
  const el = h("div", { class: "vlist", role: opts.role, "aria-label": opts.label, tabindex: feed ? null : "-1" });
  let count = opts.count;
  let start = 0, end = 0, gap = null, tops = [0];
  const rows = new Map(); // index -> { el, sig }
  let pending = 0;
  let stuck = true, placed = -1;
  const atEnd = () => sc.scrollHeight - sc.scrollTop - sc.clientHeight < 2;
  const scrollTo = (y) => { sc.scrollTop = y; placed = sc.scrollTop; };

  const est = () => {
    if (!heights.size) return opts.estimate || 60;
    let sum = 0;
    for (const v of heights.values()) sum += v;
    return sum / heights.size;
  };
  function layout() {
    if (gap === null) gap = parseFloat(getComputedStyle(el).rowGap) || 0;
    const e = est();
    tops = new Array(count + 1);
    tops[0] = 0;
    for (let i = 0; i < count; i++) tops[i + 1] = tops[i] + (heights.get(key(i)) ?? e) + gap;
  }
  const listTop = () => el.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop;
  function range() {
    if (!count) return [0, 0];
    const top = sc.scrollTop - listTop() - OVERSCAN, bottom = sc.scrollTop - listTop() + sc.clientHeight + OVERSCAN;
    let a = 0;
    while (a < count - 1 && tops[a + 1] <= top) a++;
    let b = a + 1;
    while (b < count && tops[b] < bottom) b++;
    return [a, b];
  }
  const rowOf = (node) => {
    for (let n = node; n && n !== el; n = n.parentElement) if (n.parentElement === el && n.dataset.vi) return n;
    return null;
  };
  const FOCUSABLE = "a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex='-1'])";
  const focusables = (row) => (row ? [...row.querySelectorAll(FOCUSABLE)] : []);

  function makeRow(i) {
    const row = h("div", { role: feed ? "article" : "listitem", "data-vi": i, "aria-posinset": i + 1, "aria-setsize": count, tabindex: feed ? "-1" : null }, opts.render(i));
    rows.set(i, { el: row, sig: opts.sig ? opts.sig(i) : null });
    ro.observe(row);
    return row;
  }
  /** Draw rows [a, b), keeping nodes that are already right so focus inside them stays. */
  function draw(a, b) {
    const focusRow = rowOf(document.activeElement);
    const fi = focusRow ? Number(focusRow.dataset.vi) : -1;
    if (fi >= 0 && fi < count) {
      if (fi < a && a - fi <= KEEP) a = fi;
      if (fi >= b && fi - b < KEEP) b = fi + 1;
    }
    for (const [i, r] of rows) if (i < a || i >= b || i >= count) { ro.unobserve(r.el); r.el.remove(); rows.delete(i); }
    if (focusRow && !focusRow.isConnected) (feed ? sc : el).focus({ preventScroll: true });
    for (const [i, r] of rows) {
      r.el.setAttribute("aria-setsize", count);
      if (!opts.sig) continue;
      const s = opts.sig(i);
      if (s === r.sig) continue;
      const had = r.el.contains(document.activeElement);
      r.el.replaceChildren(opts.render(i));
      r.sig = s;
      if (had) r.el.focus({ preventScroll: true });
    }
    const first = rows.size ? Math.min(...rows.keys()) : b;
    const before = rows.size ? rows.get(first).el : null;
    for (let i = a; i < Math.min(first, b); i++) el.insertBefore(makeRow(i), before);
    for (let i = Math.max(a, rows.size ? Math.max(...rows.keys()) + 1 : a); i < b; i++) if (!rows.has(i)) el.append(makeRow(i));
    start = a; end = b;
  }
  /** Remember the heights of drawn rows; true if any changed. */
  function measure() {
    let changed = false;
    for (const [i, r] of rows) {
      const hgt = r.el.getBoundingClientRect().height;
      if (Math.abs((heights.get(key(i)) ?? -1) - hgt) > 0.5) { heights.set(key(i), hgt); changed = true; }
    }
    return changed;
  }
  function pad() {
    el.style.paddingTop = (rows.size ? tops[start] : 0) + "px";
    el.style.paddingBottom = (rows.size ? tops[count] - tops[end] : Math.max(0, tops[count] - gap)) + "px";
  }
  /** The first drawn row at or below the top of the view, and where it is on screen. */
  function anchorRow() {
    const top = sc.getBoundingClientRect().top;
    for (const i of [...rows.keys()].sort((x, y) => x - y)) {
      const r = rows.get(i).el.getBoundingClientRect();
      if (r.bottom > top) return { i, y: r.top };
    }
    return null;
  }
  function update() {
    if (!el.isConnected) return;
    // A list scrolled to its end (a live chat) stays at the end; otherwise the row at the top stays put.
    const pinned = opts.anchor && (stuck || atEnd());
    const anchor = opts.anchor && !pinned ? anchorRow() : null;
    layout();
    for (let k = 0; k < 4; k++) {
      const [a, b] = range();
      if (k > 0 && a === start && b === end) break;
      draw(a, b);
      const changed = measure();
      layout();
      pad();
      if (pinned) scrollTo(sc.scrollHeight);
      else if (anchor && rows.has(anchor.i)) scrollTo(sc.scrollTop + rows.get(anchor.i).el.getBoundingClientRect().top - anchor.y);
      if (!changed && k > 0) break;
    }
  }
  function schedule() {
    if (!el.isConnected) { stop(); return; }
    if (!pending) pending = requestAnimationFrame(() => { pending = 0; update(); });
  }
  const ro = new ResizeObserver(schedule);
  // Whether the person's last scroll left the list at its end; a resize of the scroll area keeps that.
  // Scrolls this list makes itself are told apart by where they left the scroll area.
  const onScroll = () => { if (Math.abs(sc.scrollTop - placed) >= 1) stuck = atEnd(); schedule(); };
  function stop() {
    sc.removeEventListener("scroll", onScroll);
    ro.disconnect();
    if (pending) cancelAnimationFrame(pending);
    pending = 0;
  }
  sc.addEventListener("scroll", onScroll, { passive: true });
  ro.observe(sc);

  /** Scroll so row i is in view, draw it and return its wrapper. */
  function scrollToIndex(i, block = "nearest") {
    if (i < 0 || i >= count) return null;
    for (let k = 0; k < 4; k++) {
      layout();
      const y = listTop() + tops[i], hgt = heights.get(key(i)) ?? est(), view = sc.clientHeight;
      let to = sc.scrollTop;
      if (block === "center") to = y - (view - hgt) / 2;
      else if (block === "start") to = y;
      else if (y < sc.scrollTop) to = y;
      else if (y + hgt > sc.scrollTop + view) to = y + hgt - view;
      scrollTo(to);
      stuck = false;
      const anchor = opts.anchor;
      opts.anchor = false;
      update();
      opts.anchor = anchor;
      if (rows.has(i) && Math.abs(listTop() + tops[i] - y) < 1) break;
    }
    return rows.get(i)?.el || null;
  }
  function scrollToEnd() {
    for (let k = 0; k < 6; k++) {
      scrollTo(sc.scrollHeight);
      stuck = true;
      const anchor = opts.anchor;
      opts.anchor = false;
      update();
      opts.anchor = anchor;
      if (sc.scrollHeight - sc.scrollTop - sc.clientHeight < 1) break;
    }
  }

  el.addEventListener("keydown", (e) => {
    const row = rowOf(e.target);
    if (!row) return;
    const i = Number(row.dataset.vi);
    if (feed && (e.key === "PageDown" || e.key === "PageUp")) {
      const next = scrollToIndex(i + (e.key === "PageDown" ? 1 : -1));
      if (!next) return;
      e.preventDefault();
      next.focus({ preventScroll: true });
      return;
    }
    if (feed || e.key !== "Tab") return;
    const f = focusables(row);
    if (!e.shiftKey && i === end - 1 && i + 1 < count && f[f.length - 1] === e.target) {
      e.preventDefault();
      focusables(scrollToIndex(i + 1))[0]?.focus();
    } else if (e.shiftKey && i === start && i > 0 && f[0] === e.target) {
      e.preventDefault();
      focusables(scrollToIndex(i - 1)).pop()?.focus();
    }
  });

  return {
    el,
    /** Call once the list is in the page, and again whenever its rows change. */
    update(n = count) { count = n; update(); },
    scrollToIndex,
    scrollToEnd,
    row: (i) => rows.get(i)?.el || null,
    /** Stop following the scroll area (for a list about to be replaced). */
    stop,
  };
}
