// Evyro companion: a phone-sized view of `evyro serve`.
// Pair once with a code, then follow projects, tasks, changes and chats, and
// answer an agent's permission requests. Live updates use the long-poll feed,
// so the token only ever travels in the Authorization header.
"use strict";

const TOKEN_KEY = "evyro.token";
const $ = (s) => document.querySelector(s);
const main = $("#main");
let me = null;
let route = { name: "home" };
const history_ = [];
let feedSince = null;
let feedRunning = false;
const listeners = new Set();

function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.className = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const k of kids.flat(Infinity)) if (k != null && k !== false) el.append(k.nodeType ? k : String(k));
  return el;
}
const clear = (el) => { while (el.firstChild) el.removeChild(el.firstChild); return el; };
function toast(msg, bad) {
  const t = $("#toast");
  t.textContent = msg; t.className = "toast" + (bad ? " bad" : ""); t.hidden = false;
  clearTimeout(t._t); t._t = setTimeout(() => (t.hidden = true), 3500);
}
function ago(iso) {
  if (!iso) return "";
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return "just now";
  if (s < 3600) return Math.floor(s / 60) + " min ago";
  if (s < 86400) return Math.floor(s / 3600) + " h ago";
  return Math.floor(s / 86400) + " d ago";
}
const token = () => { try { return localStorage.getItem(TOKEN_KEY); } catch { return null; } };
const can = (scope) => me && ["read", "approve", "control"].indexOf(me.scope) >= ["read", "approve", "control"].indexOf(scope);

async function api(method, path, body) {
  const headers = { "Content-Type": "application/json" };
  const t = token();
  if (t) headers.Authorization = "Bearer " + t;
  const r = await fetch(path, { method, headers, body: body ? JSON.stringify(body) : undefined, cache: "no-store" });
  const data = r.status === 204 ? null : await r.json().catch(() => null);
  if (r.status === 401 && path !== "/api/v1/pair") {
    try { localStorage.removeItem(TOKEN_KEY); } catch { /* private mode */ }
    me = null;
    go({ name: "pair" }, true);
  }
  if (!r.ok) throw new Error((data && data.error && data.error.message) || "HTTP " + r.status);
  return data;
}

// ---------- navigation ----------
function go(r, replace) {
  if (!replace) history_.push(route);
  route = r;
  render();
}
$("#back").addEventListener("click", () => { route = history_.pop() || { name: "home" }; render(); });
function setTitle(t, back) { $("#title").textContent = t; $("#back").hidden = !back; }

async function render() {
  listeners.clear();
  clear(main);
  if (!token() || route.name === "pair") return viewPair();
  if (!me) {
    try { me = (await api("GET", "/api/v1/me")).device; } catch (e) { main.append(h("div", { class: "empty" }, e.message)); return; }
    startFeed();
  }
  const views = { home: viewHome, project: viewProject, task: viewTask, chat: viewChat, newchat: viewNewChat };
  try { await (views[route.name] || viewHome)(); }
  catch (e) { main.append(h("div", { class: "empty" }, e.message)); }
}

// ---------- live feed ----------
async function startFeed() {
  if (feedRunning) return;
  feedRunning = true;
  while (token()) {
    try {
      const q = feedSince == null ? "wait=0" : `since=${feedSince}&wait=25`;
      const r = await api("GET", "/api/v1/feed?" + q);
      $("#live").classList.add("on");
      const first = feedSince == null;
      feedSince = r.latest;
      if (!first) for (const it of r.items) { notifyIfAsk(it); for (const f of listeners) f(it); }
    } catch {
      $("#live").classList.remove("on");
      await new Promise((ok) => setTimeout(ok, 3000));
    }
  }
  feedRunning = false;
}
function onFeed(f) { listeners.add(f); }
function notifyIfAsk(it) {
  if (it.type !== "chat-item" || !it.item || it.item.t !== "permission") return;
  const title = (it.item.params && it.item.params.toolCall && it.item.params.toolCall.title) || "a tool call";
  if (route.name !== "chat" || route.id !== it.chat) toast("An agent is asking to: " + title);
  try {
    if (document.hidden && window.Notification && Notification.permission === "granted") new Notification("Evyro: approval needed", { body: title });
  } catch { /* notifications are optional */ }
}

