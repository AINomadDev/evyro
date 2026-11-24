// Chat threads: coding agents over ACP (Agent Client Protocol), shown as a
// conversation with the agent's plan, tool calls and permission requests.
// Loaded after threads.js and uses its helpers (state.threads, drawThreadList...).
// Everything the agent says is shown with textContent, never as HTML.
"use strict";

const CHATS = new Map(); // id -> { log: [], loaded: bool }
const TOOL_ICON = { read: "◔", edit: "✎", delete: "⌫", move: "⇄", search: "⌕", execute: "▶", think: "∴", fetch: "⇣", switch_mode: "⇆", other: "•" };

function chatData(id) {
  if (!CHATS.has(id)) CHATS.set(id, { log: [], loaded: false });
  return CHATS.get(id);
}

async function openSavedMessage(projectId, hit) {
  await loadThreads();
  if (state.project?.id !== projectId) return;
  if (!projThreads().some((t) => t.id === hit.id && t.archived)) {
    toast("That saved chat is no longer available.", true);
    return;
  }
  chatData(hit.id).searchHit = hit.message_index;
  placeThread(hit.id);
  setTab("workspace");
}

async function loadChats() {
  try { return await invoke("chat_list"); } catch { return []; }
}

/** Ask which AI and task, then start a chat thread. */
async function newChatDialog() {
  const sel = await targetSelect("antigravity-cli");
  for (const g of [...sel.querySelectorAll("optgroup")]) {
    if (![KIND_LABEL.harness, KIND_LABEL.api].includes(g.label)) g.remove();
  }
  let agents = [];
  try { agents = await invoke("acp_agents"); } catch { /* listing unavailable */ }
  const ready = agents.filter((a) => a.command).map((a) => a.harness);
  // "Needs setup" is about the agent's terminal CLI; for a chat what matters is its ACP side.
  for (const o of sel.options) if (ready.includes(o.value)) o.textContent = o.value;
  if (!ready.includes(sel.value) && ready.length && [...sel.options].some((o) => ready.includes(o.value))) sel.value = [...sel.options].find((o) => ready.includes(o.value)).value;
  let tasks = [];
  try { tasks = await invoke("list_tasks", { projectId: state.project.id }); } catch { /* no tasks */ }
  const taskSel = h("select", null, h("option", { value: "" }, "No task (let the AI ask)"),
    tasks.map((r) => h("option", { value: r.task.key }, `${r.task.key}  ${r.task.title}`)));
  const active = tasks.find((r) => r.task.status === "in_progress") || tasks.find((r) => r.task.status === "ready");
  if (active) taskSel.value = active.task.key;
  const status = h("div", { class: "hint" });
  const showStatus = () => {
    const t = (state.targets || []).find((x) => x.target.name === sel.value);
    const harness = (t && (t.target.harness || t.target.name)) || sel.value;
    const a = agents.find((x) => x.harness === harness);
    clear(status).append(a
      ? (a.command ? h("span", null, "Ready: ", h("code", null, a.command)) : h("span", null, `${a.display_name} needs its ACP side: `, h("code", null, a.install)))
      : "This AI has no chat (ACP) mode Evyro knows of. Use a terminal thread instead.");
    const hint = { gemini: "Individual Gemini accounts should use Antigravity CLI.", cursor: "Sign in with cursor-agent login before opening a chat.", "antigravity-cli": "Sign in with agy acp --login." }[harness];
    if (hint) status.append(h("div", null, hint));
  };
  sel.addEventListener("change", showStatus);
  showStatus();
  modal("New chat", [
    h("p", { class: "muted small" }, "A chat talks to the agent over ACP: you see its plan and each tool call, and you approve what it runs. It works under an Evyro session like a terminal thread, with Evyro's MCP tools."),
    field("AI", sel, status),
    field("Task", taskSel),
  ], [
    { label: "Start chat", primary: true, run: () => (sel.value ? startChat({ target: sel.value, task: taskSel.value || null }) : false) },
  ]);
}

