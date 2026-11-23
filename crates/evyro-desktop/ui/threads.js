// Agent threads: coding agents and shells running in terminals inside the window.
// Loaded after app.js and uses its helpers (h, invoke, state, act, toast...).
"use strict";

// One xterm per thread, kept across screen changes so nothing is replayed.
const XTERMS = new Map(); // id -> { term, fit, el, end, queue, ready }
state.threads = [];
state.thread = null; // selected thread id

function termTheme() {
  const css = getComputedStyle(document.documentElement);
  const v = (n) => css.getPropertyValue(n).trim();
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  return {
    background: dark ? "#0c0e14" : "#ffffff",
    foreground: v("--text") || (dark ? "#e7e9ee" : "#1b1d24"),
    cursor: v("--accent") || "#4f46e5",
    selectionBackground: dark ? "#2f2c6b" : "#dcd9fb",
  };
}

// Follow the system light/dark switch.
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  for (const x of XTERMS.values()) x.term.options.theme = termTheme();
});

async function loadThreads() {
  try { state.threads = await invoke("term_list"); } catch { state.threads = []; }
  return state.threads;
}

/** Start an agent or shell thread for the current project and show it. */
async function startThread({ kind, target = null, task = null, handoffId = null }) {
  // Start at the size the pane will have, so the program does not redraw at once.
  const v = $("#view");
  const size = state.termSize || { cols: Math.max(40, Math.floor((v.clientWidth - 320) / 8.1)), rows: Math.max(10, Math.floor((v.clientHeight - 140) / 17)) };
  const info = await act(() => invoke("term_open", { projectId: state.project.id, kind, target, task, handoffId, ...size }));
  state.threads.push(info);
  state.thread = info.id;
  ensureXterm(info.id, true);
  setTab("threads");
  toast(kind === "agent" ? `Started ${info.title}. It picks up the task and any handoff here.` : "Shell started.");
  return true;
}

