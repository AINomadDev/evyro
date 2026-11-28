// Settings card: finished worktrees Evyro made, their sizes, a Prune button, and the
// automatic daily prune.
"use strict";

/** A byte count in the reader's language, e.g. "1.4 GB". */
function fmtBytes(n) {
  const units = ["byte", "kilobyte", "megabyte", "gigabyte", "terabyte"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return new Intl.NumberFormat(document.documentElement.lang || undefined, { style: "unit", unit: units[i], unitDisplay: "short", maximumFractionDigits: 1 }).format(n);
}

function worktreesCard() {
  const card = h("div", { class: "card" }, h("h2", null, I18N.t("app.wt.title")), h("div", { class: "muted" }, I18N.t("app.loading")));
  const draw = async () => {
    let st;
    try { st = await invoke("worktree_prune_status"); }
    catch (e) { clear(card).append(h("h2", null, I18N.t("app.wt.title")), h("div", { class: "banner bad" }, errMsg(e))); return; }
    const items = st.report.projects.flatMap((p) => p.items.map((i) => ({ ...i, project: p.project })));
    const going = items.filter((i) => i.reason.startsWith("would"));
    const kept = items.filter((i) => i.reason.startsWith("kept"));
    const auto = h("input", { type: "checkbox", checked: st.settings.auto });
    const days = h("input", { type: "number", id: "wt-idle-days", min: "1", max: String(st.max_idle_days), step: "1", value: String(st.settings.idle_days) });
    const save = async () => {
      const d = Number(days.value);
      if (!Number.isInteger(d) || d < 1 || d > st.max_idle_days) { toast(I18N.t("app.wt.range", { max: st.max_idle_days }), true); return; }
      await act(() => invoke("worktree_prune_set", { auto: auto.checked, idleDays: d }), I18N.t("app.hist.saved"));
      draw();
    };
    const prune = async () => {
      if (!(await confirmDialog(I18N.t("app.wt.confirm", { count: going.length }),
        I18N.t("app.wt.confirmText", { size: fmtBytes(st.report.bytes) }), I18N.t("app.wt.prune")))) return;
      const r = await act(() => invoke("worktree_prune_run"));
      toast(I18N.t("app.wt.freed", { size: fmtBytes(r.bytes) }));
      draw();
    };
    const row = (i, extra) => h("li", null, h("div", null, h("div", null, h("b", null, i.task), " · ", i.project),
      h("div", { class: "muted small mono" }, i.path), h("div", { class: "muted small" }, extra)));
    clear(card).append(
      h("h2", null, I18N.t("app.wt.title")),
      h("div", { class: "muted small", style: "margin-bottom:10px" }, I18N.t("app.wt.intro")),
      going.length
        ? h("ul", { class: "list" }, going.map((i) => row(i, fmtBytes(i.bytes) + " · " + i.reason.replace(/^would remove: /, ""))))
        : h("div", { class: "muted" }, I18N.t("app.wt.none")),
      h("div", { class: "row", style: "margin-top:10px" },
        h("button", { class: "btn small danger", disabled: !going.length, onclick: prune },
          going.length ? I18N.t("app.wt.pruneSize", { size: fmtBytes(st.report.bytes) }) : I18N.t("app.wt.prune"))),
      kept.length ? h("details", { style: "margin-top:10px" }, h("summary", null, I18N.t("app.wt.kept", { count: kept.length })),
        h("ul", { class: "list" }, kept.map((i) => row(i, i.reason.replace(/^kept: /, ""))))) : null,
      h("label", { class: "check", style: "margin-top:12px" }, auto, I18N.t("app.wt.auto")),
      field(I18N.t("app.wt.idleDays"), days),
      h("div", { class: "row" }, h("button", { class: "btn primary", onclick: save }, I18N.t("app.wt.save"))));
  };
  draw();
  return card;
}
