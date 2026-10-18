/** S'Shop barcode stickers (roadmap 134): business name · Code 128 bars · the number · the product name (small).
 * One layout in millimetres feeds every output — on-screen preview (SVG), the print page, PDF, PNG, and the ZPL / TSPL
 * commands label printers understand. White background, black bars, no price, no internal ids. */
import { bars, QUIET, symbolWidth } from "@/lib/code128";

export interface Label {
  code: string;
  product: string;
}
export interface LabelSize {
  w: number;
  h: number;
}
export const PRESETS: { label: string; w: number; h: number }[] = [
  { label: "40 × 25 mm", w: 40, h: 25 },
  { label: "50 × 30 mm", w: 50, h: 30 },
  { label: "60 × 40 mm", w: 60, h: 40 },
];

/** Positions in millimetres for a sticker of size `s`. */
function layout(s: LabelSize, code: string) {
  const pad = Math.max(1.5, s.h * 0.06);
  const fb = s.h * 0.12; // business name
  const fn = s.h * 0.12; // the number
  const fp = s.h * 0.085; // product name
  const topBars = pad + fb + 0.9;
  const bottomBars = s.h - pad - fp - 0.7 - fn - 0.5;
  const mod = (s.w - 2 * pad) / (symbolWidth(code) + 2 * QUIET);
  return {
    pad, fb, fn, fp, mod,
    business: pad + fb * 0.85,
    barsTop: topBars,
    barsH: Math.max(4, bottomBars - topBars),
    barsX: pad + QUIET * mod,
    number: bottomBars + 0.5 + fn * 0.85,
    product: s.h - pad - fp * 0.15,
  };
}