/** Create the xterm for a thread. `fresh` threads stream from the start; others load a snapshot first. */
function ensureXterm(id, fresh = false) {
  if (XTERMS.has(id)) return XTERMS.get(id);
  const el = h("div", { class: "term-host" });
  const term = new Terminal({
    fontFamily: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    fontSize: 13, cursorBlink: true, scrollback: 5000, allowProposedApi: false, theme: termTheme(),
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  const x = { term, fit, el, end: 0, queue: [], ready: fresh, opened: false };
  XTERMS.set(id, x);
  // Fit to the pane whenever its size changes (window resize, layout settling).
  new ResizeObserver(() => fitThread(id)).observe(el);
  term.onData((d) => invoke("term_write", { id, data: d }).catch(() => {}));
  if (!fresh) {
    invoke("term_snapshot", { id }).then((snap) => {
      term.write(snap.data);
      x.end = snap.end;
      for (const ev of x.queue) if (ev.end > x.end) { term.write(ev.data); x.end = ev.end; }
      x.queue = []; x.ready = true;
    }).catch(() => { x.ready = true; });
  }
  return x;
}

function threadOutput(p) {
  if (!p) return;
  const x = XTERMS.get(p.id);
  if (!x) return; // not shown yet: its snapshot will include this
  if (!x.ready) { x.queue.push(p); return; }
  if (p.end && p.end <= x.end) return;
  x.term.write(p.data);
  if (p.end) x.end = p.end;
}

function threadExit(p) {
  const t = state.threads.find((t) => t.id === p.id);
  if (t) { t.running = false; t.exit_code = p.code; }
  const x = XTERMS.get(p.id);
  if (x) x.term.write(`\r\n\x1b[2m[${t ? t.title : "thread"} ended${p.code != null ? `, exit code ${p.code}` : ""}]\x1b[0m\r\n`);
  if (state.view === "threads") { drawThreadList(); if (state.thread === p.id) drawThreadPane(); }
  updateLiveBadge();
}

async function closeThread(id) {
  const t = state.threads.find((t) => t.id === id);
  if (t && t.running && !(await confirmDialog(`Stop ${t.title}?`, "The program is still running. Evyro stops it; its last checkpoint and session stay recorded."))) return;
  await act(() => invoke("term_close", { id }));
  const x = XTERMS.get(id);
  if (x) { x.term.dispose(); XTERMS.delete(id); }
  state.threads = state.threads.filter((t) => t.id !== id);
  if (state.thread === id) state.thread = null;
  render();
}

function confirmDialog(title, text) {
  return new Promise((resolve) => {
    const root = $("#modal-root");
    const done = (v) => { clear(root); resolve(v); };
    root.append(h("div", { class: "modal-back" }, h("div", { class: "modal", role: "dialog" },
      h("h2", null, title), h("p", { class: "muted" }, text),
      h("div", { class: "actions" },
        h("button", { class: "btn", onclick: () => done(false) }, "Cancel"),
        h("button", { class: "btn primary", onclick: () => done(true) }, "Stop it")))));
  });
}

async function newThreadDialog() {
  const sel = await targetSelect("claude-code");
  // Only coding agents and API targets run in a terminal.
  for (const g of [...sel.querySelectorAll("optgroup")]) {
    if (![KIND_LABEL.harness, KIND_LABEL.api].includes(g.label)) g.remove();
  }
  if (!sel.value && sel.options.length) sel.value = sel.options[0].value;
  let tasks = [];
  try { tasks = await invoke("list_tasks", { projectId: state.project.id }); } catch { /* no tasks */ }
  const taskSel = h("select", null, h("option", { value: "" }, "No task (let the AI ask)"),
    tasks.map((r) => h("option", { value: r.task.key }, `${r.task.key}  ${r.task.title}`)));
  const active = tasks.find((r) => r.task.status === "in_progress") || tasks.find((r) => r.task.status === "ready");
  if (active) taskSel.value = active.task.key;
  modal("New agent thread", [
    h("p", { class: "muted small" }, "The agent runs in this window through ", h("code", null, "evyro run"), ", so its session, checkpoints and any waiting handoff are tracked as usual."),
    field("AI", sel, "Coding agents and API-key targets. IDEs and chat apps open from AI targets."),
    field("Task", taskSel),
  ], [
    { label: "Open in Terminal instead", run: () => openInTerminal(sel.value, taskSel.value || null) },
    { label: "Start", primary: true, run: () => (sel.value ? startThread({ kind: "agent", target: sel.value, task: taskSel.value || null }) : false) },
  ]);
}

function threadStatus(t) {
  if (t.running) return pill("running");
  return pill(t.exit_code === 0 ? "" : "bad", t.exit_code == null ? "ended" : `exit ${t.exit_code}`);
}

function drawThreadList() {
  const list = $("#thread-list");
  if (!list) return;
  const mine = state.threads.filter((t) => !state.project || t.project_id === state.project.id);
  clear(list).append(...mine.map((t) => h("button", {
    class: "thread-item" + (t.id === state.thread ? " active" : ""),
    onclick: () => { state.thread = t.id; drawThreadList(); drawThreadPane(); },
  }, h("div", { class: "thread-row" }, h("span", { class: "thread-title" }, t.title), threadStatus(t)),
    h("div", { class: "muted small" }, t.kind === "agent" ? (t.task ? `task ${t.task}` : "no task") : "shell", " · ", agoSecs(t.started_at)))));
  if (!mine.length) list.append(h("div", { class: "muted small empty-threads" }, "No threads yet."));
}
const agoSecs = (s) => ago(new Date(s * 1000).toISOString());

function drawThreadPane() {
  const pane = $("#thread-pane");
  if (!pane) return;
  clear(pane);
  const t = state.threads.find((t) => t.id === state.thread && (!state.project || t.project_id === state.project.id));
  if (!t) {
    pane.append(h("div", { class: "thread-empty" },
      h("h2", null, "Work with an AI right here"),
      h("p", { class: "muted" }, "Start Claude Code, Codex, Gemini or any coding agent in a thread. When it stops or hits a limit, hand off and start the next one in another thread. Every thread is tracked as a session."),
      h("div", { class: "row" },
        h("button", { class: "btn primary", onclick: newThreadDialog }, "New agent thread"),
        h("button", { class: "btn", onclick: () => startThread({ kind: "shell" }) }, "New shell"))));
    return;
  }
  const x = ensureXterm(t.id);
  pane.append(h("div", { class: "thread-head" },
    h("div", null, h("div", { class: "thread-name" }, t.title, " ", threadStatus(t)), h("div", { class: "muted small mono" }, t.command)),
    h("div", { class: "row" },
      t.kind === "agent" && t.running ? h("button", { class: "btn small", onclick: () => handoffDialog(t.task, null) }, "Hand off") : null,
      h("button", { class: "btn small", onclick: () => closeThread(t.id) }, t.running ? "Stop" : "Close"))),
  x.el);
  if (!x.opened) { x.term.open(x.el); x.opened = true; }
  requestAnimationFrame(() => fitThread(t.id));
  x.term.focus();
}

function fitThread(id) {
  const x = XTERMS.get(id);
  if (!x || !x.el.isConnected) return;
  try { x.fit.fit(); } catch { return; }
  const { cols, rows } = x.term;
  if (x.cols !== cols || x.rows !== rows) {
    x.cols = cols; x.rows = rows;
    state.termSize = { cols, rows };
    invoke("term_resize", { id, cols, rows }).catch(() => {});
  }
}

async function viewThreads(v) {
  await loadThreads();
  const mine = state.threads.filter((t) => t.project_id === state.project.id);
  if (!mine.some((t) => t.id === state.thread)) state.thread = mine.length ? mine[mine.length - 1].id : null;
  v.append(h("div", { class: "threads" },
    h("div", { class: "thread-side" },
      h("div", { class: "row" },
        h("button", { class: "btn primary small", onclick: newThreadDialog }, "+ Agent"),
        h("button", { class: "btn small", onclick: () => startThread({ kind: "shell" }) }, "+ Shell")),
      h("nav", { id: "thread-list", class: "thread-list" })),
    h("div", { id: "thread-pane", class: "thread-pane" })));
  drawThreadList();
  drawThreadPane();
}