async function startChat({ target, task = null, handoffId = null }) {
  const info = await act(() => invoke("chat_open", { projectId: state.project.id, target, task, handoffId }));
  if (!state.threads.some((t) => t.id === info.id)) state.threads.push(info);
  chatData(info.id).loaded = true;
  placeThread(info.id, true);
  setTab("workspace");
  return true;
}

function chatEvent(p) {
  if (!p) return;
  const d = chatData(p.id);
  d.log.push(p.item);
  if (p.item.t === "permission" && document.hidden === false && !chatBox(p.id)) {
    const t = state.threads.find((x) => x.id === p.id);
    toast(`${t ? t.title : "An agent"} is asking for permission. It is under Needs you on the left.`);
  }
  if (["permission", "permission_answer", "turn_end", "exit"].includes(p.item.t)) {
    drawThreadList();
    const t = state.threads.find((x) => x.id === p.id);
    if (t) drawChatHead(t);
  }
  scheduleChatDraw(p.id);
}

function chatState(info) {
  if (!info) return;
  const i = state.threads.findIndex((t) => t.id === info.id);
  if (i >= 0) state.threads[i] = info; else state.threads.push(info);
  const d = chatData(info.id);
  if (!info.draft_available && d.openingDraft) {
    d.draft = "";
    d.openingDraft = false;
    const input = chatPart("chat-input", info.id);
    if (input) input.value = "";
  }
  drawThreadList();
  drawChatHead(info);
  loadOpeningDraft(info);
  scheduleChatDraw(info.id);
  updateLiveBadge();
}

const chatDrawPending = new Map();
// A chat can show in any workspace pane, so its parts are found by thread id, not by a page-wide id.
const chatPart = (cls, id) => document.querySelector(`.${cls}[data-id="${CSS.escape(id)}"]`);
const chatBox = (id) => chatPart("chat-log", id);
function scheduleChatDraw(id) {
  if (!chatBox(id)) return;
  if (chatDrawPending.has(id)) return;
  chatDrawPending.set(id, requestAnimationFrame(() => { chatDrawPending.delete(id); drawChatLog(id); }));
}

/** Turn the event log into blocks: messages, merged text, tool calls updated in place. */
function chatBlocks(log) {
  const blocks = [];
  const tools = new Map();
  const perms = new Map();
  let text = null, thought = null, plan = null, named = false;
  const endText = () => { text = null; thought = null; };
  for (const [index, it] of log.entries()) {
    if (it.t === "user") { endText(); plan = null; named = false; blocks.push({ k: "user", text: it.text, index }); continue; }
    if (it.t === "turn_end") { endText(); blocks.push({ k: "turn", reason: it.reason }); continue; }
    if (it.t === "error") { endText(); blocks.push({ k: "error", message: it.message }); continue; }
    if (it.t === "exit") { endText(); blocks.push({ k: "exit", code: it.code }); continue; }
    if (it.t === "file") { if (it.op !== "read") { endText(); blocks.push({ k: "file", ...it }); } continue; }
    if (it.t === "permission") { endText(); const b = { k: "perm", request: it.request, params: it.params, answer: undefined }; perms.set(JSON.stringify(it.request), b); blocks.push(b); continue; }
    if (it.t === "permission_answer") { const b = perms.get(JSON.stringify(it.request)); if (b) b.answer = it.option; continue; }
    if (it.t !== "update") continue;
    const u = it.update || {};
    switch (u.sessionUpdate) {
      case "agent_message_chunk": {
        const s = u.content && u.content.type === "text" ? u.content.text : "";
        if (!text) { thought = null; text = { k: "agent", text: "", first: !named, index }; named = true; blocks.push(text); }
        text.text += s;
        break;
      }
      case "agent_thought_chunk": {
        const s = u.content && u.content.text ? u.content.text : "";
        if (!thought) { text = null; thought = { k: "thought", text: "" }; blocks.push(thought); }
        thought.text += s;
        break;
      }
      case "tool_call": {
        endText();
        const b = { k: "tool", id: u.toolCallId, title: u.title || "Tool call", kind: u.kind || "other", status: u.status || "pending", content: u.content || [], locations: u.locations || [] };
        tools.set(u.toolCallId, b);
        blocks.push(b);
        break;
      }
      case "tool_call_update": {
        const b = tools.get(u.toolCallId);
        if (!b) break;
        for (const k of ["title", "kind", "status"]) if (u[k]) b[k] = u[k];
        if (u.content) b.content = u.content;
        if (u.locations) b.locations = u.locations;
        break;
      }
      case "plan": {
        if (plan) plan.entries = u.entries || [];
        else { endText(); plan = { k: "plan", entries: u.entries || [] }; blocks.push(plan); }
        break;
      }
      default: break;
    }
  }
  return blocks;
}

