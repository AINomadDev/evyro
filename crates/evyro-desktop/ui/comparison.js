// Compare two implementations from a common committed base.
"use strict";

/** Files and lines changed, counted from a unified diff. */
function diffStats(diff) {
  let files = 0, added = 0, removed = 0;
  for (const line of (diff || "").split("\n")) {
    if (line.startsWith("diff --git ")) files++;
    else if (line.startsWith("+") && !line.startsWith("+++")) added++;
    else if (line.startsWith("-") && !line.startsWith("---")) removed++;
  }
  return { files, added, removed };
}

async function openComparison(task) {
  const projectId = state.project.id;
  const args = { projectId, task: task.key };
  const [first, second] = await Promise.all([targetSelect("codex"), targetSelect("claude-code")]);
  if (state.project?.id !== projectId) return;
  for (const select of [first, second]) {
    for (const group of select.querySelectorAll("optgroup")) {
      if (![KIND_LABEL.harness, KIND_LABEL.api].includes(group.label)) group.remove();
    }
    if (!select.value && select.options.length) select.selectedIndex = 0;
  }
  if (first.value === second.value) second.value = [...second.options].find((o) => o.value !== first.value)?.value || "";
  const body = h("div", { class: "stack" });
  const results = h("div", { class: "stack" });
  const menu = h("select", null);
  const current = () => body.isConnected && state.project?.id === projectId;
  let request = 0;
  const previewStage = async (panel, button, id) => {
    button.disabled = true;
    panel.hidden = false;
    clear(panel).append(h("p", { class: "small muted", role: "status" }, t("cmp.checking")));
    try {
      const plan = await invoke("comparison_preview_stage", { ...args, id });
      if (!current() || !panel.isConnected) return;
      const heading = h("h3", { tabindex: "-1" }, t("cmp.stageTitle", { candidate: plan.candidate }));
      const cancel = h("button", { class: "btn small", onclick: () => {
        clear(panel).hidden = true;
        button.disabled = false;
        button.focus();
      } }, t("cmp.cancelStage"));
      const confirm = h("button", { class: "btn primary small", onclick: async () => {
        confirm.disabled = cancel.disabled = true;
        try {
          await invoke("comparison_stage", { ...args, id, plan });
          toast(t("cmp.staged"));
          if (current() && panel.isConnected) {
            clear(panel).append(h("p", { role: "status" }, t("cmp.stagedIn"), " ", h("code", { class: "comparison-path" }, plan.destination)));
          }
        } catch (e) {
          toast(errMsg(e), true);
          if (current() && panel.isConnected) {
            clear(panel).append(h("p", { class: "banner bad", role: "alert" }, errMsg(e)));
            button.disabled = false;
            button.focus();
          }
        }
      } }, t("cmp.stage"));
      clear(panel).append(heading,
        h("div", { class: "small" }, t("cmp.candidate"), " ", h("code", null, plan.candidate_commit.slice(0, 12))),
        h("div", { class: "small" }, t("cmp.destination", { branch: plan.destination_branch || t("cmp.detached") }), " ", h("code", null, plan.destination_commit.slice(0, 12))),
        h("code", { class: "comparison-path" }, plan.destination),
        h("details", { open: true },
          h("summary", null, t("cmp.changedFiles", { count: plan.files.length })),
          h("ul", { class: "small comparison-path" }, plan.files.map((file) => h("li", null, h("code", null, file))))),
        h("div", { class: "row" }, cancel, confirm));
      heading.focus();
    } catch (e) {
      if (current() && panel.isConnected) {
        clear(panel).append(h("p", { class: "banner bad", role: "alert" }, errMsg(e)));
        button.disabled = false;
      }
    }
  };
  const paint = async () => {
    const id = menu.value;
    const seq = ++request;
    clear(results).append(h("div", { class: "muted small" }, id ? t("cmp.reading") : t("cmp.none")));
    if (!id) return;
    try {
      const data = await invoke("comparison_results", { ...args, id });
      if (!current() || seq !== request) return;
      const stagePanel = h("section", { class: "card comparison-stage", hidden: true, "aria-label": t("cmp.stagePreview") });
      clear(results).append(
        h("div", { class: "muted small" }, t("cmp.base"), " ", h("code", null, data.comparison.base.slice(0, 12))),
        stagePanel,
        h("div", { class: "comparison-grid" }, data.comparison.candidates.map((candidate, index) => {
          const r = data.results[index];
          const selected = data.comparison.preferred === candidate.key;
          const thread = projThreads().find((t) => t.task === candidate.key && t.running && t.kind === "agent");
          const launch = async () => {
            if (!current()) return;
            if (thread) { placeThread(thread.id); setTab("workspace"); }
            else await startThread({ kind: "agent", target: candidate.target, task: candidate.key });
            closeDrawer();
            clear($("#modal-root"));
          };
          return h("article", { class: "card" },
            h("div", { class: "row" }, h("h3", null, candidate.target), h("span", { class: "key" }, candidate.key), selected ? pill("ok", t("cmp.preferred")) : null),
            h("code", { class: "comparison-path" }, candidate.branch),
            h("div", { class: "row comparison-actions" },
              h("button", { class: "btn small", disabled: !!r.error || (r.running && !thread), onclick: async (e) => {
                const button = e.currentTarget;
                button.disabled = true;
                try { await launch(); } catch { /* startThread reported the error */ }
                finally { if (button.isConnected) button.disabled = false; }
              } }, thread ? t("cmp.openThread") : r.running ? t("cmp.running") : t("cmp.startAgent")),
              h("button", { class: "btn small", onclick: () => copy(candidate.worktree, t("cmp.pathCopied")) }, t("cmp.copyPath")),
              h("button", { class: "btn small", disabled: selected || !!r.error, onclick: async (e) => {
                const button = e.currentTarget;
                button.disabled = true;
                try { await act(() => invoke("comparison_prefer", { ...args, id, candidate: candidate.key }), t("cmp.saved")); await paint(); }
                catch { if (button.isConnected) button.disabled = false; }
              } }, t("cmp.prefer")),
              selected && !r.error ? h("button", { class: "btn small", disabled: !!r.running,
                onclick: (e) => previewStage(stagePanel, e.currentTarget, id) }, t("cmp.previewStage")) : null),
            r.error ? h("div", { class: "banner bad" }, r.error) : h("div", { class: "stack" },
              (() => { const n = diffStats(r.diff); return h("div", { class: "small muted comparison-stats" },
                r.diff ? `${n.files ? t("cmp.files", { count: n.files }) + " · " : ""}+${n.added} −${n.removed}` : t("cmp.noChangeBase")); })(),
              h("div", { class: "row" }, pill(r.status), r.verification
                ? pill(r.verification.passed ? "ok" : "fail", r.verification.passed ? t("cmp.checkPassed") : t("cmp.checkFailed"))
                : h("span", { class: "muted small" }, t("cmp.noVerify")),
                r.verification && r.verification_current === false ? pill("warn", t("cmp.earlier")) : null,
                r.redactions ? pill("warn", t("cmp.redactions", { count: r.redactions })) : null),
              r.verification ? h("code", { class: "comparison-path" }, r.verification.command.join(" ")) : null,
              r.diff ? h("pre", { class: "comparison-diff", tabindex: "0", "aria-label": t("cmp.changesFor", { target: candidate.target }) }, r.diff)
                : h("div", { class: "muted small" }, t("cmp.noChangeBase.dot"))));
        })));
    } catch (e) {
      if (current() && seq === request) clear(results).append(h("div", { class: "banner bad" }, errMsg(e)));
    }
  };
  const runChecks = h("button", { class: "btn small", onclick: async () => {
    const id = menu.value;
    if (!id) return;
    const run = (await invoke("comparison_list", args)).find((r) => r.id === id);
    if (!run) return;
    runChecks.disabled = true;
    const status = h("span", { class: "small muted", role: "status" });
    runChecks.after(status);
    try {
      for (const [index, candidate] of run.candidates.entries()) {
        if (!current()) return;
        status.textContent = t("cmp.running.tests", { target: candidate.target, n: index + 1 });
        try { await invoke("run_verification", { projectId, task: candidate.key, kind: "test", index: 0 }); }
        catch (e) { toast(`${candidate.target}: ${errMsg(e)}`, true); }
      }
      if (current()) await paint();
    } finally { status.remove(); runChecks.disabled = false; }
  } }, t("cmp.runBoth"));
  const load = async (id) => {
    try {
      const runs = await invoke("comparison_list", args);
      if (!current()) return;
      clear(menu).append(...runs.map((run) => h("option", { value: run.id }, run.candidates.map((c) => `${c.target} (${c.key})`).join(t("cmp.and")))));
      menu.disabled = !runs.length;
      if (id && runs.some((r) => r.id === id)) menu.value = id;
      await paint();
    } catch (e) { if (current()) clear(results).append(h("div", { class: "banner bad" }, errMsg(e))); }
  };
  menu.addEventListener("change", paint);
  const prepare = h("button", { class: "btn primary", disabled: !first.value || !second.value, onclick: async () => {
    if (first.value === second.value) { toast(t("cmp.different"), true); return; }
    prepare.disabled = true;
    try {
      const run = await act(() => invoke("comparison_create", { ...args, targets: [first.value, second.value] }));
      if (current()) await load(run.id);
    } catch { /* act reported the error */ }
    finally { prepare.disabled = false; }
  } }, t("cmp.prepare"));
  body.append(h("p", { class: "small muted" }, t("cmp.intro")),
    h("div", { class: "row comparison-controls" }, field(t("cmp.first"), first), field(t("cmp.second"), second), prepare),
    h("div", { class: "row comparison-controls" }, field(t("cmp.comparison"), menu), h("button", { class: "btn small", onclick: () => load(menu.value) }, t("cmp.refresh")), runChecks), results);
  modal(t("cmp.title", { task: task.key }), [body], []);
  body.closest(".modal").classList.add("comparison-modal");
  await load();
}