// ---------- pair ----------
function viewPair() {
  setTitle("Pair this device", false);
  const code = h("input", { placeholder: "ABCD-EFGH", autocomplete: "one-time-code", autocapitalize: "characters", inputmode: "text" });
  const go_ = h("button", { class: "btn primary" }, "Pair");
  go_.addEventListener("click", async () => {
    go_.disabled = true;
    try {
      const r = await api("POST", "/api/v1/pair", { code: code.value });
      localStorage.setItem(TOKEN_KEY, r.token);
      me = r.device;
      toast("Paired as " + r.device.name);
      try { if (window.Notification && Notification.permission === "default") Notification.requestPermission(); } catch { /* optional */ }
      startFeed();
      go({ name: "home" }, true);
    } catch (e) { toast(e.message, true); } finally { go_.disabled = false; }
  });
  main.append(h("div", { class: "card stack" },
    h("p", null, "On your computer, run ", h("code", null, "evyro remote pair --name \"My phone\""), " and type the code here. It works once and expires in a few minutes."),
    h("label", null, "Pairing code"), code, go_,
    h("div", { class: "hint" }, "The device gets its own token, which you can revoke any time with ", h("code", null, "evyro remote revoke"), ".")));
}

// ---------- home ----------
async function viewHome() {
  setTitle("Evyro", false);
  const [projects, chats] = await Promise.all([api("GET", "/api/v1/projects"), api("GET", "/api/v1/chats")]);
  const waiting = chats.filter((c) => c.waiting > 0);
  if (waiting.length) main.append(h("div", { class: "banner", onclick: () => go({ name: "chat", id: waiting[0].id }) },
    `${waiting.length} chat${waiting.length > 1 ? "s" : ""} waiting for your approval →`));
  main.append(h("h2", null, "Chats"));
  main.append(chats.length ? h("ul", { class: "list" }, chats.map(chatRow)) : h("div", { class: "card empty" }, "No chats running."));
  if (can("control")) main.append(h("div", { class: "row" }, h("button", { class: "btn", onclick: () => go({ name: "newchat" }) }, "+ New chat")));
  main.append(h("h2", null, "Projects"));
  main.append(projects.length ? h("ul", { class: "list" }, projects.map((p) => h("li", { class: "tap", onclick: () => go({ name: "project", id: p.id, title: p.alias || p.name }) },
    h("div", { class: "grow" }, h("div", null, h("strong", null, p.alias || p.name)), h("div", { class: "sub" }, p.root_path)), "›"))) : h("div", { class: "card empty" }, "No projects."));
  main.append(h("div", { class: "hint" }, `Signed in as ${me.name} (${me.scope} access).`));
  onFeed((it) => { if (it.type === "chat-state" && route.name === "home") render(); });
}
function chatState(c) {
  if (c.waiting > 0) return h("span", { class: "pill warn" }, "needs you");
  if (c.state === "failed") return h("span", { class: "pill bad" }, "failed");
  if (!c.running) return h("span", { class: "pill" }, "ended");
  if (c.busy) return h("span", { class: "pill accent" }, "working");
  return h("span", { class: "pill ok" }, "your turn");
}
function chatRow(c) {
  return h("li", { class: "tap", onclick: () => go({ name: "chat", id: c.id }) },
    h("div", { class: "grow" }, h("div", null, h("strong", null, c.agent || c.target), " ", c.task ? h("span", { class: "sub" }, c.task) : null),
      h("div", { class: "sub" }, c.started_by === "desktop" ? "started on the desktop" : "started remotely")), chatState(c));
}

