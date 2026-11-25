// Evyro desktop UI. Plain JavaScript, no build step.
// All text from repositories, agents and the database is inserted with
// textContent (never innerHTML), because it is untrusted.
"use strict";

const invoke = (cmd, args = {}) => {
  if (!window.__TAURI__) return Promise.reject({ code: "no_backend", message: "Evyro backend not available" });
  return window.__TAURI__.core.invoke(cmd, args);
};

// ---------- tiny DOM helpers ----------
// Props for a row or card that opens something when clicked: also reachable
// with Tab and opened with Enter or Space, so the keyboard can do the same.
function opens(fn) {
  return {
    tabindex: "0",
    onclick: fn,
    onkeydown: (e) => {
      if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
      e.preventDefault();
      fn(e);
    },
  };
}

function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  if (attrs) {
    for (const [k, v] of Object.entries(attrs)) {
      if (v === undefined || v === null || v === false) continue;
      if (k === "class") el.className = v;
      else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v);
      else if (k === "value") el.value = v;
      else if (k === "checked") el.checked = !!v;
      else el.setAttribute(k, v === true ? "" : v);
    }
  }
  for (const kid of kids.flat(Infinity)) {
    if (kid === undefined || kid === null || kid === false) continue;
    el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  }
  return el;
}
const $ = (sel) => document.querySelector(sel);
const clear = (el) => { while (el.firstChild) el.removeChild(el.firstChild); return el; };

