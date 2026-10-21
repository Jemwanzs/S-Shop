// Builds the Product User Manual (roadmap 88, guides roadmap 146) from docs/manual/manual.json — one source, two outputs:
//   web/public/SShop_Product_User_Manual.pdf (served at /SShop_Product_User_Manual.pdf, no sign-in)
//   docs/user-guide.md                       (the same content for reading in the repository)
//
// A4 portrait, Outfit, S'Shop colours, page numbers. Page 1: title and directory. Part 1: one page per module (features).
// Part 2: step-by-step guides per role; long guides flow onto continuation pages automatically.
// Edit the JSON (only features that exist today, steps that match the screens), bump "version", then run:
//   node scripts/manual/build.mjs            (needs the `playwright` package and its Chromium; set PLAYWRIGHT_DIR to
//                                            the folder holding node_modules/playwright if it is not installed here)
//                                            (PLAYWRIGHT_CHANNEL=chrome: use the installed Google Chrome)
// and commit the JSON, the PDF and docs/user-guide.md together.
import { createRequire } from "node:module";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const m = JSON.parse(readFileSync(join(root, "docs/manual/manual.json"), "utf8"));
const guides = m.guides ?? [];

// ── Markdown copy (docs/user-guide.md) ────────────────────────────────────────────────────────────────────────────
const md = [
  `# ${m.title} — ${m.subtitle}`,
  "",
  `*${m.version}. Generated from [manual/manual.json](manual/manual.json) by \`scripts/manual/build.mjs\` — edit the JSON, not this file. The same content is the PDF at https://s-shop.store/SShop_Product_User_Manual.pdf.*`,
  "",
  m.intro,
  "",
  "## Contents",
  "",
  "**Part 1 — Features by module:** " + m.modules.map((x, i) => `[${i + 1}. ${x.name}](#module-${i + 1})`).join(" · "),
  "",
  "**Part 2 — Step-by-step guides:** " + guides.map((g) => `[${g.role}](#guide-${slug(g.role)})`).join(" · "),
  "",
  "## Part 1 — Features by module",
  "",
  ...m.modules.flatMap((x, i) => [
    `<a id="module-${i + 1}"></a>`,
    "",
    `### ${i + 1}. ${x.name}${x.audience === "platform" ? " *(platform owner only)*" : ""}`,
    "",
    x.summary,
    "",
    ...x.bullets.map((b) => `- ${b}`),
    "",
  ]),
  "## Part 2 — Step-by-step guides",
  "",
  m.guides_intro ?? "",
  "",
  ...guides.flatMap((g) => [
    `<a id="guide-${slug(g.role)}"></a>`,
    "",
    `### Guide: ${g.role}${g.audience === "platform" ? " *(platform owner only)*" : ""}`,
    "",
    g.summary,
    "",
    ...g.tasks.flatMap((k) => [
      `#### ${k.title}`,
      "",
      ...k.steps.map((s, j) => `${j + 1}. ${s}`),
      ...(k.note ? ["", `> ${k.note}`] : []),
      "",
    ]),
  ]),
].join("\n");
function slug(s) {
  return s.toLowerCase().replace(/&/g, "").replace(/[^a-z0-9 -]/g, "").trim().replace(/\s+/g, "-");
}
writeFileSync(join(root, "docs/user-guide.md"), md.replace(/\n{3,}/g, "\n\n"));

// ── PDF ───────────────────────────────────────────────────────────────────────────────────────────────────────────
const require = createRequire(process.env.PLAYWRIGHT_DIR ? join(process.env.PLAYWRIGHT_DIR, "x.js") : import.meta.url);
const { chromium } = require("playwright");
const icon = `data:image/png;base64,${readFileSync(join(root, "web/public/icon-192.png")).toString("base64")}`;
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// Page numbers are filled in after the guides are paginated (in the browser).
const foot = `<footer><span><img src="${icon}" alt="" /> S'Shop · ${esc(m.subtitle)}</span><span class="pn"></span></footer>`;
const dirRow = (n, name, target, tag) =>
  `<li><span class="n">${n}</span><span class="t">${esc(name)}${tag ? ` <em>${tag}</em>` : ""}</span><span class="dots"></span><span class="p" data-for="${target}"></span></li>`;