// ---------- project ----------
async function viewProject() {
  setTitle(route.title || "Project", true);
  const pid = route.id;
  const tab = route.tab || "overview";
  const tabs = h("div", { class: "tabs" }, [["overview", "Overview"], ["tasks", "Tasks"], ["changes", "Changes"], ["sessions", "Sessions"], ["handoffs", "Handoffs"]]
    .map(([k, l]) => h("button", { class: tab === k ? "on" : "", onclick: () => { route = { ...route, tab: k }; render(); } }, l)));
  main.append(tabs);
  const body = h("div");
  main.append(body);
  const base = `/api/v1/projects/${encodeURIComponent(pid)}`;
  if (tab === "overview") {
    const o = await api("GET", base + "/overview");
    const hl = o.health || {};
    const counts = hl.task_counts || {};
    body.append(
      h("div", { class: "card" }, h("h3", null, "Active task"),
        o.active_task ? h("div", null, h("strong", null, `${o.active_task.key} ${o.active_task.title}`), h("div", { class: "sub" }, `${o.active_task.status}${o.owner ? " · " + o.owner.harness + " working" : ""}`)) : h("div", { class: "sub" }, "Nothing in progress."),
        o.latest_checkpoint ? h("div", { class: "hint" }, "Last checkpoint ", ago(o.latest_checkpoint.created_at), ": ", o.latest_checkpoint.summary || "") : null),
      h("div", { class: "stats" }, ["in_progress", "ready", "blocked", "verified", "done"].map((k) => h("div", { class: "stat" }, h("b", null, String(counts[k] || 0)), k.replace("_", " ")))),
      o.pending_handoff ? h("div", { class: "card" }, h("strong", null, "Handoff waiting"), h("div", { class: "sub" }, `to ${o.pending_handoff.to_harness}`)) : null,
      o.git ? h("div", { class: "card" }, h("div", { class: "sub" }, `Branch ${o.git.branch || "?"} · ${o.git.files.length} changed`)) : null,
      h("h2", null, "Recent activity"),
      h("ul", { class: "list" }, (o.events || []).slice(0, 12).map((e) => h("li", null, h("div", { class: "grow" }, e.kind.replace(/[._]/g, " ")), h("span", { class: "sub" }, ago(e.created_at))))));
    onFeed((it) => { if (it.type === "event" && it.event.project_id === pid) render(); });
  } else if (tab === "tasks") {
    const ts = await api("GET", base + "/tasks");
    body.append(ts.length ? h("ul", { class: "list" }, ts.map((r) => h("li", { class: "tap", onclick: () => go({ name: "task", pid, key: r.task.key }) },
      h("div", { class: "grow" }, h("div", null, h("strong", null, r.task.key), " ", r.task.title), h("div", { class: "sub" }, `${r.task.status} · ${r.criteria_met}/${r.criteria_total} criteria`)), "›"))) : h("div", { class: "card empty" }, "No open tasks."));
  } else if (tab === "changes") {
    const c = await api("GET", base + "/changes");
    body.append(h("div", { class: "sub" }, `${c.files.length} changed file(s)${c.redactions ? ` · ${c.redactions} secret(s) hidden` : ""}. ${can("approve") ? "Tap a line to comment on it." : ""}`));
    body.append(diffNode(c.diff, can("approve") ? (path, line, text) => commentDialog(base, path, line, text) : null));
  } else if (tab === "sessions") {
    const ss = await api("GET", base + "/sessions");
    body.append(ss.length ? h("ul", { class: "list" }, ss.slice(0, 40).map((s) => h("li", null, h("div", { class: "grow" }, h("strong", null, s.harness), h("div", { class: "sub" }, `${s.status} · ${ago(s.started_at)}`))))) : h("div", { class: "card empty" }, "No sessions."));
  } else if (tab === "handoffs") {
    const hs = await api("GET", base + "/handoffs");
    body.append(hs.length ? h("ul", { class: "list" }, hs.map((x) => h("li", null, h("div", { class: "grow" }, h("strong", null, `${x.from_harness || "?"} → ${x.to_harness}`), h("div", { class: "sub" }, `${x.status} · ${ago(x.created_at)}`))))) : h("div", { class: "card empty" }, "No handoffs."));
  }
}

