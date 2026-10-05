/** Client-side PDF generation (receipts and report tables). Loaded on demand. */
import type { SaleDetail } from "./types";
import { amount, dateTime, methodLabel, moneyDoc, phone, toNum } from "./format";

async function load() {
  const [{ jsPDF }, { default: autoTable }] = await Promise.all([import("jspdf"), import("jspdf-autotable")]);
  return { jsPDF, autoTable };
}

export async function receiptPdf(d: SaleDetail) {
  const { jsPDF, autoTable } = await load();
  const c = d.business.currency;
  const doc = new jsPDF({ unit: "mm", format: [80, 200 + d.items.length * 8] });
  const w = 80;
  let y = 8;
  const center = (text: string, size = 9, bold = false) => {
    doc.setFont("helvetica", bold ? "bold" : "normal");
    doc.setFontSize(size);
    doc.text(text, w / 2, y, { align: "center" });
    y += size * 0.45 + 1.2;
  };
  center(d.business.name, 12, true);
  center(d.sale.branch_name);
  if (d.business.phone) center(d.business.phone, 8);
  y += 2;
  center(`Receipt ${d.sale.receipt_no}`, 9, true);
  center(dateTime(d.sale.created_at), 8);
  if (d.sale.customer) center(`${d.sale.customer.name} · ${phone(d.sale.customer.mobile)}`, 8);

  autoTable(doc, {
    startY: y + 2,
    margin: { left: 4, right: 4 },
    theme: "plain",
    styles: { fontSize: 8, cellPadding: 0.8 },
    headStyles: { fontStyle: "bold" },
    head: [["Item", "Qty", "Price", "Total"]],
    body: d.items.map((i) => [
      i.product_name + (toNum(i.discount) > 0 ? `\n(disc ${amount(i.discount)})` : ""),
      String(i.quantity),
      amount(i.unit_price),
      amount(i.line_total),
    ]),
    columnStyles: { 1: { halign: "right" }, 2: { halign: "right" }, 3: { halign: "right" } },
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  y = (doc as any).lastAutoTable.finalY + 4;
  const row = (l: string, v: string, bold = false) => {
    doc.setFont("helvetica", bold ? "bold" : "normal");
    doc.setFontSize(bold ? 10 : 8.5);
    doc.text(l, 4, y);
    doc.text(v, w - 4, y, { align: "right" });
    y += bold ? 5.5 : 4.5;
  };
  if (toNum(d.sale.discount_total) > 0) row("Discount", `-${moneyDoc(d.sale.discount_total, c)}`);
  if (toNum(d.sale.redeemed_value) > 0) row(`Points redeemed (${d.sale.redeemed_points})`, `-${moneyDoc(d.sale.redeemed_value, c)}`);
  row("TOTAL", moneyDoc(d.sale.total, c), true);
  row("Payment", methodLabel(d.sale.payment_method));
  row("Amount paid", moneyDoc(d.sale.amount_paid, c));
  if (d.credit) row("Balance", moneyDoc(d.credit.balance, c));
  if (d.payments[0]?.reference) row("Reference", d.payments[0].reference);
  if (d.sale.points_earned > 0) row("Loyalty points earned", `+${d.sale.points_earned}`);
  row("Served by", d.sale.user_name ?? "");
  y += 3;
  if (d.business.receipt_footer) center(d.business.receipt_footer, 8);
  doc.save(`${d.sale.receipt_no}.pdf`);
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
