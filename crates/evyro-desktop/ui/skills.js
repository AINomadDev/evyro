// Skills screen: reusable instructions that agents load when a task needs them.
// Lists every skill (built in, global, and this project's), shows its text,
// turns it on or off here or everywhere, and writes skill references into an
// agent's own instruction files without overwriting what someone wrote.

const sourceLabel = (src) => ({ builtin: t("skills.src.builtin"), global: t("skills.src.global"), project: t("skills.src.project") })[src] || src;

async function viewSkills(v) {
  const pid = state.project.id;
  const card = h("div", { class: "card" });
  const q = h("input", { type: "text", placeholder: t("skills.filter") });
  const listEl = h("div");
  v.append(card);
  let data = { skills: [], sync_targets: [] };

  const load = async () => {
    data = await invoke("list_skills", { projectId: pid }).catch((e) => { toast(errMsg(e), true); return data; });
    draw();
  };

  const stateOf = (k) => (k.problems.length ? "problem" : k.enabled ? "on" : "off");

  const draw = () => {
    const words = q.value.toLowerCase().split(/[^a-z0-9_]+/).filter((w) => w.length >= 3);
    const shown = data.skills.filter((k) => {
      if (!words.length) return true;
      const hay = (k.name + " " + k.tags.join(" ") + " " + k.description).toLowerCase();
      return words.some((w) => hay.includes(w));
    });
    const on = data.skills.filter((k) => stateOf(k) === "on").length;
    clear(listEl).append(
      h("div", { class: "muted small", style: "margin:6px 0 10px" },
        t("skills.summary", { on, total: data.skills.length })),
      shown.length ? h("ul", { class: "list skills-list" }, shown.map(skillRow)) : h("div", { class: "empty" }, t("skills.none")));
  };

  const skillRow = (k) => {
    const st = stateOf(k);
    const toggle = h("button", {
      class: "btn small" + (st === "on" ? "" : " primary"),
      disabled: st === "problem",
      title: st === "problem" ? k.problems.join("; ") : "",
      onclick: (ev) => { ev.stopPropagation(); toggleDialog(k); },
    }, st === "on" ? t("skills.turnOff") : t("skills.turnOn"));
    return h("li", { class: "skill-row", ...opens(() => openSkill(k.name)) },
      h("div", { class: "skill-main" },
        h("div", { class: "row" },
          h("strong", null, k.name),
          h("span", { class: "pill " + (st === "on" ? "ok" : st === "problem" ? "bad" : "") }, st === "on" ? t("skills.st.on") : st === "problem" ? t("skills.st.problem") : t("skills.st.off")),
          h("span", { class: "pill" }, sourceLabel(k.source)),
          k.overrides ? h("span", { class: "pill accent", title: t("skills.replaces.title", { name: k.overrides }) }, t("skills.replaces", { name: k.overrides })) : null,
          k.enabled_by !== "default" ? h("span", { class: "muted small" }, k.enabled_by === "global" ? t("skills.setGlobal") : t("skills.setProject")) : null,
          h("span", { class: "spacer" }),
          h("span", { class: "muted small" }, "v" + k.version)),
        h("div", { class: "small" }, k.description || t("skills.noDesc")),
        k.problems.length ? h("div", { class: "small bad-text" }, k.problems.join("; ")) : null),
      toggle);
  };

  const toggleDialog = (k) => {
    const turnOn = !k.enabled;
    const where = h("select", null,
      h("option", { value: "project" }, t("skills.where.project")),
      h("option", { value: "global" }, t("skills.where.global")));
    modal(turnOn ? t("skills.turnOnTitle", { name: k.name }) : t("skills.turnOffTitle", { name: k.name }), [field(t("skills.where"), where, turnOn ? t("skills.onHint") : t("skills.offHint"))], [{
      label: turnOn ? t("skills.turnOn") : t("skills.turnOff"), primary: true, run: async () => {
        await act(() => invoke("set_skill_enabled", { projectId: pid, name: k.name, enabled: turnOn, global: where.value === "global" }), turnOn ? t("skills.turnedOn") : t("skills.turnedOff"));
        load();
      },
    }]);
  };

  const openSkill = async (name) => {
    const r = await invoke("skill_text", { projectId: pid, name }).catch((e) => { toast(errMsg(e), true); return null; });
    if (!r) return;
    const k = r.skill;
    modal(k.name, [
      h("div", { class: "row", style: "margin-bottom:8px" },
        h("span", { class: "pill" }, sourceLabel(k.source)), h("span", { class: "pill" }, "v" + k.version),
        k.tags.slice(0, 6).map((t) => h("span", { class: "pill accent" }, t))),
      h("p", { class: "small" }, k.description),
      k.path ? h("div", { class: "muted small", style: "margin-bottom:8px" }, k.path) : null,
      k.problems.length ? h("div", { class: "bad-text small" }, t("skills.cannotUse", { problems: k.problems.join("; ") })) : h("pre", { class: "skill-text" }, r.text),
    ], []);
  };

  const newDialog = () => {
    const name = h("input", { type: "text", placeholder: "api-conventions" });
    const desc = h("input", { type: "text", placeholder: "Use when adding or changing an HTTP endpoint" });
    const where = h("select", null,
      h("option", { value: "project" }, t("skills.new.project")),
      h("option", { value: "global" }, t("skills.new.global")));
    modal(t("skills.new.title"), [
      field(t("skills.new.name"), name, t("skills.new.nameHint")),
      field(t("skills.new.when"), desc, t("skills.new.whenHint")),
      field(t("skills.where"), where)], [{
      label: t("skills.create"), primary: true, run: async () => {
        if (!name.value.trim()) { toast(t("skills.new.needName"), true); return false; }
        const r = await act(() => invoke("new_skill", { projectId: pid, name: name.value.trim(), description: desc.value.trim(), global: where.value === "global" }), t("skills.created"));
        if (r && r.path) copy(r.path, t("skills.pathCopied"));
        load();
      },
    }]);
  };

  const importDialog = async () => {
    const path = h("input", { type: "text", placeholder: t("skills.imp.ph") });
    const browse = h("button", { class: "btn small", onclick: async () => { const p = await invoke("pick_skill_folder").catch(() => null); if (p) path.value = p; } }, t("skills.imp.choose"));
    const where = h("select", null, h("option", { value: "project" }, t("skills.imp.project")), h("option", { value: "global" }, t("skills.imp.global")));
    modal(t("skills.imp.title"), [
      field(t("skills.imp.from"), h("div", { class: "row" }, path, browse), t("skills.imp.fromHint")),
      field(t("skills.imp.into"), where)], [{
      label: t("skills.imp.do"), primary: true, run: async () => {
        if (!path.value.trim()) { toast(t("skills.imp.need"), true); return false; }
        const r = await invoke("import_skills", { projectId: pid, path: path.value.trim(), global: where.value === "global", force: false }).catch((e) => { toast(errMsg(e), true); return null; });
        if (!r) return false;
        const lines = [...r.imported.map((i) => (i.replaced ? t("skills.imp.replaced", { name: i.name, version: i.version, old: i.replaced }) : t("skills.imp.done", { name: i.name, version: i.version }))), ...r.skipped.map((s) => t("skills.skipped", { name: s.name, reason: s.reason }))];
        toast(lines.join("\n") || t("skills.imp.nothing"), !r.imported.length);
        load();
      },
    }]);
  };

  const syncDialog = () => {
    const target = h("select", null, data.sync_targets.map((t) => h("option", { value: t.harness }, `${t.harness}  →  ${t.file}`)));
    const append = h("input", { type: "checkbox" });
    const out = h("div");
    modal(t("skills.sync.title"), [
      h("p", { class: "small" }, t("skills.sync.intro")),
      field(t("skills.sync.agent"), target),
      h("label", { class: "check small" }, append, " " + t("skills.sync.append")),
      h("div", { class: "hint" }, t("skills.sync.hint")),
      out], [{
      label: t("skills.sync.write"), primary: true, run: async () => {
        const r = await invoke("sync_skills", { projectId: pid, harness: target.value, append: append.checked }).catch((e) => { toast(errMsg(e), true); return null; });
        if (!r) return false;
        clear(out).append(h("ul", { class: "list small" },
          r.written.map((w) => h("li", null, t("skills.sync.wrote", { file: w }))),
          r.removed.map((w) => h("li", null, t("skills.sync.removed", { file: w }))),
          r.unchanged.length ? h("li", null, t("skills.sync.upToDate", { count: r.unchanged.length })) : null,
          r.skipped.map((s) => h("li", { class: "bad-text" }, t("skills.skipped", { name: s.name, reason: s.reason })))));
        return false;
      },
    }]);
  };

  q.addEventListener("input", () => { clearTimeout(q._t); q._t = setTimeout(draw, 150); });
  card.append(
    h("div", { class: "card-head" }, h("h2", null, t("skills.title")),
      h("span", { class: "spacer" }),
      h("button", { class: "btn small", onclick: syncDialog }, t("skills.sync.btn")),
      h("button", { class: "btn small", onclick: importDialog }, t("skills.imp.btn")),
      h("button", { class: "btn small primary", onclick: newDialog }, t("skills.new.btn"))),
    q, listEl);
  await load();
}