function diffNode(text, onLine) {
  const box = h("div", { class: "diff" });
  let file = null, newLine = 0;
  for (const line of String(text || "").split("\n")) {
    if (line.startsWith("diff --git")) { file = (line.match(/ b\/(.*)$/) || [])[1] || null; box.append(h("div", { class: "file" }, file || line)); continue; }
    if (line.startsWith("index ") || line.startsWith("--- ") || line.startsWith("+++ ") || line.startsWith("new file")) continue;
    const m = line.match(/^@@ -\d+(?:,\d+)? \+(\d+)/);
    if (m) { newLine = Number(m[1]); box.append(h("div", { class: "hunk" }, line)); continue; }
    const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "";
    const at = cls === "del" ? null : newLine;
    const el = h("div", { class: cls + (onLine && at && file ? " tap" : "") }, line || " ");
    if (onLine && at && file) { const f = file; el.addEventListener("click", () => onLine(f, at, line.slice(1))); }
    box.append(el);
    if (cls !== "del") newLine++;
  }
  return box;
}

function commentDialog(base, path, line, snippet) {
  const ta = h("textarea", { placeholder: "What should the agent change here?" });
  const send = h("button", { class: "btn primary" }, "Add comment");
  const card = h("div", { class: "card stack" }, h("strong", null, `${path}:${line}`), h("pre", null, snippet), ta,
    h("div", { class: "row" }, send, h("button", { class: "btn", onclick: () => card.remove() }, "Cancel")));
  send.addEventListener("click", async () => {
    if (!ta.value.trim()) return toast("Write a comment first", true);
    try { await api("POST", base + "/review", { path, line, snippet, body: ta.value.trim() }); toast("Comment saved. The next agent will see it."); card.remove(); }
    catch (e) { toast(e.message, true); }
  });
  main.prepend(card);
  ta.focus();
}

// ---------- task ----------
async function viewTask() {
  setTitle(route.key, true);
  const d = await api("GET", `/api/v1/projects/${encodeURIComponent(route.pid)}/tasks/${encodeURIComponent(route.key)}`);
  main.append(h("div", { class: "card" }, h("strong", null, d.task.title), h("div", { class: "sub" }, `${d.task.status} · ${d.task.kind} · P${d.task.priority}`),
    d.task.description ? h("p", null, d.task.description) : null));
  main.append(h("h2", null, "Acceptance criteria"));
  main.append(d.criteria.length ? h("ul", { class: "list" }, d.criteria.map((c) => h("li", null, h("span", null, c.met ? "✓" : "○"), h("div", { class: "grow" }, c.text)))) : h("div", { class: "card empty" }, "None."));
  main.append(h("h2", null, "Checkpoints"));
  main.append(d.checkpoints.length ? h("ul", { class: "list" }, d.checkpoints.map((c) => h("li", null, h("div", { class: "grow" }, c.summary || "(no summary)", h("div", { class: "sub" }, `${c.trigger || ""} · ${ago(c.created_at)}`))))) : h("div", { class: "card empty" }, "None yet."));
}