function ago(iso) {
  if (!iso) return "";
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return `${Math.floor(s)}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}
const short = (x, n = 8) => (x ? String(x).slice(0, n) : "-");

function toast(msg, bad = false) {
  const t = h("div", { class: "toast" + (bad ? " bad" : "") }, msg);
  $("#toast-root").append(t);
  setTimeout(() => t.remove(), bad ? 7000 : 3500);
}
function errMsg(e) {
  if (!e) return "Unknown error";
  if (typeof e === "string") return e;
  return e.message || JSON.stringify(e);
}
async function act(fn, okMsg) {
  try {
    const r = await fn();
    if (okMsg) toast(okMsg);
    return r;
  } catch (e) {
    toast(errMsg(e), true);
    throw e;
  }
}
async function copy(text, what = "Copied") {
  try { await navigator.clipboard.writeText(text); toast(what); }
  catch { toast("Could not copy to the clipboard", true); }
}

const STATUS_PILL = {
  backlog: "", ready: "accent", in_progress: "accent", blocked: "warn", in_review: "accent",
  verified: "ok", done: "ok", cancelled: "", failed: "bad",
  running: "accent", completed: "ok", paused: "warn", recovering: "warn", handing_off: "accent",
  prepared: "warn", accepted: "ok", superseded: "", ok: "ok", warn: "warn", fail: "bad",
  incompatible: "bad", unknown_version: "warn", not_installed: "", fallback_only: "warn",
  needs_setup: "warn", missing: "bad", unknown: "",
};
const PROV = { human_recorded: "checked by you", evyro_verified: "backed by a passing test receipt", agent_reported: "reported by an agent" };
const pill = (s, label) => h("span", { class: "pill " + (STATUS_PILL[s] ?? "") }, label ?? String(s).replace(/_/g, " "));

// ---------- modal ----------
/** Close the open dialog and give focus back to whatever opened it. */
function closeModal() {
  const root = $("#modal-root");
  const opener = root._opener;
  clear(root);
  root._opener = null;
  if (opener && opener !== document.body && opener.isConnected) opener.focus();
}
function modal(title, bodyNodes, actions) {
  const root = $("#modal-root");
  // Close only this dialog: a slow action finishing late must not wipe a dialog opened since.
  // Remember what opened the dialog, so closing it hands the keyboard back instead of leaving it at the top.
  if (!root.firstChild) root._opener = document.activeElement;
  const close = () => { if (back.isConnected) closeModal(); };
  const back = h("div", { class: "modal-back", onclick: (e) => { if (e.target === back) close(); } },
    h("div", { class: "modal", role: "dialog", "aria-label": title },
      h("h2", null, title), bodyNodes,
      h("div", { class: "actions" },
        h("button", { class: "btn", onclick: close }, actions.length ? "Cancel" : "Close"),
        actions.map((a) => h("button", {
          class: "btn " + (a.primary ? "primary" : ""), onclick: async (ev) => {
            ev.target.disabled = true;
            try { if ((await a.run()) !== false) close(); }
            catch { /* toast already shown */ }
            finally { ev.target.disabled = false; }
          },
        }, a.label)))));
  // Tab stays inside the dialog: it wraps from the last control to the first and back.
  back.addEventListener("keydown", (e) => {
    if (e.key !== "Tab") return;
    const items = [...back.querySelectorAll("a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])")].filter((el) => el.getClientRects().length);
    if (!items.length) return;
    const [lo, hi] = [items[0], items[items.length - 1]];
    if (e.shiftKey && document.activeElement === lo) { e.preventDefault(); hi.focus(); }
    else if (!e.shiftKey && document.activeElement === hi) { e.preventDefault(); lo.focus(); }
  });
  clear(root).append(back);
  const first = back.querySelector("input, textarea, select");
  if (first) first.focus();
  else (back.querySelector(".btn.primary") || back.querySelector("button"))?.focus();
}
const field = (label, input, hint) => h("label", { class: "field" }, label, input, hint ? h("div", { class: "hint" }, hint) : null);
const lines = (s) => String(s || "").split("\n").map((x) => x.trim()).filter(Boolean);

// ---------- state ----------
const state = {
  projects: [],
  project: null,
  view: "workspace",
  drawerTask: null,
  timer: null,
  agents: null,
  targets: null,
};
const KIND_LABEL = { harness: "Coding agent", api: "API key", ide: "AI IDE", chat: "Chat app" };

/** Switch targets with readiness, cached until something changes. */
async function loadTargets(force) {
  if (force || !state.targets) state.targets = await invoke("list_targets");
  return state.targets;
}
/** A <select> of every switch target, grouped by kind. */
async function targetSelect(preferred) {
  let ts = [];
  try { ts = await loadTargets(); } catch (e) { toast(errMsg(e), true); }
  const sel = h("select", null, ["harness", "api", "ide", "chat"].map((k) => {
    const group = ts.filter((t) => t.target.kind === k);
    return group.length ? h("optgroup", { label: KIND_LABEL[k] }, group.map((t) =>
      h("option", { value: t.target.name }, `${t.target.name}${t.state === "needs_setup" ? "  (needs setup)" : ""}`))) : null;
  }));
  const names = ts.map((t) => t.target.name);
  sel.value = names.includes(preferred) ? preferred : (names.includes("codex") ? "codex" : names[0] || "");
  return sel;
}
const targetPill = (st) => pill(st === "ready" ? "ok" : st, st.replace(/_/g, " "));
const targetKind = (name) => (state.targets || []).find((t) => t.target.name === name)?.target.kind;

/**
 * Give work to a target. IDE and chat targets get the hand-over text on the
 * clipboard and the app opened; agents start in a thread inside the window.
 * Returns true when done, false when drift stopped it (shown in `el`).
 */
async function handTo(target, taskKey, el, acceptDrift = false, handoffId = null) {
  await loadTargets();
  const kind = targetKind(target);
  if (kind === "ide" || kind === "chat") {
    let r;
    try {
      r = await invoke("handover_to_app", { projectId: state.project.id, target, task: taskKey || null, handoffId, acceptDrift });
    } catch (e) {
      if (e && e.code === "conflict" && !handoffId) {
        // The task is still with another AI: pass it on with a handoff instead.
        toast("This task is still with another AI. Write a short handoff and Evyro passes it on.");
        handoffDialog(taskKey, target);
        return false;
      }
      toast(errMsg(e), true);
      throw e;
    }
    if (r.drift) { showDrift(el, r.drift, () => handTo(target, taskKey, el, true, handoffId)); return false; }
    const x = r.handover;
    const where = kind === "ide" ? "Paste it into the IDE's AI chat." : "Paste it into a new chat.";
    toast(`${x.opened ? "Opened " + x.display + ". " : ""}${x.copied_to_clipboard ? "Hand-over text copied. " + where : "Hand-over text saved to " + x.file + ". Copy it from there."}`);
    render();
    return true;
  }
  return startThread({ kind: "agent", target, task: taskKey || null, handoffId });
}

/** Start `evyro run --to <target>` in a separate Terminal window instead of a thread. */
async function openInTerminal(target, taskKey, handoffId = null) {
  const r = await act(() => invoke("open_in_terminal", { projectId: state.project.id, target, task: taskKey || null, handoffId }));
  if (r.opened) toast(`Started ${target} in Terminal. It picks up the handoff there.`);
  else { await copy(r.command, "This computer has no Terminal hook. Command copied; run it in a terminal."); }
  return true;
}

// ---------- sidebar ----------
async function loadProjects(selectId) {
  try {
    state.projects = await invoke("list_projects", {});
  } catch (e) {
    state.projects = [];
    renderBackendError(e);
    return;
  }
  const nav = clear($("#project-list"));
  if (!state.projects.length) nav.append(h("div", { class: "muted small", style: "padding:8px 10px" }, "No projects yet."));
  for (const p of state.projects) {
    nav.append(h("button", {
      class: "project-item" + (state.project && state.project.id === p.id ? " active" : ""),
      onclick: () => selectProject(p.id),
      title: p.root_path,
    }, h("span", { class: "name" }, p.alias || p.name), h("span", { class: "path" }, p.root_path)));
  }
  const want = selectId || (state.project && state.project.id) || (state.projects[0] && state.projects[0].id);
  if (want) selectProject(want, true);
  else render();
}

function selectProject(id, silent) {
  const p = state.projects.find((x) => x.id === id);
  if (!p) return;
  state.project = p;
  closeDrawer();
  for (const el of document.querySelectorAll(".project-item")) el.classList.toggle("active", el.title === p.root_path);
  $("#project-title").textContent = p.alias || p.name;
  const sub = $("#project-sub");
  sub.textContent = `${p.root_path}${p.stack && p.stack.length ? " · " + p.stack.join(", ") : ""}`;
  sub.title = sub.textContent;
  if (!silent) state.view = "workspace";
  state.ctx = null;
  setTab(state.view);
  updateLiveBadge();
  loadThreads().then(() => drawThreadList());
  drawContext();
}

function addProjectDialog() {
  const input = h("input", { type: "text", placeholder: "/Users/you/Developer/my-app" });
  const choose = h("button", {
    class: "btn small", type: "button", onclick: async () => {
      try { const p = await invoke("pick_folder"); if (p) input.value = p; }
      catch (e) { toast(errMsg(e), true); }
    },
  }, "Choose folder…");
  modal("Add a project", [
    field("Repository folder", h("div", { class: "row" }, input, choose), "Choose a Git repository, or paste its path. Evyro creates .evyro/ there and adds an Evyro section to agent instruction files only where none exists."),
  ], [{
    label: "Add project", primary: true, run: async () => {
      const path = input.value.trim();
      if (!path) { toast("Enter a folder path", true); return false; }
      const r = await act(() => invoke("add_project", { path }), "Project added");
      await loadProjects(r.project.id);
    },
  }]);
}

// ---------- tabs ----------
// Screens that were merged: Overview and Threads into Workspace, Handoffs and Sessions into History.
const TAB_ALIAS = { overview: "workspace", threads: "workspace", handoffs: "history", sessions: "history" };
function setTab(view) {
  view = TAB_ALIAS[view] || view;
  state.view = view;
  for (const b of document.querySelectorAll(".tab")) b.classList.toggle("active", b.dataset.view === view);
  render();
}

function render() {
  clearInterval(state.timer);
  const v = clear($("#view"));
  if (!state.project) $("#project-title").textContent = t("frame.noProject");
  if (!state.project && !["agents", "settings"].includes(state.view)) {
    const footer = t("welcome.footer", { keys: "\u0000", cmd: "\u0001" }).split(/(\u0000|\u0001)/).map((part) =>
      part === "\u0000" ? h("kbd", null, MOD + " K") : part === "\u0001" ? h("code", null, "evyro init") : part);
    v.append(h("div", { class: "welcome" },
      h("img", { src: "logo.png", alt: "" }),
      h("h1", null, t("welcome.title")),
      h("p", null, t("welcome.lead")),
      h("ol", { class: "setup" },
        h("li", null, h("strong", null, t("welcome.step1.title") + " "), t("welcome.step1.body"), h("div", { class: "row" }, h("button", { class: "btn primary small", onclick: addProjectDialog }, t("welcome.step1.add")), h("button", { class: "btn small", onclick: trySample }, t("welcome.step1.sample")))),
        h("li", null, h("strong", null, t("welcome.step2.title") + " "), t("welcome.step2.body"), h("div", null, h("button", { class: "btn small", onclick: () => setTab("agents") }, t("welcome.step2.open")))),
        h("li", null, h("strong", null, t("welcome.step3.title") + " "), t("welcome.step3.body"))),
      h("p", { class: "small" }, ...footer)));
    return;
  }
  const views = { workspace: viewWorkspace, tasks: viewTasks, changes: viewChanges, history: viewHistory, knowledge: viewKnowledge, skills: viewSkills, agents: viewAgents, settings: viewSettings };
  v.classList.toggle("ws-view", state.view === "workspace");
  (views[state.view] || viewWorkspace)(v);
}

function renderBackendError(e) {
  const v = clear($("#view"));
  v.append(h("div", { class: "banner bad" }, "Could not open the Evyro database: ", errMsg(e)));
}

// ---------- overview ----------
async function viewOverview(v) {
  const pid = state.project.id;
  const draw = async () => {
    let o;
    try { o = await invoke("overview", { projectId: pid }); }
    catch (e) { clear(v).append(h("div", { class: "banner bad" }, errMsg(e))); return; }
    if (!v.isConnected || state.project.id !== pid) return;
    clear(v);
    if (o.recovered && o.recovered.length) {
      v.append(h("div", { class: "banner warn" }, `${o.recovered.length} session(s) had exited and were closed out with a recovery checkpoint.`));
    }
    if (o.pending_handoff) {
      const hd = o.pending_handoff;
      v.append(h("div", { class: "banner info" },
        h("strong", null, "Handoff waiting"), ` for ${hd.to_harness} (quality ${hd.quality.met}/${hd.quality.total}), prepared ${ago(hd.created_at)}.`,
        h("span", { class: "spacer" }),
        h("button", { class: "btn small", onclick: () => openHandoff(hd.id) }, "Review")));
    }
    if (!o.project.commands.test.length) {
      v.append(h("div", { class: "banner warn" }, "No test command is configured, so agents cannot record verification receipts.",
        h("span", { class: "spacer" }), h("button", { class: "btn small", onclick: () => setTab("settings") }, "Set one up")));
    }

    const grid = h("div", { class: "grid" });
    // Active task
    const t = o.active_task;
    grid.append(h("div", { class: "card" },
      h("div", { class: "card-head" }, h("h2", null, "Active task"), t ? pill(t.status) : null),
      t ? [
        h("div", null, h("span", { class: "key" }, t.key), " ", h("strong", null, t.title)),
        h("dl", { class: "kv", style: "margin-top:10px" },
          h("dt", null, "Owner"), h("dd", null, o.owner ? `${o.owner.harness} · session ${short(o.owner.id, 12)}` : "nobody"),
          h("dt", null, "Last checkpoint"), h("dd", null, o.latest_checkpoint ? `${ago(o.latest_checkpoint.created_at)} (${o.latest_checkpoint.trigger})` : "none"),
          h("dt", null, "Next step"), h("dd", null, (o.latest_checkpoint && o.latest_checkpoint.next_action) || "-")),
        h("div", { class: "row", style: "margin-top:12px" },
          h("button", { class: "btn small", onclick: () => openTask(t.key) }, "Open task"),
          h("button", { class: "btn small", onclick: () => handoffDialog(t.key) }, "Hand off"),
          h("button", { class: "btn small", onclick: () => copyContext(t.key) }, "Copy context for an agent")),
      ] : h("div", { class: "empty" }, "Nothing in progress. ", h("button", { class: "btn small", onclick: () => setTab("tasks") }, "Pick a task"))));

    // Repository
    const g = o.git;
    grid.append(h("div", { class: "card" },
      h("div", { class: "card-head" }, h("h2", null, "Repository"), g ? (g.dirty ? pill("warn", `${g.files.length} changed`) : pill("ok", "clean")) : pill("bad", "not readable")),
      g ? [
        h("dl", { class: "kv" },
          h("dt", null, "Branch"), h("dd", null, g.branch || "(detached)"),
          h("dt", null, "HEAD"), h("dd", { class: "mono" }, short(g.head, 10)),
          g.upstream ? [h("dt", null, "Upstream"), h("dd", null, `${g.upstream} (+${g.ahead ?? 0} / -${g.behind ?? 0})`)] : null),
        g.files.length ? h("ul", { class: "list", style: "margin-top:8px" },
          g.files.slice(0, 8).map((f) => h("li", null, h("code", null, f.status), h("span", { class: "mono" }, f.path))),
          g.files.length > 8 ? h("li", { class: "muted" }, `and ${g.files.length - 8} more`) : null) : null,
      ] : h("div", { class: "muted" }, "Git could not read this folder.")));

    // Health
    const counts = o.health.task_counts || {};
    grid.append(h("div", { class: "card" },
      h("div", { class: "card-head" }, h("h2", null, "Health"), o.health.findings.length ? pill("warn", `${o.health.findings.length} finding(s)`) : pill("ok", "all clear")),
      h("div", { class: "stat-row" },
        ["in_progress", "ready", "blocked", "verified", "done"].map((k) => h("div", { class: "stat" }, h("span", { class: "n" }, counts[k] || 0), h("span", { class: "l" }, k.replace("_", " "))))),
      o.health.findings.length ? h("ul", { class: "list", style: "margin-top:10px" },
        o.health.findings.slice(0, 6).map((f) => h("li", null, pill("warn", f.kind.replace(/_/g, " ")), h("span", null, f.detail)))) : null));

    // Activity
    grid.append(h("div", { class: "card wide" },
      h("div", { class: "card-head" }, h("h2", null, "Recent activity"), h("span", { class: "muted small" }, "refreshes every few seconds")),
      o.events.length ? h("ul", { class: "timeline" }, o.events.slice().reverse().map((e) => h("li", null, h("span", { class: "when" }, ago(e.created_at)), h("span", null, describeEvent(e))))) : h("div", { class: "empty" }, "No activity yet.")));
    v.append(grid);
  };
  v.append(h("div", { class: "muted" }, "Loading…"));
  await draw();
  state.timer = setInterval(draw, 4000);
}

function describeEvent(e) {
  const p = e.payload || {};
  switch (e.kind) {
    case "task.created": return `Task created: ${p.title || ""}`;
    case "task.status_changed": return `Task moved ${p.from} → ${p.to} by ${p.actor || "?"}${p.override_reason ? " (override: " + p.override_reason + ")" : ""}`;
    case "task.criterion_checked": return `Acceptance criterion ${p.index} marked ${p.met ? "met" : "not met"} (${p.provenance})`;
    case "handoff.prepared": return `Handoff prepared ${p.from || "?"} → ${p.to} (quality ${p.quality})`;
    case "handoff.accepted": return `Handoff accepted by ${p.agent}${p.drift ? " with drift accepted" : ""}`;
    case "handoff.reopened": return `Handoff reopened: ${p.reason}`;
    case "handoff.drift_detected": return `Resume blocked by drift: ${p.summary}`;
    case "verification.completed": return `Verification ${p.passed ? "passed" : "failed"}: ${(p.command || []).join(" ")}`;
    case "checkpoint.created": return `Checkpoint (${p.trigger || "manual"})`;
    case "session.status_changed": return `Session ${p.from} → ${p.to}${p.reason ? " (" + p.reason + ")" : ""}`;
    case "agent.started": return `Agent launched (${p.mode})`;
    case "agent.handed_to_app": return `Handed to ${p.target || "an app"}${p.opened ? " (app opened)" : ""}`;
    case "agent.stopped": return `Agent exited (code ${p.exit_code ?? "signal"})`;
    default: return e.kind.replace(/[._]/g, " ");
  }
}

async function copyContext(taskKey) {
  const r = await act(() => invoke("context_packet", { projectId: state.project.id, task: taskKey, budget: "large" }));
  copy(r.text, `Context copied (about ${r.packet.approx_tokens} tokens)`);
}

// ---------- tasks ----------
const COLUMNS = [
  ["backlog", "Backlog"], ["ready", "Ready"], ["in_progress", "In progress"], ["blocked", "Blocked"],
  ["in_review", "In review"], ["verified", "Verified"], ["done", "Done"],
];

/** Start the most important ready task on a free AI. Nothing to pick or set up. */
async function workOnNext(button = null) {
  const projectId = state.project.id;
  if (button) button.disabled = true;
  try {
    const next = await act(() => invoke("next_work", { projectId }));
    if (state.project?.id !== projectId) return;
    await startThread({ kind: "agent", target: next.agent, task: next.task.key });
    toast(`Started ${next.task.key} with ${next.agent}.`);
  } catch { /* act reported the reason */ }
  finally { if (button?.isConnected) button.disabled = false; }
}

// ---------- guided tour: the first task, checkpoint, receipt and handoff on a sample project ----------
const TOUR_KEY = "evyro.tour.dismissed";
function tourDismissed() { try { return localStorage.getItem(TOUR_KEY) === "1"; } catch { return false; } }
function setTourDismissed(v) { try { if (v) localStorage.setItem(TOUR_KEY, "1"); else localStorage.removeItem(TOUR_KEY); } catch { /* remembered until restart */ } }
const isSample = (p) => !!p && p.name === "Evyro sample" && /[\\/]samples[\\/]evyro-sample$/.test(p.root_path || "");

/** Create (or reopen) the sample project, open it, and show the tour. */
async function trySample() {
  let r;
  try { r = await act(() => invoke("create_sample"), "Sample project ready"); }
  catch { return; }
  setTourDismissed(false);
  await loadProjects(r.project.id);
  setTab("tasks");
}

/** The checklist: each step is ticked from what Evyro really recorded, not from clicks. */
async function drawTour(box, pid, again) {
  if (!box.isConnected || state.project?.id !== pid) return;
  if (!isSample(state.project) || tourDismissed()) { clear(box); return; }
  let st;
  try { st = await invoke("tour_status", { projectId: pid }); } catch { return; }
  if (!box.isConnected) return;
  const first = state.project && (await invoke("list_tasks", { projectId: pid }).catch(() => []))[0]?.task.key || "T-1";
  const steps = [
    [t("tour.step.start"), st.started, h("button", { class: "btn small", onclick: () => newThreadDialog(first) }, t("tour.step.start.action"))],
    [t("tour.step.checkpoint"), st.checkpoint, null],
    [t("tour.step.verify"), st.verified, h("button", { class: "btn small", onclick: () => runVerify(first, again) }, t("tour.step.verify.action"))],
    [t("tour.step.handoff"), st.handoff, h("button", { class: "btn small", onclick: () => handoffDialog(first) }, t("tour.step.handoff.action"))],
  ];
  const done = steps.filter((x) => x[1]).length;
  clear(box).append(h("section", { class: "card tour", "aria-label": t("tour.label") },
    h("div", { class: "row" }, h("h3", { style: "margin:0" }, done === steps.length ? t("tour.titleDone") : t("tour.title")),
      h("span", { class: "spacer" }), h("span", { class: "muted small" }, t("tour.progress", { n: done, of: steps.length })),
      h("button", { class: "btn small", onclick: () => { setTourDismissed(true); clear(box); } }, t("tour.dismiss"))),
    done === steps.length
      ? h("p", { class: "small" }, t("tour.closing"), h("button", { class: "btn small primary", onclick: addProjectDialog }, t("tour.addOwn")))
      : h("ol", { class: "tour-steps" }, steps.map(([label, ok, action]) => h("li", { class: ok ? "done" : "" },
        h("span", { class: "tour-mark", "aria-hidden": "true" }, ok ? "✓" : "○"), h("span", null, label, ok ? h("span", { class: "sr-only" }, " " + t("tour.done")) : null),
        !ok && action ? action : null)))));
}

async function viewTasks(v) {
  const pid = state.project.id;
  const head = h("div", { class: "row", style: "margin-bottom:14px" },
    h("button", { class: "btn primary", onclick: newTaskDialog }, "+ New task"),
    h("button", { class: "btn", onclick: (e) => workOnNext(e.currentTarget),
      title: "Evyro picks the most important ready task and an AI that is free, and uses a separate worktree only when another AI's work could collide" }, "Work on next task"),
    h("span", { class: "spacer" }),
    h("span", { class: "muted small" }, "Cards show the AIs working on them live. Click a task for criteria, checkpoints and receipts."));
  v.append(head);
  const tourBox = h("div");
  v.append(tourBox);
  const board = h("div", { class: "board" });
  v.append(board);
  const canRefresh = () => board.isConnected && state.project?.id === pid && !board.contains(document.activeElement);
  const draw = async () => {
    if (!canRefresh()) return;
    let rows;
    await loadThreads();
    try { rows = await invoke("list_tasks", { projectId: pid, includeClosed: true }); }
    catch (e) { if (canRefresh()) clear(board).append(h("div", { class: "banner bad" }, errMsg(e))); return; }
    if (!canRefresh()) return;
    drawTour(tourBox, pid, draw);
    clear(board);
    for (const [st, label] of COLUMNS) {
      const items = rows.filter((r) => r.task.status === st);
      board.append(h("div", { class: "column" },
        h("div", { class: "column-head" }, h("span", null, label), h("span", null, items.length)),
        items.map(taskCard)));
    }
    const other = rows.filter((r) => !COLUMNS.some(([s]) => s === r.task.status));
    if (other.length) board.append(h("div", { class: "column" }, h("div", { class: "column-head" }, h("span", null, "Closed"), h("span", null, other.length)), other.map(taskCard)));
  };
  await draw();
  if (board.isConnected) state.timer = setInterval(draw, 5000);
}

function taskCard(r) {
  const t = r.task;
  const pct = r.criteria_total ? Math.round((100 * r.criteria_met) / r.criteria_total) : 0;
  // AIs working on this task right now, with their latest line, so the board shows live work.
  const live = (state.threads || []).filter((x) => x.project_id === state.project.id && x.task === t.key && x.running);
  const stop = (fn) => (e) => { e.stopPropagation(); fn(); };
  const open = ["backlog", "ready", "blocked"].includes(t.status);
  return h("div", { class: "task-card" + (live.length ? " live" : ""), ...opens(() => openTask(t.key)), "aria-label": `${t.key} ${t.title}` },
    h("div", { class: "meta" }, h("span", { class: "key" }, t.key), h("span", { class: "pill" }, t.kind), h("span", { class: "pill" }, "P" + t.priority)),
    h("div", { class: "t" }, t.title),
    r.criteria_total ? [h("div", { class: "progress", title: `${r.criteria_met}/${r.criteria_total} criteria met` }, h("span", { style: `width:${pct}%` })),
      h("div", { class: "muted small" }, `${r.criteria_met}/${r.criteria_total} criteria met`)] : h("div", { class: "muted small" }, "No acceptance criteria"),
    live.map((x) => h("div", { class: "card-ai" },
      h("div", { class: "row" }, agentBadge(x, true), h("span", { class: "small card-ai-name" }, x.title), h("span", { class: "spacer" }), threadStatus(x)),
      liveLine(x) ? h("div", { class: "card-ai-line mono" }, liveLine(x)) : null,
      h("button", { class: "btn small", onclick: stop(() => { placeThread(x.id); setTab("workspace"); }) }, needsYou(x) ? "Answer" : "Open"))),
    h("div", { class: "meta" },
      r.owner && !live.length ? pill("running", r.owner) : null,
      r.blocked_by.length ? pill("blocked", "blocked by " + r.blocked_by.join(", ")) : null,
      open && !live.length && state.project ? h("button", { class: "btn small", onclick: stop(() => newThreadDialog(t.key)), title: "Start an AI on this task" }, "Start with…") : null));
}

/** The last line an AI wrote: the bottom of its terminal, or its latest chat message. */
function liveLine(t) {
  if (t.kind === "chat") {
    const blocks = chatBlocks(chatData(t.id).log);
    for (let i = blocks.length - 1; i >= 0; i--) {
      const b = blocks[i];
      if (b.k === "perm" && b.answer === undefined) return "asks: " + ((b.params && b.params.toolCall && b.params.toolCall.title) || "permission");
      if (b.k === "agent" && b.text.trim()) return b.text.trim().split("\n").pop().slice(0, 120);
      if (b.k === "tool") return `${b.title} (${b.status})`;
    }
    return "";
  }
  const x = XTERMS.get(t.id);
  if (!x || !x.opened) return "";
  const buf = x.term.buffer.active;
  for (let i = buf.baseY + buf.cursorY; i >= 0 && i > buf.baseY + buf.cursorY - 30; i--) {
    const line = buf.getLine(i)?.translateToString(true).trim();
    if (line) return line.slice(0, 120);
  }
  return "";
}

function newTaskDialog() {
  const title = h("input", { type: "text", placeholder: "Add rate limiting to the API" });
  const desc = h("textarea", { placeholder: "What and why (optional)" });
  const acc = h("textarea", { placeholder: "One criterion per line, e.g.\n429 returned after 100 requests/min\nIntegration test covers the limit" });
  const kind = h("select", null, ["task", "feature", "bug", "tech_debt", "spike", "docs", "security", "maintenance"].map((k) => h("option", { value: k }, k)));
  const prio = h("select", null, [0, 1, 2, 3, 4].map((p) => h("option", { value: p, selected: p === 2 }, "P" + p)));
  const ready = h("input", { type: "checkbox", checked: true });
  modal("New task", [
    field("Title", title),
    field("Description", desc),
    field("Acceptance criteria", acc, "The task can only reach Verified once every criterion is checked off and a test run passes."),
    h("div", { class: "row" }, field("Kind", kind), field("Priority", prio)),
    h("label", { class: "row small" }, ready, "Mark as ready to pick up"),
  ], [{
    label: "Create task", primary: true, run: async () => {
      if (!title.value.trim()) { toast("Give the task a title", true); return false; }
      const t = await act(() => invoke("create_task", {
        projectId: state.project.id,
        input: { title: title.value.trim(), description: desc.value.trim(), kind: kind.value, priority: Number(prio.value), acceptance: lines(acc.value), ready: ready.checked },
      }), "Task created");
      if (state.view === "tasks") render();
      openTask(t.key);
    },
  }]);
}

// ---------- task drawer ----------
function closeDrawer() {
  state.drawerTask = null;
  $("#drawer").classList.add("hidden");
  $("#drawer").setAttribute("aria-hidden", "true");
}

/** What is failing on the task's pull request, and a way to hand it to the AI that owns the branch. */
async function prFeedbackDialog(t) {
  let f;
  try { f = await act(() => invoke("pr_feedback", { projectId: state.project.id, task: t.key })); }
  catch { return; }
  showPrFeedback(t, f);
}

function showPrFeedback(t, f) {
  const live = (state.threads || []).filter((x) => x.project_id === state.project.id && x.task === t.key && x.running && x.kind !== "shell");
  const summary = !f.pr ? "No pull request for this branch yet."
    : !f.actionable ? f.prompt
    : `${f.failed.length} failing check${f.failed.length === 1 ? "" : "s"} and ${f.review.length} open review comment${f.review.length === 1 ? "" : "s"} on ${f.pr.url}.${f.pending ? ` ${f.pending} checks are still running.` : ""}`;
  const buttons = [{ label: "Close", run: () => true }];
  if (f.actionable) {
    buttons.unshift({ label: "Copy prompt", run: () => { copy(f.prompt, "Prompt copied"); return false; } });
    for (const x of live.slice(0, 2)) {
      buttons.unshift({ label: `Send to ${x.title}`, primary: true, run: async () => {
        // The text was written by other people and logs can hold secrets: ask once more, in words.
        if (!(await confirmDialog(`Send this to ${x.title}?`, "It includes build log and review comment text written by others, and may include secrets Evyro did not recognise. Send it only if you have read it.", "Send"))) {
          showPrFeedback(t, f);
          return false;
        }
        try { await sendToThread(x.id, f.prompt); toast(`Sent to ${x.title}.`); return true; }
        catch (e) { toast(errMsg(e), true); return false; }
      } });
    }
  }
  modal(`PR feedback for ${t.key}`, [
    h("p", null, summary),
    f.actionable && !live.length ? h("p", { class: "muted small" }, `No AI is running on ${t.key}. Copy the prompt, or start one with Start with…, then send it.`) : null,
    f.actionable ? h("p", { class: "muted small" }, "Read it before you send it: build logs can hold secrets that Evyro does not recognise, and comments are written by other people. `evyro pr feedback --links-only` leaves that text out.") : null,
    f.actionable ? h("pre", { class: "plan-body", "aria-label": "Prompt" }, f.prompt) : null,
  ], buttons);
}

/** The plan an AI proposed for a task. A person approves or rejects it; an AI has no way to. */
function planCard(key, plan, refresh) {
  const waiting = plan.status === "proposed";
  const note = h("input", { type: "text", placeholder: "Optional note for the AI", "aria-label": "Note for the AI" });
  const decide = (approve) => async () => {
    try {
      await act(() => invoke("plan_decide", { projectId: state.project.id, task: key, approve, note: note.value.trim() || null }), approve ? `Plan for ${key} approved.` : `Plan for ${key} rejected.`);
      refresh();
    } catch { refresh(); }
  };
  return h("div", { class: "card plan-card", role: "group", "aria-label": `Plan for ${key}` },
    h("div", { class: "row" }, h("h3", { style: "margin:0" }, "Plan"), pill(waiting ? "warn" : plan.status === "approved" ? "ok" : "bad", waiting ? "waiting for you" : plan.status),
      h("span", { class: "spacer" }), h("span", { class: "muted small" }, `by ${plan.proposed_by}`)),
    h("pre", { class: "plan-body" }, plan.body),
    plan.note ? h("div", { class: "muted small" }, `Note: ${plan.note}`) : null,
    waiting ? h("div", { class: "row", style: "margin-top:8px" }, note,
      h("button", { class: "btn small primary", onclick: decide(true) }, "Approve plan"),
      h("button", { class: "btn small", onclick: decide(false) }, "Reject")) : null);
}

async function openTask(key) {
  state.drawerTask = key;
  const dr = $("#drawer");
  dr.classList.remove("hidden");
  dr.setAttribute("aria-hidden", "false");
  const body = clear($("#drawer-body"));
  body.append(h("div", { class: "muted" }, "Loading…"));
  let d;
  try { d = await invoke("task_detail", { projectId: state.project.id, task: key }); }
  catch (e) { clear(body).append(h("div", { class: "banner bad" }, errMsg(e))); return; }
  if (state.drawerTask !== key) return;
  const t = d.task;
  const title = clear($("#drawer-title"));
  title.append(h("span", { class: "key" }, t.key), " ", t.title);
  clear(body);
  const refresh = () => { openTask(key); if (state.view === "tasks" || (state.view === "workspace" && !projThreads().length)) render(); drawContext(); };

  // Status
  const statusSel = h("select", { style: "width:auto" }, h("option", { value: "" }, "Move to…"), d.allowed_next.filter((s) => s !== t.status).map((s) => h("option", { value: s }, s.replace(/_/g, " "))));
  statusSel.addEventListener("change", async () => {
    const to = statusSel.value;
    if (!to) return;
    try {
      await act(() => invoke("set_task_status", { projectId: state.project.id, task: t.key, status: to }), `${t.key} moved to ${to.replace(/_/g, " ")}`);
      refresh();
    } catch (e) {
      statusSel.value = "";
      if (e && e.code === "verification_required" && to === "verified") overrideDialog(t.key, errMsg(e), refresh);
    }
  });
  body.append(h("div", { class: "row" }, pill(t.status), h("span", { class: "pill" }, t.kind), h("span", { class: "pill" }, "P" + t.priority),
    d.owner ? pill("running", "owned by " + d.owner.harness) : h("span", { class: "pill" }, "no owner"),
    h("span", { class: "spacer" }), statusSel));
  if (d.blocked_by.length) body.append(h("div", { class: "banner warn", style: "margin-top:10px" }, "Blocked by unfinished tasks: " + d.blocked_by.join(", ")));
  if (d.pending_handoff) body.append(h("div", { class: "banner info", style: "margin-top:10px" }, `Open handoff for ${d.pending_handoff.to_harness}.`, h("span", { class: "spacer" }), h("button", { class: "btn small", onclick: () => openHandoff(d.pending_handoff.id) }, "Review")));
  if (t.description) body.append(h("p", { style: "white-space:pre-wrap" }, t.description));

  // Plan: what an AI proposed before editing. Only a person approves.
  let planInfo = null;
  try { planInfo = (await invoke("plan_get", { projectId: state.project.id, task: t.key })).plan; } catch { /* no plan section */ }
  if (state.drawerTask !== key) return;
  if (planInfo) body.append(planCard(t.key, planInfo, refresh));

  // Criteria
  const met = d.criteria.filter((c) => c.met && c.counts).length;
  body.append(h("h3", null, `Acceptance criteria · ${met}/${d.criteria.length}`));
  if (!d.criteria.length) body.append(h("div", { class: "muted small" }, "None yet. Without criteria, Verified only needs a passing test run."));
  body.append(h("ul", { class: "criteria" }, d.criteria.map((c) => {
    const box = h("input", { type: "checkbox", checked: c.met && c.counts, "aria-label": "Criterion " + c.index });
    box.addEventListener("change", async () => {
      try { await act(() => invoke("check_criterion", { projectId: state.project.id, task: t.key, index: c.index, met: box.checked, note: "" })); refresh(); }
      catch { box.checked = !box.checked; }
    });
    return h("li", null, box, h("div", { class: "ctext" }, c.text,
      c.provenance ? h("div", { class: "muted small" }, c.counts ? `${c.met ? "met" : "not met"} · ${PROV[c.provenance] || c.provenance} ${ago(c.checked_at)}` : `${c.actor} says this is met; check it to confirm`, c.note ? ` · ${c.note}` : "") : null));
  })));
  const newCrit = h("input", { type: "text", placeholder: "Add a criterion" });
  body.append(h("div", { class: "row", style: "margin-top:8px" }, newCrit, h("button", {
    class: "btn small", onclick: async () => {
      if (!newCrit.value.trim()) return;
      await act(() => invoke("add_criterion", { projectId: state.project.id, task: t.key, text: newCrit.value }));
      refresh();
    },
  }, "Add")));

  // Actions
  body.append(h("h3", null, "Actions"));
  body.append(h("div", { class: "row" },
    h("button", { class: "btn small", onclick: () => runVerify(t.key, refresh) }, "Run tests"),
    h("button", { class: "btn small", onclick: () => openComparison(t) }, "Compare agents"),
    h("button", { class: "btn small", onclick: () => handoffDialog(t.key) }, "Hand off"),
    h("button", { class: "btn small", onclick: () => prFeedbackDialog(t) }, "PR feedback…"),
    h("button", { class: "btn small", onclick: () => copyContext(t.key) }, "Copy context"),
    h("button", { class: "btn small", onclick: () => copy(`evyro run --agent claude-code --task ${t.key}`, "Command copied") }, "Copy run command")));

  // Verification receipts
  body.append(h("h3", null, "Verification receipts"));
  if (!d.verifications.length) body.append(h("div", { class: "muted small" }, "No test runs recorded for this task."));
  else body.append(h("ul", { class: "list" }, d.verifications.slice().reverse().slice(0, 8).map((r) => h("li", null,
    pill(r.passed ? "ok" : "fail", r.passed ? "pass" : "fail"), h("code", null, r.command.join(" ")), h("span", { class: "spacer" }), h("span", { class: "muted small" }, ago(r.started_at))))));

  // Checkpoints
  body.append(h("h3", null, "Checkpoints"));
  if (!d.checkpoints.length) body.append(h("div", { class: "muted small" }, "None yet."));
  else body.append(h("ul", { class: "list" }, d.checkpoints.map((c) => h("li", { style: "flex-direction:column;align-items:stretch;gap:2px" },
    h("div", { class: "row" }, h("strong", null, c.trigger.replace(/_/g, " ")), h("span", { class: "spacer" }), h("span", { class: "muted small" }, ago(c.created_at))),
    c.summary ? h("div", null, c.summary) : null,
    c.next_action ? h("div", { class: "small" }, "Next: ", c.next_action) : null,
    h("div", { class: "muted small" }, `${c.git.branch || "detached"} @ ${short(c.git.head, 8)} · ${c.git.files.length} changed file(s)`)))));

  // Comments
  body.append(h("h3", null, "Notes"));
  for (const [author, text, at] of d.comments) body.append(h("div", { class: "small", style: "margin-bottom:6px" }, h("strong", null, author), ` · ${ago(at)}`, h("div", { style: "white-space:pre-wrap" }, text)));
  const note = h("textarea", { placeholder: "Add a note for the next agent or for yourself" });
  body.append(note, h("div", { class: "row", style: "margin-top:6px" }, h("button", {
    class: "btn small", onclick: async () => {
      if (!note.value.trim()) return;
      await act(() => invoke("add_comment", { projectId: state.project.id, task: t.key, body: note.value }), "Note added");
      refresh();
    },
  }, "Add note")));
}

function overrideDialog(key, reason, done) {
  const why = h("textarea", { placeholder: "Why this task is verified without the usual evidence" });
  modal("Verify with a human override?", [
    h("div", { class: "banner warn" }, reason),
    field("Reason (logged in the task history)", why),
  ], [{
    label: "Override and verify", primary: true, run: async () => {
      if (!why.value.trim()) { toast("A reason is required", true); return false; }
      await act(() => invoke("set_task_status", { projectId: state.project.id, task: key, status: "verified", overrideReason: why.value.trim() }), `${key} verified with override`);
      done();
    },
  }]);
}

async function runVerify(key, done) {
  toast("Running tests…");
  try {
    const r = await invoke("run_verification", { projectId: state.project.id, task: key, kind: "test", index: 0 });
    const ok = r.receipt.passed;
    modal(ok ? "Tests passed" : "Tests failed", [
      h("div", { class: "banner " + (ok ? "ok" : "bad") }, `${r.receipt.command.join(" ")} exited ${r.receipt.exit_code ?? "by signal"} in ${(r.receipt.duration_ms / 1000).toFixed(1)}s`),
      h("pre", null, r.output_tail || "(no output)"),
    ], []);
    done && done();
  } catch (e) {
    toast(errMsg(e), true);
    if (e && /no test command/.test(errMsg(e))) setTab("settings");
  }
}

// ---------- handoffs ----------
async function handoffDialog(taskKey, preferred, source = null) {
  const projectId = state.project.id;
  const to = await targetSelect(preferred || "codex");
  const checkpoint = source && await act(() => invoke("handoff_checkpoint", { projectId, task: taskKey }));
  if (state.project.id !== projectId) return;
  if (source && to.value === source) {
    const other = [...to.options].find((o) => o.value !== source && o.parentElement.label === KIND_LABEL.harness && !o.textContent.includes("needs setup"));
    if (other) to.value = other.value;
  }
  const summary = h("textarea", { placeholder: "What happened so far (optional; Git state is captured automatically)" });
  const done = h("textarea", { placeholder: "Completed items, one per line" });
  const remaining = h("textarea", { placeholder: "Remaining items, one per line" });
  const next = h("input", { type: "text", placeholder: "The very next step for the receiving AI" });
  if (checkpoint) {
    summary.value = (checkpoint.summary || "").slice(0, 800);
    done.value = (checkpoint.completed || []).slice(-5).map((line) => line.slice(0, 240)).join("\n");
    remaining.value = (checkpoint.remaining || []).slice(0, 5).map((line) => line.slice(0, 240)).join("\n");
    next.value = (checkpoint.next_action || "").slice(0, 500);
  }
  modal(source ? `Continue ${taskKey} with another AI` : `Hand off ${taskKey || "work"}`, [
    source ? h("p", { class: "hint" }, checkpoint
      ? "Review these notes from the task’s latest checkpoint before transferring ownership."
      : "This task has no checkpoint yet. Add a short summary and next action before transferring ownership.") : null,
    field("Continue with", to, "Any AI: a coding agent, an API key, an AI IDE or a chat app. Add more under AI targets."),
    field("Summary", summary),
    field("Completed", done),
    field("Remaining", remaining),
    field("Next action", next),
    h("div", { class: "hint" }, "Evyro writes a signed continuation contract. Open the handoff afterwards to start the receiving AI."),
  ], [{
    label: "Prepare handoff", primary: true, run: async () => {
      const r = await act(() => invoke("prepare_handoff", {
        projectId: state.project.id,
        input: { task: taskKey || null, to: to.value, summary: summary.value, completed: lines(done.value), remaining: lines(remaining.value), next_action: next.value },
      }), "Handoff prepared");
      openHandoff(r.record.id);
    },
  }]);
}

async function viewHandoffs(v) {
  const pid = state.project.id;
  v.append(h("div", { class: "row", style: "margin-bottom:14px" }, h("button", { class: "btn primary", onclick: () => handoffDialog(null) }, "+ New handoff")));
  let hs;
  try { hs = await invoke("list_handoffs", { projectId: pid }); } catch (e) { v.append(h("div", { class: "banner bad" }, errMsg(e))); return; }
  if (!hs.length) { v.append(h("div", { class: "card" }, h("div", { class: "empty" }, "No handoffs yet. Prepare one from a task when you want another agent to continue."))); return; }
  v.append(h("div", { class: "card" }, h("table", null,
    h("thead", null, h("tr", null, ["Created", "From", "To", "Quality", "Status", "Accepted"].map((x) => h("th", null, x)))),
    h("tbody", null, hs.map((x) => h("tr", { class: "clickable", ...opens(() => openHandoff(x.id)) },
      h("td", null, ago(x.created_at)), h("td", null, x.from_harness || "human"), h("td", null, x.to_harness),
      h("td", null, `${x.quality.met}/${x.quality.total}`), h("td", null, pill(x.status)), h("td", null, x.accepted_at ? ago(x.accepted_at) : "-")))))));
}

async function openHandoff(id) {
  let d;
  try { d = await invoke("handoff_detail", { handoffId: id }); } catch (e) { toast(errMsg(e), true); return; }
  const rec = d.record;
  const agent = await targetSelect(rec.to_harness);
  const result = h("div");
  const nodes = [
    h("div", { class: "row" }, pill(rec.status), h("span", null, `${rec.from_harness || "human"} → ${rec.to_harness}`), h("span", { class: "spacer" }),
      d.integrity_ok ? pill("ok", "signature valid") : pill("bad", "signature invalid")),
    d.integrity_error ? h("div", { class: "banner bad", style: "margin-top:8px" }, d.integrity_error) : null,
    h("h3", null, "Quality " + `${rec.quality.met}/${rec.quality.total}`),
    h("ul", { class: "list" }, rec.quality.criteria.map((c) => h("li", null, pill(c.met ? "ok" : "warn", c.met ? "yes" : "no"), h("span", null, c.name), h("span", { class: "muted small" }, c.detail)))),
    h("h3", null, "Contract"),
    h("pre", null, d.markdown),
    h("div", { class: "row", style: "margin-top:10px" },
      h("button", { class: "btn small", onclick: () => copy(d.resume_prompt, "Resume prompt copied") }, "Copy resume prompt"),
      h("button", { class: "btn small", onclick: () => copy(`evyro run --to ${agent.value}`, "Command copied") }, "Copy launch command")),
    result,
  ];
  const actions = rec.status === "prepared" ? [{
    label: "Start with this AI", primary: true, run: () => handTo(agent.value, d.contract?.task?.key || null, result, false, rec.id),
  }, {
    label: "Accept only", run: async () => {
      const r = await act(() => invoke("resume_handoff", { projectId: state.project.id, handoffId: rec.id, agent: agent.value, acceptDrift: false }));
      if (r.accepted) { toast(`Accepted for ${agent.value}. Start it yourself with the copied prompt.`); copy(d.resume_prompt, "Resume prompt copied"); render(); return true; }
      showDrift(result, r, async () => {
        const r2 = await act(() => invoke("resume_handoff", { projectId: state.project.id, handoffId: rec.id, agent: agent.value, acceptDrift: true }), `Accepted for ${agent.value} with drift`);
        return r2.accepted;
      });
      return false;
    },
  }] : [];
  if (rec.status === "prepared") nodes.splice(4, 0, field("Continue with", agent,
    "Coding agents open in the workspace with an editable continuation draft. IDEs and chat apps receive the continuation on your clipboard."));
  modal("Handoff " + short(rec.id, 14), nodes, actions);
}

/** Show what changed since the handoff, with an "accept anyway" button. */
function showDrift(el, r, acceptAnyway) {
  const d = r.drift;
  clear(el).append(h("div", { class: "banner warn", style: "margin-top:10px;flex-direction:column;align-items:flex-start" },
    h("strong", null, "The repository changed since this handoff was prepared."),
    h("div", null, `HEAD: ${d.head_relation}`),
    d.modified_since.length ? h("div", null, "Modified: " + d.modified_since.join(", ")) : null,
    d.newly_dirty.length ? h("div", null, "Newly changed: " + d.newly_dirty.join(", ")) : null,
    h("button", {
      class: "btn small", onclick: async () => {
        if ((await acceptAnyway()) !== false) { clear($("#modal-root")); render(); }
      },
    }, "Accept anyway")));
}

// ---------- sessions ----------
async function viewSessions(v) {
  const pid = state.project.id;
  v.append(h("div", { class: "row", style: "margin-bottom:14px" },
    h("button", {
      class: "btn", onclick: async () => {
        const r = await act(() => invoke("recover_sessions", { projectId: pid, includeUnknown: true }));
        toast(r.length ? `Recovered ${r.length} session(s)` : "No stale sessions");
        render();
      },
    }, "Recover stale sessions"),
    h("span", { class: "muted small" }, "Closes sessions whose agent is no longer running, saving a recovery checkpoint.")));
  let ss;
  try { ss = await invoke("list_sessions", { projectId: pid, activeOnly: false }); } catch (e) { v.append(h("div", { class: "banner bad" }, errMsg(e))); return; }
  if (!ss.length) { v.append(h("div", { class: "card" }, h("div", { class: "empty" }, "No sessions yet."))); return; }
  v.append(h("div", { class: "card" }, h("table", null,
    h("thead", null, h("tr", null, ["Agent", "Status", "Started", "Ended", "Reason", "Session"].map((x) => h("th", null, x)))),
    h("tbody", null, ss.map((s) => h("tr", null,
      h("td", null, s.harness, s.model ? h("div", { class: "muted small" }, s.model) : null), h("td", null, pill(s.status)),
      h("td", null, ago(s.started_at)), h("td", null, s.ended_at ? ago(s.ended_at) : "-"), h("td", { class: "small" }, s.exit_reason || "-"),
      h("td", { class: "mono" }, short(s.id, 16))))))));
}

// ---------- decisions & memory ----------
async function viewKnowledge(v) {
  const pid = state.project.id;
  const grid = h("div", { class: "grid" });
  v.append(grid);
  const dCard = h("div", { class: "card" });
  const mCard = h("div", { class: "card" });
  grid.append(dCard, mCard);

  const drawDecisions = async () => {
    const ds = await invoke("list_decisions", { projectId: pid }).catch((e) => { toast(errMsg(e), true); return []; });
    clear(dCard).append(h("div", { class: "card-head" }, h("h2", null, "Decisions"), h("button", { class: "btn small", onclick: decisionDialog }, "+ Record")),
      ds.length ? h("ul", { class: "list" }, ds.slice().reverse().map((d) => h("li", { style: "flex-direction:column;align-items:stretch;gap:2px" },
        h("div", { class: "row" }, h("strong", null, d.title), h("span", { class: "spacer" }), h("span", { class: "muted small" }, `${d.source} · ${ago(d.created_at)}`)),
        h("div", null, d.decision), d.rationale ? h("div", { class: "muted small" }, "Why: " + d.rationale) : null))) : h("div", { class: "empty" }, "No decisions recorded."));
  };
  const decisionDialog = () => {
    const title = h("input", { type: "text", placeholder: "Storage engine" });
    const decision = h("input", { type: "text", placeholder: "Use SQLite with WAL" });
    const why = h("textarea", { placeholder: "Rationale" });
    modal("Record a decision", [field("Title", title), field("Decision", decision), field("Why", why)], [{
      label: "Record", primary: true, run: async () => {
        if (!title.value.trim() || !decision.value.trim()) { toast("Title and decision are required", true); return false; }
        await act(() => invoke("add_decision", { projectId: pid, title: title.value.trim(), decision: decision.value.trim(), rationale: why.value.trim(), task: null }), "Decision recorded");
        drawDecisions();
      },
    }]);
  };

  const q = h("input", { type: "text", placeholder: "Search memory" });
  const mList = h("div");
  const drawMemory = async () => {
    const ms = await invoke("search_memory", { projectId: pid, query: q.value }).catch((e) => { toast(errMsg(e), true); return []; });
    clear(mList).append(ms.length ? h("ul", { class: "list" }, ms.map((m) => h("li", { style: "flex-direction:column;align-items:stretch;gap:2px" },
      h("div", { class: "row" }, h("span", { class: "pill accent" }, m.category), m.verification !== "verified" ? h("span", { class: "pill" }, m.verification) : null, h("span", { class: "spacer" }), h("span", { class: "muted small" }, `${m.source} · ${ago(m.created_at)}`)),
      h("div", null, m.content)))) : h("div", { class: "empty" }, q.value ? "No matches." : "No memory saved."));
  };
  q.addEventListener("input", () => { clearTimeout(q._t); q._t = setTimeout(drawMemory, 250); });
  const memoryDialog = async () => {
    const cats = await invoke("memory_categories").catch(() => ["note"]);
    const cat = h("select", null, cats.map((c) => h("option", { value: c, selected: c === "note" }, c)));
    const content = h("textarea", { placeholder: "Something every agent working here should know" });
    modal("Save to project memory", [field("Category", cat), field("Content", content, "Secrets are redacted before saving.")], [{
      label: "Save", primary: true, run: async () => {
        if (!content.value.trim()) { toast("Write something to save", true); return false; }
        await act(() => invoke("add_memory", { projectId: pid, category: cat.value, content: content.value.trim() }), "Saved");
        drawMemory();
      },
    }]);
  };
  mCard.append(h("div", { class: "card-head" }, h("h2", null, "Project memory"), h("button", { class: "btn small", onclick: memoryDialog }, "+ Add")), q, h("div", { style: "height:8px" }), mList);
  await Promise.all([drawDecisions(), drawMemory()]);
}

// ---------- AI targets ----------
async function viewAgents(v) {
  v.append(h("div", { class: "muted", style: "margin-bottom:12px" },
    "Every AI you can switch to: coding agents (signed in or with an API key), API keys through an agent, AI IDEs and chat apps. Evyro stores which environment variable or Keychain item holds a key, never the key itself."));
  const tcard = h("div", { class: "card" }, h("div", { class: "muted" }, "Checking AI targets…"));
  const dcard = h("div", { class: "card", style: "margin-top:16px" }, h("div", { class: "muted" }, "Checking installed agents…"));
  v.append(tcard, dcard);

  let ts;
  try { ts = await loadTargets(true); } catch (e) { clear(tcard).append(h("div", { class: "banner bad" }, errMsg(e))); ts = null; }
  if (ts) {
    const groups = ["harness", "api", "ide", "chat"].map((k) => [k, ts.filter((t) => t.target.kind === k)]).filter(([, g]) => g.length);
    clear(tcard).append(
      h("div", { class: "card-head" }, h("h2", null, "AI targets"),
        h("div", { class: "row" },
          h("button", { class: "btn small", onclick: () => render() }, "Re-check"),
          h("button", { class: "btn small primary", onclick: () => addTargetDialog() }, "+ Add target"))),
      ...groups.flatMap(([k, g]) => [
        h("h3", null, KIND_LABEL[k]),
        h("table", null,
          h("thead", null, h("tr", null, ["Target", "Status", "Uses", "Next step", ""].map((x) => h("th", null, x)))),
          h("tbody", null, g.map((t) => h("tr", { class: "clickable", ...opens(() => targetDialog(t)) },
            h("td", null, h("strong", { class: "mono" }, t.target.name), t.target.description ? h("div", { class: "muted small" }, t.target.description) : null),
            h("td", null, targetPill(t.state)),
            h("td", { class: "small" }, t.display),
            h("td", { class: "small" }, t.next_step || "-"),
            h("td", null, state.project ? h("button", {
              class: "btn small", style: "white-space:nowrap", onclick: (e) => { e.stopPropagation(); handTo(t.target.name, null, h("div")); },
            }, "Switch to") : null))))),
      ]));
  }

  let ds;
  try { ds = state.agents || (state.agents = await invoke("detect_agents")); } catch (e) { clear(dcard).append(h("div", { class: "banner bad" }, errMsg(e))); return; }
  clear(dcard).append(h("div", { class: "card-head" }, h("h2", null, "Coding agents on this computer"), h("button", { class: "btn small", onclick: () => { state.agents = null; render(); } }, "Re-check")),
    h("table", null,
      h("thead", null, h("tr", null, ["Agent", "Status", "Version", "Can do", "Instruction file"].map((x) => h("th", null, x)))),
      h("tbody", null, ds.map((d) => h("tr", null,
        h("td", null, h("strong", null, d.display_name), h("div", { class: "muted small mono" }, d.id)),
        h("td", null, pill(d.compatibility), d.diagnostic ? h("div", { class: "muted small" }, d.diagnostic) : null),
        h("td", { class: "mono" }, d.version || "-"),
        h("td", { class: "small" }, Object.entries(d.capabilities).filter(([, on]) => on === true).map(([k]) => k.replace(/_/g, " ")).join(", ") || "-"),
        h("td", { class: "mono small" }, d.instruction_file))))));
}

function targetDialog(t) {
  const x = t.target;
  const rows = [["Kind", KIND_LABEL[x.kind]], ["Uses", t.display], ["Sign-in", x.auth.replace(/_/g, " ")],
    ["Provider", x.provider], ["Model", x.model], ["Key variable", x.key_env], ["Keychain item", x.key_keychain], ["Base URL", x.base_url]]
    .filter(([, val]) => val);
  const actions = [];
  if (state.project) actions.push({ label: "Switch to this AI", primary: true, run: () => handTo(x.name, null, h("div")) });
  if (!t.builtin) actions.push({
    label: "Remove", run: async () => {
      if (!confirmInline.has(x.name)) { confirmInline.add(x.name); toast(`Press Remove again to delete target ${x.name}.`); return false; }
      confirmInline.delete(x.name);
      await act(() => invoke("remove_target", { name: x.name }), `Removed ${x.name}`);
      state.targets = null; render(); return true;
    },
  });
  modal("Target " + x.name, [
    h("div", { class: "row" }, targetPill(t.state)),
    h("table", null, h("tbody", null, rows.map(([k, val]) => h("tr", null, h("td", { class: "muted" }, k), h("td", { class: "mono small" }, val))))),
    h("h3", null, "Checks"),
    h("ul", { class: "list" }, t.checks.map((c) => h("li", null, pill(c.state === "ok" ? "ok" : c.state), h("span", null, c.name), h("span", { class: "muted small" }, c.detail)))),
    t.next_step ? h("div", { class: "banner warn", style: "margin-top:10px" }, t.next_step) : null,
    t.builtin ? h("div", { class: "hint" }, "Built-in target. Add your own to pick a model, provider or key.") : null,
  ], actions);
}
const confirmInline = new Set();

async function addTargetDialog() {
  let opt;
  try { opt = await invoke("target_options"); } catch (e) { toast(errMsg(e), true); return; }
  const name = h("input", { type: "text", placeholder: "e.g. claude-work, gpt5-key, my-cursor" });
  const kind = h("select", null, Object.entries(KIND_LABEL).map(([k, l]) => h("option", { value: k }, l)));
  const harness = h("select", null, opt.harnesses.map((a) => h("option", { value: a.id }, a.display_name)));
  const app = h("select");
  const auth = h("select", null, [["auto", "The tool's own setup"], ["app_login", "Signed-in app account (no API key)"], ["api_key", "API key"], ["local", "Local model (Ollama, LM Studio)"]]
    .map(([k, l]) => h("option", { value: k }, l)));
  const provider = h("select", null, h("option", { value: "" }, "(tool default)"), opt.providers.map((p) => h("option", { value: p.id }, p.display_name)));
  const model = h("input", { type: "text", placeholder: "Model id, optional" });
  const keyEnv = h("input", { type: "text", placeholder: "Variable name, e.g. OPENROUTER_API_KEY (not the key)" });
  const keychain = h("input", { type: "text", placeholder: "Keychain service name, optional (macOS)" });
  const baseUrl = h("input", { type: "text", placeholder: "https://… (only for compatible or self-hosted endpoints)" });
  const desc = h("input", { type: "text", placeholder: "Optional note" });
  const fHarness = field("Coding agent", harness, "The CLI that does the work. API-key targets default to OpenCode, which supports most providers.");
  const fApp = field("App", app);
  const fAuth = field("Sign-in", auth);
  const fProvider = field("Provider", provider);
  const fModel = field("Model", model);
  const fKey = field("API key from", keyEnv, "Put the key in your shell profile or the Keychain. Evyro reads it only when it starts the agent, and never saves it.");
  const fKeychain = field("Or from the Keychain", keychain, "Store it with: security add-generic-password -s <name> -a evyro -w");
  const fBase = field("Base URL", baseUrl);
  const sync = () => {
    const k = kind.value;
    const show = (el, on) => { el.style.display = on ? "" : "none"; };
    if (k === "ide" || k === "chat") {
      const list = k === "ide" ? opt.ides : opt.chat_apps;
      clear(app).append(...list.map((a) => h("option", { value: a.id }, a.display_name)));
    }
    if (k === "api") { auth.value = provider.value && opt.providers.find((p) => p.id === provider.value)?.local ? "local" : "api_key"; }
    const agentKind = k === "harness" || k === "api";
    const needsKey = agentKind && auth.value === "api_key";
    show(fHarness, agentKind); show(fApp, !agentKind); show(fAuth, agentKind); show(fProvider, agentKind);
    show(fModel, agentKind); show(fKey, needsKey); show(fKeychain, needsKey); show(fBase, agentKind && auth.value !== "app_login");
    const p = opt.providers.find((x) => x.id === provider.value);
    model.placeholder = p ? `e.g. ${p.example_model}` : "Model id, optional";
    keyEnv.placeholder = p && p.key_env ? `e.g. ${p.key_env} (the variable name, not the key)` : "Variable name (not the key)";
  };
  [kind, auth, provider, harness].forEach((el) => el.addEventListener("change", sync));
  sync();
  modal("Add an AI target", [
    field("Name", name), field("Kind", kind), fHarness, fApp, fAuth, fProvider, fModel, fKey, fKeychain, fBase, field("Note", desc),
  ], [{
    label: "Add target", primary: true, run: async () => {
      const k = kind.value;
      const agentKind = k === "harness" || k === "api";
      const blank = (x) => (x.trim() ? x.trim() : null);
      const target = {
        name: name.value.trim(), kind: k,
        harness: agentKind ? harness.value : null, app: agentKind ? null : app.value,
        auth: agentKind ? auth.value : "auto",
        provider: agentKind ? (provider.value || null) : null, model: agentKind ? blank(model.value) : null,
        key_env: agentKind && auth.value === "api_key" ? blank(keyEnv.value) : null,
        key_keychain: agentKind && auth.value === "api_key" ? blank(keychain.value) : null,
        base_url: agentKind ? blank(baseUrl.value) : null,
        description: desc.value.trim(), builtin: false,
      };
      await act(() => invoke("add_target", { target }), `Added ${target.name}`);
      state.targets = null; render(); return true;
    },
  }]);
}

// ---------- settings ----------
/** Limits on how many ended threads and chats stay saved, and a way to delete them now. */
function historyCard() {
  const card = h("div", { class: "card" }, h("h2", null, "Saved history"), h("div", { class: "muted" }, "Loading…"));
  const forgetGone = async () => {
    await loadThreads();
    for (const [id, x] of [...XTERMS]) {
      if (!state.threads.some((t) => t.id === id)) { x.term.dispose(); XTERMS.delete(id); }
    }
    if (state.project) {
      const slots = panes();
      slots.forEach((id, i) => { if (id && !state.threads.some((t) => t.id === id)) slots[i] = null; });
    }
    if (state.thread && !state.threads.some((t) => t.id === state.thread)) state.thread = null;
  };
  const draw = async () => {
    let st;
    try { st = await invoke("history_status_cmd"); }
    catch (e) { clear(card).append(h("h2", null, "Saved history"), h("div", { class: "banner bad" }, errMsg(e))); return; }
    const number = (id, value) => h("input", { type: "number", id, min: "0", max: String(st.max), step: "1", value: String(value) });
    const threads = number("keep-threads", st.keep.threads);
    const chats = number("keep-chats", st.keep.chats);
    const save = async () => {
      const t = Number(threads.value), c = Number(chats.value);
      const ok = (n) => Number.isInteger(n) && n >= 0 && n <= st.max;
      if (!ok(t) || !ok(c)) { toast(`Enter whole numbers from 0 to ${st.max}.`, true); return; }
      const r = await act(() => invoke("history_set", { threadsKeep: t, chatsKeep: c }));
      const gone = r.removed.threads + r.removed.chats;
      toast(gone ? `Saved. Deleted ${plural(gone, "older item", "older items")}.` : "Saved.");
      await forgetGone();
      draw();
    };
    const clearBtn = (what, label, count) => h("button", { class: "btn small danger", disabled: !count, onclick: async () => {
      const noun = what === "threads" ? "thread" : "chat";
      if (!(await confirmDialog(`Delete ${plural(count, "saved " + noun, "saved " + noun + "s")}?`,
        `Evyro permanently deletes the saved ${noun} output from this computer. ${what === "threads" ? "Threads that are still running are not affected." : "Chats that are still running keep going."}`, "Delete"))) return;
      const r = await act(() => invoke("history_clear", { what }));
      toast(`Deleted ${plural(r.removed[what], "saved " + noun, "saved " + noun + "s")}.`);
      await forgetGone();
      draw();
    } }, label);
    clear(card).append(
      h("h2", null, "Saved history"),
      h("div", { class: "muted small", style: "margin-bottom:10px" }, "Ended threads and chats are saved on this computer so you can reopen them. When you go over a limit the oldest are deleted. Use 0 to keep none."),
      field("Saved threads to keep", threads, `${st.saved.threads} saved now`),
      field("Saved chats to keep", chats, `${st.saved.chats} saved now`),
      h("div", { class: "row" }, h("button", { class: "btn primary", onclick: save }, "Save limits")),
      h("div", { class: "row", style: "margin-top:12px" },
        clearBtn("threads", "Clear saved threads", st.saved.threads),
        clearBtn("chats", "Clear saved chats", st.saved.chats)));
  };
  draw();
  return card;
}

async function viewSettings(v) {
  const grid = h("div", { class: "grid" });
  v.append(grid);
  if (state.project) {
    const p = state.project;
    const toText = (list) => list.map((argv) => argv.join(" ")).join("\n");
    // Each line is a command; words split on spaces, "quoted words" stay together. No shell.
    const parse = (text) => lines(text).map((l) => (l.match(/"[^"]*"|\S+/g) || []).map((w) => w.replace(/^"|"$/g, "")));
    const test = h("textarea", { value: toText(p.commands.test), placeholder: "cargo test\nnpm test" });
    const lint = h("textarea", { value: toText(p.commands.lint), placeholder: "cargo clippy -- -D warnings" });
    const build = h("textarea", { value: toText(p.commands.build), placeholder: "cargo build" });
    grid.append(h("div", { class: "card" },
      h("h2", null, "Verification commands"),
      h("div", { class: "muted small", style: "margin-bottom:10px" }, "Saved to .evyro/project.toml. One command per line. Commands run directly, without a shell."),
      field("Test", test), field("Lint", lint), field("Build", build),
      h("div", { class: "row" }, h("button", {
        class: "btn primary", onclick: async () => {
          const updated = await act(() => invoke("save_commands", { projectId: p.id, commands: { test: parse(test.value), lint: parse(lint.value), build: parse(build.value) } }), "Commands saved");
          state.project = updated;
          const i = state.projects.findIndex((x) => x.id === updated.id);
          if (i >= 0) state.projects[i] = updated;
        },
      }, "Save"))));
    grid.append(h("div", { class: "card" },
      h("h2", null, "This project"),
      h("dl", { class: "kv" }, h("dt", null, "Name"), h("dd", null, p.name), h("dt", null, "Folder"), h("dd", { class: "mono" }, p.root_path),
        h("dt", null, "Main branch"), h("dd", null, p.main_branch || "-"), h("dt", null, "Project id"), h("dd", { class: "mono" }, p.id)),
      h("div", { class: "row", style: "margin-top:12px" }, h("button", {
        class: "btn danger small", onclick: () => modal(`Archive ${p.name}?`, [h("p", null, "The project is hidden from the list. Its tasks and history stay in the database, and you can register the folder again later.")], [{
          label: "Archive", primary: true, run: async () => {
            await act(() => invoke("set_project_archived", { projectId: p.id, archived: true }), "Archived");
            state.project = null;
            loadProjects();
          },
        }]),
      }, "Archive project"))));
  }
  const info = h("div", { class: "card" }, h("h2", null, "Evyro"), h("div", { class: "muted" }, "Loading…"));
  const doc = h("div", { class: "card wide" }, h("h2", null, "Doctor"), h("div", { class: "muted" }, "Running checks…"));
  grid.prepend(appearanceCard());
  grid.append(info, updatesCard(), historyCard(), doc, remoteCard());
  invoke("app_info").then((a) => {
    clear(info).append(h("h2", null, "Evyro"), h("dl", { class: "kv" },
      h("dt", null, "Version"), h("dd", null, a.version), h("dt", null, "Data folder"), h("dd", { class: "mono" }, a.data_dir),
      h("dt", null, "Schema"), h("dd", null, "v" + a.schema_version), h("dt", null, "Git"), h("dd", null, a.git || "not found")),
      h("div", { class: "row", style: "margin-top:12px" }, h("button", {
        class: "btn small", onclick: async () => { const r = await act(() => invoke("backup_now")); toast("Backup saved to " + r.path); },
      }, "Back up database")));
  }).catch((e) => clear(info).append(h("div", { class: "banner bad" }, errMsg(e))));
  invoke("run_doctor", { projectId: state.project ? state.project.id : null }).then((cs) => {
    clear(doc).append(h("h2", null, "Doctor"), h("ul", { class: "list" }, cs.map((c) => h("li", null, pill(c.status), h("strong", null, c.name), h("span", null, c.detail),
      c.remediation ? h("span", { class: "muted small" }, "· " + c.remediation) : null))));
  }).catch((e) => clear(doc).append(h("div", { class: "banner bad" }, errMsg(e))));
}

// ---------- changes ----------
// The Changes view and review comments live in review.js.

// ---------- live updates ----------
const LIVE_VIEWS = new Set(["tasks", "history", "changes"]);
let liveTimer = null;
/** Refresh the current screen when something changed elsewhere (CLI, agent, MCP), unless the person is busy in a form. */
function onLiveEvents(evs) {
  const mine = state.project && evs.some((e) => e.project_id === state.project.id);
  if (mine) { updateLiveBadge(); drawContext(); }
  // The workspace keeps its terminals and chats; only its overview (shown with no threads) refreshes.
  if (mine && state.view === "workspace" && !projThreads().length) { clearTimeout(liveTimer); liveTimer = setTimeout(() => render(), 300); return; }
  if (!mine || !LIVE_VIEWS.has(state.view)) return;
  const busy = $("#modal-root").firstChild || $("#palette-root").firstChild || state.drawerTask
    || (document.activeElement && /INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName));
  if (busy) return;
  clearTimeout(liveTimer);
  const view = state.view;
  const projectId = state.project.id;
  liveTimer = setTimeout(() => {
    if (state.view === view && state.project?.id === projectId && !document.activeElement?.closest(".board")) render();
  }, 300);
}
async function updateLiveBadge() {
  const b = $("#live-badge");
  if (!state.project) { b.textContent = ""; b.className = "live-badge"; return; }
  try {
    const ss = await invoke("list_sessions", { projectId: state.project.id, activeOnly: true });
    const running = ss.filter((x) => x.status === "running");
    b.className = "live-badge" + (running.length ? " on" : "");
    b.textContent = running.length ? `With ${[...new Set(running.map((x) => x.harness))].join(", ")}` : "No AI working";
    b.title = running.length ? "Sessions in progress for this project" : "";
  } catch { b.textContent = ""; }
}

// ---------- command palette ----------
const MOD = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl";
/** Every keyboard shortcut, in one place: the sheet shows this list and the smoke test presses each one. */
const shortcutList = () => [
  [t("keys.group.everywhere"), [
    [t("keys.palette"), `${MOD} K`],
    [t("keys.screens"), `${MOD} 1 … 8`],
    [t("keys.file"), `${MOD} P`],
    [t("keys.text"), `${MOD} ⇧ F`],
    [t("keys.symbol"), `${MOD} ⇧ O`],
    [t("keys.newTask"), `${MOD} N`],
    [t("keys.context"), `${MOD} .`],
    [t("keys.drawer"), "Ctrl `"],
    [t("keys.sheet"), "?"],
    [t("keys.escape"), "Esc"],
  ]],
  [t("keys.group.dialog"), [
    [t("keys.tab"), "Tab / ⇧ Tab"],
    [t("keys.enter.dialog"), "Enter"],
  ]],
  [t("keys.group.palette"), [
    [t("keys.arrows"), "↑ ↓"],
    [t("keys.enter.palette"), "Enter"],
  ]],
  [t("keys.group.threads"), [
    [t("keys.f6"), "F6"],
    [t("keys.send"), "Enter"],
    [t("keys.newline"), "⇧ Enter"],
  ]],
];

