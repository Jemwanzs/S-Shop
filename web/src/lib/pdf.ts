/** Client-side PDF generation for report tables (receipts: lib/receipt.ts). Loaded on demand. */
import { amount, dateTime } from "./format";

async function load() {
  const [{ jsPDF }, { default: autoTable }] = await Promise.all([import("jspdf"), import("jspdf-autotable")]);
  return { jsPDF, autoTable };
}

export interface PdfColumn {
  key: string;
  label: string;
  kind: string;
}

export async function tablePdf(opts: {
  title: string;
  subtitle: string;
  business: string;
  columns: PdfColumn[];
  rows: Record<string, unknown>[];
  totals: Record<string, unknown>;
  filename: string;
}) {
  const { jsPDF, autoTable } = await load();
  const landscape = opts.columns.length > 6;
  const doc = new jsPDF({ orientation: landscape ? "landscape" : "portrait", unit: "mm", format: "a4" });
  doc.setFont("helvetica", "bold");
  doc.setFontSize(14);
  doc.text(`${opts.business} — ${opts.title}`, 12, 14);
  doc.setFont("helvetica", "normal");
  doc.setFontSize(9);
  doc.text(opts.subtitle, 12, 20);
  const fmt = (kind: string, v: unknown) => {
    if (v === null || v === undefined || v === "") return "";
    if (kind === "money") return amount(v as string, true);
    if (kind === "int") return amount(v as string);
    if (kind === "percent") return `${v}%`;
    if (kind === "datetime") return dateTime(String(v));
    if (kind === "date") return String(v);
    return String(v);
  };
  const numeric = (k: string) => ["money", "int", "percent"].includes(k);
  autoTable(doc, {
    startY: 25,
    margin: { left: 10, right: 10 },
    styles: { fontSize: 7.5, cellPadding: 1.5 },
    headStyles: { fillColor: [232, 92, 14], textColor: 255 },
    head: [opts.columns.map((c) => c.label)],
    body: opts.rows.map((r) => opts.columns.map((c) => fmt(c.kind, r[c.key]))),
    foot: [opts.columns.map((c, i) => (i === 0 ? "Total" : opts.totals[c.key] !== undefined ? fmt(c.kind, opts.totals[c.key]) : ""))],
    footStyles: { fillColor: [244, 231, 218], textColor: 20, fontStyle: "bold" },
    columnStyles: Object.fromEntries(opts.columns.map((c, i) => [i, { halign: numeric(c.kind) ? "right" : "left" }])),
  });
  doc.save(opts.filename);
}