const cover = `
<section class="page cover">
  <div class="band"></div>
  <header class="brand"><img src="${icon}" alt="S'Shop" /><span>S'Shop</span></header>
  <h1>${esc(m.title)}</h1>
  <p class="sub">${esc(m.subtitle)}</p>
  <p class="intro">${esc(m.intro)}</p>
  <h2 class="dir-title">Part 1 · Modules</h2>
  <ol class="dir">
    ${m.modules.map((x, i) => dirRow(String(i + 1).padStart(2, "0"), x.name, `mod-${i}`, x.audience === "platform" ? "Platform owner" : "")).join("")}
  </ol>
  ${guides.length ? `<h2 class="dir-title">Part 2 · Step-by-step guides</h2>
  ${m.guides_intro ? `<p class="gintro">${esc(m.guides_intro)}</p>` : ""}
  <ol class="dir">
    ${guides.map((g, i) => dirRow(String.fromCharCode(65 + i), g.role, `guide-${i}`, g.audience === "platform" ? "Platform owner" : "")).join("")}
  </ol>` : ""}
  <p class="ver">${esc(m.version)}</p>
  ${foot}
</section>`;

const modules = m.modules.map((x, i) => `
<section class="page module" id="mod-${i}">
  <div class="band"></div>
  <p class="kicker">Module ${String(i + 1).padStart(2, "0")}${x.audience === "platform" ? " · Platform owner only" : ""}</p>
  <h2>${esc(x.name)}</h2>
  <p class="summary">${esc(x.summary)}</p>
  <ul>${x.bullets.map((b) => `<li>${esc(b)}</li>`).join("")}</ul>
  ${x.audience === "platform" ? `<p class="note">Available only to the S'Shop platform owner. Business administrators never see platform functions or other businesses' data.</p>` : ""}
  ${foot}
</section>`).join("");