// ---------- chats ----------
async function viewNewChat() {
  setTitle("New chat", true);
  const [projects, agents] = await Promise.all([api("GET", "/api/v1/projects"), api("GET", "/api/v1/agents")]);
  const proj = h("select", null, projects.map((p) => h("option", { value: p.id }, p.alias || p.name)));
  const ready = agents.filter((a) => a.command);
  const agent = h("select", null, (ready.length ? ready : agents).map((a) => h("option", { value: a.harness }, a.display_name + (a.command ? "" : " (not installed)"))));
  const task = h("input", { placeholder: "T-1 (optional: the active task or a waiting handoff)" });
  const start = h("button", { class: "btn primary" }, "Start chat");
  start.addEventListener("click", async () => {
    start.disabled = true;
    try {
      const info = await api("POST", "/api/v1/chats", { project: proj.value, target: agent.value, task: task.value.trim() || null });
      go({ name: "chat", id: info.id }, true);
    } catch (e) { toast(e.message, true); start.disabled = false; }
  });
  main.append(h("div", { class: "card stack" }, h("label", null, "Project"), proj, h("label", null, "Agent"), agent, h("label", null, "Task"), task, start,
    h("div", { class: "hint" }, "The agent runs on the computer that runs evyro serve, under an Evyro session. It asks before each tool call, and you answer here.")));
}

async function viewChat() {
  const id = route.id;
  let data = await api("GET", `/api/v1/chats/${encodeURIComponent(id)}`);
  setTitle(data.info.agent || data.info.title, true);
  const head = h("div", { class: "row" });
  const log = h("div");
  const asks = h("div");
  main.append(head, log, asks);
  const titles = {};
  let msg = null;
  const drawHead = () => {
    const i = data.info;
    clear(head).append(chatState(i), h("span", { class: "sub grow" }, `${i.task || ""} ${i.cwd ? "· " + i.cwd : ""}`),
      can("approve") && i.busy ? h("button", { class: "btn", onclick: () => act("cancel") }, "Stop turn") : null,
      can("control") && i.running ? h("button", { class: "btn danger", onclick: () => { if (confirm("End this chat? The agent stops and Evyro checkpoints.")) act("close", true); } }, "End chat") : null);
  };
  const act = async (what, leave) => {
    try { await api("POST", `/api/v1/chats/${encodeURIComponent(id)}/${what}`); if (leave) go({ name: "home" }, true); }
    catch (e) { toast(e.message, true); }
  };
  const add = (it) => {
    if (it.t === "user") { msg = null; log.append(h("div", { class: "msg user" }, typeof it.text === "string" ? it.text : "Evyro's task context")); return; }
    if (it.t === "update") {
      const u = it.update || {};
      if (u.sessionUpdate === "agent_message_chunk") {
        if (!msg) { msg = h("div", { class: "msg agent" }); log.append(msg); }
        msg.append((u.content && u.content.text) || "");
      } else if (u.sessionUpdate === "tool_call") {
        msg = null; titles[u.toolCallId] = u.title;
        log.append(h("div", { class: "tool" }, `• ${u.title} (${u.kind || "tool"})`));
      } else if (u.sessionUpdate === "tool_call_update" && (u.status === "completed" || u.status === "failed")) {
        log.append(h("div", { class: "tool" }, `${u.status === "completed" ? "✓" : "✗"} ${u.title || titles[u.toolCallId] || ""}`));
      } else if (u.sessionUpdate === "plan") {
        msg = null;
        log.append(h("div", { class: "card plan" }, (u.entries || []).map((e) => h("div", { class: e.status === "completed" ? "done" : "" }, `${e.status === "completed" ? "✓" : e.status === "in_progress" ? "…" : "○"} ${e.content}`))));
      }
      return;
    }
    if (it.t === "file") { log.append(h("div", { class: "tool" }, it.op === "refused" ? `⊘ refused ${it.path}: ${it.reason}` : it.op === "write" ? `✎ wrote ${it.path}` : `read ${it.path}`)); return; }
    if (it.t === "permission_answer") { log.append(h("div", { class: "answered" }, `Answered: ${it.option || "cancelled"}${it.by ? " (" + it.by.replace("remote:", "") + ")" : ""}`)); return; }
    if (it.t === "turn_end") { msg = null; log.append(h("div", { class: "answered" }, `Turn ended (${it.reason})`)); return; }
    if (it.t === "error") { log.append(h("div", { class: "pill bad" }, it.message)); return; }
    if (it.t === "exit") log.append(h("div", { class: "answered" }, "The agent exited."));
  };
  const drawAsks = () => {
    clear(asks);
    for (const p of data.pending || []) asks.append(askCard(id, p));
  };
  for (const it of data.log) add(it);
  drawHead(); drawAsks();
  if (can("control")) {
    const ta = h("textarea", { placeholder: "Message the agent" });
    const send = h("button", { class: "btn primary" }, "Send");
    send.addEventListener("click", async () => {
      if (!ta.value.trim()) return;
      try { await api("POST", `/api/v1/chats/${encodeURIComponent(id)}/messages`, { text: ta.value }); ta.value = ""; }
      catch (e) { toast(e.message, true); }
    });
    main.append(h("div", { class: "composer" }, ta, send));
  }
  let next = data.next_seq;
  onFeed(async (it) => {
    if (it.chat !== id && !(it.type === "chat-state" && it.info && it.info.id === id)) return;
    if (it.type === "chat-state") { data.info = it.info; drawHead(); }
    if (it.type === "chat-item") {
      if (it.item_seq < next) return;
      if (it.item_seq > next) { // missed something: fetch the gap
        const r = await api("GET", `/api/v1/chats/${encodeURIComponent(id)}?after=${next - 1}`);
        for (const x of r.log) add(x);
        next = r.next_seq; data.pending = r.pending; drawAsks();
        return;
      }
      add(it.item); next = it.item_seq + 1;
      if (["permission", "permission_answer", "turn_end", "exit", "error"].includes(it.item.t)) {
        const r = await api("GET", `/api/v1/chats/${encodeURIComponent(id)}?after=${next - 1}`);
        data.pending = r.pending; drawAsks();
      }
    }
    window.scrollTo(0, document.body.scrollHeight);
  });
  window.scrollTo(0, document.body.scrollHeight);
}

