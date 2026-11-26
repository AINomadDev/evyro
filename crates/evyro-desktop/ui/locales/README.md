# Translating Evyro

The desktop app reads its words from `locales/<code>.js`. `en.js` is the source.

1. Copy `en.js` to `<code>.js` (for example `es.js`) and change `I18N.catalogs.en` to `I18N.catalogs.es`.
2. Translate the values. Keep every `{placeholder}` exactly as written, keep the keys, and keep whole
   sentences whole. A value written as `{ one: "...", other: "..." }` is a plural; give each form your
   language needs (`one`, `few`, `many`, `other`...).
3. Add the code and its name to `names` in `i18n.js` and the file to `index.html` after `locales/en.js`.
4. Run `node crates/evyro-desktop/tests/i18n-lint.cjs`. It fails on a key that does not exist in English,
   a changed placeholder, or an unknown plural form.
5. Choose the language in Settings, Appearance, Language. **Pseudo-language** shows which words on a
   screen still come from outside a catalog.

Every word a person reads comes from a catalog, and the lint fails on any new hard-coded text with its
file and line. A literal that only looks like text (a key name such as `"Enter"`, a command line, a font
name, the product name) goes in `crates/evyro-desktop/tests/i18n-allow.json` under its file instead.

## A variable named `t`

Many functions hold a task in a variable called `t`, which hides the translation function. Inside
one, write `I18N.t("key")` instead of `t("key")`. The lint counts both. A screen that shows
"t is not a function" is this mistake.
