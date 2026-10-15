/**
 * Roadmap 124 — a customer's credit statement as an A4 PDF: business (logo per Branding, prepared once as for receipts
 * so transparency is kept), customer, period, opening balance, every charge / payment / return / write-off with a
 * running balance, closing balance and what is still open. Built in the browser like receipts.
 */
import { amount, date, dateTime } from "./format";
import { logoUrl, receiptImage } from "./receipt";

export interface StatementLine {
  date: string;
  kind: "charge" | "payment" | "return" | "write_off";
  reference: string;
  /** English wording (the PDF); the screen words it from `kind` + `detail` in the reader's language. */
  description: string;
  /** Payment method, or the return kind (return / recall / cancellation). */
  detail: string;
  /** The payment's reference, or the sale a return was for. */
  detail_reference: string;
  charge: string | null;
  credit: string | null;
  balance: string;
}
export interface Statement {
  business: { name: string; phone: string; logo: string | null };
  customer: { id: string; name: string; mobile: string };
  from: string | null;
  to: string;
  currency: string;
  timezone: string;
  opening_balance: string;
  closing_balance: string;
  lines: StatementLine[];
  open: { receipt_no: string; date: string; due_date: string; balance: string; overdue: boolean }[];
  generated_at: string;
  generated_by: string;
}

const RETURN_LABEL: Record<string, string> = { recall: "Goods recalled", cancellation: "Sale cancelled", return: "Goods returned" };
/** A statement line in the reader's language (`tr` = the app's translator, `method` = its payment-method label). */
export function lineLabel(l: StatementLine, tr: (s: string) => string, method: (m: string) => string): string {
  switch (l.kind) {
    case "charge":
      return tr("Sale on credit");
    case "payment":
      return `${tr("Payment")} · ${method(l.detail)}${l.detail_reference ? ` ${l.detail_reference}` : ""}`;
    case "return":
      return `${tr(RETURN_LABEL[l.detail] ?? "Goods returned")} · ${l.detail_reference}`;
    case "write_off":
      return tr("Written off");
    default:
      return tr(l.description);
  }
}

export const periodText = (s: Statement) => (s.from ? `${date(s.from)} – ${date(s.to)}` : `Up to ${date(s.to)}`);
export const statementFile = (s: Statement) => `Statement-${s.customer.name.replace(/[^\w]+/g, "-")}-${s.to}.pdf`;

export async function statementPdf(s: Statement): Promise<Blob> {
  const [{ jsPDF }, { default: autoTable }] = await Promise.all([import("jspdf"), import("jspdf-autotable")]);
  const doc = new jsPDF({ unit: "mm", format: "a4" });
  const W = 210;
  const money = (v: string | null) => (v === null ? "" : amount(v, true));
  let y = 16;
  const logo = s.business.logo ? await receiptImage(logoUrl(s.business.logo)!) : null;
  if (logo) {
    const h = 16, w = Math.min(40, (h * logo.w) / logo.h);
    doc.addImage(logo.url, "PNG", 18, y - 4, w, (w * logo.h) / logo.w, undefined, "SLOW");
  }
  const left = logo ? 18 + Math.min(40, (16 * logo.w) / logo.h) + 4 : 18;
  doc.setFont("helvetica", "bold");
  doc.setFontSize(15);
  doc.text(s.business.name, left, y + 2);
  doc.setFont("helvetica", "normal");
  doc.setFontSize(9);
  if (s.business.phone) doc.text(s.business.phone, left, y + 7);
  doc.setFont("helvetica", "bold");
  doc.setFontSize(14);
  doc.text("CREDIT STATEMENT", W - 18, y + 2, { align: "right" });
  doc.setFont("helvetica", "normal");
  doc.setFontSize(9);
  doc.text(periodText(s), W - 18, y + 7, { align: "right" });

  y = 40;
  doc.setFont("helvetica", "bold");
  doc.text("Customer", 18, y);
  doc.setFont("helvetica", "normal");
  doc.text([s.customer.name, `+${s.customer.mobile}`], 18, y + 5);
  const summary: [string, string][] = [
    ["Opening balance", money(s.opening_balance)],
    ["Closing balance", money(s.closing_balance)],
    ["Currency", s.currency],
  ];
  summary.forEach(([k, v], i) => {
    doc.setFont("helvetica", "bold");
    doc.text(k, 125, y + i * 5);
    doc.setFont("helvetica", "normal");
    doc.text(v, W - 18, y + i * 5, { align: "right" });
  });

  autoTable(doc, {
    startY: y + 18,
    margin: { left: 18, right: 18 },
    theme: "striped",
    styles: { fontSize: 8.5, cellPadding: 1.6 },
    headStyles: { fillColor: [232, 92, 14], textColor: 255 },
    head: [["Date", "Reference", "Description", "Charged", "Paid / reduced", "Balance"]],
    body: [
      ["", "", "Opening balance", "", "", money(s.opening_balance)],
      ...s.lines.map((l) => [date(l.date), l.reference, l.description, money(l.charge), money(l.credit), money(l.balance)]),
    ],
    foot: [["", "", "Closing balance", "", "", money(s.closing_balance)]],
    footStyles: { fillColor: [244, 231, 218], textColor: 20, fontStyle: "bold" },
    columnStyles: { 3: { halign: "right" }, 4: { halign: "right" }, 5: { halign: "right" } },
  });
  // What is still to be paid, by due date.
  if (s.open.length) {
    const after = (doc as unknown as { lastAutoTable: { finalY: number } }).lastAutoTable.finalY + 8;
    doc.setFont("helvetica", "bold");
    doc.setFontSize(10);
    doc.text("Still to pay", 18, after);
    autoTable(doc, {
      startY: after + 2,
      margin: { left: 18, right: 18 },
      theme: "plain",
      styles: { fontSize: 8.5, cellPadding: 1.4 },
      head: [["Receipt", "Sale date", "Due date", "Balance"]],
      body: s.open.map((o) => [o.receipt_no, date(o.date), `${date(o.due_date)}${o.overdue ? " · OVERDUE" : ""}`, money(o.balance)]),
      columnStyles: { 3: { halign: "right" } },
    });
  }
  const pages = doc.getNumberOfPages();
  for (let i = 1; i <= pages; i++) {
    doc.setPage(i);
    doc.setFont("helvetica", "normal");
    doc.setFontSize(7.5);
    doc.setTextColor(120);
    doc.text(`Generated ${dateTime(s.generated_at)} by ${s.generated_by} · Page ${i} of ${pages} · Powered by S'Shop`, W / 2, 289, { align: "center" });
    doc.setTextColor(0);
  }
  return doc.output("blob");
}
