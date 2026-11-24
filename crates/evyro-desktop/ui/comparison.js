// Compare two implementations from a common committed base.
"use strict";

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
    clear(panel).append(h("p", { class: "small muted", role: "status" }, "Checking the destination checkout…"));
    try {
      const plan = await invoke("comparison_preview_stage", { ...args, id });
      if (!current() || !panel.isConnected) return;
      const heading = h("h3", { tabindex: "-1" }, `Stage changes from ${plan.candidate}?`);
      const cancel = h("button", { class: "btn small", onclick: () => {
        clear(panel).hidden = true;
        button.disabled = false;
        button.focus();
      } }, "Cancel staging");
      const confirm = h("button", { class: "btn primary small", onclick: async () => {
        confirm.disabled = cancel.disabled = true;
        try {
          await invoke("comparison_stage", { ...args, id, plan });
          toast("Preferred changes staged");
          if (current() && panel.isConnected) {
            clear(panel).append(h("p", { role: "status" }, "Changes staged in ", h("code", { class: "comparison-path" }, plan.destination)));
          }
        } catch (e) {
          toast(errMsg(e), true);
          if (current() && panel.isConnected) {
            clear(panel).append(h("p", { class: "banner bad", role: "alert" }, errMsg(e)));
            button.disabled = false;
            button.focus();
          }
        }
      } }, "Stage changes");
      clear(panel).append(heading,
        h("div", { class: "small" }, "Candidate commit ", h("code", null, plan.candidate_commit.slice(0, 12))),
        h("div", { class: "small" }, `Destination ${plan.destination_branch || "detached HEAD"} at `, h("code", null, plan.destination_commit.slice(0, 12))),
        h("code", { class: "comparison-path" }, plan.destination),
        h("details", { open: true },
          h("summary", null, `${plan.files.length} changed ${plan.files.length === 1 ? "file" : "files"}`),
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
    clear(results).append(h("div", { class: "muted small" }, id ? "Reading worktrees…" : "No comparisons yet."));
    if (!id) return;
    try {
      const data = await invoke("comparison_results", { ...args, id });
      if (!current() || seq !== request) return;
      const stagePanel = h("section", { class: "card comparison-stage", hidden: true, "aria-label": "Staging preview" });
      clear(results).append(
        h("div", { class: "muted small" }, "Shared base ", h("code", null, data.comparison.base.slice(0, 12))),
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
            h("div", { class: "row" }, h("h3", null, candidate.target), h("span", { class: "key" }, candidate.key), selected ? pill("ok", "Preferred") : null),
            h("code", { class: "comparison-path" }, candidate.branch),
            h("div", { class: "row comparison-actions" },
              h("button", { class: "btn small", disabled: !!r.error || (r.running && !thread), onclick: async (e) => {
                const button = e.currentTarget;
                button.disabled = true;
                try { await launch(); } catch { /* startThread reported the error */ }
                finally { if (button.isConnected) button.disabled = false; }
              } }, thread ? "Open thread" : r.running ? "Agent running" : "Start agent"),
              h("button", { class: "btn small", onclick: () => copy(candidate.worktree, "Worktree path copied") }, "Copy worktree path"),
              h("button", { class: "btn small", disabled: selected || !!r.error, onclick: async (e) => {
                const button = e.currentTarget;
                button.disabled = true;
                try { await act(() => invoke("comparison_prefer", { ...args, id, candidate: candidate.key }), "Preference saved"); await paint(); }
                catch { if (button.isConnected) button.disabled = false; }
              } }, "Prefer this result"),
              selected && !r.error ? h("button", { class: "btn small", disabled: !!r.running,
                onclick: (e) => previewStage(stagePanel, e.currentTarget, id) }, "Preview staging") : null),
            r.error ? h("div", { class: "banner bad" }, r.error) : h("div", { class: "stack" },
              h("div", { class: "row" }, pill(r.status), r.verification
                ? pill(r.verification.passed ? "ok" : "fail", r.verification.passed ? "Last check passed" : "Last check failed")
                : h("span", { class: "muted small" }, "No verification yet"),
                r.verification && r.verification_current === false ? pill("warn", "Earlier snapshot") : null,
                r.redactions ? pill("warn", `${r.redactions} redactions`) : null),
              r.verification ? h("code", { class: "comparison-path" }, r.verification.command.join(" ")) : null,
              r.diff ? h("pre", { class: "comparison-diff", tabindex: "0", "aria-label": `Changes for ${candidate.target}` }, r.diff)
                : h("div", { class: "muted small" }, "No changes from the shared base.")));
        })));
    } catch (e) {
      if (current() && seq === request) clear(results).append(h("div", { class: "banner bad" }, errMsg(e)));
    }
  };
  const load = async (id) => {
    try {
      const runs = await invoke("comparison_list", args);
      if (!current()) return;
      clear(menu).append(...runs.map((run) => h("option", { value: run.id }, run.candidates.map((c) => `${c.target} (${c.key})`).join(" and "))));
      menu.disabled = !runs.length;
      if (id && runs.some((r) => r.id === id)) menu.value = id;
      await paint();
    } catch (e) { if (current()) clear(results).append(h("div", { class: "banner bad" }, errMsg(e))); }
  };
  menu.addEventListener("change", paint);
  const prepare = h("button", { class: "btn primary", disabled: !first.value || !second.value, onclick: async () => {
    if (first.value === second.value) { toast("Choose two different agent targets.", true); return; }
    prepare.disabled = true;
    try {
      const run = await act(() => invoke("comparison_create", { ...args, targets: [first.value, second.value] }));
      if (current()) await load(run.id);
    } catch { /* act reported the error */ }
    finally { prepare.disabled = false; }
  } }, "Prepare comparison");
  body.append(h("p", { class: "small muted" }, "Each agent gets its own worktree at the same commit. Uncommitted edits stay in the original checkout. Preferences are recorded on this task."),
    h("div", { class: "row comparison-controls" }, field("First agent", first), field("Second agent", second), prepare),
    h("div", { class: "row comparison-controls" }, field("Comparison", menu), h("button", { class: "btn small", onclick: () => load(menu.value) }, "Refresh results")), results);
  modal(`Compare agents on ${task.key}`, [body], []);
  body.closest(".modal").classList.add("comparison-modal");
  await load();
}
