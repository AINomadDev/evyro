// Workspace: several AI threads side by side, the thread list in the sidebar,
// a shared-context panel every AI in the project draws from, a composer that
// messages several AIs at once, and a status bar.
// Loaded after app.js, threads.js, chat.js and review.js and uses their helpers.
// Everything from agents, repositories and the database goes in as text, never HTML.
"use strict";

const MAX_PANES = 24;
const MAX_COLUMNS = 6;
const SIDE_BY_SIDE = "side-by-side";

// ---------- small persisted preferences (per viewer, optional) ----------
function pref(key, fallback) {
  try { const v = localStorage.getItem("evyro." + key); return v === null ? fallback : JSON.parse(v); } catch { return fallback; }
}
function setPref(key, value) {
  try { localStorage.setItem("evyro." + key, JSON.stringify(value)); } catch { /* storage unavailable: keep it for this run only */ }
}

state.layoutsBy = pref("layoutsBy", {});
state.panesBy = pref("panesBy", {}); // project id -> [thread id | null] per pane, kept across restarts
state.focus = 0; // index of the focused pane
state.focusMode = false;
state.focusOverlay = null;
state.ctxOpen = pref("ctxOpen", appearance().inspector && window.innerWidth >= 1180);
state.ctxPane = "context"; // inspector tab: context | changes
state.termDrawer = pref("termDrawer", false); // bottom terminal drawer open
state.drawerThread = null; // shell thread shown in the drawer
state.ctx = null; // last shared-context snapshot for the current project
const workspaceDrafts = new Map();
const terminalDrafts = new Map();

const AGENT_HUE = { "claude-code": 18, claude: 18, codex: 170, gemini: 220, aider: 280, opencode: 130, cursor: 250, shell: 215 };
function agentKey(t) { return t.kind === "shell" ? "shell" : String(t.target || t.agent || t.title || "?").toLowerCase(); }
/** A small colored badge with the agent's first letter, so threads are easy to tell apart. */
function agentBadge(t, small) {
  const k = agentKey(t);
  let hue = AGENT_HUE[k];
  if (hue === undefined) { hue = 0; for (const c of k) hue = (hue * 31 + c.charCodeAt(0)) % 360; }
  return h("span", { class: "agent-badge" + (small ? " small" : ""), style: `--hue:${hue}`, "aria-hidden": "true" }, t.kind === "shell" ? "$" : k.charAt(0).toUpperCase());
}

/** Whether a thread is waiting on the person (a chat asking permission). */
function needsYou(t) {
  if (t.kind !== "chat" || !t.running) return false;
  const d = CHATS.get(t.id);
  return !!d && chatBlocks(d.log).some((b) => b.k === "perm" && b.answer === undefined);
}
const projThreads = () => (state.project ? state.threads.filter((t) => t.project_id === state.project.id) : []);

function layoutValue(value, min, max, fallback) {
  return Number.isInteger(value) && value >= min && value <= max ? value : fallback;
}

function workspaceLayout() {
  const saved = state.layoutsBy[state.project?.id];
  const legacy = pref("layout", null);
  const fallback = layoutValue(legacy, 1, MAX_PANES, appearance().panes);
  const columns = saved?.columns ?? (legacy === 2 ? SIDE_BY_SIDE : 0);
  return {
    count: layoutValue(saved?.count, 1, MAX_PANES, fallback),
    columns: columns === SIDE_BY_SIDE ? columns : layoutValue(columns, 0, MAX_COLUMNS, 0),
  };
}

// ---------- panes ----------
/** The pane slots for the current project, cleaned of closed threads and filled to the layout. */
function panes() {
  if (!state.project) return [];
  const { count } = workspaceLayout();
  const mine = projThreads().filter((t) => t.id !== state.drawerThread);
  const ids = new Set(mine.map((t) => t.id));
  const slots = state.panesBy[state.project.id];
  let p = (slots || []).map((id) => (ids.has(id) ? id : null));
  if (!p.some(Boolean) && !slots?.length) {
    p = mine.slice().sort((a, b) => (b.running - a.running) || (b.started_at - a.started_at)).slice(0, count).map((t) => t.id);
  }
  // Grow or shrink to the layout; a thread pushed out of view stays in the sidebar.
  if (p.length > count) {
    const keep = p.filter(Boolean).slice(0, count);
    p = [...keep, ...Array(count - keep.length).fill(null)];
  }
  while (p.length < count) p.push(null);
  state.panesBy[state.project.id] = p;
  if (JSON.stringify(pref("panesBy", {})[state.project.id] || null) !== JSON.stringify(p)) setPref("panesBy", state.panesBy);
  if (state.focus >= p.length) state.focus = 0;
  if (!ids.has(state.focusOverlay)) state.focusOverlay = null;
  state.thread = (state.focusMode && state.focusOverlay) || p[state.focus] || p.find(Boolean) || null;
  return p;
}

/** Show a thread: focus it if it is already in a pane, else put it in an empty or the focused pane. */
function placeThread(id, expand = false) {
  if (state.drawerThread === id) state.drawerThread = null;
  const p = panes();
  let i = p.indexOf(id);
  if (i < 0 && state.focusMode && !expand) {
    state.focusOverlay = id;
    state.thread = id;
    return;
  }
  state.focusOverlay = null;
  if (i < 0 && expand && p.every(Boolean) && p.length < MAX_PANES) {
    i = p.length;
    p.push(id);
    saveWorkspaceLayout({ ...workspaceLayout(), count: p.length });
  }
  if (i < 0) {
    // Prefer an empty pane, then one showing a thread that ended, then the focused pane.
    const ended = (x) => { const t = state.threads.find((y) => y.id === x); return !t || !t.running; };
    if (!p[state.focus]) i = state.focus;
    else if (p.indexOf(null) >= 0) i = p.indexOf(null);
    else if (ended(p[state.focus])) i = state.focus;
    else { i = p.findIndex(ended); if (i < 0) i = state.focus; }
    p[i] = id;
  }
  state.focus = i;
  state.thread = id;
}

function saveWorkspaceLayout(layout) {
  state.layoutsBy[state.project.id] = layout;
  setPref("layoutsBy", state.layoutsBy);
}

function layoutControls() {
  const { count, columns } = workspaceLayout();
  const set = (patch, control) => {
    state.layoutFocus = { projectId: state.project.id, control };
    saveWorkspaceLayout({ count, columns, ...patch });
    render();
  };
  const paneCount = h("input", { type: "number", "aria-label": I18N.t("ws.panes"), "data-layout-control": "count", min: "1", max: String(MAX_PANES), step: "1", value: count,
    onchange: (e) => {
      const value = layoutValue(e.target.valueAsNumber, 1, MAX_PANES, count);
      if (value === count) e.target.value = count;
      else set({ count: value }, "count");
    } });
  const step = (delta, label, symbol, control) => h("button", {
    class: "btn", type: "button", "aria-label": label, title: label, "data-layout-control": control,
    disabled: count + delta < 1 || count + delta > MAX_PANES,
    onclick: () => set({ count: count + delta }, control),
  }, h("span", { "aria-hidden": "true" }, symbol));
  const cols = h("select", { "aria-label": I18N.t("ws.columns"), "data-layout-control": "columns", onchange: (e) => {
    const value = e.target.value;
    set({ columns: value === SIDE_BY_SIDE ? value : Number(value) }, "columns");
  } },
    h("option", { value: "0" }, I18N.t("ws.auto")),
    h("option", { value: SIDE_BY_SIDE }, I18N.t("ws.sideBySide")),
    Array.from({ length: MAX_COLUMNS }, (_, i) => h("option", { value: String(i + 1) }, String(i + 1))));
  cols.value = String(columns);
  return h("div", { class: "ws-layout", role: "group", "aria-label": I18N.t("ws.layout") },
    h("div", { class: "ws-layout-count" }, h("span", { class: "ws-layout-label", "aria-hidden": "true" }, I18N.t("ws.panes")),
      h("div", { class: "ws-layout-stepper" }, step(-1, I18N.t("ws.removePane"), "−", "remove"), paneCount, step(1, I18N.t("ws.addPane"), "+", "add"))),
    h("label", { class: "ws-layout-columns" }, h("span", { class: "ws-layout-label" }, I18N.t("ws.columns")), h("span", { class: "ws-layout-select" }, cols)));
}

function toggleFocusMode() {
  state.focusMode = !state.focusMode;
  state.focusOverlay = null;
  render();
}

