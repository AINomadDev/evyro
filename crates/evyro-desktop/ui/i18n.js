// Translation layer. Every user-visible string moves, over time, into locales/<code>.js and is read
// with t("key", { name: value }). Loaded first, so the other scripts can call t() at any time.
//
//   t("tab.tasks")                      -> "Tasks"
//   t("tour.progress", { n: 2, of: 4 })  -> "2 of 4"
//   t("sample.tasks", { count: 3 })      -> plural: the catalog entry is { one: "...", other: "..." }
//
// A missing key falls back to English, and a key missing there shows the key itself (and warns once
// in the console), so a gap is visible and never blank. "pseudo" is a test language that wraps every
// string in ⟦ ⟧ and accents its letters: anything that stays plain English on a screen was not moved.
"use strict";

const I18N = (() => {
  const catalogs = {};
  const names = { en: "English", pseudo: "Pseudo-language (for translators)" };
  const warned = new Set();
  let code = "en";
  let choice = "system";
  let dirty = true;
  // Worked out on first use, because the catalogs load after this file.
  function settle() {
    if (!dirty) return;
    dirty = false;
    let want = choice;
    if (!want || want === "system") {
      want = String(navigator.language || "en").toLowerCase();
      want = catalogs[want] ? want : catalogs[want.split("-")[0]] ? want.split("-")[0] : "en";
    }
    code = catalogs[want] || want === "pseudo" ? want : "en";
    document.documentElement.lang = code === "pseudo" ? "en" : code;
  }

  const pseudo = (s) => "⟦" + String(s).replace(/\{[^}]+\}|[aeiouyAEIOUYcnsCNS]/g, (m) => (m[0] === "{" ? m : ({ a: "á", e: "é", i: "í", o: "ó", u: "ú", y: "ý", A: "Á", E: "É", I: "Í", O: "Ó", U: "Ú", Y: "Ý", c: "ç", n: "ñ", s: "š", C: "Ç", N: "Ñ", S: "Š" })[m])) + "⟧";

  function lookup(key) {
    settle();
    const own = catalogs[code] && catalogs[code][key];
    if (own !== undefined) return own;
    const base = catalogs.en[key];
    if (base !== undefined) return code === "pseudo" ? undefined : base;
    return undefined;
  }
  function render(value, params) {
    let text = value;
    if (value && typeof value === "object") {
      const n = Number(params && params.count);
      const rule = new Intl.PluralRules(code === "pseudo" ? "en" : code).select(Number.isFinite(n) ? n : 1);
      text = value[rule] !== undefined ? value[rule] : value.other;
    }
    return String(text).replace(/\{(\w+)\}/g, (m, k) => (params && params[k] !== undefined ? String(params[k]) : m));
  }

  return {
    catalogs,
    names,
    /** Languages that can be chosen. */
    available: () => Object.keys(names),
    /** Pick the language: "system" follows the browser's language when a catalog exists for it. */
    use(next) { choice = next || "system"; dirty = true; },
    current: () => { settle(); return code; },
    t(key, params) {
      const value = lookup(key);
      if (value === undefined) {
        if (code === "pseudo" && catalogs.en[key] !== undefined) return pseudo(render(catalogs.en[key], params));
        if (!warned.has(key)) { warned.add(key); console.warn("[i18n] missing string:", key); }
        return key;
      }
      return code === "pseudo" ? pseudo(render(value, params)) : render(value, params);
    },
    /** Fill the fixed text in index.html: data-i18n (text), data-i18n-title, -aria-label, -placeholder. */
    applyStatic(root = document) {
      for (const el of root.querySelectorAll("[data-i18n]")) el.textContent = I18N.t(el.dataset.i18n);
      for (const attr of ["title", "aria-label", "placeholder"]) {
        for (const el of root.querySelectorAll(`[data-i18n-${attr}]`)) el.setAttribute(attr, I18N.t(el.getAttribute(`data-i18n-${attr}`)));
      }
    },
  };
})();
const t = (key, params) => I18N.t(key, params);

// The saved choice is read here, before the other scripts draw anything.
try { I18N.use((JSON.parse(localStorage.getItem("evyro.appearance") || "{}") || {}).language); } catch { /* English */ }

