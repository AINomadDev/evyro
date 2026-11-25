// Code search: find a file by name, search the code for text or a regex, or
// jump to a symbol, then read the file with its outline. Read-only.
// Loaded after app.js and uses its helpers (h, invoke, state, act, toast...).
"use strict";

// [mode, tab label, placeholder]; a function so the text follows the chosen language.
const searchModes = () => [
  ["files", t("search.mode.files"), t("search.ph.files")],
  ["text", t("search.mode.text"), t("search.ph.text")],
  ["symbols", t("search.mode.symbols"), t("search.ph.symbols")],
  ["chats", t("search.mode.chats"), t("search.ph.chats")],
];
state.search = { mode: "files", query: "", regex: false, caseSensitive: false, path: "" };

/** Text with the given character positions wrapped in <mark>. */
function marked(text, positions) {
  const set = new Set(positions || []);
  const out = [];
  let run = "", hit = false;
  const flush = () => { if (run) out.push(hit ? h("mark", null, run) : run); run = ""; };
  [...text].forEach((ch, i) => {
    const m = set.has(i);
    if (m !== hit) { flush(); hit = m; }
    run += ch;
  });
  flush();
  return out;
}

/** A line of a text hit with the matched words highlighted. */
function markedLine(text, s) {
  if (!s.query) return [text];
  let re;
  try { re = new RegExp(s.regex ? s.query : s.query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), s.caseSensitive ? "g" : "gi"); }
  catch { return [text]; }
  const out = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (!m[0]) break;
    out.push(text.slice(last, m.index), h("mark", null, m[0]));
    last = m.index + m[0].length;
  }
  out.push(text.slice(last));
  return out;
}

function splitPath(p) {
  const i = p.lastIndexOf("/");
  return i < 0 ? ["", p] : [p.slice(0, i + 1), p.slice(i + 1)];
}