function focusPane(i, moveFocus) {
  const p = panes();
  state.focus = i;
  state.thread = p[i] || null;
  for (const el of document.querySelectorAll(".ws-grid > .pane")) el.classList.toggle("focus", Number(el.dataset.idx) === i);
  drawThreadList();
  if (moveFocus && state.thread && XTERMS.has(state.thread)) XTERMS.get(state.thread).term.focus();
}

// ---------- sidebar thread list ----------
function drawThreadList() {
  const box = $("#side-threads");
  if (!box) return;
  clear(box);
  if (!state.project) return;
  const mine = projThreads();
  const shown = new Set(panes().filter(Boolean));
  const yourTurn = (t) => t.kind === "chat" && t.running && !t.busy && t.state === "ready" && !needsYou(t);
  const groups = appearance().grouping === "time" ? [[I18N.t("ws.group.all"), mine.slice().sort((a, b) => b.started_at - a.started_at)]] : [
    [I18N.t("ws.group.needs"), mine.filter((t) => needsYou(t))],
    [I18N.t("ws.group.working"), mine.filter((t) => t.running && !needsYou(t) && !yourTurn(t))],
    [I18N.t("ws.group.turn"), mine.filter(yourTurn)],
    [I18N.t("ws.group.ended"), mine.filter((t) => !t.running)],
  ];
  box.append(h("div", { class: "side-label side-threads-head" }, h("span", null, I18N.t("ws.threads")), mine.length ? h("span", null, String(mine.length)) : null));
  for (const [label, list] of groups) {
    if (!list.length) continue;
    box.append(h("div", { class: "side-group" }, h("span", null, label), h("span", null, String(list.length))),
      h("nav", { class: "thread-list", "aria-label": label }, list.map((t) => h("button", {
        class: "thread-item" + (t.id === state.thread ? " active" : "") + (shown.has(t.id) ? " shown" : ""),
        title: shown.has(t.id) ? I18N.t("ws.inPane") : I18N.t("ws.clickShow"),
        onclick: () => { placeThread(t.id); if (state.view === "workspace") render(); else setTab("workspace"); },
      }, agentBadge(t),
      h("span", { class: "thread-text" },
        h("span", { class: "thread-row" }, h("span", { class: "thread-title" }, t.title), threadStatus(t)),
        h("span", { class: "muted small thread-sub" }, threadSub(t)))))));
  }
  if (!mine.length) box.append(h("div", { class: "muted small empty-threads" }, I18N.t("ws.noThreads")));
  box.append(h("div", { class: "row side-new" },
    h("button", { class: "btn primary small", onclick: newThreadDialog, title: I18N.t("ws.agent.title") }, I18N.t("ws.agent")),
    h("button", { class: "btn small", onclick: newChatDialog, title: I18N.t("ws.chat.title") }, I18N.t("ws.chat")),
    h("button", { class: "btn small", onclick: () => startThread({ kind: "shell" }), title: I18N.t("ws.shell.title") }, I18N.t("ws.shell"))));
  drawStatusBar();
}
function threadSub(t) {
  const kind = t.kind === "chat" ? I18N.t("review.kind.chat") : t.kind === "agent" ? I18N.t("review.kind.terminal") : I18N.t("ws.kind.shell");
  return [kind, t.kind === "shell" ? null : taskLabel(t), agoSecs(t.started_at), t.restored ? I18N.t("ws.earlier") : null].filter(Boolean).join(" · ");
}
/** The tasks a thread is on: the one it was started with, else the in-progress tasks its agent claimed itself. */
function threadTasks(t) {
  if (t.task) return [t.task];
  if (t.kind === "shell" || !t.running || !state.ctx || !state.ctx.tasks) return [];
  const who = new Set([agentKey(t), String(t.harness || "").toLowerCase()]);
  return state.ctx.tasks.filter((r) => r.owner && who.has(String(r.owner).toLowerCase())).map((r) => r.task.key);
}
function taskLabel(t) {
  const ks = threadTasks(t);
  return ks.length ? I18N.t("ws.tasksLabel", { count: ks.length, keys: ks.join(", ") }) : I18N.t("thread.noTask");
}

// ---------- workspace view ----------
async function viewWorkspace(v) {
  const projectId = state.project.id;
  await loadThreads();
  if (state.view !== "workspace" || state.project?.id !== projectId || v.querySelector(".ws, .ws-hero")) return;
  const p = panes();
  drawThreadList();
  if (!projThreads().length) {
    // Nothing running yet: how to start, then the project overview.
    v.append(h("div", { class: "thread-empty ws-hero" },
      h("h2", null, I18N.t("ws.hero.title")),
      h("p", { class: "muted" }, I18N.t("ws.hero.body")),
      h("div", { class: "row" },
        h("button", { class: "btn primary", onclick: () => workOnNext() }, I18N.t("ws.workNext")),
        h("button", { class: "btn", onclick: newThreadDialog }, I18N.t("thread.newTitle")),
        h("button", { class: "btn", onclick: newChatDialog }, I18N.t("chat.newTitle")),
        h("button", { class: "btn", onclick: () => startThread({ kind: "shell" }) }, I18N.t("ws.newShell")))));
    const ov = h("div", { class: "ws-overview" });
    v.append(ov);
    await viewOverview(ov);
    return;
  }
  const { columns } = workspaceLayout();
  const gridColumns = columns === SIDE_BY_SIDE ? p.length : columns;
  const grid = h("div", { class: `ws-grid${state.focusMode ? " zoomed" : ""}`, style: gridColumns ? `--pane-columns:${gridColumns}` : null });
  const switcher = h("select", { "aria-label": I18N.t("ws.focusThread"), onchange: (e) => { if (e.target.value) { placeThread(e.target.value); render(); } } },
    projThreads().map((t) => h("option", { value: t.id }, t.title)));
  switcher.value = state.thread || "";
  v.classList.add("ws-view");
  v.append(h("div", { class: "ws" },
    h("div", { class: "ws-bar" },
      state.focusMode ? switcher : layoutControls(),
      h("span", { class: "spacer" }),
      h("button", { class: "btn small", "aria-pressed": String(state.focusMode), onclick: toggleFocusMode }, state.focusMode ? I18N.t("ws.exitFocus") : I18N.t("ws.focusPane")),
      h("button", { class: "btn small", onclick: () => setTab("changes"), title: I18N.t("ws.review.title") }, I18N.t("ws.review")),
      h("button", { class: "btn small" + (state.termDrawer ? " on" : ""), "aria-pressed": String(state.termDrawer), onclick: toggleTermDrawer, title: I18N.t("ws.drawer.title") }, I18N.t("ws.drawer"))),
    !state.focusMode && p.length >= 3 && !pref("hintFocus", false) ? h("div", { class: "ws-hint small" },
      h("span", null, I18N.t("ws.tip")),
      h("button", { class: "btn small", onclick: toggleFocusMode }, I18N.t("ws.tip.use")),
      h("button", { class: "btn small ghost", "aria-label": I18N.t("ws.tip.dismiss"), onclick: (e) => { setPref("hintFocus", true); e.currentTarget.parentElement.remove(); } }, "×")) : null,
    grid,
    composer(),
    state.termDrawer ? termDrawer() : null));
  p.forEach((id, i) => grid.append(h("div", { class: "pane thread-pane" + (i === state.focus ? " focus" : ""), hidden: state.focusMode && i !== state.focus ? true : null, "data-idx": String(i) })));
  for (const el of grid.children) {
    el.addEventListener("pointerdown", () => { if (state.focus !== Number(el.dataset.idx)) focusPane(Number(el.dataset.idx)); }, true);
    el.addEventListener("focusin", () => { if (state.focus !== Number(el.dataset.idx)) focusPane(Number(el.dataset.idx)); });
    // Clicking anywhere in a terminal pane (header, padding, status strip) sends keys, including Enter, to it.
    el.addEventListener("click", (e) => {
      if (e.target.closest("button, a, input, textarea, select, summary, [contenteditable]")) return;
      if (window.getSelection()?.toString()) return;
      const id = el.dataset.id;
      if (id && XTERMS.has(id) && id === state.thread) XTERMS.get(id).term.focus();
    });
  }
  const control = state.layoutFocus?.projectId === projectId ? state.layoutFocus.control : null;
  state.layoutFocus = null;
  p.forEach((id, i) => drawPaneAt(i, i === state.focus && !control));
  if (control) {
    const focused = v.querySelector(`[data-layout-control="${control}"]:not(:disabled)`)
      || v.querySelector('[data-layout-control="count"]');
    focused?.focus({ preventScroll: true });
  }
  if (columns === SIDE_BY_SIDE && !state.focusMode) {
    grid.querySelector(".pane.focus")?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }
}

