// Changes view and review workbench: read the diff, comment on lines, and send
// the open comments to an agent. Loaded after app.js and threads.js.
"use strict";

const STATUS_GIT = { "M": ["accent", "modified"], "A": ["ok", "added"], "D": ["bad", "deleted"], "R": ["accent", "renamed"], "??": ["ok", "new"], "MM": ["accent", "modified"], "AM": ["ok", "added"] };
const lineClass = (l) => (l.startsWith("@@") ? "hunk" : l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : "");

/** Split a unified diff by file, numbering lines on both sides. */
function splitDiff(text) {
  const out = {};
  let cur = null, oldNo = 0, newNo = 0;
  for (const l of String(text || "").replace(/\n$/, "").split("\n")) {
    const m = /^diff --git a\/(.+?) b\/(.+)$/.exec(l);
    if (m) { cur = out[m[2]] = { lines: [], add: 0, del: 0 }; continue; }
    if (!cur || /^(index |--- |\+\+\+ |new file mode|deleted file mode|similarity|rename |\\ No newline)/.test(l)) continue;
    const hk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(l);
    if (hk) { oldNo = +hk[1]; newNo = +hk[2]; cur.lines.push({ text: l, kind: "hunk" }); continue; }
    if (l.startsWith("+")) { cur.add++; cur.lines.push({ text: l, kind: "add", n: newNo++ }); }
    else if (l.startsWith("-")) { cur.del++; cur.lines.push({ text: l, kind: "del", o: oldNo++ }); }
    else { cur.lines.push({ text: l, kind: "", o: oldNo++, n: newNo++ }); }
  }
  return out;
}

async function loadReview(pid) {
  try { return await invoke("list_review_comments", { projectId: pid }); } catch { return []; }
}

function commentBox(c, refresh) {
  const sentNote = c.status === "sent" ? pill("accent", "sent to agent") : null;
  return h("div", { class: "rv-comment" + (c.status === "sent" ? " sent" : "") },
    h("div", { class: "rv-head" },
      h("span", { class: "small muted" }, c.author === "human" ? "You" : c.author, " · ", ago(c.created_at)), sentNote,
      h("span", { class: "spacer" }),
      h("button", { class: "btn ghost small", title: "Mark as addressed", onclick: async () => { await act(() => invoke("set_review_status", { id: c.id, status: "resolved" })); refresh(); } }, "Resolve"),
      h("button", { class: "btn ghost small", title: "Delete comment", onclick: async () => { await act(() => invoke("delete_review_comment", { id: c.id })); refresh(); } }, "Delete")),
    h("div", { class: "rv-body" }, c.body));
}

function commentForm(path, line, snippet, refresh, onCancel) {
  const ta = h("textarea", { rows: 3, placeholder: line ? `Comment on line ${line}…` : `Comment on ${path}…`, "aria-label": "Review comment" });
  const save = async () => {
    if (!ta.value.trim()) return;
    await act(() => invoke("add_review_comment", { projectId: state.project.id, path, line: line || null, snippet: snippet || null, body: ta.value, task: null }));
    refresh();
  };
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); save(); }
    if (e.key === "Escape") { e.stopPropagation(); onCancel(); }
  });
  const f = h("div", { class: "rv-form" }, ta,
    h("div", { class: "row" },
      h("span", { class: "muted small" }, `${MOD}+Enter to save`), h("span", { class: "spacer" }),
      h("button", { class: "btn small", onclick: onCancel }, "Cancel"),
      h("button", { class: "btn primary small", onclick: save }, "Comment")));
  setTimeout(() => ta.focus(), 0);
  return f;
}

/** Send the open comments to a running agent thread, or copy them as a prompt. */
async function sendReviewDialog(refresh) {
  const pid = state.project.id;
  await loadThreads();
  const running = state.threads.filter((t) => t.project_id === pid && t.running && t.kind === "agent");
  const preview = await invoke("review_prompt_text", { projectId: pid, markSent: false });
  if (!preview.count) { toast("No open comments to send."); return; }
  const sel = h("select", null,
    running.map((t) => h("option", { value: t.id }, `${t.title} (running thread)`)),
    h("option", { value: "copy" }, "Copy as a prompt for any AI"));
  modal(`Send ${preview.count} comment${preview.count === 1 ? "" : "s"}`, [
    field("Send to", sel, running.length ? "The comments are typed into the agent's terminal as one message." : "No agent is running in a thread. Copy the prompt and paste it into any AI, or start an agent thread first."),
    h("pre", { class: "rv-preview" }, preview.prompt),
  ], [{
    label: "Send", primary: true, run: async () => {
      const r = await invoke("review_prompt_text", { projectId: pid, markSent: true });
      if (sel.value === "copy") {
        await copy(r.prompt, "Review comments copied. Paste them into the AI.");
      } else {
        // Paste like a terminal does: bracketed when the agent asked for it, so a
        // multi-line message stays one message. Enter submits it.
        const x = ensureXterm(sel.value);
        await new Promise((res) => setTimeout(res, 300)); // let a fresh view load its output first
        let bracketed = false;
        try { bracketed = !!x.term.modes.bracketedPasteMode; } catch { /* unknown: send plain */ }
        const text = r.prompt.replace(/\r?\n/g, "\r");
        await invoke("term_write", { id: sel.value, data: bracketed ? "\x1b[200~" + text + "\x1b[201~" : text });
        await new Promise((res) => setTimeout(res, 150));
        await invoke("term_write", { id: sel.value, data: "\r" });
        state.thread = sel.value;
        toast("Comments sent to the agent.");
      }
      refresh();
    },
  }]);
}