function relPath(p, t) {
  const cwd = (t && t.cwd) || "";
  return cwd && p && p.startsWith(cwd + "/") ? p.slice(cwd.length + 1) : p;
}

/** A small line diff for a tool call's {type:"diff"} content. */
function diffView(c, t) {
  const old = (c.oldText || "").split("\n"), neu = (c.newText || "").split("\n");
  let a = 0; while (a < old.length && a < neu.length && old[a] === neu[a]) a++;
  let b = 0; while (b < old.length - a && b < neu.length - a && old[old.length - 1 - b] === neu[neu.length - 1 - b]) b++;
  const rows = [];
  for (const l of old.slice(a, old.length - b).slice(0, 80)) rows.push(h("span", { class: "del" }, "- " + l));
  for (const l of neu.slice(a, neu.length - b).slice(0, 80)) rows.push(h("span", { class: "add" }, "+ " + l));
  return h("div", { class: "chat-diff" }, h("div", { class: "muted small mono" }, relPath(c.path, t) + (c.oldText == null ? " (new file)" : "")),
    h("pre", { class: "diff" }, rows.length ? rows : h("span", { class: "muted" }, "(no change)")));
}

function toolContent(b, t) {
  const out = [];
  for (const c of b.content || []) {
    if (c.type === "diff") out.push(diffView(c, t));
    else if (c.type === "content" && c.content && c.content.type === "text" && c.content.text) out.push(h("pre", { class: "chat-tool-out" }, c.content.text.slice(0, 4000)));
    else if (c.type === "terminal") out.push(h("div", { class: "muted small" }, "Terminal output (shown in the agent)"));
  }
  return out;
}