/** Redraw the pane that shows a thread (the focused one by default). */
function drawThreadPane(id = state.thread) {
  if (state.view !== "workspace") return;
  const i = state.focusMode && state.focusOverlay === id ? state.focus : panes().indexOf(id);
  if (i >= 0) drawPaneAt(i, false);
}

function drawPaneAt(i, takeFocus) {
  const el = document.querySelector(`.ws-grid > .pane[data-idx="${i}"]`);
  if (!el) return;
  // Redrawing re-parents the terminal, which drops keyboard focus; keep it if the user was typing here.
  if (el.querySelector(".xterm") && el.contains(document.activeElement)) takeFocus = true;
  clear(el);
  const p = panes();
  const id = state.focusMode && state.focus === i && state.focusOverlay ? state.focusOverlay : p[i];
  const t = state.threads.find((x) => x.id === id);
  if (!t) { drawEmptyPane(el, i); return; }
  el.dataset.id = t.id;
  if (t.kind === "chat") {
    drawChatPane(el, t, takeFocus).then(() => { if (el.dataset.id === t.id && !el.querySelector(".sees")) el.append(seesStrip(t)); });
    return;
  }
  const x = ensureXterm(t.id);
  const banner = limitBanner(t) || quotaBanner(t) || stuckBanner(t);
  const restart = t.running ? null : restartTerminalButton(t);
  const subtitle = t.running ? ([t.target, t.task].filter(Boolean).join(" · ") || I18N.t("ws.projectShell")) : I18N.t("chat.saved");
  el.append(h("div", { class: "thread-head" },
    h("div", { class: "pane-title" }, agentBadge(t),
      h("div", null, h("div", { class: "thread-name" }, h("span", { class: "thread-title", title: t.title }, t.title), threadStatus(t)),
        h("div", { class: "muted small mono", title: I18N.t("ws.f6", { command: t.command }) }, subtitle))),
    h("div", { class: "row" },
      t.running && t.kind === "agent" ? h("button", { class: "btn small", onclick: () => sendContext(t), title: I18N.t("ws.sendCtx.title") }, I18N.t("chat.sendContext")) : null,
      t.running && terminalDrafts.has(t.id) ? h("button", { class: "btn small", onclick: () => loadThreadDraft(t, terminalDrafts.get(t.id)) }, I18N.t("ws.loadDraft")) : null,
      t.kind === "agent" ? continueButton(t) : null,
      restart,
      h("button", { class: "btn small", onclick: () => closeThread(t.id) }, t.running ? I18N.t("ws.stop") : I18N.t("ws.closeBtn")),
      paneRemoveBtn(t.id))),
  ...(banner ? [banner] : []), x.el, seesStrip(t));
  if (t.kind === "agent" && !t.running) offerRelease(el, t);
  if (!x.opened) { x.term.open(x.el); x.opened = true; }
  requestAnimationFrame(() => fitThread(t.id));
  if (takeFocus) {
    if (restart) restart.focus();
    else x.term.focus();
  }
  stageCheckpointDraft(t);
}

const quotaLeft = (q) => q?.state === "ready" && q.windows?.length
  ? Math.min(...q.windows.filter((w) => !w.resets_at || w.resets_at * 1000 > Date.now()).map((w) => w.remaining))
  : null;

function checkpointPrompt(t) {
  if (!t.running || t.kind !== "agent" || !t.task) return null;
  const quota = accountQuota.get(quotaKey(agentKey(t), t.project_id));
  const left = quotaLeft(quota);
  if (quota?.stale || !(left > 0 && left <= 10)) return null;
  return I18N.t("ws.checkpointPrompt", { task: t.task });
}

function stageCheckpointDraft(t, manual = false) {
  const x = XTERMS.get(t.id);
  const prompt = checkpointPrompt(t);
  if (!x || !prompt) return;
  // A PTY can be at a login, approval, or command prompt. Leave submission to the composer.
  if (!manual && (x.checkpointStaged || state.thread !== t.id || $(".ws-input") === document.activeElement)) return;
  if (loadThreadDraft(t, prompt, !manual)) x.checkpointStaged = true;
}

/** The other AI with the most usage left, from the readings already taken (null when none is known). */
function bestFallback(exclude) {
  let best = null;
  for (const [key, q] of accountQuota) {
    const ai = key.split(":")[0];
    const left = quotaLeft(q);
    if (ai === exclude || left === null || !Number.isFinite(left) || left <= 10) continue;
    if (!best || left > best.left) best = { ai, left };
  }
  return best;
}

/**
 * Switch the AI on a task because the current one is out of usage (or about to be).
 * Only one AI works on a task at a time, so the person confirms, Evyro stops the old AI,
 * carries the latest checkpoint over and starts the new one. A model on the same AI
 * can be chosen too. `thread` is a pane to reuse for the new AI.
 */
async function switchOwnerDialog(task, { from = null, to = null, thread = null, evidence = null } = {}) {
  const projectId = state.project.id;
  const sel = await targetSelect(to || "codex");
  for (const g of [...sel.querySelectorAll("optgroup")]) {
    if (![KIND_LABEL.harness, KIND_LABEL.api].includes(g.label)) g.remove();
  }
  if (state.project?.id !== projectId) return;
  const model = h("input", { type: "text", placeholder: I18N.t("ws.switch.modelPh"), spellcheck: "false" });
  const sure = h("input", { type: "checkbox", id: "switch-sure" });
  const who = from || I18N.t("ws.switch.current");
  modal(I18N.t("ws.switch.title", { task }), [
    evidence ? h("p", { class: "banner warn", role: "status" }, I18N.t("ws.switch.printed", { who, evidence })) : null,
    h("p", { class: "hint" }, I18N.t("ws.switch.body", { task, who })),
    field(I18N.t("ws.switch.to"), sel, I18N.t("ws.switch.toHint")),
    field(I18N.t("ws.switch.model"), model),
    h("label", { class: "check", for: "switch-sure" }, sure, " " + I18N.t("ws.switch.sure", { who })),
  ], [{
    label: I18N.t("ws.switch.ai"), primary: true, run: async () => {
      if (!sure.checked) { toast(I18N.t("ws.switch.confirm"), true); return false; }
      const target = sel.value;
      const chosen = model.value.trim() || null;
      try {
        const owners = projThreads().filter((t) => t.task === task && t.running && (t.kind === "agent" || t.kind === "chat") && t.id !== thread?.id);
        for (const t of owners) await invoke(t.kind === "chat" ? "chat_close" : "term_stop", { id: t.id });
        for (let i = 0; i < 20 && owners.length; i++) {
          await loadThreads();
          if (!owners.some((o) => state.threads.find((t) => t.id === o.id)?.running)) break;
          await new Promise((r) => setTimeout(r, 200));
        }
        const r = await act(() => invoke("switch_owner", { projectId, task, to: target, model: chosen, confirmed: true }));
        if (state.project?.id !== projectId) return true;
        const handoffId = r.handoff.record.id;
        if (thread) await resumeThread(thread, { handoffId, model: chosen });
        else await startThread({ kind: "agent", target, task, handoffId, model: chosen });
        toast(r.from && !r.stopped && !owners.length ? I18N.t("ws.switch.doneClose", { task, target, from: r.from.harness }) : I18N.t("ws.switch.done", { task, target }));
        return true;
      } catch (e) {
        if (!e?.message) toast(errMsg(e), true);
        return false;
      }
    },
  }]);
}

/** A running AI whose account is out of usage (or nearly) gets a way to carry its task to another AI. */
function quotaBanner(t) {
  if (!t.running || t.kind !== "agent") return null;
  const left = quotaLeft(accountQuota.get(quotaKey(agentKey(t), t.project_id)));
  if (left === null || !Number.isFinite(left) || left > 10) return null;
  const out = left <= 0;
  const alt = bestFallback(agentKey(t));
  const hint = alt ? " " + I18N.t("ws.quota.alt", { ai: alt.ai, left: Math.floor(alt.left) }) : "";
  const go = () => handoffDialog(t.task || null, alt?.ai || null, t.task ? t.target : null).catch(() => {});
  return h("div", { class: "quota-banner small" + (out ? " out" : ""), role: "status" },
    h("span", null, out
      ? I18N.t("ws.quota.out", { title: t.title }) + hint
      : I18N.t("ws.quota.low", { title: t.title, left: Math.floor(left) }) + hint),
    h("div", { class: "row" },
      checkpointPrompt(t) ? h("button", { class: "btn small", onclick: () => stageCheckpointDraft(t, true) }, I18N.t("ws.quota.draft")) : null,
      t.task ? h("button", { class: "btn small primary", onclick: () => switchOwnerDialog(t.task, { from: t.target, to: alt?.ai || null }).catch(() => {}) },
        alt ? I18N.t("ws.switchTo", { ai: alt.ai }) : I18N.t("ws.switchAi")) : null,
      h("button", { class: "btn small", onclick: go }, t.task ? I18N.t("ws.quota.notes") : I18N.t("ws.quota.handoff"))));
}