function shortcutSheet() {
  const [before, after] = t("keys.intro", { keys: "\u0000" }).split("\u0000");
  modal(t("keys.title"), [
    h("p", { class: "muted small" }, before, h("kbd", null, `${MOD} K`), after),
    ...shortcutList().map(([group, rows]) => h("section", { "aria-label": group },
      h("h3", { class: "ks-head" }, group),
      h("dl", { class: "ks" }, rows.flatMap(([what, keys]) => [h("dt", null, what), h("dd", null, h("kbd", null, keys))])))),
  ], []);
}
const tabList = () => [["workspace", t("tab.workspace")], ["tasks", t("tab.tasks")], ["changes", t("tab.changes")], ["history", t("tab.history")], ["knowledge", t("tab.knowledge.long")], ["skills", t("tab.skills")], ["agents", t("tab.agents")], ["settings", t("tab.settings")]];
async function paletteActions() {
  const acts = tabList().map(([v, l], i) => ({ label: t("palette.goto", { screen: l }), hint: i < 9 ? `${MOD} ${i + 1}` : "", run: () => setTab(v) }));
  acts.push({ label: t("keys.title"), hint: "?", run: shortcutSheet });
  acts.push({ label: "Add a project", run: addProjectDialog });
  acts.push({ label: "Try the sample project", run: trySample });
  if (isSample(state.project)) acts.push({ label: "Show the guided tour", run: () => { setTourDismissed(false); setTab("tasks"); } });
  if (state.project) {
    acts.push({ label: "New task", hint: `${MOD} N`, run: () => { setTab("tasks"); newTaskDialog(); } });
    acts.push({ label: "New handoff", run: () => handoffDialog(null) });
    acts.push({ label: "New agent thread", run: () => { setTab("threads"); newThreadDialog(); } });
    acts.push({ label: "New shell thread", run: () => startThread({ kind: "shell" }) });
    acts.push({ label: "Go to file", hint: `${MOD} P`, run: () => openSearch("files") });
    acts.push({ label: "Search in code", hint: `${MOD} ⇧ F`, run: () => openSearch("text") });
    acts.push({ label: "Go to symbol", hint: `${MOD} ⇧ O`, run: () => openSearch("symbols") });
    acts.push({ label: "Search saved chats", run: () => openSearch("chats") });
    acts.push({ label: "Work on the next ready task", run: () => workOnNext() });
    acts.push({ label: "New chat with an AI (ACP)", run: () => { setTab("threads"); newChatDialog(); } });
    acts.push({ label: "Copy context for an agent", run: () => copyContext(null) });
    acts.push({ label: "Layout: one pane", run: () => { setTab("workspace"); setLayout(1); } });
    acts.push({ label: "Layout: two panes side by side", run: () => { setTab("workspace"); setLayout(2); } });
    acts.push({ label: "Layout: grid of four panes", run: () => { setTab("workspace"); setLayout(4); } });
    acts.push({ label: state.ctxOpen ? "Hide shared context" : "Show shared context", hint: `${MOD} .`, run: () => setCtxOpen(!state.ctxOpen) });
    acts.push({ label: "Toggle terminal drawer", hint: "Ctrl `", run: () => toggleTermDrawer() });
    acts.push({ label: state.focusMode ? "Leave focus mode" : "Focus on one pane", run: () => { setTab("workspace"); toggleFocusMode(); } });
    acts.push({ label: "Next pane", run: () => { setTab("workspace"); const n = Math.max(1, panes().length); focusPane(((state.focus ?? -1) + 1) % n, true); } });
    acts.push({ label: "Previous pane", run: () => { setTab("workspace"); const n = Math.max(1, panes().length); focusPane(((state.focus ?? 0) - 1 + n) % n, true); } });
    const live = (state.threads || []).find((x) => x.id === state.thread && x.running && x.kind !== "shell");
    if (live) {
      acts.push({ label: `Send the shared context to ${live.title}`, run: () => sendContext(live) });
      acts.push({ label: `Stop ${live.title}`, run: () => closeThread(live.id) });
    }
    acts.push({ label: "Recover sessions whose AI has gone", run: async () => { try { const r = await act(() => invoke("recover_sessions", { projectId: state.project.id, includeUnknown: false })); toast(r.length ? `Recovered ${r.length} session(s).` : "No stale sessions."); } catch { /* toast shown */ } } });
    acts.push({ label: "Settings: appearance, themes and motion", run: () => setTab("settings") });
    try {
      const rows = await invoke("list_tasks", { projectId: state.project.id });
      const active = rows.find((r) => r.task.status === "in_progress") || null;
      if (active) {
        const t = active.task;
        acts.push({ label: `Run tests for ${t.key}`, run: () => runVerify(t.key, () => render()) });
        acts.push({ label: `Hand off ${t.key}`, run: () => handoffDialog(t.key) });
        acts.push({ label: `Switch ${t.key} to another AI…`, run: () => switchOwnerDialog(t.key, {}) });
        acts.push({ label: `Compare agents on ${t.key}`, run: () => openComparison(t) });
        acts.push({ label: `PR feedback for ${t.key}`, run: () => prFeedbackDialog(t) });
      }
      for (const r of rows.slice(0, 40)) acts.push({ label: `Open task ${r.task.key} ${r.task.title}`, run: () => { setTab("tasks"); openTask(r.task.key); } });
    } catch { /* tasks unavailable */ }
    acts.push({ label: "Add an AI target", run: () => { setTab("agents"); addTargetDialog(); } });
    try {
      for (const t of await loadTargets()) acts.push({ label: `Switch to ${t.target.name}`, hint: KIND_LABEL[t.target.kind], run: () => handTo(t.target.name, null, h("div")) });
    } catch { /* targets unavailable */ }
  }
  for (const p of state.projects) acts.push({ label: `Open project ${p.alias || p.name}`, run: () => selectProject(p.id) });
  return acts;
}
/** Close the command palette and give focus back to whatever had it. */
function closePalette() {
  const root = $("#palette-root");
  const opener = root._opener;
  clear(root);
  root._opener = null;
  if (opener && opener !== document.body && opener.isConnected) opener.focus();
}
async function openPalette() {
  const root = $("#palette-root");
  if (root.firstChild) { closePalette(); return; }
  root._opener = document.activeElement;
  const acts = await paletteActions();
  const input = h("input", { type: "text", placeholder: "Type a command, a screen, an AI or a project…", "aria-label": "Command" });
  const list = h("ul", { class: "palette-list", role: "listbox" });
  let sel = 0, shown = acts;
  const close = () => closePalette();
  const draw = () => {
    const q = input.value.trim().toLowerCase();
    shown = acts.filter((a) => q.split(/\s+/).every((w) => a.label.toLowerCase().includes(w)));
    sel = Math.min(sel, Math.max(0, shown.length - 1));
    clear(list).append(...shown.slice(0, 40).map((a, i) => h("li", {
      class: i === sel ? "sel" : "", role: "option", onmousedown: (e) => { e.preventDefault(); close(); a.run(); },
    }, h("span", null, a.label), a.hint ? h("span", { class: "muted small" }, a.hint) : null)));
    if (!shown.length) list.append(h("li", { class: "muted" }, "No matching command"));
  };
  input.addEventListener("input", () => { sel = 0; draw(); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { sel = Math.min(sel + 1, shown.length - 1); draw(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { sel = Math.max(sel - 1, 0); draw(); e.preventDefault(); }
    else if (e.key === "Enter" && shown[sel]) { const a = shown[sel]; close(); a.run(); }
    else if (e.key === "Escape") { close(); e.stopPropagation(); }
  });
  root.append(h("div", { class: "modal-back palette-back", onclick: (e) => { if (e.target.classList.contains("palette-back")) close(); } },
    h("div", { class: "palette", role: "dialog", "aria-label": "Command palette" }, input, list)));
  draw();
  input.focus();
}

// ---------- boot ----------
document.addEventListener("DOMContentLoaded", () => {
  I18N.applyStatic();
  for (const b of document.querySelectorAll(".tab")) b.addEventListener("click", () => setTab(b.dataset.view));
  $("#add-project-btn").addEventListener("click", addProjectDialog);
  $("#drawer-close").addEventListener("click", closeDrawer);
  document.addEventListener("keydown", (e) => {
    // Inside a terminal, Ctrl keys belong to the program (Ctrl+K, Ctrl+N...); only ⌘ shortcuts stay global.
    if (e.target.closest && e.target.closest(".xterm") && !e.metaKey) return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === "Escape") { if ($("#palette-root").firstChild) closePalette(); else if ($("#modal-root").firstChild) closeModal(); else closeDrawer(); return; }
    if (mod && e.key.toLowerCase() === "k") { e.preventDefault(); openPalette(); return; }
    if (mod && state.project && (e.key.toLowerCase() === "p" && !e.shiftKey)) { e.preventDefault(); openSearch("files"); return; }
    if (mod && state.project && e.shiftKey && e.key.toLowerCase() === "f") { e.preventDefault(); openSearch("text"); return; }
    if (mod && state.project && e.shiftKey && e.key.toLowerCase() === "o") { e.preventDefault(); openSearch("symbols"); return; }
    if (mod && /^[1-9]$/.test(e.key) && tabList()[Number(e.key) - 1]) { e.preventDefault(); setTab(tabList()[Number(e.key) - 1][0]); return; }
    if (mod && e.key === "." && state.project) { e.preventDefault(); setCtxOpen(!state.ctxOpen); return; }
    if (e.ctrlKey && e.key === "`" && state.project) { e.preventDefault(); toggleTermDrawer(); return; }
    if (mod && e.key.toLowerCase() === "n" && state.project) { e.preventDefault(); setTab("tasks"); newTaskDialog(); return; }
    // "?" opens the shortcut list, except where it is a character being typed.
    const typing = e.target.closest && e.target.closest("input, textarea, select, [contenteditable=true], .xterm");
    if (e.key === "?" && !mod && !e.altKey && !typing && !$("#modal-root").firstChild && !$("#palette-root").firstChild) { e.preventDefault(); shortcutSheet(); }
  });
  $("#palette-btn").addEventListener("click", openPalette);
  $("#palette-btn").textContent = `${MOD} K`;
  $("#search-btn").addEventListener("click", () => openSearch());
  $("#ctx-btn").addEventListener("click", () => setCtxOpen(!state.ctxOpen));
  $("#ctx-close").addEventListener("click", () => setCtxOpen(false));
  for (const b of document.querySelectorAll(".ctx-tab")) b.addEventListener("click", () => setCtxPane(b.dataset.pane));
  $("#search-btn").textContent = `Search  ${MOD} P`;
  if (window.__TAURI__ && window.__TAURI__.event) {
    window.__TAURI__.event.listen("evyro-events", (ev) => onLiveEvents(ev.payload || []));
    window.__TAURI__.event.listen("term-output", (ev) => threadOutput(ev.payload));
    window.__TAURI__.event.listen("term-exit", (ev) => threadExit(ev.payload));
    window.__TAURI__.event.listen("chat-event", (ev) => chatEvent(ev.payload));
    window.__TAURI__.event.listen("chat-state", (ev) => chatState(ev.payload));
  }
  invoke("app_info").then((a) => { $("#side-foot").textContent = `v${a.version} · local only`; }).catch(() => {});
  loadProjects();
});
