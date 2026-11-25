// Checks the translation catalogs and holds the line on hard-coded text.
//   node crates/evyro-desktop/tests/i18n-lint.cjs            check (CI runs this)
//   node crates/evyro-desktop/tests/i18n-lint.cjs --update   record lower counts after moving strings
//
// 1. Every t("key") and data-i18n="key" names a key that exists in locales/en.js, and every key in
//    en.js is used somewhere.
// 2. Every other locale uses only keys English has, with the same {placeholders}, and plural forms
//    that are valid.
// 3. A ratchet: each UI file's count of literal user-visible strings may not grow, and when it
//    shrinks the baseline must be lowered, so text only ever moves into the catalogs.
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const ui = path.resolve(__dirname, "../ui");
const baselinePath = path.join(__dirname, "i18n-baseline.json");
const update = process.argv.includes("--update");
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
const stripComments = (src) => src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");
for (const f of files) {
  const src = stripComments(fs.readFileSync(path.join(ui, f), "utf8"));
  for (const m of src.matchAll(/(?<![\w.$])t\(\s*(["'`])([^"'`]*)\1/g)) used.add(m[2]);
  for (const m of src.matchAll(/(?<![\w.$])t\(\s*(?!["'`])/g)) problems.push(`${f}: t() needs a literal key, not an expression`);
}
for (const m of html.matchAll(/data-i18n(?:-[\w-]+)?="([^"]+)"/g)) used.add(m[1]);
for (const k of used) if (en[k] === undefined) problems.push(`a string asks for "${k}", which is not in locales/en.js`);
for (const k of Object.keys(en)) if (!used.has(k)) problems.push(`locales/en.js has "${k}", which nothing uses`);

// ---- the ratchet ----
const SKIP_BEFORE = /(invoke|querySelector(All)?|\$|matches|closest|getAttribute|setAttribute|require|console\.\w+|localStorage\.\w+|listen|emit|dataset\.\w+|classList\.\w+|addEventListener|removeAttribute|getElementById|new RegExp|createElement|waitFor\w*|includes|startsWith|endsWith|split|JSON\.\w+|import|class|type|role|id|for|href|src|style|name|value|target|key|kind|mode|status|cmd|action|data-[\w-]+|aria-[a-z]+|autocomplete|spellcheck|inputmode|rel|method|loading|decoding)\s*[:(=]?\s*$/;
function visible(text, before) {
  if (text.length < 3 || !/[A-Za-z]{3}/.test(text)) return false;
  if (SKIP_BEFORE.test(before.slice(-40))) return false;
  if (/^[\w-]+\s*:\s*[^ ]/.test(text) && !/[A-Z]/.test(text[0])) return false; // CSS declarations
  const words = text.trim().split(/\s+/);
  if (words.length >= 2 && /[A-Za-z]{2}/.test(words[0]) && /[A-Za-z]{2}/.test(words[1])) return !/^[a-z][\w-]*(\s+[\w.#:\[\]="'-]+)+$/.test(text) || /[A-Z]/.test(text);
  return /^[A-Z]/.test(text) && text.length >= 4 && !/^[A-Z0-9_]+$/.test(text);
}
const counts = {};
for (const f of files) {
  const src = stripComments(fs.readFileSync(path.join(ui, f), "utf8"));
  let n = 0;
  for (const m of src.matchAll(/"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\]|\\.)*)`/g)) {
    const text = (m[1] !== undefined ? m[1] : m[2]).replace(/\$\{[^}]*\}/g, "…");
    if (visible(text, src.slice(Math.max(0, m.index - 40), m.index))) n++;
  }
  counts[f] = n;
}
let baseline = {};
try { baseline = JSON.parse(fs.readFileSync(baselinePath, "utf8")); } catch { /* first run */ }
if (update) {
  fs.writeFileSync(baselinePath, JSON.stringify(counts, null, 2) + "\n");
  console.log("Baseline written: " + Object.values(counts).reduce((a, b) => a + b, 0) + " literal strings in " + files.length + " files.");
} else {
  for (const f of files) {
    const was = baseline[f] ?? 0;
    if (counts[f] > was) problems.push(`${f}: ${counts[f]} literal strings, ${was} allowed. Put new text in locales/en.js and read it with t("key").`);
    else if (counts[f] < was) problems.push(`${f}: down to ${counts[f]} from ${was}. Good: run with --update so it cannot grow back.`);
  }
}
const total = Object.values(counts).reduce((a, b) => a + b, 0);
console.log(`i18n: ${Object.keys(en).length} English strings, ${Object.keys(catalogs).length} catalog(s), ${used.size} keys used, ${total} literal strings still to move.`);
if (problems.length) { for (const p of problems) console.error("  " + p); process.exit(1); }
console.log("i18n lint passed.");