/** A running AI that printed a usage-limit message gets the switch offered at once, with what it said. */
function limitBanner(t) {
  if (!t.running || t.kind !== "agent" || !t.limit_hit) return null;
  const alt = bestFallback(agentKey(t));
  return h("div", { class: "quota-banner small out", role: "status" },
    h("span", null, I18N.t("ws.limit.says", { title: t.title, said: t.limit_hit })),
    h("div", { class: "row" },
      t.task ? h("button", { class: "btn small primary", onclick: () => switchOwnerDialog(t.task, { from: t.target, to: alt?.ai || null, thread: t, evidence: t.limit_hit }).catch(() => {}) },
        alt ? I18N.t("ws.switchTo", { ai: alt.ai }) : I18N.t("ws.switchAi")) : null,
      h("button", { class: "btn small", onclick: () => closeThread(t.id) }, I18N.t("ws.stop"))));
}

/** A running AI that is silent or repeating itself gets a way to stop it or carry its task on. */
function stuckBanner(t) {
  if (!t.running || t.kind !== "agent" || !t.stuck) return null;
  const alt = bestFallback(agentKey(t));
  return h("div", { class: "quota-banner small out", role: "status" },
    h("span", null, I18N.t("ws.stuck.says", { title: t.title, why: t.stuck })),
    h("div", { class: "row" },
      t.task ? h("button", { class: "btn small primary", onclick: () => switchOwnerDialog(t.task, { from: t.target, to: alt?.ai || null, thread: t }).catch(() => {}) },
        alt ? I18N.t("ws.switchTo", { ai: alt.ai }) : I18N.t("ws.switchAi")) : null,
      h("button", { class: "btn small", onclick: () => closeThread(t.id) }, I18N.t("ws.stop"))));
}

function continueButton(t) {
  const task = t.task;
  if (!task) return null;
  return h("button", { class: "btn small", disabled: t.busy || t.switching_model,
    onclick: () => handoffDialog(task, null, t.target).catch(() => {}) }, I18N.t("ws.continue"));
}

function restartTerminalButton(t) {
  const shell = t.kind === "shell";
  return h("button", { class: "btn small primary", title: shell ? I18N.t("ws.restart.shell.title") : I18N.t("ws.restart.agent.title"),
    onclick: async (e) => {
      const button = e.currentTarget;
      button.disabled = true;
      try { await resumeThread(t); } catch { /* startThread reports the error */ }
      finally { button.disabled = false; }
    },
  }, shell ? I18N.t("ws.restart.shell") : I18N.t("ws.resume"));
}

/** Relaunch a stopped terminal in its existing thread. */
async function resumeThread(t, opts = {}) {
  const i = panes().indexOf(t.id);
  if (i >= 0) state.focus = i;
  return startThread({ kind: t.kind, target: t.target, task: t.task, resumeId: t.id, ...opts });
}

/**
 * An agent that stopped because a task is "owned by session ..." of an AI that
 * is gone (an earlier run, a crash) gets a button that frees the task and retries.
 */
async function offerRelease(el, t) {
  let text = "";
  try { text = (await invoke("term_snapshot", { id: t.id })).data || ""; } catch { return; }
  const m = text.match(/(T-\d+) is owned by session (ses_[0-9a-z]+) \(([^)]+)\)/);
  if (!m || el.dataset.id !== t.id || el.querySelector(".pane-fix")) return;
  const [, key, sid, who] = m;
  const fix = async () => {
    // The session id comes from terminal output, so show it and ask first.
    if (!(await confirmDialog(I18N.t("ws.free.title", { key, sid }), I18N.t("ws.free.body", { who }), I18N.t("ws.free.do")))) return;
    const r = await act(() => invoke("release_session", { projectId: state.project.id, sessionId: sid, confirmed: true }));
    if (!r) return;
    toast(I18N.t("ws.free.done", { key, who }));
    resumeThread(t);
  };
  el.querySelector(".thread-head").after(h("div", { class: "banner warn pane-fix", role: "status" },
    h("span", null, I18N.t("ws.free.held", { key, who })),
    h("button", { class: "btn small primary", onclick: fix }, I18N.t("ws.free.retry", { key })),
    h("button", { class: "btn small", onclick: () => switchOwnerDialog(key, { from: who, to: t.target, thread: t }).catch(() => {}) }, I18N.t("ws.free.switch", { key }))));
}

/** The ✕ on a pane: take the thread out of view (it keeps running). */
function paneRemoveBtn(id) {
  const i = panes().indexOf(id);
  if (i < 0) return null;
  return h("button", { class: "btn small ghost", "aria-label": I18N.t("ws.unpane"), title: I18N.t("ws.unpane.title"), onclick: () => unpane(panes().indexOf(id)) }, "✕");
}

function unpane(i) {
  const p = panes();
  const id = p[i];
  p[i] = null;
  render();
  if (id) toast(I18N.t("ws.unpane.done"));
}

function drawEmptyPane(el, i) {
  delete el.dataset.id;
  const shown = panes();
  const others = projThreads().filter((t) => !shown.includes(t.id));
  const start = (fn) => () => { state.focus = i; fn(); };
  el.append(h("div", { class: "pane-empty" },
    h("div", { class: "muted" }, I18N.t("ws.empty")),
    others.length ? h("label", { class: "field" }, I18N.t("ws.showThread"), h("select", {
      onchange: (e) => { if (e.target.value) { state.focus = i; placeThread(e.target.value); render(); } },
    }, h("option", { value: "" }, I18N.t("skills.imp.choose")), others.map((t) => h("option", { value: t.id }, t.title)))) : null,
    h("div", { class: "row" },
      h("button", { class: "btn small primary", onclick: start(newThreadDialog) }, I18N.t("ws.agent")),
      h("button", { class: "btn small", onclick: start(newChatDialog) }, I18N.t("ws.chat")),
      h("button", { class: "btn small", onclick: start(() => startThread({ kind: "shell" })) }, I18N.t("ws.shell")))));
}

/** What this AI can draw from: its task and the project's shared context. */
function seesStrip(t) {
  const el = h("div", { class: "sees", "data-id": t.id });
  fillSees(el, t);
  return el;
}
function fillSees(el, t) {
  const c = state.ctx;
  clear(el).append(h("span", { class: "muted" }, I18N.t("ws.sharedWith")),
    h("span", { class: "chip" + (threadTasks(t).length ? " acc" : "") }, taskLabel(t)));
  if (!c) return;
  el.append(...[
    h("span", { class: "chip" }, I18N.t("ws.sees.decisions", { count: c.decisions.length })),
    h("span", { class: "chip" }, I18N.t("ws.sees.notes", { count: c.review.length })),
    h("span", { class: "chip" }, I18N.t("ws.sees.memory", { count: c.memory.length })),
    c.overview && c.overview.latest_checkpoint ? h("span", { class: "chip" }, `checkpoint ${ago(c.overview.latest_checkpoint.created_at)}`) : h("span", { class: "chip" }, "no checkpoint yet"),
  ].filter(Boolean));
}

const plural = (n, one, many) => `${n} ${n === 1 ? one : many || one + "s"}`;

// ---------- sending text to threads ----------
function terminalSetupPrompt(t, term) {
  const harness = state.targets?.find((r) => r.target.name === t.target)?.target.harness || t.target;
  if (harness !== "cursor" && harness !== "copilot") return null;
  const screen = term.buffer.active;
  const lines = [];
  for (let i = 0; i < term.rows; i++) lines.push(screen.getLine(screen.baseY + i)?.translateToString(true) || "");
  const text = lines.join(" ");
  if (harness === "cursor" && /Press any key to log in/i.test(text)) return I18N.t("ws.setup.cursor");
  if (harness === "copilot" && /Confirm folder trust|Do you trust the files/i.test(text)) return I18N.t("ws.setup.copilot");
  return null;
}

