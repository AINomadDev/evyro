// Appearance: theme, accent color, typography, density and layout.
// Saved on this computer (browser storage of the app window) and applied at once.
// Loaded right after app.js, before anything draws.
"use strict";

const APPEARANCE_DEFAULTS = {
  theme: "system", // system | light | dark | midnight | paper
  accent: "indigo", // a name from ACCENTS, or "custom"
  customAccent: "#0e7490",
  uiFont: "system", // a key of UI_FONTS, or "custom"
  customUiFont: "",
  monoFont: "jetbrains", // a key of MONO_FONTS, or "custom"
  customMonoFont: "",
  uiSize: 14, // px, base text size
  termSize: 13, // px, terminal text size
  density: "comfortable", // comfortable | compact
  radius: 10, // px, corner radius of cards and panes
  panes: 2, // initial pane count for a project without its own layout
  inspector: true, // shared context open on start
  inspectorSide: "right", // right | left
  sidebarWidth: 232,
  inspectorWidth: 320,
  drawerHeight: 220,
  statusBar: true,
  grouping: "state", // state: Needs you / Working / Your turn / Ended; time: newest first
};

const THEMES = [
  ["system", "System", null],
  ["light", "Light", "light"],
  ["dark", "Dark", "dark"],
  ["midnight", "Midnight", "dark"],
  ["paper", "Paper", "light"],
];
// Each accent has a light-scheme and a dark-scheme shade, both readable as text (WCAG AA) on panels.
const ACCENTS = {
  indigo: ["Relay indigo", "#4f46e5", "#8b85ff"],
  cyan: ["Signal cyan", "#0e7490", "#22d3ee"],
  emerald: ["Emerald", "#047857", "#34d399"],
  amber: ["Amber", "#9a5b00", "#fbbf24"],
  rose: ["Rose", "#be123c", "#fb7185"],
  violet: ["Violet", "#7c3aed", "#b69cff"],
  slate: ["Graphite", "#3f4a5a", "#c3ccd9"],
};
const UI_FONTS = {
  system: ["System", '-apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Roboto, sans-serif'],
  inter: ["Inter", 'Inter, "Inter Display", -apple-system, "Segoe UI", Roboto, sans-serif'],
  hyperlegible: ["Atkinson Hyperlegible", '"Atkinson Hyperlegible", "Atkinson Hyperlegible Next", -apple-system, "Segoe UI", sans-serif'],
  rounded: ["Rounded", 'ui-rounded, "SF Pro Rounded", "Nunito", -apple-system, sans-serif'],
  serif: ["Serif", 'ui-serif, "New York", Georgia, "Times New Roman", serif'],
};
const MONO_FONTS = {
  jetbrains: ["JetBrains Mono", '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace'],
  system: ["System mono", 'ui-monospace, "SF Mono", Menlo, Consolas, monospace'],
  fira: ["Fira Code", '"Fira Code", ui-monospace, Menlo, Consolas, monospace'],
  cascadia: ["Cascadia Code", '"Cascadia Code", "Cascadia Mono", Consolas, ui-monospace, monospace'],
  iosevka: ["Iosevka", 'Iosevka, "Iosevka Term", ui-monospace, Menlo, monospace'],
};

let APPEARANCE = null;
function appearance() {
  if (!APPEARANCE) {
    let saved = {};
    try { saved = JSON.parse(localStorage.getItem("evyro.appearance") || "{}") || {}; } catch { saved = {}; }
    APPEARANCE = { ...APPEARANCE_DEFAULTS, ...cleanAppearance(saved) };
  }
  return APPEARANCE;
}

