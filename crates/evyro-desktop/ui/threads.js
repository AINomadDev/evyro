// Agent threads: coding agents and shells running in terminals inside the window.
// Loaded after app.js and uses its helpers (h, invoke, state, act, toast...).
"use strict";

// One xterm per thread, kept across screen changes so nothing is replayed.
const XTERMS = new Map();
state.threads = [];
state.thread = null; // selected thread id

function termTheme() {
  const css = getComputedStyle(document.documentElement);
  const v = (n) => css.getPropertyValue(n).trim();
  const dark = colorScheme() === "dark";
  return {
    background: v("--term-bg") || (dark ? "#0c0e14" : "#ffffff"),
    foreground: v("--text") || (dark ? "#e7e9ee" : "#1b1d24"),
    cursor: accentColor(),
    selectionBackground: dark ? "#2f2c6b" : "#dcd9fb",
  };
}

async function loadThreads() {
  let terms = [];
  try { terms = await invoke("term_list"); } catch { terms = []; }
  const chats = typeof loadChats === "function" ? await loadChats() : [];
  state.threads = [...terms, ...chats].sort((a, b) => a.started_at - b.started_at);
  return state.threads;
}

/** Pick up "looks stuck" flags the app works out, without reloading everything. */
async function refreshStuck() {
  // Only worth asking while an AI is running in a terminal.
  if (!(state.threads || []).some((t) => t.running && t.kind === "agent")) return;
  let terms;
  try { terms = await invoke("term_list"); } catch { return; }
  const changed = [];
  for (const info of terms) {
    const t = state.threads.find((x) => x.id === info.id);
    if (t && t.kind !== "chat" && ((t.stuck || null) !== (info.stuck || null) || (t.limit_hit || null) !== (info.limit_hit || null))) {
      const newLimit = info.limit_hit && !t.limit_hit;
      t.stuck = info.stuck || null;
      t.limit_hit = info.limit_hit || null;
      changed.push(t.id);
      if (newLimit) toast(I18N.t("thread.limitToast", { title: t.title }), true);
    }
  }
  if (!changed.length) return;
  drawThreadList();
  for (const id of changed) if (panes().includes(id)) drawThreadPane(id);
}
setInterval(refreshStuck, 5000);

/** Start an agent or shell thread for the current project and show it. */
async function startThread({ kind, target = null, task = null, handoffId = null, model = null, title = null, resumeId = null, creditBudget = null, planFirst = false }) {
  // Start at the size the pane will have, so the program does not redraw at once.
  const v = $("#view");
  const size = state.termSize || { cols: Math.max(40, Math.floor((v.clientWidth - 320) / 8.1)), rows: Math.max(10, Math.floor((v.clientHeight - 140) / 17)) };
  const projectId = state.project.id;
  const info = await act(() => invoke("term_open", { projectId, kind, target, task, handoffId, model, title, resumeId, creditBudget, planFirst, ...size }));
  if (resumeId) {
    XTERMS.get(info.id)?.term.dispose();
    XTERMS.delete(info.id);
    state.threads = state.threads.map((t) => t.id === info.id ? info : t);
  } else {
    state.threads.push(info);
  }
  ensureXterm(info.id);
  if (kind === "shell" && !resumeId && state.termDrawer && !state.drawerThread) state.drawerThread = info.id;
  else if (state.drawerThread !== info.id) placeThread(info.id, !resumeId);
  setTab("workspace");
  if (kind === "agent") prepareTerminalDraft(info, handoffId);
  toast(resumeId ? t("thread.resumed", { title: info.title }) : kind === "agent" ? t("thread.started", { title: info.title }) : t("thread.shellStarted"));
  return true;
}

