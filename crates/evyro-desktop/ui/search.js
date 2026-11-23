// Code search: find a file by name, search the code for text or a regex, or
// jump to a symbol, then read the file with its outline. Read-only.
// Loaded after app.js and uses its helpers (h, invoke, state, act, toast...).
"use strict";

const SEARCH_MODES = [
  ["files", "Files", "Go to a file by name…"],
  ["text", "Text", "Search the code…"],
  ["symbols", "Symbols", "Go to a function, type or class…"],
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

/** Open the search panel in a mode ("files", "text" or "symbols"). */
function openSearch(mode) {
  if (!state.project) { toast("Pick a project first", true); return; }
  const root = $("#palette-root");
  const s = state.search;
  if (mode) s.mode = mode;
  const close = () => clear(root);
  const input = h("input", { type: "text", "aria-label": "Search", spellcheck: "false", autocomplete: "off" });
  input.value = s.query;
  const modes = h("div", { class: "seg", role: "tablist" });
  const opts = h("div", { class: "search-opts" });
  const list = h("ul", { class: "palette-list search-list", role: "listbox" });
  const status = h("div", { class: "search-status muted small", "aria-live": "polite" });
  let rows = [], sel = 0, seq = 0, timer = null;

  const drawModes = () => {
    clear(modes).append(...SEARCH_MODES.map(([m, label]) => h("button", {
      class: "seg-btn" + (s.mode === m ? " active" : ""), role: "tab", "aria-selected": s.mode === m ? "true" : "false",
      onmousedown: (e) => { e.preventDefault(); s.mode = m; refresh(); },
    }, label)));
    input.placeholder = SEARCH_MODES.find((x) => x[0] === s.mode)[2];
    clear(opts);
    if (s.mode === "text") {
      const check = (label, key, title) => h("label", { class: "check small", title },
        h("input", { type: "checkbox", checked: s[key], onchange: (e) => { s[key] = e.target.checked; run(); } }), label);
      const pathIn = h("input", { type: "text", class: "small", placeholder: "in paths containing…", value: s.path, "aria-label": "Only paths containing" });
      pathIn.addEventListener("input", () => { s.path = pathIn.value; schedule(); });
      pathIn.addEventListener("keydown", keys);
      opts.append(check(".* Regex", "regex", "Treat the search as a regular expression"), check("Aa Match case", "caseSensitive", "Match upper and lower case exactly"), pathIn);
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
    const pid = state.project.id;
    if (s.mode !== "files" && !q) { rows = []; status.textContent = s.mode === "text" ? "Type to search every file Git knows about." : "Type part of a name."; drawRows(); return; }
    status.textContent = "Searching…";
    try {
      if (s.mode === "files") {
        const hits = await invoke("find_files", { projectId: pid, query: q, limit: 60 });
        if (my !== seq) return;
        rows = hits.map((f) => {
          const [dir, base] = splitPath(f.path);
          const off = dir.length;
          return { open: () => openFile(f.path), node: [
            h("span", { class: "sr-name" }, marked(base, f.matched.filter((i) => i >= off).map((i) => i - off))),
            h("span", { class: "sr-dir muted small" }, marked(dir, f.matched.filter((i) => i < off))),
          ] };
        });
        status.textContent = hits.length ? `${hits.length} file${hits.length === 1 ? "" : "s"}` : "No file matches";
      } else if (s.mode === "text") {
        const r = await invoke("search_text", { projectId: pid, pattern: q, regex: s.regex, caseSensitive: s.caseSensitive, path: s.path || null, limit: 300 });
        if (my !== seq) return;
        rows = r.hits.map((x) => ({ open: () => openFile(x.path, x.line), node: [
          h("span", { class: "sr-loc small" }, `${x.path}:${x.line}`),
          h("code", { class: "sr-code" }, markedLine(x.text.trim(), s)),
        ] }));
        const files = new Set(r.hits.map((x) => x.path)).size;
        status.textContent = r.hits.length
          ? `${r.hits.length}${r.truncated ? "+" : ""} match${r.hits.length === 1 ? "" : "es"} in ${files} file${files === 1 ? "" : "s"} (${r.files_searched} searched)${r.truncated ? ". Showing the first ones; narrow the search to see more." : ""}`
          : `No matches in ${r.files_searched} files`;
      } else {
        const syms = await invoke("find_symbols", { projectId: pid, query: q, limit: 60 });
        if (my !== seq) return;
        rows = syms.map((x) => ({ open: () => openFile(x.path, x.line), node: [
          h("span", { class: "pill sym-kind" }, x.kind),
          h("span", { class: "sr-name" }, x.name),
          h("span", { class: "sr-dir muted small" }, `${x.path}:${x.line}`),
        ] }));
        status.textContent = syms.length ? `${syms.length} symbol${syms.length === 1 ? "" : "s"}` : "No symbol matches";
      }
    } catch (e) {
      if (my !== seq) return;
      rows = [];
      status.textContent = errMsg(e);
    }
    sel = 0;
    drawRows();
  };
  const schedule = () => { clearTimeout(timer); timer = setTimeout(run, s.mode === "files" ? 60 : 180); };
  const refresh = () => { drawModes(); run(); input.focus(); };
  function keys(e) {
    if (e.key === "ArrowDown") { sel = Math.min(sel + 1, rows.length - 1); drawRows(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { sel = Math.max(sel - 1, 0); drawRows(); e.preventDefault(); }
    else if (e.key === "Enter" && rows[sel]) { e.preventDefault(); const r = rows[sel]; close(); r.open(); }
    else if (e.key === "Tab" && e.target === input) {
      e.preventDefault();
      const i = SEARCH_MODES.findIndex((x) => x[0] === s.mode);
      s.mode = SEARCH_MODES[(i + (e.shiftKey ? SEARCH_MODES.length - 1 : 1)) % SEARCH_MODES.length][0];
      refresh();
    }
    else if (e.key === "Escape") { close(); e.stopPropagation(); }
  }
  input.addEventListener("input", schedule);
  input.addEventListener("keydown", keys);
  clear(root).append(h("div", { class: "modal-back palette-back", onclick: (e) => { if (e.target.classList.contains("palette-back")) close(); } },
    h("div", { class: "palette search", role: "dialog", "aria-label": "Search the code" },
      h("div", { class: "search-head" }, modes, h("span", { class: "muted small" }, "Tab switches")),
      input, opts, status, list)));
  refresh();
  input.select();
}

/** Show a file, read-only, with line numbers and its outline. */
async function openFile(path, line) {
  let f;
  try { f = await invoke("view_file", { projectId: state.project.id, path }); }
  catch (e) { toast(errMsg(e), true); return; }
  const code = h("div", { class: "fv-code", tabindex: "0", "aria-label": `${f.path}, ${f.lines} lines` });
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
  const outline = h("nav", { class: "fv-outline", "aria-label": "Outline" },
    h("div", { class: "side-label" }, "Outline"),
    f.symbols.length
      ? f.symbols.map((x) => h("button", { class: "fv-sym", title: `${x.kind} at line ${x.line}`, onclick: () => go(x.line) },
        h("span", { class: "pill sym-kind" }, x.kind), h("span", null, x.name), h("span", { class: "muted small" }, String(x.line))))
      : h("div", { class: "muted small" }, "No symbols found in this file."));
  const ref = () => (line ? `${f.path}:${line}` : f.path);
  modal(f.path, [
    h("div", { class: "fv-meta muted small" },
      `${f.lines} lines · read only`,
      f.redactions ? h("span", { class: "pill warn" }, `${f.redactions} secret${f.redactions === 1 ? "" : "s"} hidden`) : null,
      h("button", { class: "btn ghost small", onclick: () => copy(ref(), `Copied ${ref()}`) }, "Copy path"),
      h("button", { class: "btn ghost small", onclick: () => { clear($("#modal-root")); openSearch("text"); } }, "Search again")),
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