async function viewChanges(v) {
  const pid = state.project.id;
  let c;
  v.append(h("div", { class: "muted" }, "Reading changes…"));
  try { c = await invoke("repo_changes", { projectId: pid }); } catch (e) { clear(v).append(h("div", { class: "banner bad" }, errMsg(e))); return; }
  const comments = await loadReview(pid);
  if (state.view !== "changes" || state.project.id !== pid) return;
  clear(v);
  const refresh = () => { if (state.view === "changes") render(); };
  const files = c.files.filter((f) => !f.path.startsWith(".evyro/"));
  const open = comments.filter((x) => x.status === "open").length;
  v.append(h("div", { class: "row rv-bar" },
    h("span", { class: "muted" }, files.length ? `${files.length} changed file(s) since the last commit` : "No uncommitted changes."),
    comments.length ? pill(open ? "warn" : "accent", open ? `${open} open comment${open === 1 ? "" : "s"}` : `${comments.length} sent, waiting`) : null,
    h("span", { class: "spacer" }),
    c.redactions ? pill("warn", `${c.redactions} secret(s) hidden`) : null,
    h("button", { class: "btn small", onclick: refresh }, "Refresh"),
    h("button", { class: "btn small", onclick: () => copy(c.diff, "Diff copied") }, "Copy diff"),
    h("button", { class: "btn primary small", disabled: open ? null : true, onclick: () => sendReviewDialog(refresh) }, "Send comments to agent")));
  if (!files.length && !comments.length) return;
  if (files.length) v.append(h("p", { class: "muted small", style: "margin:0 0 10px" }, "Click a line number or + to comment on that line. Comments go to the next agent with its context, or straight into a running agent thread."));

  const byFile = splitDiff(c.diff);
  const byPath = {};
  for (const x of comments) (byPath[x.path] = byPath[x.path] || []).push(x);
  const card = h("div", { class: "card" });
  const shown = new Set();
  for (const f of files) {
    shown.add(f.path);
    const d = byFile[f.path];
    const fileComments = byPath[f.path] || [];
    const st = STATUS_GIT[f.status.trim()];
    const body = h("div", { class: "diff-body" });
    const det = h("details", { class: "diff-file", open: files.length <= 6 || fileComments.length ? true : null },
      h("summary", null, pill(st ? st[0] : "", st ? st[1] : f.status.trim()),
        h("span", { class: "mono" }, f.path), d ? h("span", { class: "muted small" }, `+${d.add} −${d.del}`) : null,
        fileComments.length ? pill("warn", `${fileComments.length} comment${fileComments.length === 1 ? "" : "s"}`) : null),
      body);
    // Outside <summary>: a button inside it would be a control within a control.
    const tools = h("div", { class: "diff-tools" },
      h("button", { class: "btn ghost small", onclick: () => { tools.after(commentForm(f.path, null, null, refresh, () => refresh())); } }, "Comment on file"));
    body.append(tools);
    for (const x of fileComments.filter((x) => !x.line)) body.append(commentBox(x, refresh));
    if (d) {
      const pre = h("div", { class: "diff", role: "group", "aria-label": `Changes in ${f.path}` });
      for (const ln of d.lines) {
        const here = ln.n ? fileComments.filter((x) => x.line === ln.n) : [];
        const add = () => {
          if (row.nextSibling && row.nextSibling.classList && row.nextSibling.classList.contains("rv-form")) return;
          row.after(commentForm(f.path, ln.n, ln.text, refresh, () => form && form.remove()));
          form = row.nextSibling;
        };
        let form = null;
        const row = h("div", { class: "dl " + ln.kind },
          h("span", { class: "ln" }, ln.o || ""), h("span", { class: "ln" + (ln.n ? " can" : ""), onclick: ln.n ? add : null, title: ln.n ? "Comment on this line" : null }, ln.n || ""),
          h("span", { class: "code" }, ln.text),
          ln.n ? h("button", { class: "cbtn", title: "Comment on this line", "aria-label": `Comment on line ${ln.n}`, onclick: add }, "+") : null);
        pre.append(row, ...here.map((x) => commentBox(x, refresh)));
      }
      body.append(pre);
    } else {
      body.append(h("div", { class: "muted small", style: "padding:6px 10px" }, f.status.trim() === "??" ? "New file, not yet tracked by Git. Comment on the whole file above." : "No text diff (binary, deleted or excluded)."));
    }
    card.append(det);
  }
  // Comments on files that have no changes any more (committed or reverted).
  const orphans = comments.filter((x) => !shown.has(x.path));
  if (orphans.length) {
    card.append(h("div", { class: "diff-file" }, h("div", { class: "rv-orphans-head muted small" }, "Comments on files with no uncommitted changes"),
      ...orphans.map((x) => h("div", null, h("div", { class: "mono small", style: "padding:6px 4px 0" }, x.path + (x.line ? `:${x.line}` : "")), commentBox(x, refresh)))));
  }
  v.append(card);
  if (c.truncated) v.append(h("div", { class: "banner warn", style: "margin-top:10px" }, "The diff is long and was cut off. Use git diff in a terminal for the rest."));
}