function blockNode(b, t, id) {
  switch (b.k) {
    case "user":
      if (b.text && typeof b.text === "object" && b.text.context) {
        return h("details", { class: "chat-context" }, h("summary", null, "Evyro gave the agent the task context"), h("pre", null, b.text.context));
      }
      return h("div", { class: "chat-msg you", "data-message-index": b.index }, h("div", { class: "chat-who" }, "You"), h("div", { class: "chat-text" }, b.text));
    case "agent":
      return h("div", { class: "chat-msg agent", "data-message-index": b.index }, b.first ? h("div", { class: "chat-who" }, t.agent || t.target) : null, h("div", { class: "chat-text" }, b.text));
    case "thought":
      return h("details", { class: "chat-thought" }, h("summary", null, "Thinking"), h("div", { class: "chat-text muted" }, b.text));
    case "tool": {
      const loc = (b.locations || []).map((l) => relPath(l.path, t)).filter(Boolean)[0];
      return h("div", { class: "chat-tool " + b.status },
        h("div", { class: "chat-tool-head" },
          h("span", { class: "chat-tool-icon", "aria-hidden": "true" }, TOOL_ICON[b.kind] || "•"),
          h("span", { class: "chat-tool-title" }, b.title),
          loc && !b.title.includes(loc) ? h("span", { class: "muted small mono" }, loc) : null,
          pill(b.status === "completed" ? "ok" : b.status === "failed" ? "bad" : "running", b.status.replace("_", " "))),
        toolContent(b, t));
    }
    case "plan":
      return h("div", { class: "chat-plan" }, h("div", { class: "side-label" }, "Plan"),
        h("ul", null, b.entries.map((e) => h("li", { class: e.status }, h("span", { class: "chk", "aria-hidden": "true" }, e.status === "completed" ? "✓" : e.status === "in_progress" ? "◐" : "○"), e.content))));
    case "perm": {
      const tc = b.params.toolCall || {};
      const opts = b.params.options || [];
      const chosen = opts.find((o) => o.optionId === b.answer);
      const canAnswer = t.running && b.answer === undefined;
      return h("div", { class: "chat-perm" + (canAnswer ? " open" : "") },
        h("div", null, h("strong", null, "The agent asks: "), tc.title || "run a tool", tc.kind ? h("span", { class: "muted small" }, ` (${tc.kind})`) : null),
        canAnswer
          ? h("div", { class: "row" }, opts.map((o) => h("button", {
            class: "btn small" + (o.kind && o.kind.startsWith("allow") ? " primary" : ""),
            onclick: (ev) => { ev.target.disabled = true; act(() => invoke("chat_permission", { id, request: b.request, optionId: o.optionId })); },
          }, o.name)))
          : h("div", { class: "muted small" }, chosen ? `You chose: ${chosen.name}` : b.answer === undefined ? "No answer recorded" : "Cancelled"));
    }
    case "file":
      if (b.op === "write") return h("div", { class: "chat-note" }, `✎ Wrote ${b.path}`);
      return h("div", { class: "chat-note warn" }, `⊘ Refused ${relPath(b.path, t)}: ${b.reason}`);
    case "turn":
      return h("div", { class: "chat-note" }, b.reason === "end_turn" ? "Turn ended" : `Turn ended: ${String(b.reason).replace(/_/g, " ")}`);
    case "error":
      return h("div", { class: "banner bad" }, b.message);
    case "exit":
      return h("div", { class: "chat-note" }, `The agent stopped${b.code != null ? ` (exit code ${b.code})` : ""}.`);
    default:
      return null;
  }
}

function drawChatLog(id) {
  const box = chatBox(id);
  const t = state.threads.find((x) => x.id === id);
  if (!box || !t) return;
  const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 60;
  const blocks = chatBlocks(chatData(id).log);
  clear(box).append(...blocks.map((b) => blockNode(b, t, id)).filter(Boolean));
  if (t.busy) box.append(h("div", { class: "chat-working muted small" }, t.state === "starting" ? "Starting the agent…" : "Working…"));
  if (!blocks.length && !t.busy) box.append(h("div", { class: "muted small" }, "No messages yet."));
  if (atBottom) box.scrollTop = box.scrollHeight;
  drawChatComposer(t);
}

function drawChatHead(t) {
  const head = chatPart("chat-head", t.id);
  if (!head) return;
  clear(head).append(
    h("div", { class: "pane-title" }, agentBadge(t), h("div", null, h("div", { class: "thread-name" }, h("span", { class: "thread-title" }, t.title), threadStatus(t)),
      h("div", { class: "row muted small", title: t.session ? `Session ${t.session}` : null },
        t.archived ? "Saved transcript · read-only" : t.agent ? `${t.agent} over ACP` : "ACP chat", chatModelPicker(t)))),
    h("div", { class: "row" },
      t.running ? h("button", { class: "btn small", onclick: () => sendContext(t), disabled: t.busy || t.switching_model || t.state !== "ready", title: "Send the shared context packet as a message" }, "Send context") : null,
      t.busy && t.state !== "starting" ? h("button", { class: "btn small", onclick: () => act(() => invoke("chat_cancel", { id: t.id })) }, "Stop turn") : null,
      continueButton(t),
      h("button", { class: "btn small", onclick: () => closeThread(t.id) }, t.running ? "End chat" : t.archived ? "Delete transcript" : "Close chat"),
      paneRemoveBtn(t.id)));
}