/** Type text into a thread as one message: a chat turn, or a bracketed paste plus Enter in a terminal. */
async function sendToThread(id, text) {
  const t = state.threads.find((x) => x.id === id);
  if (!t || !t.running) throw { message: t ? I18N.t("ws.send.notRunning", { name: t.title }) : I18N.t("ws.send.threadNotRunning") };
  if (t.kind === "chat") {
    if (t.busy || t.state !== "ready") throw { message: I18N.t("ws.send.busy", { name: t.title }) };
    await invoke("chat_send", { id, text });
    return;
  }
  const x = ensureXterm(id);
  await x.loaded;
  // xterm parses output asynchronously, even when this thread has no visible pane.
  await new Promise((resolve) => x.term.write("", resolve));
  const setup = terminalSetupPrompt(t, x.term);
  if (setup) throw new Error(`${t.title}: ${setup}`);
  const body = text.replace(/\r\n?/g, "\n");
  if (/\p{Cc}/u.test(body.replace(/[\n\t]/g, ""))) throw new Error(I18N.t("ws.send.control", { name: t.title }));
  const bracketed = x.term.modes.bracketedPasteMode;
  if (!bracketed && /[\n\t]/.test(body)) throw new Error(I18N.t("ws.send.multiline", { name: t.title }));
  if (x.sending) throw new Error(I18N.t("ws.send.already", { name: t.title }));
  x.sending = true;
  try {
    await invoke("term_write", { id, data: bracketed ? `\x1b[200~${body.replace(/\n/g, "\r")}\x1b[201~` : body });
    await new Promise((resolve) => setTimeout(resolve, 150));
    await invoke("term_write", { id, data: "\r" });
  } finally {
    x.sending = false;
  }
}

/** Send text to the focused AI, or say how to get one. */
async function sendToFocused(text, what) {
  const t = state.threads.find((x) => x.id === state.thread && x.running && x.kind !== "shell");
  if (!t) { await copy(text, I18N.t("ws.send.copied", { what })); return false; }
  try { await sendToThread(t.id, text); toast(I18N.t("ws.send.sentWhat", { what, name: t.title })); return true; }
  catch (e) { toast(errMsg(e), true); return false; }
}

async function sendContext(t) {
  let r;
  try { r = await invoke("context_packet", { projectId: state.project.id, task: t.task || null, budget: "small" }); }
  catch (e) { toast(errMsg(e), true); return; }
  try { await sendToThread(t.id, r.text); toast(I18N.t("ws.send.context", { name: t.title, tokens: r.packet.approx_tokens })); }
  catch (e) { toast(errMsg(e), true); }
}

// ---------- composer: message several AIs at once ----------
function workspaceDraft(projectId) {
  if (!workspaceDrafts.has(projectId)) workspaceDrafts.set(projectId, { text: "", recipients: null, sending: false, status: "" });
  return workspaceDrafts.get(projectId);
}

function loadThreadDraft(t, text, quiet = false) {
  if (!text) return false;
  const draft = workspaceDraft(t.project_id);
  if (draft.text.trim() || draft.sending) {
    if (!quiet) toast(I18N.t("ws.draft.busy"));
    return false;
  }
  draft.text = text;
  draft.recipients = [t.id];
  draft.status = "";
  $(".ws-compose")?.dispatchEvent(new Event("delivery-updated"));
  return true;
}

async function prepareTerminalDraft(t, handoffId) {
  try {
    let text;
    if (handoffId) {
      const detail = await invoke("handoff_detail", { handoffId });
      if (!detail.integrity_ok) return;
      text = detail.resume_prompt;
    } else {
      ({ text } = await invoke("context_packet", { projectId: t.project_id, task: t.task || null, budget: "small" }));
    }
    if (typeof text !== "string" || !text.trim() || !state.threads.some((x) => x.id === t.id)) return;
    terminalDrafts.set(t.id, text);
    if (state.project?.id === t.project_id && state.thread === t.id) loadThreadDraft(t, text, true);
    drawThreadPane(t.id);
  } catch { /* The terminal is ready for a message without task context. */ }
}

function composer() {
  const projectId = state.project.id;
  const draft = workspaceDraft(projectId);
  const input = h("textarea", { class: "ws-input", rows: "2", "aria-label": I18N.t("ws.compose.label"), placeholder: I18N.t("ws.compose.ph") });
  const status = h("div", { class: "ws-send-status small", role: "status" });
  const to = h("div", { class: "ws-to", role: "group", "aria-label": I18N.t("ws.compose.sendTo") });
  input.value = draft.text;
  const meta = h("span", { class: "ws-meta muted small" });
  const updateText = () => {
    draft.text = input.value;
    if (!draft.sending) {
      draft.status = "";
      status.textContent = "";
    }
    fit();
  };
  const fit = () => {
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight + 2, 120)}px`;
    const lines = input.value ? input.value.split("\n").length : 0;
    clear(meta);
    if (lines > 3 || input.value.length > 300) {
      meta.append(I18N.t("ws.compose.size", { lines: I18N.t("ws.compose.lines", { count: lines }), chars: input.value.length.toLocaleString() }) + " ",
        h("button", { class: "link-btn", type: "button", onclick: () => { input.value = ""; updateText(); input.focus(); } }, I18N.t("ws.compose.discard")));
    } else meta.append(I18N.t("ws.compose.keys"));
  };
  input.addEventListener("input", updateText);
  const drawTo = () => {
    const running = projThreads().filter((t) => t.running && t.kind !== "shell");
    const sel = running.filter((t) => !draft.recipients || draft.recipients.includes(t.id)).map((t) => t.id);
    clear(to).append(h("span", { class: "muted small" }, I18N.t("ws.compose.to", { count: sel.length })));
    if (!running.length) to.append(h("span", { class: "muted small" }, I18N.t("ws.noAiRunning")));
    else if (running.length > 1) to.append(h("button", { class: "chip", "aria-pressed": String(draft.recipients === null),
      onclick: () => { draft.recipients = null; drawTo(); } }, I18N.t("ws.compose.all")));
    for (const t of running) {
      const on = sel.includes(t.id);
      to.append(h("button", {
        class: "chip toggle" + (on ? " acc" : ""), "aria-pressed": String(on), title: t.title,
        onclick: () => { draft.recipients = on ? sel.filter((x) => x !== t.id) : [...sel, t.id]; drawTo(); },
      }, agentBadge(t, true), t.title));
    }
    return sel;
  };
  const send = async () => {
    const original = draft.text;
    const text = original.trim();
    if (!text || draft.sending) return;
    const sel = drawTo();
    if (!sel.length) { status.textContent = I18N.t("ws.compose.pick"); return; }
    draft.sending = true;
    button.disabled = true;
    status.textContent = I18N.t("ws.compose.sending");
    const results = await Promise.allSettled(sel.map((id) => sendToThread(id, text)));
    results.forEach((r, i) => { if (r.status === "fulfilled" && terminalDrafts.get(sel[i])?.trim() === text) terminalDrafts.delete(sel[i]); });
    const failures = results.flatMap((r, i) => r.status === "rejected" ? [{ id: sel[i], reason: errMsg(r.reason) }] : []);
    const ok = results.length - failures.length;
    draft.status = [...(ok ? [I18N.t("ws.compose.sent", { count: ok })] : []), ...failures.map((f) => f.reason)].join(" ");
    if (draft.text === original) {
      if (failures.length) draft.recipients = failures.map((f) => f.id);
      else draft.text = "";
    }
    draft.sending = false;
    $(".ws-compose")?.dispatchEvent(new Event("delivery-updated"));
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); }
  });
  const save = async () => {
    const text = input.value.trim();
    if (!text) { toast(I18N.t("ws.memory.empty"), true); return; }
    await act(() => invoke("add_memory", { projectId: state.project.id, category: "note", content: text }), I18N.t("ws.memory.saved"));
    drawContext();
  };
  const button = h("button", { class: "btn primary small", onclick: send }, I18N.t("ws.send"));
  const refresh = () => {
    if (input.value !== draft.text) input.value = draft.text;
    fit();
    status.textContent = draft.status;
    button.disabled = draft.sending;
    drawTo();
  };
  refresh();
  return h("div", { class: "ws-compose", "ondelivery-updated": refresh }, to,
    input,
    h("div", { class: "ws-compose-foot" }, meta,
      h("div", { class: "row" },
        h("button", { class: "btn small ghost", onclick: save, title: I18N.t("ws.memory.title") }, I18N.t("ws.memory.save")),
        button)),
    status);
}

// ---------- shared context panel ----------
let ctxTimer = null;
let ctxDrawing = false;
function setCtxOpen(open) {
  state.ctxOpen = open;
  setPref("ctxOpen", open);
  applyCtxVisibility();
  if (open) drawContext();
}
function applyCtxVisibility() {
  const show = !!state.project && state.ctxOpen;
  const el = $("#ctx");
  el.classList.toggle("hidden", !show);
  el.setAttribute("aria-hidden", String(!show));
  const b = $("#ctx-btn");
  b.classList.toggle("hidden", !state.project);
  b.setAttribute("aria-pressed", String(show));
  b.textContent = show ? I18N.t("ws.ctx.hide") : I18N.t("ws.ctx.show");
}

/** Load the project's shared context and draw the panel, the status bar and each pane's strip. */
async function drawContext() {
  clearTimeout(ctxTimer);
  if (!state.project) { applyCtxVisibility(); drawStatusBar(); return; }
  ctxTimer = setTimeout(drawContext, 5000);
  if (ctxDrawing) return;
  ctxDrawing = true;
  const pid = state.project.id;
  try {
    const [overview, decisions, memory, review, tasks, sessions] = await Promise.all([
      invoke("overview", { projectId: pid }).catch(() => null),
      invoke("list_decisions", { projectId: pid }).catch(() => []),
      invoke("search_memory", { projectId: pid, query: "" }).catch(() => []),
      invoke("list_review_comments", { projectId: pid }).catch(() => []),
      invoke("list_tasks", { projectId: pid, includeClosed: false }).catch(() => []),
      invoke("list_sessions", { projectId: pid, activeOnly: false }).catch(() => []),
    ]);
    let detail = null;
    if (overview && overview.active_task) detail = await invoke("task_detail", { projectId: pid, task: overview.active_task.key }).catch(() => null);
    if (!state.project || state.project.id !== pid) return;
    state.ctx = { overview, decisions, memory, review: review.filter((c) => c.status !== "resolved"), detail, tasks, sessions };
  } finally { ctxDrawing = false; }
  applyCtxVisibility();
  drawStatusBar();
  for (const el of document.querySelectorAll(".sees[data-id]")) {
    const t = state.threads.find((x) => x.id === el.dataset.id);
    if (t) fillSees(el, t);
  }
  // Task ownership can change from inside an agent, so refresh the thread list too.
  const side = $("#side-threads");
  if (side && !side.contains(document.activeElement)) drawThreadList();
  if (state.view === "workspace") {
    for (const t of projThreads()) {
      const ai = agentKey(t);
      if (t.running && t.kind === "agent" && t.task && canReadQuota(ai)) refreshQuota(ai, pid);
    }
  }
  const body = $("#ctx-body");
  // Do not pull a control out from under someone using it.
  if (!state.ctxOpen || body.contains(document.activeElement)) return;
  const kids = state.ctxPane === "changes" ? await ctxChanges() : ctxSections(state.ctx);
  if (body.contains(document.activeElement)) return;
  clear(body).append(...kids);
}

/** Per AI: tasks it holds now, runs, failed runs, time worked and when it was last active. */
function usageByAI(sessions, tasks) {
  const by = new Map();
  const now = Date.now();
  for (const x of sessions) {
    const u = by.get(x.harness) || { ai: x.harness, runs: 0, failed: 0, secs: 0, last: x.started_at, live: false, holds: [] };
    const start = Date.parse(x.started_at), end = x.ended_at ? Date.parse(x.ended_at) : now;
    u.runs++;
    if (x.status === "failed") u.failed++;
    if (!x.ended_at) u.live = true;
    if (start && end >= start) u.secs += (end - start) / 1000;
    const last = x.ended_at || x.last_event_at || x.started_at;
    if (last > u.last) u.last = last;
    by.set(x.harness, u);
  }
  for (const r of tasks) if (r.owner && by.has(r.owner)) by.get(r.owner).holds.push(r.task.key);
  return [...by.values()].sort((a, b) => (b.live - a.live) || (a.last < b.last ? 1 : -1));
}
function duration(secs) {
  if (secs < 60) return `${Math.round(secs)}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  return `${Math.floor(secs / 3600)}h ${Math.round((secs % 3600) / 60)}m`;
}