/** Keep only known keys with values of the right kind, so a bad paste cannot break the window. */
function cleanAppearance(o) {
  const out = {};
  if (!o || typeof o !== "object") return out;
  const num = (k, lo, hi) => { const n = Number(o[k]); if (Number.isFinite(n)) out[k] = Math.min(hi, Math.max(lo, Math.round(n))); };
  const one = (k, list) => { if (list.includes(o[k])) out[k] = o[k]; };
  const bool = (k) => { if (typeof o[k] === "boolean") out[k] = o[k]; };
  const text = (k, re) => { if (typeof o[k] === "string" && re.test(o[k])) out[k] = o[k]; };
  one("theme", THEMES.map((t) => t[0]));
  one("accent", [...Object.keys(ACCENTS), "custom"]);
  text("customAccent", /^#[0-9a-fA-F]{6}$/);
  one("uiFont", [...Object.keys(UI_FONTS), "custom"]);
  one("monoFont", [...Object.keys(MONO_FONTS), "custom"]);
  // Font names only: letters, digits, spaces, quotes, commas, dashes. Nothing that could end the CSS value.
  text("customUiFont", /^[\w\s"',.-]{0,120}$/);
  text("customMonoFont", /^[\w\s"',.-]{0,120}$/);
  num("uiSize", 12, 18); num("termSize", 10, 20); num("radius", 0, 16);
  num("sidebarWidth", 180, 360); num("inspectorWidth", 260, 520); num("drawerHeight", 120, 480);
  one("density", ["comfortable", "compact"]);
  num("panes", 1, 24);
  bool("inspector"); bool("statusBar");
  one("inspectorSide", ["right", "left"]);
  one("grouping", ["state", "time"]);
  return out;
}

function saveAppearance(patch) {
  APPEARANCE = { ...appearance(), ...cleanAppearance(patch) };
  try { localStorage.setItem("evyro.appearance", JSON.stringify(APPEARANCE)); } catch { /* storage unavailable: applies until restart */ }
  applyAppearance();
}

const systemDark = matchMedia("(prefers-color-scheme: dark)");
/** "light" or "dark": the scheme the current theme uses. */
function colorScheme() {
  const t = THEMES.find((x) => x[0] === appearance().theme);
  return (t && t[2]) || (systemDark.matches ? "dark" : "light");
}

function hexLum(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
const contrast = (a, b) => { const [x, y] = [hexLum(a), hexLum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };

/** The accent color for the current scheme. */
function accentColor() {
  const a = appearance();
  if (a.accent === "custom") return a.customAccent;
  const pair = ACCENTS[a.accent] || ACCENTS.indigo;
  return colorScheme() === "dark" ? pair[2] : pair[1];
}

function applyAppearance() {
  const a = appearance();
  const root = document.documentElement;
  const scheme = colorScheme();
  const theme = a.theme === "system" ? scheme : a.theme;
  root.dataset.theme = theme;
  root.dataset.scheme = scheme;
  root.dataset.density = a.density;
  root.dataset.inspector = a.inspectorSide;
  const acc = accentColor();
  const st = root.style;
  st.setProperty("--accent", acc);
  // Text on an accent button: white or near-black, whichever reads better.
  st.setProperty("--on-accent", contrast(acc, "#ffffff") >= contrast(acc, "#111318") ? "#ffffff" : "#111318");
  st.setProperty("--accent-soft", `color-mix(in srgb, ${acc} ${scheme === "dark" ? 15 : 12}%, var(--panel))`);
  const font = (key, custom, table) => (key === "custom" && custom.trim() ? `${custom}, ${table.system[1]}` : (table[key] || table.system)[1]);
  st.setProperty("--ui-font", font(a.uiFont, a.customUiFont, UI_FONTS));
  st.setProperty("--mono", font(a.monoFont, a.customMonoFont, MONO_FONTS));
  st.setProperty("--ui-size", a.uiSize + "px");
  st.setProperty("--radius", a.radius + "px");
  st.setProperty("--sidebar-w", a.sidebarWidth + "px");
  st.setProperty("--inspector-w", a.inspectorWidth + "px");
  // Terminals follow the theme and the terminal font.
  if (typeof XTERMS !== "undefined") {
    for (const [id, x] of XTERMS) {
      x.term.options.theme = termTheme();
      x.term.options.fontSize = a.termSize;
      x.term.options.fontFamily = getComputedStyle(root).getPropertyValue("--mono").trim();
      fitThread(id);
    }
  }
  if (typeof drawStatusBar === "function") drawStatusBar();
}
systemDark.addEventListener("change", () => { if (appearance().theme === "system") applyAppearance(); });
applyAppearance();

// ---------- the Settings card ----------
function appearanceCard() {
  const card = h("div", { class: "card wide appearance" });
  const draw = () => {
    const a = appearance();
    const set = (patch) => { saveAppearance(patch); draw(); };
    const seg = (label, key, opts) => h("div", { class: "ap-row" }, h("span", { class: "ap-label", id: `ap-${key}` }, label),
      h("div", { class: "seg", role: "group", "aria-labelledby": `ap-${key}` }, opts.map(([v, l]) => h("button", {
        class: "seg-btn" + (a[key] === v ? " active" : ""), "aria-pressed": String(a[key] === v), onclick: () => set({ [key]: v }),
      }, l))));
    const range = (label, key, lo, hi, unit = "px") => {
      const out = h("span", { class: "muted small ap-val" }, `${a[key]}${unit}`);
      const input = h("input", { type: "range", min: String(lo), max: String(hi), value: String(a[key]), "aria-label": label });
      input.addEventListener("input", () => { out.textContent = `${input.value}${unit}`; saveAppearance({ [key]: Number(input.value) }); });
      return h("label", { class: "ap-row" }, h("span", { class: "ap-label" }, label), input, out);
    };
    const select = (label, key, opts) => {
      const s = h("select", { "aria-label": label }, opts.map(([v, l]) => h("option", { value: String(v), selected: String(a[key]) === String(v) }, l)));
      s.addEventListener("change", () => set({ [key]: typeof a[key] === "number" ? Number(s.value) : s.value }));
      return h("label", { class: "ap-row" }, h("span", { class: "ap-label" }, label), s);
    };
    const check = (label, key) => h("label", { class: "check ap-row" }, h("input", { type: "checkbox", checked: a[key], onchange: (e) => set({ [key]: e.target.checked }) }), label);
    const custom = (key, placeholder) => {
      const i = h("input", { type: "text", value: a[key], placeholder, "aria-label": placeholder });
      i.addEventListener("change", () => { saveAppearance({ [key]: i.value }); if (appearance()[key] !== i.value) toast("Use a font name, like \"Berkeley Mono\".", true); });
      return i;
    };
    const acc = accentColor();
    const panel = getComputedStyle(document.documentElement).getPropertyValue("--panel").trim();
    const ratio = /^#[0-9a-f]{6}$/i.test(panel) ? contrast(acc, panel) : 5;

    clear(card).append(
      h("div", { class: "card-head" }, h("h2", null, "Appearance"), h("div", { class: "row" },
        h("button", { class: "btn small", onclick: () => copy(JSON.stringify(appearance(), null, 2), "Appearance settings copied") }, "Copy settings"),
        h("button", { class: "btn small", onclick: importAppearance }, "Paste settings"),
        h("button", { class: "btn small", onclick: () => { try { localStorage.removeItem("evyro.appearance"); } catch { /* none */ } APPEARANCE = null; applyAppearance(); draw(); toast("Appearance reset"); } }, "Reset"))),
      h("p", { class: "muted small" }, "Changes apply at once and are kept on this computer. Fonts must be installed on this computer; Evyro falls back to the system font."),
      h("div", { class: "ap-grid" },
        h("section", null, h("h3", null, "Theme and color"),
          h("div", { class: "ap-themes", role: "group", "aria-label": "Theme" }, THEMES.map(([v, l]) => h("button", {
            class: "ap-theme" + (a.theme === v ? " active" : ""), "data-theme-preview": v, "aria-pressed": String(a.theme === v), onclick: () => set({ theme: v }),
          }, h("span", { class: "ap-swatch", "aria-hidden": "true" }), l))),
          h("div", { class: "ap-row" }, h("span", { class: "ap-label", id: "ap-accent" }, "Accent"),
            h("div", { class: "ap-accents", role: "group", "aria-labelledby": "ap-accent" },
              Object.entries(ACCENTS).map(([k, [l, light, dark]]) => h("button", {
                class: "ap-accent" + (a.accent === k ? " active" : ""), style: `--sw:${colorScheme() === "dark" ? dark : light}`, title: l, "aria-label": l, "aria-pressed": String(a.accent === k), onclick: () => set({ accent: k }),
              })),
              h("label", { class: "ap-accent custom" + (a.accent === "custom" ? " active" : ""), title: "Your own color" },
                h("input", { type: "color", value: a.customAccent, "aria-label": "Custom accent color", onchange: (e) => set({ accent: "custom", customAccent: e.target.value }) })))),
          ratio < 4.5 ? h("div", { class: "hint bad-text" }, `This accent reads at ${ratio.toFixed(1)}:1 on panels, below the 4.5:1 that text needs. Pick a ${colorScheme() === "dark" ? "lighter" : "darker"} shade.`) : null,
          seg("Density", "density", [["comfortable", "Comfortable"], ["compact", "Compact"]]),
          range("Corners", "radius", 0, 16)),
        h("section", null, h("h3", null, "Typography"),
          select("Interface font", "uiFont", [...Object.entries(UI_FONTS).map(([k, [l]]) => [k, l]), ["custom", "Other…"]]),
          a.uiFont === "custom" ? custom("customUiFont", "Font name, e.g. IBM Plex Sans") : null,
          range("Text size", "uiSize", 12, 18),
          select("Code and terminal font", "monoFont", [...Object.entries(MONO_FONTS).map(([k, [l]]) => [k, l]), ["custom", "Other…"]]),
          a.monoFont === "custom" ? custom("customMonoFont", "Font name, e.g. Berkeley Mono") : null,
          range("Terminal size", "termSize", 10, 20),
          h("div", { class: "ap-sample" }, h("div", null, "The quick brown fox hands off to the next AI."), h("div", { class: "mono" }, "evyro run --to codex  # 0Oo 1lI {}"))),
        h("section", null, h("h3", null, "Layout"),
          range("Initial pane count", "panes", 1, 24, ""),
          seg("Shared context side", "inspectorSide", [["left", "Left"], ["right", "Right"]]),
          check("Open the shared context on start", "inspector"),
          range("Sidebar width", "sidebarWidth", 180, 360),
          range("Context width", "inspectorWidth", 260, 520),
          range("Terminal drawer height", "drawerHeight", 120, 480),
          select("Sidebar threads", "grouping", [["state", "By state"], ["time", "Newest first"]]),
          check("Show the status bar", "statusBar"))));
  };
  draw();
  return card;
}

function importAppearance() {
  const ta = h("textarea", { placeholder: '{ "theme": "midnight", "accent": "cyan" }', rows: "8", "aria-label": "Appearance settings" });
  modal("Paste appearance settings", [field("Settings (JSON)", ta, "Unknown keys and bad values are ignored.")], [{
    label: "Apply", primary: true, run: () => {
      let o;
      try { o = JSON.parse(ta.value); } catch { toast("That is not valid JSON.", true); return false; }
      saveAppearance(o);
      if (state.view === "settings") render();
      toast("Appearance applied");
    },
  }]);
}