const guidePages = guides.map((g, i) => `
<section class="page guide" id="guide-${i}" data-kicker="Guide ${String.fromCharCode(65 + i)} · ${esc(g.role)} (continued)">
  <div class="band"></div>
  <p class="kicker">Guide ${String.fromCharCode(65 + i)}${g.audience === "platform" ? " · Platform owner only" : ""}</p>
  <h2>${esc(g.role)}</h2>
  <p class="summary">${esc(g.summary)}</p>
  <div class="tasks">
    ${g.tasks.map((k) => `<div class="task"><h3>${esc(k.title)}</h3><ol>${k.steps.map((s) => `<li>${esc(s)}</li>`).join("")}</ol>${k.note ? `<p class="tnote">${esc(k.note)}</p>` : ""}</div>`).join("")}
  </div>
  ${foot}
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
  .cover h1 { margin: 10mm 0 0; font-size: 26pt; line-height: 1.15; font-weight: 700; letter-spacing: -0.01em; }
  .cover .sub { margin: 2.5mm 0 0; font-size: 13.5pt; color: var(--orange); font-weight: 500; }
  .cover .intro { margin: 5mm 0 0; font-size: 10pt; line-height: 1.5; color: var(--muted); max-width: 150mm; }
  .dir-title { margin: 7mm 0 2.5mm; font-size: 9.5pt; font-weight: 600; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); }
  .dir { list-style: none; margin: 0; padding: 3mm 6mm; background: var(--card); border: 0.3mm solid var(--line); border-radius: 4mm; columns: 2; column-gap: 10mm; }
  .dir li { display: flex; align-items: baseline; gap: 2mm; padding: 1.2mm 0; font-size: 9.3pt; break-inside: avoid; }
  .dir .n { color: var(--orange); font-weight: 600; font-variant-numeric: tabular-nums; width: 6mm; }
  .dir .t em { font-style: normal; font-size: 7.5pt; color: var(--orange); background: var(--soft); border-radius: 2mm; padding: 0.2mm 1.5mm; margin-left: 1mm; white-space: nowrap; }
  .dir .dots { flex: 1; border-bottom: 0.3mm dotted #cdbfb0; transform: translateY(-1mm); }
  .dir .p { font-variant-numeric: tabular-nums; color: var(--muted); }
  .ver { position: absolute; bottom: 21mm; left: 22mm; margin: 0; font-size: 9pt; color: var(--muted); }
  .kicker { margin: 4mm 0 0; font-size: 9.5pt; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--orange); }
  .module h2, .guide h2 { margin: 3mm 0 0; font-size: 26pt; font-weight: 700; line-height: 1.15; }
  .summary { margin: 6mm 0 9mm; padding: 5mm 6mm; background: var(--soft); border-radius: 4mm; font-size: 12pt; line-height: 1.5; }
  .gintro { margin: -1mm 0 3mm; font-size: 9pt; line-height: 1.45; color: var(--muted); max-width: 160mm; }
  .module ul { list-style: none; margin: 0; padding: 0; }
  .module li { position: relative; margin: 0 0 4.2mm; padding: 4mm 6mm 4mm 13mm; background: var(--card); border: 0.3mm solid var(--line); border-radius: 3.5mm; font-size: 11pt; line-height: 1.45; }
  .module li::before { content: ""; position: absolute; left: 6mm; top: 6mm; width: 2.6mm; height: 2.6mm; border-radius: 50%; background: var(--orange); }
  .note { margin-top: 6mm; font-size: 9.5pt; color: var(--muted); }
  .guide .summary { margin: 4mm 0 5mm; padding: 4mm 5.5mm; font-size: 11.5pt; }
  .guide.cont .kicker { margin: 2mm 0 5mm; }
  .tasks { display: flex; flex-direction: column; gap: 3.5mm; }
  .task { padding: 3.5mm 5mm 3mm; background: var(--card); border: 0.3mm solid var(--line); border-radius: 3.5mm; break-inside: avoid; }
  .task h3 { margin: 0 0 1.5mm; font-size: 11.5pt; font-weight: 600; color: var(--brown); }
  .task ol { margin: 0; padding-left: 5.5mm; }
  .task li { margin: 0 0 1.1mm; font-size: 9.8pt; line-height: 1.42; }
  .task li::marker { color: var(--orange); font-weight: 600; }
  .tnote { margin: 1.5mm 0 0; padding: 1.8mm 3mm; background: var(--soft); border-radius: 2mm; font-size: 8.8pt; color: var(--muted); }
</style></head><body>${cover}${modules}${guidePages}</body></html>`;

// MANUAL_HTML=<file>: also keep the HTML (to preview pages as images).
// PLAYWRIGHT_CHANNEL=chrome uses the installed Google Chrome instead of Playwright's own Chromium.
const browser = await chromium.launch(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {});
const page = await browser.newPage();
await page.setContent(html, { waitUntil: "networkidle" });
await page.evaluate(() => document.fonts.ready);
const outlined = await page.evaluate(() => document.fonts.check('12px "Outfit"'));
if (!outlined) throw new Error("Outfit did not load (needs internet access to Google Fonts)");
// Guides: move tasks that do not fit above the footer onto continuation pages, then number every page.
const pages = await page.evaluate(() => {
  const fits = (sec) => {
    const limit = sec.querySelector("footer").getBoundingClientRect().top - 4;
    const last = sec.querySelector(".tasks").lastElementChild;
    return !last || last.getBoundingClientRect().bottom <= limit;
  };
  let sec = document.querySelector(".page.guide");
  while (sec) {
    if (!fits(sec)) {
      const cont = document.createElement("section");
      cont.className = "page guide cont";
      cont.innerHTML = `<div class="band"></div><p class="kicker"></p><div class="tasks"></div>${sec.querySelector("footer").outerHTML}`;
      cont.querySelector(".kicker").textContent = sec.dataset.kicker;
      cont.dataset.kicker = sec.dataset.kicker;
      sec.after(cont);
      const tasks = sec.querySelector(".tasks");
      while (!fits(sec) && tasks.children.length > 1) cont.querySelector(".tasks").prepend(tasks.lastElementChild);
    }
    sec = sec.nextElementSibling?.classList.contains("guide") ? sec.nextElementSibling : null;
  }
  const all = [...document.querySelectorAll(".page")];
  all.forEach((p, i) => (p.querySelector(".pn").textContent = `${i + 1} / ${all.length}`));
  document.querySelectorAll(".dir .p").forEach((el) => (el.textContent = String(all.indexOf(document.getElementById(el.dataset.for)) + 1)));
  const tooLong = all.filter((p) => p.classList.contains("guide") && p.querySelector(".tasks").lastElementChild &&
    p.querySelector(".tasks").lastElementChild.getBoundingClientRect().bottom > p.querySelector("footer").getBoundingClientRect().top);
  if (tooLong.length) throw new Error("A single guide task is longer than a page — split it");
  return all.length;
});
if (process.env.MANUAL_HTML) writeFileSync(process.env.MANUAL_HTML, await page.content());
const pdf = await page.pdf({ format: "A4", printBackground: true, preferCSSPageSize: true });
await browser.close();
const out = join(root, "web/public/SShop_Product_User_Manual.pdf");
writeFileSync(out, pdf);
console.log(`${out} — ${pages} pages, ${(pdf.length / 1024).toFixed(0)} KB; docs/user-guide.md written`);