function chatModelPicker(t) {
  const model = t.model_choice;
  if (!t.running || !model?.options?.length) return null;
  const select = h("select", { class: "chat-model", "aria-label": "Chat model", title: "Change model for this chat",
    disabled: t.busy || t.switching_model || t.state !== "ready" },
    model.options.map((item) => h("option", { value: item.value }, item.name)));
  select.value = model.current;
  select.addEventListener("change", async () => {
    select.disabled = true;
    try {
      await act(() => invoke("chat_set_model", { id: t.id, model: select.value }));
    } catch {
      select.value = model.current;
    } finally {
      select.disabled = false;
    }
  });
  return select;
}

function drawChatComposer(t) {
  const send = chatPart("chat-send", t.id), input = chatPart("chat-input", t.id);
  if (!send || !input) return;
  const can = t.running && !t.busy && !t.switching_model && t.state === "ready";
  send.disabled = !can;
  input.placeholder = !t.running ? "The chat has ended." : t.busy ? "The agent is working…" : `Message ${t.agent || t.target}…  (Enter sends, Shift+Enter for a new line)`;
}

async function loadOpeningDraft(t) {
  const d = chatData(t.id);
  if (!t.draft_available || d.draft !== undefined || d.loadingDraft) return;
  d.loadingDraft = true;
  try {
    const { draft } = await invoke("chat_log", { id: t.id });
    if (typeof draft === "string" && d.draft === undefined && state.threads.some((x) => x.id === t.id && x.draft_available)) {
      d.draft = draft;
      d.openingDraft = true;
      const input = chatPart("chat-input", t.id);
      if (input) input.value = draft;
    }
  } catch { /* The draft can be retried when this pane is shown again. */ }
  finally { d.loadingDraft = false; }
}

async function drawChatPane(pane, t, takeFocus = true) {
  const d = chatData(t.id);
  if (!d.loaded || (t.archived && !d.archiveLoaded)) {
    try {
      const r = await invoke("chat_log", { id: t.id });
      d.log = r.log;
      d.archiveLoaded = !!r.info.archived;
      Object.assign(t, r.info);
    } catch (e) {
      if (t.archived) { toast(errMsg(e), true); return; }
    }
    d.loaded = true;
  }
  if (!pane.isConnected || pane.dataset.id !== t.id) return;
  const input = h("textarea", { class: "chat-input", "data-id": t.id, rows: "2", "aria-label": `Message to ${t.title}` });
  input.value = d.draft || "";
  input.addEventListener("input", () => { d.draft = input.value; d.openingDraft = false; });
  const submit = async () => {
    const text = input.value.trim();
    const cur = state.threads.find((x) => x.id === t.id) || t;
    if (!text || !cur.running || cur.busy || cur.switching_model || cur.state !== "ready") return;
    try {
      await invoke("chat_send", { id: t.id, text });
      if (input.value.trim() === text) d.draft = input.value = "";
    }
    catch (e) { toast(errMsg(e), true); }
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); submit(); }
  });
  pane.append(h("div", { class: "chat" },
    h("div", { class: "thread-head chat-head", "data-id": t.id }),
    t.history_truncated ? h("div", { class: "chat-note" }, "Earlier messages were trimmed from this transcript.") : null,
    h("div", { class: "chat-log", "data-id": t.id, "aria-live": "polite", tabindex: "0", "aria-label": `Conversation with ${t.title}` }),
    t.archived ? null : h("div", { class: "chat-compose" }, input, h("button", { class: "btn primary chat-send", "data-id": t.id, onclick: submit }, "Send"))));
  drawChatHead(t);
  loadOpeningDraft(t);
  drawChatLog(t.id);
  const box = chatBox(t.id);
  if (box) box.scrollTop = box.scrollHeight;
  const hit = d.searchHit == null ? null : box?.querySelector(`[data-message-index="${d.searchHit}"]`);
  delete d.searchHit;
  if (hit) {
    hit.classList.add("chat-search-hit");
    hit.tabIndex = -1;
    hit.scrollIntoView({ block: "center", inline: "nearest" });
    hit.focus({ preventScroll: true });
  } else if (takeFocus) (t.archived ? box : input)?.focus();
}
