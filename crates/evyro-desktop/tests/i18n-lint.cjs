// Checks the translation catalogs and holds the line on hard-coded text.
//   node crates/evyro-desktop/tests/i18n-lint.cjs            check (CI runs this)
//
// 1. Every t("key") and data-i18n="key" names a key that exists in locales/en.js, and every key in
//    en.js is used somewhere.
// 2. Every other locale uses only keys English has, with the same {placeholders}, and plural forms
//    that are valid.
// 3. No user-visible text outside the catalogs: every literal that looks like words a person reads
//    fails, unless i18n-allow.json lists it for that file as not being text (a key name, a command
//    line, a font name, the product name). Allowlist entries that no longer appear fail too.
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const ui = path.resolve(__dirname, "../ui");
const allowPath = path.join(__dirname, "i18n-allow.json");
const problems = [];

// ---- catalogs ----
const sandbox = { I18N: { catalogs: {} } };
vm.createContext(sandbox);
const localeFiles = fs.readdirSync(path.join(ui, "locales")).filter((f) => f.endsWith(".js"));
for (const f of localeFiles) vm.runInContext(fs.readFileSync(path.join(ui, "locales", f), "utf8"), sandbox, { filename: f });
const catalogs = sandbox.I18N.catalogs;
const en = catalogs.en;
if (!en) { console.error("locales/en.js does not define I18N.catalogs.en"); process.exit(1); }
const PLURALS = new Set(["zero", "one", "two", "few", "many", "other"]);
const placeholders = (v) => [...new Set([...(typeof v === "string" ? v : Object.values(v).join(" ")).matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort().join(",");
for (const [key, value] of Object.entries(en)) {
  if (value && typeof value === "object" && value.other === undefined) problems.push(`en: plural "${key}" has no "other" form`);
}
for (const [code, cat] of Object.entries(catalogs)) {
  if (code === "en") continue;
  for (const [key, value] of Object.entries(cat)) {
    if (en[key] === undefined) { problems.push(`${code}: key "${key}" does not exist in English`); continue; }
    if (placeholders(value) !== placeholders(en[key])) problems.push(`${code}: "${key}" changes the placeholders ({${placeholders(en[key])}} became {${placeholders(value)}})`);
    if (value && typeof value === "object") for (const form of Object.keys(value)) if (!PLURALS.has(form)) problems.push(`${code}: "${key}" has an unknown plural form "${form}"`);
    if (value && typeof value === "object" && value.other === undefined) problems.push(`${code}: plural "${key}" has no "other" form`);
  }
}

// ---- keys used in the UI ----
const files = fs.readdirSync(ui).filter((f) => f.endsWith(".js") && f !== "i18n.js");
const html = fs.readFileSync(path.join(ui, "index.html"), "utf8");
const used = new Set();
const stripComments = (src) => src.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, "")).replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");
for (const f of files) {
  const src = stripComments(fs.readFileSync(path.join(ui, f), "utf8"));
  // t("key"), or I18N.t("key") where a local variable named t hides the function.
  for (const m of src.matchAll(/(?:(?<![\w.$])|I18N\.)t\(\s*(["'`])([^"'`]*)\1/g)) used.add(m[2]);
  for (const m of src.matchAll(/(?<![\w.$])t\(\s*(?!["'`])/g)) problems.push(`${f}: t() needs a literal key, not an expression`);
}
for (const m of html.matchAll(/data-i18n(?:-[\w-]+)?="([^"]+)"/g)) used.add(m[1]);
for (const k of used) if (en[k] === undefined) problems.push(`a string asks for "${k}", which is not in locales/en.js`);
for (const k of Object.keys(en)) if (!used.has(k)) problems.push(`locales/en.js has "${k}", which nothing uses`);

// ---- no hard-coded text ----
const SKIP_BEFORE = /(invoke|querySelector(All)?|\$|matches|closest|getAttribute|setAttribute|require|console\.\w+|localStorage\.\w+|listen|emit|dataset\.\w+|classList\.\w+|addEventListener|removeAttribute|getElementById|new RegExp|createElement|waitFor\w*|includes|startsWith|endsWith|split|JSON\.\w+|import|class|type|role|id|for|href|src|style|name|value|target|key|kind|mode|status|cmd|action|data-[\w-]+|aria-[a-z]+|autocomplete|spellcheck|inputmode|rel|method|loading|decoding)\s*[:(=]?\s*$/;
function visible(text, before) {
  if (text.length < 3 || !/[A-Za-z]{3}/.test(text)) return false;
  if (SKIP_BEFORE.test(before.slice(-40))) return false;
  if (/^[\w-]+\s*:\s*[^ ]/.test(text) && !/[A-Z]/.test(text[0])) return false; // CSS declarations
  const words = text.trim().split(/\s+/);
  if (words.length >= 2 && /[A-Za-z]{2}/.test(words[0]) && /[A-Za-z]{2}/.test(words[1])) return !/^[a-z][\w-]*(\s+[\w.#:\[\]="'-]+)+$/.test(text) || /[A-Z]/.test(text);
  return /^[A-Z]/.test(text) && text.length >= 4 && !/^[A-Z0-9_]+$/.test(text);
}
const allow = JSON.parse(fs.readFileSync(allowPath, "utf8"));
let allowed = 0;
for (const f of files) {
  const src = stripComments(fs.readFileSync(path.join(ui, f), "utf8"));
  const ok = new Set(allow[f] || []);
  const seen = new Set();
  for (const m of src.matchAll(/"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\]|\\.)*)`/g)) {
    const text = (m[1] !== undefined ? m[1] : m[2]).replace(/\$\{[^}]*\}/g, "…");
    if (!visible(text, src.slice(Math.max(0, m.index - 40), m.index))) continue;
    if (ok.has(text)) { allowed++; seen.add(text); continue; }
    const line = src.slice(0, m.index).split("\n").length;
    problems.push(`${f}:${line}: "${text.slice(0, 80)}" is text outside the catalog. Put it in locales/en.js and read it with t("key"), or, if it is not text a person reads, add it to tests/i18n-allow.json.`);
  }
  for (const text of ok) if (!seen.has(text)) problems.push(`i18n-allow.json lists "${text}" for ${f}, which no longer appears there: remove it.`);
}
for (const f of Object.keys(allow)) if (!files.includes(f)) problems.push(`i18n-allow.json names ${f}, which is not a UI file.`);
console.log(`i18n: ${Object.keys(en).length} English strings, ${Object.keys(catalogs).length} catalog(s), ${used.size} keys used, ${allowed} allowed literals that are not text.`);
if (problems.length) { for (const p of problems) console.error("  " + p); process.exit(1); }
console.log("i18n lint passed.");