const accountQuota = new Map();
const quotaRefreshes = new Map();
let quotaAgents;
const canReadQuota = (ai) => ["codex", "copilot", "antigravity-cli", "claude-code", "opencode"].includes(ai);

const quotaKey = (ai, projectId) => ai === "opencode" ? `${ai}:${projectId}` : ai;

function quotaText(q) {
  if (q.state === "stats") {
    const count = new Intl.NumberFormat();
    const cost = q.cost === null ? "" : " · " + I18N.t("ws.quota.cost", { cost: new Intl.NumberFormat(undefined, { style: "currency", currency: "USD", maximumFractionDigits: 4 }).format(q.cost) });
    return [I18N.t("ws.quota.week"), I18N.t("ws.quota.tokens", { input: count.format(q.input), output: count.format(q.output) }) + cost];
  }
  if (q.state === "loading") return I18N.t("ws.quota.loading");
  if (q.state !== "ready") return q.message || I18N.t("ws.quota.unavailable");
  return [
    ...q.windows.map((w) => {
      const window = w.minutes === 10080 ? I18N.t("ws.quota.weekly") : w.minutes && w.minutes % 60 === 0 ? I18N.t("ws.quota.hours", { count: w.minutes / 60 }) : w.minutes ? I18N.t("ws.quota.minutes", { count: w.minutes }) : "";
      const label = [w.label, window].filter(Boolean).join(" · ");
      const reset = w.resets_at ? new Date(w.resets_at * 1000) : null;
      const remaining = I18N.t("ws.quota.left", { pct: Number(w.remaining.toFixed(1)) });
      return `${label ? `${label} ` : ""}${reset && +reset <= Date.now() ? I18N.t("ws.quota.passed", { remaining }) : remaining}${reset && +reset > Date.now() ? " · " + I18N.t("ws.quota.resets", { when: reset.toLocaleString() }) : ""}`;
    }),
    q.credits === null ? null : q.credits === "Unlimited" ? I18N.t("ws.quota.unlimited") : I18N.t("ws.quota.credits", { count: q.credits }),
  ];
}

function quotaSetup(q) {
  const config = JSON.stringify({ statusLine: { type: "command", command: q.capture_command } }, null, 2);
  modal(I18N.t("ws.quota.connectTitle"), [
    h("p", null, I18N.t("ws.quota.connectText")),
    h("pre", null, config),
    h("button", { class: "btn small", onclick: () => copy(config, I18N.t("ws.quota.settingsCopied")) }, I18N.t("ws.quota.copySettings")),
    h("button", { class: "btn small", onclick: () => copy(q.capture_command, I18N.t("ws.quota.commandCopied")) }, I18N.t("ws.quota.copyCommand")),
    h("p", { class: "hint" }, I18N.t("ws.quota.api")),
    h("button", { class: "btn small ghost", onclick: () => copy("https://platform.claude.com/usage", I18N.t("ws.quota.consoleCopied")) }, I18N.t("ws.quota.copyConsole")),
  ], []);
}

function drawQuota(el, q) {
  const text = quotaText(q);
  clear(el).append(...[].concat(text).map((line) => h("div", null, line)));
  if (q.observed_at) {
    const source = q.stale ? I18N.t("ws.quota.stale") : q.source || (q.cached ? I18N.t("ws.quota.snapshot") : I18N.t("ws.quota.reading"));
    el.append(h("div", { class: "muted small", title: new Date(q.observed_at).toLocaleString() }, `${source} · ${ago(q.observed_at)}`));
  }
  if (q.capture_command) el.append(h("button", { class: "btn small ghost", onclick: () => quotaSetup(q) }, I18N.t("ws.quota.connect")));
  if (el.dataset.ai === "opencode") {
    el.append(h("div", { class: "muted small" }, I18N.t("ws.quota.opencode")));
  }
  if (el.dataset.ai === "cursor") {
    clear(el).append(
      h("div", null, I18N.t("ws.quota.cursor")),
      h("button", { class: "btn small ghost", onclick: () => copy("https://cursor.com/dashboard", I18N.t("ws.quota.cursorCopied")) }, I18N.t("ws.quota.copyCursor")),
      h("button", { class: "btn small ghost", onclick: () => copy("https://grok.com/dashboard", I18N.t("ws.quota.grokCopied")) }, I18N.t("ws.quota.copyGrok")));
  }
}