/** Text that fits `width` mm at `size` mm (average sans-serif glyph ≈ 0.55 em), shortened with an ellipsis. */
export function fit(text: string, width: number, size: number): string {
  const max = Math.max(4, Math.floor(width / (size * 0.56)));
  const t = text.trim().replace(/\s+/g, " ");
  return t.length <= max ? t : `${t.slice(0, max - 1).trimEnd()}…`;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** One sticker as SVG (sized in millimetres). */
export function labelSvg(business: string, l: Label, s: LabelSize): string {
  const L = layout(s, l.code);
  const inner = s.w - 2 * L.pad;
  const rects = bars(l.code)
    .map((b) => `<rect x="${(L.barsX + b.x * L.mod).toFixed(4)}" y="${L.barsTop.toFixed(3)}" width="${(b.w * L.mod).toFixed(4)}" height="${L.barsH.toFixed(3)}"/>`)
    .join("");
  const font = `font-family="Arial, Helvetica, sans-serif" text-anchor="middle"`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${s.w}mm" height="${s.h}mm" viewBox="0 0 ${s.w} ${s.h}">`
    + `<rect width="${s.w}" height="${s.h}" fill="#fff"/>`
    + `<text x="${s.w / 2}" y="${L.business.toFixed(3)}" ${font} font-size="${L.fb.toFixed(3)}" font-weight="700" letter-spacing="0.15" fill="#000">${esc(fit(business.toUpperCase(), inner, L.fb))}</text>`
    + `<g fill="#000" shape-rendering="crispEdges">${rects}</g>`
    + `<text x="${s.w / 2}" y="${L.number.toFixed(3)}" ${font} font-size="${L.fn.toFixed(3)}" font-weight="700" letter-spacing="0.6" fill="#000">${esc(l.code)}</text>`
    + `<text x="${s.w / 2}" y="${L.product.toFixed(3)}" ${font} font-size="${L.fp.toFixed(3)}" fill="#333">${esc(fit(l.product, inner, L.fp))}</text>`
    + `</svg>`;
}

/** Stickers placed on A4 (for sticker sheets): columns and rows with a 2 mm gap and 8 mm margins. */
export function a4Grid(s: LabelSize) {
  const cols = Math.max(1, Math.floor((210 - 16 + 2) / (s.w + 2)));
  const rows = Math.max(1, Math.floor((297 - 16 + 2) / (s.h + 2)));
  return { cols, rows, perPage: cols * rows };
}

export type Layout = "roll" | "a4";

/** A printable HTML page: one sticker per page (label rolls) or A4 sheets. */
export function printHtml(business: string, labels: Label[], s: LabelSize, layoutKind: Layout): string {
  const svgs = labels.map((l) => labelSvg(business, l, s));
  const body = layoutKind === "roll"
    ? svgs.map((x) => `<div class="roll">${x}</div>`).join("")
    : (() => {
        const g = a4Grid(s);
        const pages: string[] = [];
        for (let i = 0; i < svgs.length; i += g.perPage) {
          pages.push(`<div class="sheet" style="grid-template-columns:repeat(${g.cols}, ${s.w}mm)">${svgs.slice(i, i + g.perPage).join("")}</div>`);
        }
        return pages.join("");
      })();
  const page = layoutKind === "roll" ? `${s.w}mm ${s.h}mm` : "A4";
  return `<!doctype html><html><head><meta charset="utf-8"><title>Labels</title><style>
@page { size: ${page}; margin: 0; }
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { background: #fff; }
.roll { width: ${s.w}mm; height: ${s.h}mm; page-break-after: always; break-after: page; overflow: hidden; }
.roll:last-child { page-break-after: auto; break-after: auto; }
.sheet { display: grid; gap: 2mm; padding: 8mm; width: 210mm; height: 297mm; align-content: start; page-break-after: always; break-after: page; }
.sheet:last-child { page-break-after: auto; break-after: auto; }
.sheet svg { outline: 0.1mm dashed #ccc; }
svg { display: block; }
</style></head><body>${body}</body></html>`;
}

/** Opens the system print dialog for the labels (a hidden frame, so no pop-up blocker gets in the way). */
export function printLabels(business: string, labels: Label[], s: LabelSize, layoutKind: Layout): Promise<void> {
  return new Promise((resolve) => {
    const frame = document.createElement("iframe");
    frame.setAttribute("aria-hidden", "true");
    frame.style.cssText = "position:fixed;right:0;bottom:0;width:0;height:0;border:0;";
    document.body.appendChild(frame);
    const doc = frame.contentDocument!;
    doc.open();
    doc.write(printHtml(business, labels, s, layoutKind));
    doc.close();
    setTimeout(() => {
      frame.contentWindow?.focus();
      frame.contentWindow?.print();
      setTimeout(() => { frame.remove(); resolve(); }, 1000);
    }, 300);
  });
}

/** PDF: one sticker per page at the sticker size (rolls), or A4 sheets. Vector bars, so they stay sharp. */
export async function labelsPdf(business: string, labels: Label[], s: LabelSize, layoutKind: Layout): Promise<Blob> {
  const { jsPDF } = await import("jspdf");
  const roll = layoutKind === "roll";
  const doc = roll
    ? new jsPDF({ unit: "mm", format: [s.w, s.h], orientation: s.w >= s.h ? "landscape" : "portrait" })
    : new jsPDF({ unit: "mm", format: "a4", orientation: "portrait" });
  const g = a4Grid(s);
  const PT = 72 / 25.4; // points per millimetre
  labels.forEach((l, i) => {
    let ox = 0;
    let oy = 0;
    if (roll) {
      if (i > 0) doc.addPage([s.w, s.h], s.w >= s.h ? "landscape" : "portrait");
    } else {
      const k = i % g.perPage;
      if (i > 0 && k === 0) doc.addPage("a4", "portrait");
      ox = 8 + (k % g.cols) * (s.w + 2);
      oy = 8 + Math.floor(k / g.cols) * (s.h + 2);
      doc.setDrawColor(200);
      doc.setLineWidth(0.1);
      doc.rect(ox, oy, s.w, s.h);
    }
    const L = layout(s, l.code);
    const inner = s.w - 2 * L.pad;
    doc.setTextColor(0);
    doc.setFont("helvetica", "bold");
    doc.setFontSize(L.fb * PT);
    doc.text(fit(business.toUpperCase(), inner, L.fb), ox + s.w / 2, oy + L.business, { align: "center" });
    doc.setFillColor(0, 0, 0);
    for (const b of bars(l.code)) doc.rect(ox + L.barsX + b.x * L.mod, oy + L.barsTop, b.w * L.mod, L.barsH, "F");
    doc.setFontSize(L.fn * PT);
    doc.text(l.code, ox + s.w / 2, oy + L.number, { align: "center" });
    doc.setFont("helvetica", "normal");
    doc.setFontSize(L.fp * PT);
    doc.setTextColor(51);
    doc.text(fit(l.product, inner, L.fp), ox + s.w / 2, oy + L.product, { align: "center" });
  });
  return doc.output("blob");
}

/** PNG at about 300 dpi: one sticker, or a sheet of all of them (up to 5 across). */
export async function labelsPng(business: string, labels: Label[], s: LabelSize): Promise<Blob> {
  const PX = 12; // pixels per millimetre ≈ 305 dpi
  const cols = Math.min(5, labels.length);
  const rows = Math.ceil(labels.length / cols);
  const gap = labels.length > 1 ? 2 : 0;
  const canvas = document.createElement("canvas");
  canvas.width = Math.round((cols * s.w + (cols - 1) * gap) * PX);
  canvas.height = Math.round((rows * s.h + (rows - 1) * gap) * PX);
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  for (let i = 0; i < labels.length; i++) {
    const svg = labelSvg(business, labels[i], s);
    const img = new Image();
    img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
    await img.decode();
    const x = (i % cols) * (s.w + gap) * PX;
    const y = Math.floor(i / cols) * (s.h + gap) * PX;
    ctx.drawImage(img, x, y, s.w * PX, s.h * PX);
  }
  return new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("PNG failed"))), "image/png"));
}

