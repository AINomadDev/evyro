// Changes view and review workbench: read the diff, comment on lines, and send
// the open comments to an agent. Loaded after app.js and threads.js.
"use strict";

const statusGit = () => {
  const modified = ["accent", t("review.git.modified")], added = ["ok", t("review.git.added")];
  return { "M": modified, "A": added, "D": ["bad", t("review.git.deleted")], "R": ["accent", t("review.git.renamed")], "??": ["ok", t("review.git.new")], "MM": modified, "AM": added };
};
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
  const sentNote = c.status === "sent" ? pill("accent", t("review.sent")) : null;
  return h("div", { class: "rv-comment" + (c.status === "sent" ? " sent" : "") },
    h("div", { class: "rv-head" },
      h("span", { class: "small muted" }, c.author === "human" ? t("review.you") : c.author, " · ", ago(c.created_at)), sentNote,
      h("span", { class: "spacer" }),
      h("button", { class: "btn ghost small", title: t("review.resolve.title"), onclick: async () => { await act(() => invoke("set_review_status", { id: c.id, status: "resolved" })); refresh(); } }, t("review.resolve")),
      h("button", { class: "btn ghost small", title: t("review.delete.title"), onclick: async () => { await act(() => invoke("delete_review_comment", { id: c.id })); refresh(); } }, t("review.delete"))),
    h("div", { class: "rv-body" }, c.body));
}

function commentForm(path, line, snippet, refresh, onCancel) {
  const ta = h("textarea", { rows: 3, placeholder: line ? t("review.ph.line", { line }) : t("review.ph.file", { path }), "aria-label": t("review.formLabel") });
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
      h("span", { class: "muted small" }, t("review.saveHint", { keys: MOD + "+Enter" })), h("span", { class: "spacer" }),
      h("button", { class: "btn small", onclick: onCancel }, t("common.cancel")),
      h("button", { class: "btn primary small", onclick: save }, t("review.comment"))));
  setTimeout(() => ta.focus(), 0);
  return f;
}

/** Send the open comments to a running agent thread, or copy them as a prompt. */
async function sendReviewDialog(refresh) {
  const pid = state.project.id;
  await loadThreads();
  if (state.project?.id !== pid) return;
  const running = projThreads().filter((t) => t.running && (t.kind === "agent"
    || (t.kind === "chat" && t.state === "ready" && !t.busy && !t.switching_model && !needsYou(t))));
  const preview = await act(() => invoke("review_prompt_text", { projectId: pid }));
  if (state.project?.id !== pid) return;
  if (!preview.count) { toast(t("review.none")); return; }
  const sel = h("select", null,
    running.map((t) => h("option", { value: t.id }, `${t.title} (${t.kind === "chat" ? I18N.t("review.kind.chat") : I18N.t("review.kind.terminal")})`)),
    h("option", { value: "copy" }, t("review.copyOption")));
  modal(t("review.sendTitle", { count: preview.count }), [
    field(t("review.sendTo"), sel, running.length ? t("review.sendHint") : t("review.sendHintNone")),
    h("pre", { class: "rv-preview" }, preview.prompt),
  ], [{
    label: t("review.send"), primary: true, run: async () => {
      if (state.project?.id !== pid) return;
      if (sel.value === "copy") {
        await copy(preview.prompt, t("review.copied"));
      } else {
        const id = sel.value;
        try { await sendToThread(id, preview.prompt); }
        catch (e) { toast(errMsg(e), true); return false; }
        toast(t("review.delivered"));
        await recordReviewDelivery(pid, preview.ids);
        if (state.project?.id === pid) placeThread(id);
      }
      refresh();
    },
  }]);
}

async function recordReviewDelivery(projectId, ids) {
  try { await invoke("review_mark_sent", { projectId, ids }); }
  catch (e) { toast(t("review.deliveryFailed", { error: errMsg(e) }), true); }
}