function refreshQuota(ai, projectId, force = false) {
  const key = quotaKey(ai, projectId);
  const last = quotaRefreshes.get(key);
  const interval = ai === "claude-code" ? 15 * 1000 : 2 * 60 * 1000;
  if (last?.pending || (!force && last && Date.now() - last.at < interval)) return;
  const ticket = { pending: true, at: Date.now() };
  quotaRefreshes.set(key, ticket);
  const update = (q) => {
    accountQuota.set(key, q);
    if (state.project?.id !== projectId) return;
    const el = document.querySelector(`.ctx-quota[data-ai="${CSS.escape(ai)}"]`);
    if (el) drawQuota(el, q);
    // Update the banners and fallback suggestions without detaching live terminals.
    for (const t of state.threads) {
      if (t.project_id !== projectId || !t.running || t.kind !== "agent") continue;
      const x = XTERMS.get(t.id);
      if (x && agentKey(t) === ai) {
        const left = quotaLeft(q);
        if (!q.stale && Number.isFinite(left) && left > 10) x.checkpointStaged = false;
      }
      const pane = document.querySelector(`.ws-grid > .pane[data-id="${CSS.escape(t.id)}"]`);
      if (!pane || x?.el.parentElement !== pane) continue;
      const old = pane.querySelector(".quota-banner");
      const banner = quotaBanner(t);
      if (old?.textContent !== banner?.textContent) {
        if (banner) pane.insertBefore(banner, x.el);
        old?.remove();
      }
      if (agentKey(t) === ai) stageCheckpointDraft(t);
    }
  };
  if (!accountQuota.has(key)) update({ state: "loading" });
  const read = ai === "opencode" ? invoke("opencode_usage", { projectId }) : invoke("agent_quota", { target: ai });
  read.then(update, () => {
    const previous = accountQuota.get(key);
    update(["ready", "stats"].includes(previous?.state) ? { ...previous, stale: true } : { state: "unavailable" });
  }).finally(() => { ticket.pending = false; });
}

function sessionCredits(t) {
  if (!t.credit_budget) return null;
  if (typeof t.credits_used !== "number") return I18N.t("ws.credits.waiting", { budget: t.credit_budget });
  const left = Math.max(0, t.credit_budget - t.credits_used);
  return I18N.t("ws.credits.left", { left: new Intl.NumberFormat(undefined, { maximumFractionDigits: 3 }).format(left), budget: t.credit_budget });
}

function usagePanel(sessions, tasks) {
  const projectId = state.project.id;
  const stats = usageByAI(sessions, tasks);
  const threads = projThreads().filter((t) => t.kind !== "shell");
  if (quotaAgents === undefined) {
    quotaAgents = [];
    invoke("acp_agents").then((rows) => {
      quotaAgents = rows.filter((r) => ["antigravity-cli", "opencode"].includes(r.harness) && r.command).map((r) => r.harness);
    }, () => {});
  }
  const agents = [...new Set([...stats.map((u) => u.ai), ...threads.map(agentKey), ...quotaAgents])];
  return ctxSection(I18N.t("ws.usage.title"), null, agents.length ? h("ul", { class: "ctx-list ctx-usage" }, agents.map((ai) => {
    if (canReadQuota(ai)) refreshQuota(ai, projectId);
    const quota = h("div", { class: "ctx-quota small", "data-ai": ai, role: "status" });
    drawQuota(quota, accountQuota.get(quotaKey(ai, projectId)) || { state: "unsupported" });
    const u = stats.find((row) => row.ai === ai);
    const named = threads.filter((t) => agentKey(t) === ai);
    return h("li", null,
      h("div", { class: "row" }, agentBadge({ target: ai }, true), h("strong", null, ai), h("span", { class: "spacer" }),
        canReadQuota(ai) ? h("button", { class: "btn small ghost", "aria-label": I18N.t("ws.usage.refreshLabel", { name: ai }), onclick: () => refreshQuota(ai, projectId, true) }, I18N.t("ws.usage.refresh")) : null),
      quota,
      u ? h("div", { class: "muted small" }, [I18N.t("ws.usage.runs", { count: u.runs }), duration(u.secs), u.failed ? I18N.t("ws.usage.failed", { count: u.failed }) : null, u.holds.length ? I18N.t("ws.usage.holds", { tasks: u.holds.join(", ") }) : null].filter(Boolean).join(" · ")) : null,
      named.length ? h("ul", { class: "ctx-usage-threads" }, named.map((t) => h("li", null,
        h("button", { class: "btn small ghost", title: t.title, onclick: () => { placeThread(t.id); setTab("workspace"); } }, t.title),
        threadStatus(t), sessionCredits(t) ? h("div", { class: "muted small" }, sessionCredits(t)) : null))) : null);
  })) : h("div", { class: "muted small" }, I18N.t("ws.usage.none")),
  agents.length ? h("span", { class: "muted small", title: I18N.t("ws.usage.sharedTitle") }, I18N.t("ws.usage.reported")) : null);
}

function ctxSection(title, count, kids, extra) {
  return h("section", { class: "ctx-sec" },
    h("div", { class: "ctx-hd" }, h("h3", null, title), h("span", { class: "row" }, count !== null && count !== undefined ? h("span", { class: "muted small" }, String(count)) : null, extra)),
    kids);
}
const sendBtn = (label, fn) => h("button", { class: "btn small ghost ctx-send", onclick: fn, title: I18N.t("ws.ctx.sendTitle") }, label);

function ctxSections(c) {
  const o = c.overview;
  const out = [];
  // Active task, with its acceptance criteria.
  const t = o && o.active_task;
  out.push(ctxSection(I18N.t("ws.ctx.active"), null, t ? [
    h("div", { class: "ctx-task" }, h("span", { class: "key" }, t.key), " ", h("strong", null, t.title), " ", pill(t.status)),
    c.detail && c.detail.criteria.length ? h("ul", { class: "ctx-crit" }, c.detail.criteria.map((k) => h("li", { class: k.met ? "met" : "" },
      h("span", { class: "box", "aria-hidden": "true" }, k.met ? "✓" : ""), h("span", null, k.text, h("span", { class: "sr-only" }, " " + (k.met ? I18N.t("ws.ctx.met") : I18N.t("ws.ctx.notMet"))))))) : null,
    h("dl", { class: "kv" },
      h("dt", null, I18N.t("ws.ctx.owner")), h("dd", null, o.owner ? o.owner.harness : I18N.t("ws.ctx.nobody")),
      h("dt", null, I18N.t("ws.ctx.next")), h("dd", null, (o.latest_checkpoint && o.latest_checkpoint.next_action) || "-")),
    h("div", { class: "row ctx-actions" },
      h("button", { class: "btn small", onclick: () => openTask(t.key) }, I18N.t("ws.ctx.openTask")),
      sendBtn(I18N.t("ws.ctx.sendContext"), async () => {
        const r = await act(() => invoke("context_packet", { projectId: state.project.id, task: t.key, budget: "small" }));
        sendToFocused(r.text, I18N.t("ws.what.task"));
      })),
  ] : h("div", { class: "muted small" }, I18N.t("ws.ctx.idle") + " ", h("button", { class: "btn small", onclick: () => setTab("tasks") }, I18N.t("ws.ctx.pickTask")))));
  // Handoff
  const hd = o && o.pending_handoff;
  out.push(ctxSection(I18N.t("ws.ctx.handoff"), null, hd ? h("div", { class: "row ctx-item" },
    pill("warn", "waiting"), h("span", null, I18N.t("ws.ctx.handoffLine", { from: hd.from_harness || I18N.t("ws.ctx.you"), to: hd.to_harness, met: hd.quality.met, total: hd.quality.total })),
    h("span", { class: "spacer" }), h("button", { class: "btn small", onclick: () => openHandoff(hd.id) }, I18N.t("ws.ctx.review")))
    : h("div", { class: "muted small" }, I18N.t("ws.ctx.noHandoff"))));
  // Decisions
  const ds = c.decisions.slice().reverse();
  out.push(ctxSection(I18N.t("ws.ctx.decisions"), ds.length, ds.length ? h("ul", { class: "ctx-list" }, ds.slice(0, 5).map((d) => h("li", null,
    h("div", null, h("strong", null, d.title, ": "), d.decision), h("div", { class: "row" }, h("span", { class: "muted small" }, `${d.source} · ${ago(d.created_at)}`), h("span", { class: "spacer" }),
      sendBtn(I18N.t("ws.send"), () => sendToFocused(d.rationale ? I18N.t("ws.ctx.decisionWhy", { title: d.title, decision: d.decision, why: d.rationale }) : I18N.t("ws.ctx.decisionMsg", { title: d.title, decision: d.decision }), I18N.t("ws.what.decision")))))))
    : h("div", { class: "muted small" }, I18N.t("ws.ctx.noDecisions")), ds.length > 5 ? h("button", { class: "btn small ghost", onclick: () => setTab("knowledge") }, "All") : null));
  // Review notes
  out.push(ctxSection(I18N.t("ws.ctx.reviewNotes"), c.review.length, c.review.length ? [
    h("ul", { class: "ctx-list" }, c.review.slice(0, 5).map((r) => h("li", null,
      h("div", { class: "mono small" }, `${r.path}${r.line ? ":" + r.line : ""}`, r.status === "sent" ? h("span", { class: "muted" }, " · " + I18N.t("ws.ctx.sent")) : null), h("div", null, r.body)))),
    h("div", { class: "row ctx-actions" }, h("button", { class: "btn small", onclick: () => setTab("changes") }, I18N.t("ws.ctx.openReview")),
      c.review.some((r) => r.status === "open") ? sendBtn(I18N.t("ws.ctx.sendNotes"), sendOpenNotes) : null),
  ] : h("div", { class: "muted small" }, I18N.t("ws.ctx.noNotes"))));
  // Memory
  out.push(ctxSection(I18N.t("ws.ctx.memory"), c.memory.length, c.memory.length ? h("ul", { class: "ctx-list" }, c.memory.slice(0, 5).map((m) => h("li", null,
    h("div", null, h("span", { class: "pill accent" }, m.category), " ", m.content),
    h("div", { class: "row" }, h("span", { class: "spacer" }), sendBtn(I18N.t("ws.send"), () => sendToFocused(I18N.t("ws.ctx.memoryMsg", { category: m.category, content: m.content }), I18N.t("ws.what.memory")))))))
    : h("div", { class: "muted small" }, I18N.t("ws.ctx.noMemory")), c.memory.length > 5 ? h("button", { class: "btn small ghost", onclick: () => setTab("knowledge") }, "All") : null));
  out.push(usagePanel(c.sessions || [], c.tasks || []));
  // Activity across every AI
  const evs = o ? o.events.slice().reverse().slice(0, 8) : [];
  out.push(ctxSection(I18N.t("ws.ctx.activity"), null, evs.length ? h("ul", { class: "timeline ctx-feed" }, evs.map((e) => h("li", null, h("span", { class: "when" }, ago(e.created_at)), h("span", null, describeEvent(e)))))
    : h("div", { class: "muted small" }, I18N.t("ws.ctx.noActivity"))));
  return out;
}