/** Load the snapshot before applying live output, including output from a fast startup. */
function ensureXterm(id) {
  const info = state.threads.find((t) => t.id === id);
  const running = !!info?.running;
  const existing = XTERMS.get(id);
  if (existing) {
    existing.running = running;
    existing.el.classList.toggle("saved-size", !running && existing.savedSize);
    existing.term.options.disableStdin = !running;
    existing.term.options.cursorBlink = running && !reducedMotion();
    return existing;
  }
  const savedSize = info?.cols > 0 && info?.rows > 0;
  const el = h("div", { class: "term-host" + (savedSize && !running ? " saved-size" : "") });
  const term = new Terminal({
    ...(savedSize ? { cols: info.cols, rows: info.rows } : {}),
    fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--mono").trim() || "ui-monospace, Menlo, Consolas, monospace",
    fontSize: appearance().termSize, cursorBlink: running && !reducedMotion(), disableStdin: !running, scrollback: 5000, allowProposedApi: false, theme: termTheme(),
    // Keep agent output readable: xterm raises any color below WCAG AA (4.5:1).
    minimumContrastRatio: 4.5,
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  const x = { term, fit, el, end: 0, queue: [], opened: false,
    running, savedSize, cols: info?.cols, rows: info?.rows };
  XTERMS.set(id, x);
  // Fit to the pane whenever its size changes (window resize, layout settling).
  new ResizeObserver(() => fitThread(id)).observe(el);
  // Replaying saved output makes xterm answer the old terminal queries in it (cursor position,
  // device attributes). Sent to the live program those replies show up as typed text ("3R3R3R").
  term.onData((d) => { if (!x.replaying) invoke("term_write", { id, data: d }).catch(() => {}); });
  // Tab belongs to the program in the terminal (shell completion), so F6 is
  // the way out for keyboard users: it moves focus to the thread's buttons.
  term.attachCustomKeyEventHandler((e) => {
    if (e.key !== "F6") return true;
    if (e.type === "keydown") {
      const b = el.closest(".thread-pane, .view, main")?.querySelector(".thread-head button");
      if (b) { b.focus(); e.preventDefault(); }
    }
    return false;
  });
  x.loaded = (async () => {
    try {
      const snap = await invoke("term_snapshot", { id, attach: true });
      x.replaying = true;
      if (snap.data) await new Promise((resolve) => term.write(snap.data, resolve));
      x.replaying = false;
      if (snap.live) await new Promise((resolve) => term.write(snap.live, resolve));
      x.end = snap.end;
    } catch { x.replaying = false; /* live output is still available */ }
    const queue = x.queue;
    x.queue = null;
    for (const ev of queue) threadOutput(ev);
    fitThread(id);
  })();
  return x;
}

function threadOutput(p) {
  if (!p) return;
  const x = XTERMS.get(p.id);
  if (!x) return; // not shown yet: its snapshot will include this
  if (x.queue) { x.queue.push(p); return; }
  if (p.end && p.end <= x.end) return;
  x.term.write(p.data);
  if (p.end) x.end = p.end;
}

function threadExit(p) {
  const t = state.threads.find((t) => t.id === p.id);
  if (t) { t.running = false; t.exit_code = p.code; t.credits_used = p.credits_used; }
  const x = XTERMS.get(p.id);
  if (x) {
    x.running = false;
    x.savedSize = true;
    x.el.classList.add("saved-size");
    x.term.options.disableStdin = true;
    x.term.options.cursorBlink = false;
    const name = t ? t.title : I18N.t("thread.word");
    x.term.write(`\r\n\x1b[2m[${p.code != null ? I18N.t("thread.endedCode", { title: name, code: p.code }) : I18N.t("thread.ended", { title: name })}]\x1b[0m\r\n`);
  }
  drawThreadList();
  if (state.view === "workspace" && state.drawerThread === p.id) render();
  else drawThreadPane(p.id);
  updateLiveBadge();
}

async function closeThread(id) {
  const t = state.threads.find((t) => t.id === id);
  if (t && t.kind === "chat") {
    if (t.running && !(await confirmDialog(I18N.t("thread.endChatTitle", { agent: t.agent || t.target }), I18N.t("thread.endChatBody")))) return;
    if (t.archived && !(await confirmDialog(I18N.t("thread.deleteTitle"), I18N.t("thread.deleteBody"), I18N.t("thread.deleteAction")))) return;
    await act(() => invoke("chat_close", { id, forget: !!t.archived }));
    if (state.project?.id === t.project_id) {
      const slots = panes();
      const index = slots.indexOf(id);
      if (index >= 0) slots[index] = null;
    }
    if (state.focusOverlay === id) state.focusOverlay = null;
    CHATS.delete(id);
    state.threads = state.threads.filter((x) => x.id !== id);
    if (state.thread === id) state.thread = null;
    render();
    return;
  }
  if (t && t.running && !(await confirmDialog(I18N.t("thread.stopTitle", { title: t.title }), I18N.t("thread.stopBody")))) return;
  await act(() => invoke("term_close", { id }));
  const x = XTERMS.get(id);
  if (x) { x.term.dispose(); XTERMS.delete(id); }
  state.threads = state.threads.filter((t) => t.id !== id);
  if (state.thread === id) state.thread = null;
  render();
}

function confirmDialog(title, text, action = t("thread.stopIt")) {
  return new Promise((resolve) => {
    const root = $("#modal-root");
    const done = (v) => { clear(root); resolve(v); };
    root.append(h("div", { class: "modal-back" }, h("div", { class: "modal", role: "dialog" },
      h("h2", null, title), h("p", { class: "muted" }, text),
      h("div", { class: "actions" },
        h("button", { class: "btn", onclick: () => done(false) }, t("common.cancel")),
        h("button", { class: "btn primary", onclick: () => done(true) }, action)))));
  });
}

async function newThreadDialog(taskKey = null) {
  const title = h("input", { type: "text", maxlength: "80", placeholder: t("common.optional") });
  const budget = h("input", { type: "number", min: "30", max: "4294967295", step: "1", placeholder: t("common.optional") });
  const budgetField = field(t("thread.budget"), budget, t("thread.budgetHint"));
  const sel = await targetSelect("claude-code");
  // Only coding agents and API targets run in a terminal.
  for (const g of [...sel.querySelectorAll("optgroup")]) {
    if (![KIND_LABEL.harness, KIND_LABEL.api].includes(g.label)) g.remove();
  }
  if (!sel.value && sel.options.length) sel.value = sel.options[0].value;
  let tasks = [];
  try { tasks = await invoke("list_tasks", { projectId: state.project.id }); } catch { /* no tasks */ }
  const taskSel = h("select", null, h("option", { value: "" }, t("thread.autoTask")),
    tasks.map((r) => h("option", { value: r.task.key }, `${r.task.key}  ${r.task.title}`)));
  const active = tasks.find((r) => r.task.status === "in_progress") || tasks.find((r) => r.task.status === "ready");
  if (active) taskSel.value = active.task.key;
  if (taskKey) taskSel.value = taskKey;
  // The same AI and account already running here: usually better to use that thread.
  const dupNote = h("div", { class: "dup-note", role: "status" });
  const setup = h("div", { class: "hint" });
  const checkDup = () => {
    const harness = (state.targets || []).find((r) => r.target.name === sel.value)?.target.harness || sel.value;
    const hint = {
      cursor: I18N.t("thread.hint.cursor"),
      copilot: I18N.t("thread.hint.copilot"),
    }[harness];
    setup.textContent = hint || "";
    budgetField.hidden = harness !== "copilot";
    clear(dupNote);
    const same = (state.threads || []).filter((t) => t.project_id === state.project.id && t.kind === "agent" && t.running && t.target === sel.value);
    if (!same.length) return;
    const onTask = same.find((t) => (t.task || null) === (taskSel.value || null));
    const t = onTask || same[0];
    dupNote.append(h("div", { class: "banner warn" },
      h("span", null, onTask
        ? I18N.t("thread.dupSame", { title: t.title, task: taskSel.value || I18N.t("thread.noTask") })
        : I18N.t("thread.dupOther", { target: sel.value, tasks: same.map((x) => x.task || I18N.t("thread.noTask")).join(", ") })),
      h("button", { class: "btn small", onclick: () => { clear($("#modal-root")); placeThread(t.id); setTab("workspace"); } }, I18N.t("thread.showIt"))));
  };
  const planBox = h("input", { type: "checkbox", id: "plan-first" });
  const planField = h("label", { class: "check", for: "plan-first" }, planBox,
    h("span", null, t("thread.planFirst")));
  const showPlan = () => { planField.hidden = !taskSel.value; if (!taskSel.value) planBox.checked = false; };
  sel.addEventListener("change", checkDup);
  taskSel.addEventListener("change", checkDup);
  taskSel.addEventListener("change", showPlan);
  showPlan();
  checkDup();
  modal(t("thread.newTitle"), [
    h("p", { class: "muted small" }, t("thread.newIntro")),
    field(t("thread.ai"), sel),
    setup,
    field(t("thread.name"), title),
    budgetField,
    field(t("thread.task"), taskSel),
    planField,
    dupNote,
  ], [
    { label: t("thread.openTerminal"), run: () => openInTerminal(sel.value, taskSel.value || null) },
    { label: t("common.start"), primary: true, run: () => {
      if (!sel.value || (!budgetField.hidden && !budget.reportValidity())) return false;
      const creditBudget = !budgetField.hidden && budget.value !== "" ? budget.valueAsNumber : null;
      return startThread({ kind: "agent", target: sel.value, task: taskSel.value || null, title: title.value.trim() || null, creditBudget, planFirst: planBox.checked });
    } },
  ]);
}

function threadStatus(t) {
  if (t.kind === "chat") {
    if (t.archived) return pill("", I18N.t("thread.st.saved"));
    if (t.state === "failed") return pill("bad", I18N.t("thread.st.failed"));
    if (!t.running) return pill("", I18N.t("thread.st.ended"));
    if (t.state === "starting") return pill("warn", I18N.t("thread.st.starting"));
    const asking = (CHATS.get(t.id)?.log || []).some((it) => it.t === "permission") && chatBlocks(CHATS.get(t.id).log).some((b) => b.k === "perm" && b.answer === undefined);
    if (asking) return pill("warn", I18N.t("thread.st.needsYou"));
    return t.busy ? pill("running", I18N.t("thread.st.working")) : pill("ok", I18N.t("thread.st.yourTurn"));
  }
  if (t.running) return t.limit_hit ? pill("warn", I18N.t("thread.st.limit")) : t.stuck ? pill("warn", I18N.t("thread.st.stuck")) : pill("running", I18N.t("thread.st.running"));
  if (t.stopped_by_app) return pill("", I18N.t("thread.st.stoppedApp"));
  return pill(t.exit_code === 0 ? "" : "bad", t.exit_code == null ? I18N.t("thread.st.ended") : I18N.t("thread.st.exit", { code: t.exit_code }));
}

const agoSecs = (s) => ago(new Date(s * 1000).toISOString());

function fitThread(id) {
  const x = XTERMS.get(id);
  if (!x?.opened || !x.el.getClientRects().length || (x.savedSize && (x.queue || !x.running))) return;
  try { x.fit.fit(); } catch { return; }
  if (!x.running) return;
  const { cols, rows } = x.term;
  if (x.cols !== cols || x.rows !== rows) {
    x.cols = cols; x.rows = rows;
    x.savedSize = true;
    state.termSize = { cols, rows };
    invoke("term_resize", { id, cols, rows }).catch(() => {});
  }
}

// The thread list, panes and workspace live in workspace.js.
