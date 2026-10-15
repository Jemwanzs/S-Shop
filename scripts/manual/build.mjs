// Builds the public Product User Manual (roadmap 88) from docs/manual/manual.json:
//   web/public/SShop_Product_User_Manual.pdf (served at /SShop_Product_User_Manual.pdf, no sign-in).
//
// A4 portrait, Outfit, S'Shop colours, page numbers; page 1 is the title and the module directory, then one page
// per module. Edit the JSON (only features that exist today), bump "version", then run:
//   node scripts/manual/build.mjs            (needs the `playwright` package and its Chromium; set PLAYWRIGHT_DIR to
//                                            the folder holding node_modules/playwright if it is not installed here)
//                                            (PLAYWRIGHT_CHANNEL=chrome: use the installed Google Chrome)
// and commit the JSON and the PDF together.
import { createRequire } from "node:module";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire(process.env.PLAYWRIGHT_DIR ? join(process.env.PLAYWRIGHT_DIR, "x.js") : import.meta.url);
const { chromium } = require("playwright");

const m = JSON.parse(readFileSync(join(root, "docs/manual/manual.json"), "utf8"));
const icon = `data:image/png;base64,${readFileSync(join(root, "web/public/icon-192.png")).toString("base64")}`;
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const total = m.modules.length + 1;

const foot = (n) => `<footer><span><img src="${icon}" alt="" /> S'Shop · ${esc(m.subtitle)}</span><span>${n} / ${total}</span></footer>`;

const cover = `
<section class="page cover">
  <div class="band"></div>
  <header class="brand"><img src="${icon}" alt="S'Shop" /><span>S'Shop</span></header>
  <h1>${esc(m.title)}</h1>
  <p class="sub">${esc(m.subtitle)}</p>
  <p class="intro">${esc(m.intro)}</p>
  <h2 class="dir-title">Modules</h2>
  <ol class="dir">
    ${m.modules.map((x, i) => `<li><span class="n">${String(i + 1).padStart(2, "0")}</span><span class="t">${esc(x.name)}${x.audience === "platform" ? ' <em>Platform owner</em>' : ""}</span><span class="dots"></span><span class="p">${i + 2}</span></li>`).join("")}
  </ol>
  <p class="ver">${esc(m.version)}</p>
  ${foot(1)}
</section>`;

const pages = m.modules.map((x, i) => `
<section class="page module">
  <div class="band"></div>
  <p class="kicker">Module ${String(i + 1).padStart(2, "0")}${x.audience === "platform" ? " · Platform owner only" : ""}</p>
  <h2>${esc(x.name)}</h2>
  <p class="summary">${esc(x.summary)}</p>
  <ul>${x.bullets.map((b) => `<li>${esc(b)}</li>`).join("")}</ul>
  ${x.audience === "platform" ? `<p class="note">Available only to the S'Shop platform owner. Business administrators never see platform functions or other businesses' data.</p>` : ""}
  ${foot(i + 2)}
</section>`).join("");