// ---------- status bar ----------
function drawStatusBar() {
  const bar = $("#statusbar");
  if (!bar) return;
  clear(bar);
  if (!state.project || !appearance().statusBar) { bar.classList.add("hidden"); return; }
  bar.classList.remove("hidden");
  const mine = projThreads();
  const need = mine.filter(needsYou).length;
  const running = mine.filter((t) => t.running && t.kind !== "shell").length;
  const o = state.ctx && state.ctx.overview;
  const g = o && o.git;
  bar.append(...[
    g ? h("span", null, g.branch || "(detached)", " · ", g.dirty ? I18N.t("ws.bar.changed", { count: g.files.length }) : I18N.t("ws.bar.clean")) : null,
    h("span", null, running ? I18N.t("ws.bar.running", { count: running }) : I18N.t("ws.noAiRunning")),
    need ? h("span", { class: "sb-warn" }, I18N.t("ws.bar.need", { count: need })) : null,
    o && o.pending_handoff ? h("span", null, I18N.t("ws.bar.handoff", { name: o.pending_handoff.to_harness })) : null,
    h("span", { class: "spacer" }),
    h("span", null, I18N.t("ws.bar.local"))].filter(Boolean));
}

// ---------- history: handoffs and sessions ----------
async function viewHistory(v) {
  const a = h("section", { class: "history-sec" }, h("h2", null, I18N.t("ws.history.handoffs")));
  const b = h("section", { class: "history-sec" }, h("h2", null, I18N.t("ws.history.sessions")));
  v.append(a, b);
  await Promise.all([viewHandoffs(a), viewSessions(b)]);
}

// ---------- terminal drawer ----------
function toggleTermDrawer() {
  state.termDrawer = !state.termDrawer;
  setPref("termDrawer", state.termDrawer);
  if (!state.termDrawer) state.drawerThread = null;
  if (state.view === "workspace") render(); else setTab("workspace");
}

/** A shell docked under the panes, like an editor's terminal panel. */
function termDrawer() {
  const shells = projThreads().filter((t) => t.kind === "shell");
  const shell = shells.find((t) => t.id === state.drawerThread)
    || shells.find((t) => t.running && !(state.panesBy[state.project.id] || []).includes(t.id));
  state.drawerThread = shell?.id ?? null;
  const box = h("div", { class: "ws-drawer", style: `height:${appearance().drawerHeight}px` });
  const head = h("div", { class: "ws-drawer-head" },
    h("strong", null, I18N.t("ws.term.title")),
    shell ? h("span", { class: "muted small" }, shell.title, " · ", shell.running ? (shell.cwd || "") : I18N.t("ws.term.saved")) : null,
    h("span", { class: "spacer" }),
    shell && !shell.running ? restartTerminalButton(shell) : null,
    state.drawerThread ? h("button", { class: "btn small ghost", onclick: () => { const id = state.drawerThread; state.drawerThread = null; placeThread(id); render(); }, title: I18N.t("ws.term.moveTitle") }, I18N.t("ws.term.move")) : null,
    h("button", { class: "btn small ghost", "aria-label": I18N.t("ws.term.close"), onclick: toggleTermDrawer }, "✕"));
  box.append(head);
  if (!state.drawerThread) {
    box.append(h("div", { class: "ws-drawer-empty" }, h("button", { class: "btn small", onclick: () => startThread({ kind: "shell" }) }, I18N.t("ws.term.start"))));
    return box;
  }
  const x = ensureXterm(shell.id);
  box.append(x.el);
  requestAnimationFrame(() => {
    if (!x.opened) { x.term.open(x.el); x.opened = true; }
    fitThread(shell.id);
  });
  return box;
}

// ---------- inspector tabs ----------
function setCtxPane(pane) {
  state.ctxPane = pane;
  for (const b of document.querySelectorAll(".ctx-tab")) {
    const on = b.dataset.pane === pane;
    b.classList.toggle("active", on);
    b.setAttribute("aria-selected", String(on));
  }
  clear($("#ctx-body"));
  drawContext();
}

/** The Changes tab: changed files, open review notes, and the way into the full review. */
async function ctxChanges() {
  let c = { files: [] };
  try { c = await invoke("repo_changes", { projectId: state.project.id }); } catch { /* git unreadable */ }
  const review = (state.ctx && state.ctx.review) || [];
  const byFile = {};
  for (const r of review) byFile[r.path] = (byFile[r.path] || 0) + 1;
  return [
    ctxSection(I18N.t("ws.changes.title"), c.files.length, c.files.length ? h("ul", { class: "ctx-list" }, c.files.slice(0, 30).map((f) => h("li", { class: "row ctx-file" },
      h("code", null, f.status.trim() || "M"), h("span", { class: "mono small ctx-path" }, f.path),
      byFile[f.path] ? h("span", { class: "pill warn" }, I18N.t("ws.changes.notes", { count: byFile[f.path] })) : null)))
      : h("div", { class: "muted small" }, I18N.t("ws.changes.none"))),
    h("div", { class: "row ctx-actions", style: "padding:0 14px 12px" },
      h("button", { class: "btn small", onclick: () => setTab("changes") }, I18N.t("ws.ctx.openReview")),
      review.some((r) => r.status === "open") ? sendBtn(I18N.t("ws.changes.send", { notes: I18N.t("ws.changes.notes", { count: review.filter((r) => r.status === "open").length }) }), sendOpenNotes) : null),
  ];
}
async function sendOpenNotes() {
  const projectId = state.project.id;
  const r = await act(() => invoke("review_prompt_text", { projectId }));
  if (!r.count) { toast(I18N.t("ws.changes.nothing")); return; }
  if (state.project?.id === projectId && await sendToFocused(r.prompt, I18N.t("ws.what.review"))) {
    await recordReviewDelivery(projectId, r.ids);
    drawContext();
  }
}
