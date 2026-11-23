// Renders the README banners and the GitHub social preview from banner.html.
//
//   node docs/brand/render.cjs
//
// Needs Playwright and the Inter, Inter Display and JetBrains Mono fonts installed.
"use strict";
const { chromium } = require("playwright");
const path = require("path");
const { pathToFileURL } = require("url");

const SRC = pathToFileURL(path.join(__dirname, "banner.html")).href;
const OUT = path.join(__dirname, "../assets");
const shots = [
  { file: "banner-dark.png", q: "theme=dark&size=1280x440", w: 1280, h: 440 },
  { file: "banner-light.png", q: "theme=light&size=1280x440", w: 1280, h: 440 },
  { file: "social-preview.png", q: "theme=dark&size=1280x640&social=1", w: 1280, h: 640 },
];

(async () => {
  const browser = await chromium.launch();
  for (const s of shots) {
    const page = await browser.newPage({ viewport: { width: s.w, height: s.h }, deviceScaleFactor: s.file.startsWith("social") ? 1 : 2 });
    await page.goto(`${SRC}?${s.q}`);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: path.join(OUT, s.file) });
    await page.close();
    console.log("wrote docs/assets/" + s.file);
  }
  await browser.close();
})().catch((e) => { console.error(e); process.exit(1); });