/** A permission request, shown with the exact action and the agent's own options. */
function askCard(id, p) {
  const tc = (p.params && p.params.toolCall) || {};
  const opts = (p.params && p.params.options) || [];
  const locs = (tc.locations || []).map((l) => l.path).filter(Boolean);
  const diffs = (tc.content || []).filter((c) => c.type === "diff");
  const card = h("div", { class: "ask" },
    h("h3", null, "The agent wants to: ", tc.title || "run a tool"),
    h("div", { class: "sub" }, `Kind: ${tc.kind || "other"} · tool call ${tc.toolCallId || "?"}`),
    locs.length ? h("div", { class: "mono" }, locs.join("\n")) : null,
    diffs.map((d) => h("div", null, h("div", { class: "sub" }, d.path), h("pre", null, `- ${(d.oldText || "").split("\n").join("\n- ")}\n+ ${(d.newText || "").split("\n").join("\n+ ")}`))),
    tc.rawInput ? h("pre", null, JSON.stringify(tc.rawInput, null, 2)) : null);
  if (can("approve")) {
    card.append(h("div", { class: "opts" }, opts.map((o) => {
      const b = h("button", { class: "btn" + (String(o.kind || "").startsWith("allow") ? " primary" : " danger") }, o.name || o.optionId);
      b.addEventListener("click", async () => {
        b.disabled = true;
        try { await api("POST", `/api/v1/chats/${encodeURIComponent(id)}/permission`, { request: p.request, tool_call_id: tc.toolCallId || "", option_id: o.optionId }); card.remove(); }
        catch (e) { toast(e.message, true); b.disabled = false; }
      });
      return b;
    })));
  } else {
    card.append(h("div", { class: "hint" }, "This device can only look. Answer on the computer, or pair with approve access."));
  }
  return card;
}

render();