const html = `<!doctype html><html lang="en"><head><meta charset="utf-8" />
<title>${esc(m.title)}</title>
<link href="https://fonts.googleapis.com/css2?family=Outfit:wght@300;400;500;600;700&display=swap" rel="stylesheet" />
<style>
  @page { size: A4; margin: 0; }
  :root { --cream: #faf5ee; --card: #fffdf9; --brown: #2b1d14; --muted: #76685d; --orange: #e8620f; --soft: #f9e3cf; --line: #ece2d6; }
  * { box-sizing: border-box; }
  html, body { margin: 0; font-family: "Outfit", sans-serif; color: var(--brown); -webkit-print-color-adjust: exact; print-color-adjust: exact; }
  .page { position: relative; width: 210mm; height: 297mm; padding: 26mm 22mm 24mm; background: var(--cream); page-break-after: always; overflow: hidden; }
  .page:last-child { page-break-after: auto; }
  .band { position: absolute; inset: 0 0 auto 0; height: 7mm; background: linear-gradient(90deg, var(--orange), #f4a261); }
  footer { position: absolute; left: 22mm; right: 22mm; bottom: 12mm; display: flex; justify-content: space-between; align-items: center;
    padding-top: 3mm; border-top: 0.3mm solid var(--line); font-size: 8.5pt; color: var(--muted); }
  footer span:first-child { display: flex; align-items: center; gap: 2mm; }
  footer img { width: 4.5mm; height: 4.5mm; }
  .brand { display: flex; align-items: center; gap: 3mm; font-weight: 600; font-size: 14pt; }
  .brand img { width: 12mm; height: 12mm; }
  .cover h1 { margin: 12mm 0 0; font-size: 27pt; line-height: 1.15; font-weight: 700; letter-spacing: -0.01em; }
  .cover .sub { margin: 3mm 0 0; font-size: 14pt; color: var(--orange); font-weight: 500; }
  .cover .intro { margin: 6mm 0 0; font-size: 10.5pt; line-height: 1.55; color: var(--muted); max-width: 150mm; }
  .dir-title { margin: 9mm 0 3mm; font-size: 10pt; font-weight: 600; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); }
  .dir { list-style: none; margin: 0; padding: 4mm 6mm; background: var(--card); border: 0.3mm solid var(--line); border-radius: 4mm; columns: 2; column-gap: 10mm; }
  .dir li { display: flex; align-items: baseline; gap: 2mm; padding: 1.45mm 0; font-size: 9.6pt; break-inside: avoid; }
  .dir .n { color: var(--orange); font-weight: 600; font-variant-numeric: tabular-nums; width: 6mm; }
  .dir .t em { font-style: normal; font-size: 7.5pt; color: var(--orange); background: var(--soft); border-radius: 2mm; padding: 0.2mm 1.5mm; margin-left: 1mm; white-space: nowrap; }
  .dir .dots { flex: 1; border-bottom: 0.3mm dotted #cdbfb0; transform: translateY(-1mm); }
  .dir .p { font-variant-numeric: tabular-nums; color: var(--muted); }
  .ver { position: absolute; bottom: 22mm; left: 22mm; margin: 0; font-size: 9pt; color: var(--muted); }
  .module .kicker { margin: 4mm 0 0; font-size: 9.5pt; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--orange); }
  .module h2 { margin: 3mm 0 0; font-size: 26pt; font-weight: 700; line-height: 1.15; }
  .module .summary { margin: 6mm 0 9mm; padding: 5mm 6mm; background: var(--soft); border-radius: 4mm; font-size: 12pt; line-height: 1.5; }
  .module ul { list-style: none; margin: 0; padding: 0; }
  .module li { position: relative; margin: 0 0 4.2mm; padding: 4mm 6mm 4mm 13mm; background: var(--card); border: 0.3mm solid var(--line); border-radius: 3.5mm; font-size: 11pt; line-height: 1.45; }
  .module li::before { content: ""; position: absolute; left: 6mm; top: 6mm; width: 2.6mm; height: 2.6mm; border-radius: 50%; background: var(--orange); }
  .note { margin-top: 6mm; font-size: 9.5pt; color: var(--muted); }
</style></head><body>${cover}${pages}</body></html>`;

// MANUAL_HTML=<file>: also keep the HTML (to preview pages as images).
if (process.env.MANUAL_HTML) writeFileSync(process.env.MANUAL_HTML, html);
// PLAYWRIGHT_CHANNEL=chrome uses the installed Google Chrome instead of Playwright's own Chromium.
const browser = await chromium.launch(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {});
const page = await browser.newPage();
await page.setContent(html, { waitUntil: "networkidle" });
await page.evaluate(() => document.fonts.ready);
const outlined = await page.evaluate(() => document.fonts.check('12px "Outfit"'));
if (!outlined) throw new Error("Outfit did not load (needs internet access to Google Fonts)");
const pdf = await page.pdf({ format: "A4", printBackground: true, preferCSSPageSize: true });
await browser.close();
const out = join(root, "web/public/SShop_Product_User_Manual.pdf");
writeFileSync(out, pdf);
console.log(`${out} — ${total} pages, ${(pdf.length / 1024).toFixed(0)} KB`);