/** Open the project search panel. */
function openSearch(mode) {
  if (!state.project) { toast(t("search.pickProject"), true); return; }
  const root = $("#palette-root");
  const s = state.search;
  const pid = state.project.id;
  if (mode) s.mode = mode;
  const close = () => { clearTimeout(timer); seq++; clear(root); };
  const input = h("input", { type: "text", "aria-label": t("search.label"), spellcheck: "false", autocomplete: "off" });
  input.value = s.query;
  const modes = h("div", { class: "seg", role: "tablist" });
  const opts = h("div", { class: "search-opts" });
  const list = h("ul", { class: "palette-list search-list", role: "listbox" });
  const status = h("div", { class: "search-status muted small", "aria-live": "polite" });
  let rows = [], sel = 0, seq = 0, timer = null;

  const drawModes = () => {
    clear(modes).append(...searchModes().map(([m, label]) => h("button", {
      class: "seg-btn" + (s.mode === m ? " active" : ""), role: "tab", "aria-selected": s.mode === m ? "true" : "false",
      onmousedown: (e) => { e.preventDefault(); s.mode = m; refresh(); },
    }, label)));
    input.placeholder = searchModes().find((x) => x[0] === s.mode)[2];
    clear(opts);
    if (s.mode === "text") {
      const check = (label, key, title) => h("label", { class: "check small", title },
        h("input", { type: "checkbox", checked: s[key], onchange: (e) => { s[key] = e.target.checked; run(); } }), label);
      const pathIn = h("input", { type: "text", class: "small", placeholder: t("search.pathPh"), value: s.path, "aria-label": t("search.pathLabel") });
      pathIn.addEventListener("input", () => { s.path = pathIn.value; schedule(); });
      pathIn.addEventListener("keydown", keys);
      opts.append(check(t("search.regex"), "regex", t("search.regex.title")), check(t("search.case"), "caseSensitive", t("search.case.title")), pathIn);
    }
  };
  const drawRows = () => {
    sel = Math.min(sel, Math.max(0, rows.length - 1));
    clear(list).append(...rows.map((r, i) => h("li", {
      class: (i === sel ? "sel " : "") + "search-row", role: "option",
      onmousedown: (e) => { e.preventDefault(); close(); r.open(); },
      onmousemove: () => { if (sel !== i) { sel = i; drawRows(); } },
    }, r.node)));
    const cur = list.children[sel];
    if (cur && cur.scrollIntoView) cur.scrollIntoView({ block: "nearest" });
  };
  const run = async () => {
    clearTimeout(timer);
    const my = ++seq;
    const q = input.value.trim();
    s.query = q;
    const current = () => my === seq && input.isConnected && state.project?.id === pid;
    if (!current()) return;
    rows = [];
    sel = 0;
    drawRows();
    if (s.mode !== "files" && !q) {
      status.textContent = { text: t("search.hint.text"), chats: t("search.hint.chats"), symbols: t("search.hint.symbols") }[s.mode];
      return;
    }
    status.textContent = t("search.searching");
    try {
      if (s.mode === "files") {
        const hits = await invoke("find_files", { projectId: pid, query: q, limit: 60 });
        if (!current()) return;
        rows = hits.map((f) => {
          const [dir, base] = splitPath(f.path);
          const off = dir.length;
          return { open: () => openFile(f.path), node: [
            h("span", { class: "sr-name" }, marked(base, f.matched.filter((i) => i >= off).map((i) => i - off))),
            h("span", { class: "sr-dir muted small" }, marked(dir, f.matched.filter((i) => i < off))),
          ] };
        });
        status.textContent = hits.length ? t("search.files.count", { count: hits.length }) : t("search.files.none");
      } else if (s.mode === "text") {
        const r = await invoke("search_text", { projectId: pid, pattern: q, regex: s.regex, caseSensitive: s.caseSensitive, path: s.path || null, limit: 300 });
        if (!current()) return;
        rows = r.hits.map((x) => ({ open: () => openFile(x.path, x.line), node: [
          h("span", { class: "sr-loc small" }, `${x.path}:${x.line}`),
          h("code", { class: "sr-code" }, markedLine(x.text.trim(), s)),
        ] }));
        const files = new Set(r.hits.map((x) => x.path)).size;
        status.textContent = r.hits.length
          ? (r.truncated ? t("search.text.countMore", { count: r.hits.length, files, searched: r.files_searched }) : t("search.text.count", { count: r.hits.length, files, searched: r.files_searched }))
          : t("search.text.none", { count: r.files_searched });
      } else if (s.mode === "symbols") {
        const syms = await invoke("find_symbols", { projectId: pid, query: q, limit: 60 });
        if (!current()) return;
        rows = syms.map((x) => ({ open: () => openFile(x.path, x.line), node: [
          h("span", { class: "pill sym-kind" }, x.kind),
          h("span", { class: "sr-name" }, x.name),
          h("span", { class: "sr-dir muted small" }, `${x.path}:${x.line}`),
        ] }));
        status.textContent = syms.length ? t("search.symbols.count", { count: syms.length }) : t("search.symbols.none");
      } else {
        const hits = await invoke("chat_search_saved", { projectId: pid, query: q });
        if (!current()) return;
        rows = hits.map((hit) => ({ open: () => openSavedMessage(pid, hit), node: [
          h("span", { class: "sr-loc small" }, hit.title),
          h("span", { class: "sr-code" }, markedLine(hit.excerpt, { query: q })),
        ] }));
        status.textContent = hits.length ? (hits.length === 60 ? t("search.chats.first") : t("search.chats.count", { count: hits.length })) : t("search.chats.none");
      }
    } catch (e) {
      if (!current()) return;
      rows = [];
      status.textContent = errMsg(e);
    }
    drawRows();
  };
  const schedule = () => { clearTimeout(timer); seq++; rows = []; drawRows(); timer = setTimeout(run, s.mode === "files" ? 60 : 180); };
  const refresh = () => { drawModes(); run(); input.focus(); };
  function keys(e) {
    if (e.key === "ArrowDown") { sel = Math.min(sel + 1, rows.length - 1); drawRows(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { sel = Math.max(sel - 1, 0); drawRows(); e.preventDefault(); }
    else if (e.key === "Enter" && rows[sel]) { e.preventDefault(); const r = rows[sel]; close(); r.open(); }
    else if (e.key === "Tab" && e.target === input) {
      e.preventDefault();
      const modes = searchModes();
      const i = modes.findIndex((x) => x[0] === s.mode);
      s.mode = modes[(i + (e.shiftKey ? modes.length - 1 : 1)) % modes.length][0];
      refresh();
    }
    else if (e.key === "Escape") { close(); e.stopPropagation(); }
  }
  input.addEventListener("input", schedule);
  input.addEventListener("keydown", keys);
  clear(root).append(h("div", { class: "modal-back palette-back", onclick: (e) => { if (e.target.classList.contains("palette-back")) close(); } },
    h("div", { class: "palette search", role: "dialog", "aria-label": t("search.dialog") },
      h("div", { class: "search-head" }, modes, h("span", { class: "muted small" }, t("search.tab"))),
      input, opts, status, list)));
  refresh();
  input.select();
}

/** Show a file, read-only, with line numbers and its outline. */
async function openFile(path, line) {
  let f;
  try { f = await invoke("view_file", { projectId: state.project.id, path }); }
  catch (e) { toast(errMsg(e), true); return; }
  const code = h("div", { class: "fv-code", tabindex: "0", "aria-label": t("file.aria", { path: f.path, count: f.lines }) });
  const text = f.text.endsWith("\n") ? f.text.slice(0, -1) : f.text;
  const rowsEls = text.split("\n").map((l, i) => h("div", { class: "fv-line", "data-line": String(i + 1) },
    h("span", { class: "ln" }, String(i + 1)), h("span", { class: "code" }, l || " ")));
  code.append(...rowsEls);
  const go = (n) => {
    for (const el of code.querySelectorAll(".fv-line.hit")) el.classList.remove("hit");
    const el = rowsEls[Math.max(0, Math.min(rowsEls.length - 1, n - 1))];
    if (!el) return;
    el.classList.add("hit");
    code.scrollTop = Math.max(0, el.offsetTop - code.clientHeight / 3);
  };
  const outline = h("nav", { class: "fv-outline", "aria-label": t("file.outline") },
    h("div", { class: "side-label" }, t("file.outline")),
    f.symbols.length
      ? f.symbols.map((x) => h("button", { class: "fv-sym", title: t("file.symAt", { kind: x.kind, line: x.line }), onclick: () => go(x.line) },
        h("span", { class: "pill sym-kind" }, x.kind), h("span", null, x.name), h("span", { class: "muted small" }, String(x.line))))
      : h("div", { class: "muted small" }, t("file.noSymbols")));
  const ref = () => (line ? `${f.path}:${line}` : f.path);
  modal(f.path, [
    h("div", { class: "fv-meta muted small" },
      t("file.meta", { count: f.lines }),
      f.redactions ? h("span", { class: "pill warn" }, t("file.hidden", { count: f.redactions })) : null,
      h("button", { class: "btn ghost small", onclick: () => copy(ref(), t("file.copied", { ref: ref() })) }, t("file.copyPath")),
      h("button", { class: "btn ghost small", onclick: () => { clear($("#modal-root")); openSearch("text"); } }, t("file.searchAgain"))),
    h("div", { class: "fv" }, outline, code),
  ], []);
  const m = $("#modal-root .modal");
  if (m) m.classList.add("wide", "file-viewer");
  // Clicking a line selects it as the reference to copy.
  code.addEventListener("click", (e) => {
    const el = e.target.closest(".fv-line");
    if (el) { line = Number(el.dataset.line); go(line); }
  });
  requestAnimationFrame(() => { if (line) go(line); code.focus(); });
}