async function viewChanges(v) {
  const pid = state.project.id;
  let c;
  v.append(h("div", { class: "muted" }, t("review.reading")));
  try { c = await invoke("repo_changes", { projectId: pid }); } catch (e) { clear(v).append(h("div", { class: "banner bad" }, errMsg(e))); return; }
  const comments = await loadReview(pid);
  if (state.view !== "changes" || state.project.id !== pid) return;
  clear(v);
  const refresh = () => { if (state.view === "changes") render(); };
  const files = c.files.filter((f) => !f.path.startsWith(".evyro/"));
  const open = comments.filter((x) => x.status === "open").length;
  v.append(h("div", { class: "row rv-bar" },
    h("span", { class: "muted" }, files.length ? t("review.changed", { count: files.length }) : t("review.noChanges")),
    comments.length ? pill(open ? "warn" : "accent", open ? t("review.open", { count: open }) : t("review.waiting", { count: comments.length })) : null,
    h("span", { class: "spacer" }),
    c.redactions ? pill("warn", t("review.secrets", { count: c.redactions })) : null,
    h("button", { class: "btn small", onclick: refresh }, t("review.refresh")),
    h("button", { class: "btn small", onclick: () => copy(c.diff, t("review.diffCopied")) }, t("review.copyDiff")),
    h("button", { class: "btn primary small", disabled: open ? null : true, onclick: () => sendReviewDialog(refresh) }, t("review.sendBtn"))));
  if (!files.length && !comments.length) return;
  if (files.length) v.append(h("p", { class: "muted small", style: "margin:0 0 10px" }, t("review.help")));

  const byFile = splitDiff(c.diff);
  const byPath = {};
  for (const x of comments) (byPath[x.path] = byPath[x.path] || []).push(x);
  const card = h("div", { class: "card" });
  const shown = new Set();
  for (const f of files) {
    shown.add(f.path);
    const d = byFile[f.path];
    const fileComments = byPath[f.path] || [];
    const st = statusGit()[f.status.trim()];
    const body = h("div", { class: "diff-body" });
    const det = h("details", { class: "diff-file", open: files.length <= 6 || fileComments.length ? true : null },
      h("summary", null, pill(st ? st[0] : "", st ? st[1] : f.status.trim()),
        h("span", { class: "mono" }, f.path), d ? h("span", { class: "muted small" }, `+${d.add} −${d.del}`) : null,
        fileComments.length ? pill("warn", t("review.comments", { count: fileComments.length })) : null),
      body);
    // Outside <summary>: a button inside it would be a control within a control.
    const tools = h("div", { class: "diff-tools" },
      h("button", { class: "btn ghost small", onclick: () => { tools.after(commentForm(f.path, null, null, refresh, () => refresh())); } }, t("review.commentFile")));
    body.append(tools);
    for (const x of fileComments.filter((x) => !x.line)) body.append(commentBox(x, refresh));
    if (d) {
      const pre = h("div", { class: "diff", role: "group", "aria-label": t("review.changesIn", { path: f.path }) });
      for (const ln of d.lines) {
        const here = ln.n ? fileComments.filter((x) => x.line === ln.n) : [];
        const add = () => {
          if (row.nextSibling && row.nextSibling.classList && row.nextSibling.classList.contains("rv-form")) return;
          row.after(commentForm(f.path, ln.n, ln.text, refresh, () => form && form.remove()));
          form = row.nextSibling;
        };
        let form = null;
        const row = h("div", { class: "dl " + ln.kind },
          h("span", { class: "ln" }, ln.o || ""), h("span", { class: "ln" + (ln.n ? " can" : ""), onclick: ln.n ? add : null, title: ln.n ? t("review.commentLine") : null }, ln.n || ""),
          h("span", { class: "code" }, ln.text),
          ln.n ? h("button", { class: "cbtn", title: t("review.commentLine"), "aria-label": t("review.ph.line", { line: ln.n }), onclick: add }, "+") : null);
        pre.append(row, ...here.map((x) => commentBox(x, refresh)));
      }
      body.append(pre);
    } else {
      body.append(h("div", { class: "muted small", style: "padding:6px 10px" }, f.status.trim() === "??" ? t("review.newFile") : t("review.noDiff")));
    }
    card.append(det);
  }
  // Comments on files that have no changes any more (committed or reverted).
  const orphans = comments.filter((x) => !shown.has(x.path));
  if (orphans.length) {
    card.append(h("div", { class: "diff-file" }, h("div", { class: "rv-orphans-head muted small" }, t("review.orphans")),
      ...orphans.map((x) => h("div", null, h("div", { class: "mono small", style: "padding:6px 4px 0" }, x.path + (x.line ? `:${x.line}` : "")), commentBox(x, refresh)))));
  }
  v.append(card);
  if (c.truncated) v.append(h("div", { class: "banner warn", style: "margin-top:10px" }, t("review.cut")));
}
