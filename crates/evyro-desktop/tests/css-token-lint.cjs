// Holds the line on design tokens in the desktop UI stylesheets.
//   node crates/evyro-desktop/tests/css-token-lint.cjs            check (CI runs this)
//
// 1. Colors and sizes live in ui/tokens.css only. Every other ui/*.css declaration that uses a raw
//    color (#hex, rgb(), hsl() and friends, a named color) or a px value fails, unless
//    css-token-allow.json lists it for that file with a reason. Allowlist entries that no longer
//    match fail too. (Media query conditions are not checked: custom properties cannot go there.)
// 2. Every var(--name) refers to a custom property that some stylesheet declares or ui/*.js sets,
//    so a mistyped token name fails instead of silently falling back.
"use strict";
const fs = require("node:fs");
const path = require("node:path");

const ui = path.resolve(__dirname, "../ui");
const allowPath = path.join(__dirname, "css-token-allow.json");
const TOKENS = "tokens.css";
const allow = JSON.parse(fs.readFileSync(allowPath, "utf8"));
const used = new Set();
const problems = [];

const RAW = [
  [/(^|[^\w-])\d*\.?\d+px\b/, "px value"],
  [/#[0-9a-fA-F]{3,8}\b/, "hex color"],
  [/\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/, "color function"],
  [/\b(white|black|red|green|blue|gray|grey|orange|yellow|purple|pink|silver|navy|teal|maroon)\b/i, "named color"],
];

const sheets = fs.readdirSync(ui).filter((f) => f.endsWith(".css"));
const declared = new Set();
const refs = [];
for (const f of sheets) {
  // Blank out comments but keep newlines, so offsets still give line numbers.
  const src = fs.readFileSync(path.join(ui, f), "utf8").replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "));
  const lineAt = (i) => src.slice(0, i).split("\n").length;
  for (const m of src.matchAll(/(--[\w-]+)\s*:/g)) declared.add(m[1]);
  for (const m of src.matchAll(/var\(\s*(--[\w-]+)/g)) refs.push({ f, name: m[1], line: lineAt(m.index) });
  if (f === TOKENS) continue;
  // Innermost { ... } blocks hold the declarations.
  for (const block of src.matchAll(/\{([^{}]*)\}/g)) {
    let at = block.index + 1;
    for (const decl of block[1].split(";")) {
      const value = decl.slice(decl.indexOf(":") + 1).replace(/var\(\s*--[\w-]+/g, "var(");
      const hit = decl.includes(":") && RAW.find(([re]) => re.test(value));
      if (hit) {
        const text = decl.trim();
        const ok = allow.find((a) => a.file === f && text.includes(a.text));
        if (ok) used.add(ok);
        else problems.push(`${f}:${lineAt(at)}: ${hit[1]} in "${text}" — use a token from ${TOKENS}, or allowlist it in css-token-allow.json with a reason`);
      }
      at += decl.length + 1;
    }
  }
}

// Custom properties set from script: style.setProperty("--x", …) and inline style strings "--x:".
for (const f of fs.readdirSync(ui).filter((x) => x.endsWith(".js"))) {
  const src = fs.readFileSync(path.join(ui, f), "utf8");
  for (const m of src.matchAll(/["'`](--[\w-]+)["'`:]/g)) declared.add(m[1]);
  for (const m of src.matchAll(/[`"'{;\s](--[\w-]+):/g)) declared.add(m[1]);
}
for (const r of refs) if (!declared.has(r.name)) problems.push(`${r.f}:${r.line}: var(${r.name}) is not declared in any stylesheet or set by ui/*.js`);

for (const a of allow) {
  if (!a.why) problems.push(`css-token-allow.json: the entry for ${a.file} "${a.text}" needs a "why"`);
  if (!used.has(a)) problems.push(`css-token-allow.json: ${a.file} "${a.text}" no longer matches anything; remove it`);
}

if (problems.length) {
  for (const p of problems) console.error(p);
  console.error(`\n${problems.length} design-token problem(s).`);
  process.exit(1);
}
console.log(`Design tokens OK (${sheets.length} stylesheets, ${allow.length} allowlisted).`);
