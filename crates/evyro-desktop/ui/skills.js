// Skills screen: reusable instructions that agents load when a task needs them.
// Lists every skill (built in, global, and this project's), shows its text,
// turns it on or off here or everywhere, and writes skill references into an
// agent's own instruction files without overwriting what someone wrote.

const SOURCE_LABEL = { builtin: "Built in", global: "Yours (all projects)", project: "This project" };

async function viewSkills(v) {
  const pid = state.project.id;
  const card = h("div", { class: "card" });
  const q = h("input", { type: "text", placeholder: "Filter skills, or describe a task to see which fit" });
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
        `${on} of ${data.skills.length} on. Agents see only the names and one-line descriptions, and load a skill's text when the task needs it.`),
      shown.length ? h("ul", { class: "list skills-list" }, shown.map(skillRow)) : h("div", { class: "empty" }, "No skill matches."));
  };

  const skillRow = (k) => {
    const st = stateOf(k);
    const toggle = h("button", {
      class: "btn small" + (st === "on" ? "" : " primary"),
      disabled: st === "problem",
      title: st === "problem" ? k.problems.join("; ") : "",
      onclick: (ev) => { ev.stopPropagation(); toggleDialog(k); },
    }, st === "on" ? "Turn off" : "Turn on");
    return h("li", { class: "skill-row", ...opens(() => openSkill(k.name)) },
      h("div", { class: "skill-main" },
        h("div", { class: "row" },
          h("strong", null, k.name),
          h("span", { class: "pill " + (st === "on" ? "ok" : st === "problem" ? "bad" : "") }, st),
          h("span", { class: "pill" }, SOURCE_LABEL[k.source] || k.source),
          k.overrides ? h("span", { class: "pill accent", title: `Replaces the ${k.overrides} skill with the same name` }, "replaces " + k.overrides) : null,
          k.enabled_by !== "default" ? h("span", { class: "muted small" }, k.enabled_by === "global" ? "set for every project" : "set for this project") : null,
          h("span", { class: "spacer" }),
          h("span", { class: "muted small" }, "v" + k.version)),
        h("div", { class: "small" }, k.description || "(no description)"),
        k.problems.length ? h("div", { class: "small bad-text" }, k.problems.join("; ")) : null),
      toggle);
  };

  const toggleDialog = (k) => {
    const turnOn = !k.enabled;
    const where = h("select", null,
      h("option", { value: "project" }, "In this project only"),
      h("option", { value: "global" }, "In every project"));
    modal(`${turnOn ? "Turn on" : "Turn off"} ${k.name}`, [field("Where", where, turnOn ? "Agents will see this skill in the list and can load it." : "Agents will no longer see this skill.")], [{
      label: turnOn ? "Turn on" : "Turn off", primary: true, run: async () => {
        await act(() => invoke("set_skill_enabled", { projectId: pid, name: k.name, enabled: turnOn, global: where.value === "global" }), turnOn ? "Turned on" : "Turned off");
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
        h("span", { class: "pill" }, SOURCE_LABEL[k.source] || k.source), h("span", { class: "pill" }, "v" + k.version),
        k.tags.slice(0, 6).map((t) => h("span", { class: "pill accent" }, t))),
      h("p", { class: "small" }, k.description),
      k.path ? h("div", { class: "muted small", style: "margin-bottom:8px" }, k.path) : null,
      k.problems.length ? h("div", { class: "bad-text small" }, "Cannot be used: " + k.problems.join("; ")) : h("pre", { class: "skill-text" }, r.text),
    ], []);
  };

  const newDialog = () => {
    const name = h("input", { type: "text", placeholder: "api-conventions" });
    const desc = h("input", { type: "text", placeholder: "Use when adding or changing an HTTP endpoint" });
    const where = h("select", null,
      h("option", { value: "project" }, "This project (.evyro/skills, kept with the code)"),
      h("option", { value: "global" }, "All my projects"));
    modal("New skill", [
      field("Name", name, "Lowercase letters, digits and hyphens."),
      field("When to use it", desc, "Agents decide from this line whether to load the skill."),
      field("Where", where)], [{
      label: "Create", primary: true, run: async () => {
        if (!name.value.trim()) { toast("Give the skill a name", true); return false; }
        const r = await act(() => invoke("new_skill", { projectId: pid, name: name.value.trim(), description: desc.value.trim(), global: where.value === "global" }), "Created. Edit the file to write the steps.");
        if (r && r.path) copy(r.path, "Path copied");
        load();
      },
    }]);
  };

  const importDialog = async () => {
    const path = h("input", { type: "text", placeholder: "Folder with SKILL.md, or a folder of skill folders" });
    const browse = h("button", { class: "btn small", onclick: async () => { const p = await invoke("pick_skill_folder").catch(() => null); if (p) path.value = p; } }, "Choose…");
    const where = h("select", null, h("option", { value: "project" }, "This project"), h("option", { value: "global" }, "All my projects"));
    modal("Import skills", [
      field("From", h("div", { class: "row" }, path, browse), "Only SKILL.md is copied. Skills that hold secrets or have problems are skipped, and an installed version that is the same or newer is kept."),
      field("Into", where)], [{
      label: "Import", primary: true, run: async () => {
        if (!path.value.trim()) { toast("Choose a folder", true); return false; }
        const r = await invoke("import_skills", { projectId: pid, path: path.value.trim(), global: where.value === "global", force: false }).catch((e) => { toast(errMsg(e), true); return null; });
        if (!r) return false;
        const lines = [...r.imported.map((i) => `Imported ${i.name} ${i.version}${i.replaced ? ` (replaced ${i.replaced})` : ""}`), ...r.skipped.map((s) => `Skipped ${s.name}: ${s.reason}`)];
        toast(lines.join("\n") || "Nothing to import", !r.imported.length);
        load();
      },
    }]);
  };

  const syncDialog = () => {
    const target = h("select", null, data.sync_targets.map((t) => h("option", { value: t.harness }, `${t.harness}  →  ${t.file}`)));
    const append = h("input", { type: "checkbox" });
    const out = h("div");
    modal("Add skills to an agent's own files", [
      h("p", { class: "small" }, "For agents that read their own instruction files. Claude Code gets one file per skill. Other agents get a short list of skills, between Evyro's markers, in the file they read."),
      field("Agent", target),
      h("label", { class: "check small" }, append, " If the file already has instructions, add the list at its end"),
      h("div", { class: "hint" }, "Evyro never replaces a file or text someone wrote. Files it wrote before are updated."),
      out], [{
      label: "Write files", primary: true, run: async () => {
        const r = await invoke("sync_skills", { projectId: pid, harness: target.value, append: append.checked }).catch((e) => { toast(errMsg(e), true); return null; });
        if (!r) return false;
        clear(out).append(h("ul", { class: "list small" },
          r.written.map((w) => h("li", null, "Wrote " + w)),
          r.removed.map((w) => h("li", null, "Removed " + w + " (the skill is off)")),
          r.unchanged.length ? h("li", null, `${r.unchanged.length} file(s) already up to date`) : null,
          r.skipped.map((s) => h("li", { class: "bad-text" }, `Skipped ${s.name}: ${s.reason}`))));
        return false;
      },
    }]);
  };

  q.addEventListener("input", () => { clearTimeout(q._t); q._t = setTimeout(draw, 150); });
  card.append(
    h("div", { class: "card-head" }, h("h2", null, "Skills"),
      h("span", { class: "spacer" }),
      h("button", { class: "btn small", onclick: syncDialog }, "Add to agent files…"),
      h("button", { class: "btn small", onclick: importDialog }, "Import…"),
      h("button", { class: "btn small primary", onclick: newDialog }, "+ New skill")),
    q, listEl);
  await load();
}