// ── Label printers (roadmap 138): commands for ZPL (Zebra) and TSPL (TSC and many low-cost printers), 203 dpi ─────────

const DOTS = 8; // dots per millimetre at 203 dpi
const ascii = (s: string) => s.normalize("NFKD").replace(/[^\x20-\x7e]/g, "");

/** ZPL II: one ^XA…^XZ block per sticker. */
export function labelsZpl(business: string, labels: Label[], s: LabelSize): string {
  const W = Math.round(s.w * DOTS);
  const H = Math.round(s.h * DOTS);
  return labels.map((l) => {
    const L = layout(s, l.code);
    const modDots = Math.max(2, Math.floor((L.mod * DOTS)));
    const barW = symbolWidth(l.code) * modDots;
    const x = Math.max(0, Math.round((W - barW) / 2));
    const inner = s.w - 2 * L.pad;
    const fb = Math.round(L.fb * DOTS), fn = Math.round(L.fn * DOTS), fp = Math.round(L.fp * DOTS);
    const line = (y: number, size: number, text: string) => `^FO0,${y}^A0N,${size},${size}^FB${W},1,0,C,0^FD${text.replace(/[\^~]/g, "")}^FS`;
    return [
      "^XA", `^PW${W}`, `^LL${H}`, "^CI28",
      line(Math.round(L.pad * DOTS), fb, ascii(fit(business.toUpperCase(), inner, L.fb))),
      `^BY${modDots},3^FO${x},${Math.round(L.barsTop * DOTS)}^BCN,${Math.round(L.barsH * DOTS)},N,N,N,A^FD${l.code}^FS`,
      line(Math.round((L.number - L.fn * 0.85) * DOTS), fn, l.code),
      line(Math.round((L.product - L.fp * 0.85) * DOTS), fp, ascii(fit(l.product, inner, L.fp))),
      "^XZ",
    ].join("\n");
  }).join("\n");
}

/** TSPL: SIZE / GAP once, then CLS … PRINT per sticker. */
export function labelsTspl(business: string, labels: Label[], s: LabelSize): string {
  const W = Math.round(s.w * DOTS);
  const head = [`SIZE ${s.w} mm, ${s.h} mm`, "GAP 2 mm, 0 mm", "DIRECTION 1", "CODEPAGE 1252"];
  const body = labels.map((l) => {
    const L = layout(s, l.code);
    const modDots = Math.max(2, Math.floor(L.mod * DOTS));
    const barW = symbolWidth(l.code) * modDots;
    const inner = s.w - 2 * L.pad;
    // Font "2" is 12×20 dots, "3" 16×24: centred by its fixed width.
    const text = (y: number, font: "2" | "3", t: string) => {
      const cw = font === "3" ? 16 : 12;
      const x = Math.max(0, Math.round((W - t.length * cw) / 2));
      return `TEXT ${x},${y},"${font}",0,1,1,"${t.replace(/"/g, "'")}"`;
    };
    return [
      "CLS",
      text(Math.round(L.pad * DOTS), "3", ascii(fit(business.toUpperCase(), inner, L.fb))),
      `BARCODE ${Math.max(0, Math.round((W - barW) / 2))},${Math.round(L.barsTop * DOTS)},"128",${Math.round(L.barsH * DOTS)},0,0,${modDots},${modDots},"${l.code}"`,
      text(Math.round((L.number - L.fn * 0.85) * DOTS), "3", l.code),
      text(Math.round((L.product - L.fp * 0.85) * DOTS), "2", ascii(fit(l.product, inner, L.fp))),
      "PRINT 1,1",
    ].join("\n");
  });
  return [...head, ...body].join("\n") + "\n";
}

export function download(blob: Blob, name: string) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  document.body.appendChild(a);
  a.click();
  setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
}
