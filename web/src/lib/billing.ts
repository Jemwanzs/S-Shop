/** Platform billing (roadmap 37–40): shared types, labels and the downloadable invoice / quotation / receipt. */
import type { Money } from "./types";
import { date, dateTime, moneyDoc } from "./format";
import { t } from "./i18n";

export type BillingStatus = "not_set" | "paid" | "pending" | "due_soon" | "grace" | "overdue";

export interface BillingSummary {
  status: BillingStatus;
  model: "subscription" | "one_off" | null;
  currency: string;
  amount: Money | null;
  frequency: string | null;
  custom_months: number | null;
  next_due: string | null;
  grace_days: number;
  outstanding: Money;
  open_invoices: number;
  oldest_open_due: string | null;
  last_payment_at: string | null;
  last_payment_amount: Money | null;
  period_start: string | null;
  period_end: string | null;
  one_off_amount: Money | null;
  one_off_status: "paid" | "pending" | null;
  maintenance: boolean;
  paid_total: Money;
}

export interface BillingPlan {
  model: "subscription" | "one_off";
  currency: string;
  one_off_amount: Money;
  recurring: boolean;
  amount: Money;
  frequency: string;
  custom_months: number;
  start_date: string | null;
  next_due_date: string | null;
  grace_days: number;
  auto_renew: boolean;
  notes?: string;
}

export interface BillingDocument {
  id: string;
  kind: "quotation" | "invoice";
  number: string;
  category: "subscription" | "one_off" | "maintenance" | "other";
  description: string;
  amount: Money;
  currency: string;
  issue_date: string;
  due_date: string;
  period_start: string | null;
  period_end: string | null;
  status: "open" | "accepted" | "declined" | "paid" | "void";
  quotation_id: string | null;
  paid_at: string | null;
  void_reason: string;
  created_at: string;
  overdue: boolean;
}

export interface BillingPayment {
  id: string;
  invoice_id: string;
  invoice_number: string;
  amount: Money;
  currency: string;
  method: string;
  reference: string;
  status: "pending" | "success" | "failed" | "abandoned";
  channel: string;
  receipt_no: string | null;
  paid_at: string | null;
  note: string;
  created_at: string;
}

export interface VendorPublic {
  bank_name: string;
  account_name: string;
  account_masked: string;
  branch: string;
  instructions: string;
}

export interface DocumentView {
  document: BillingDocument;
  payments: BillingPayment[];
  business: { name: string; phone: string; email: string; address: string };
  vendor: VendorPublic;
  support_phones: string[];
}

export const STATUS_LABEL: Record<BillingStatus, string> = {
  not_set: "No plan",
  paid: "Paid up",
  pending: "Payment pending",
  due_soon: "Due soon",
  grace: "In grace period",
  overdue: "Overdue",
};

export const STATUS_TONE: Record<BillingStatus, "success" | "warning" | "danger" | "neutral" | "info"> = {
  not_set: "neutral",
  paid: "success",
  pending: "warning",
  due_soon: "info",
  grace: "warning",
  overdue: "danger",
};

export const FREQUENCIES: [string, string][] = [
  ["monthly", "Monthly"],
  ["quarterly", "Quarterly"],
  ["semi_annual", "Semi-annual"],
  ["annual", "Annual"],
  ["custom", "Custom"],
];

export function frequencyLabel(f: string | null | undefined, months?: number | null): string {
  if (f === "custom") return `${t("Every")} ${months ?? 1} ${t("months")}`;
  return t(FREQUENCIES.find((x) => x[0] === f)?.[1] ?? "Monthly");
}

export const CATEGORY_LABEL: Record<BillingDocument["category"], string> = {
  subscription: "Subscription",
  one_off: "One-off",
  maintenance: "Maintenance",
  other: "Other",
};

export const METHOD_LABEL: Record<string, string> = { paystack: "Paystack", bank: "Bank transfer", mpesa: "M-Pesa", cash: "Cash", other: "Other" };

export function periodLabel(d: { period_start: string | null; period_end: string | null }): string {
  return d.period_start && d.period_end ? `${date(d.period_start)} – ${date(d.period_end)}` : "";
}

/** A4 invoice, quotation or (with `payment`) receipt, built in the browser like sales receipts. */
export async function billingPdf(v: DocumentView, payment?: BillingPayment) {
  const [{ jsPDF }, { default: autoTable }] = await Promise.all([import("jspdf"), import("jspdf-autotable")]);
  const d = v.document;
  const doc = new jsPDF({ unit: "mm", format: "a4" });
  const W = 210;
  const title = payment ? "RECEIPT" : d.kind === "quotation" ? "QUOTATION" : "INVOICE";
  const number = payment?.receipt_no ?? d.number;
  doc.setFont("helvetica", "bold");
  doc.setFontSize(20);
  doc.text("S'Shop", 18, 22);
  doc.setFontSize(16);
  doc.text(title, W - 18, 22, { align: "right" });
  doc.setFont("helvetica", "normal");
  doc.setFontSize(9);
  doc.text(`Support: ${v.support_phones.join(" / ")}`, 18, 28);
  doc.text(number, W - 18, 28, { align: "right" });

  let y = 42;
  doc.setFont("helvetica", "bold");
  doc.text(payment ? "Received from" : "Billed to", 18, y);
  doc.setFont("helvetica", "normal");
  [v.business.name, v.business.address, v.business.phone, v.business.email].filter(Boolean).forEach((line, i) => doc.text(line, 18, y + 5 + i * 4.5));
  const meta: [string, string][] = payment
    ? [["Receipt", number], ["Date paid", dateTime(payment.paid_at)], ["Invoice", d.number], ["Method", METHOD_LABEL[payment.method] ?? payment.method], ["Reference", payment.reference]]
    : [
        [d.kind === "quotation" ? "Quotation" : "Invoice", d.number],
        ["Issue date", date(d.issue_date)],
        [d.kind === "quotation" ? "Valid until" : "Due date", date(d.due_date)],
        ["Status", d.status === "open" && d.overdue ? "OVERDUE" : d.status.toUpperCase()],
      ];
  if (!payment && d.period_start) meta.push(["Billing period", periodLabel(d)]);
  meta.forEach(([k, val], i) => {
    doc.setFont("helvetica", "bold");
    doc.text(k, 120, y + i * 4.5);
    doc.setFont("helvetica", "normal");
    doc.text(val, W - 18, y + i * 4.5, { align: "right" });
  });

  autoTable(doc, {
    startY: y + 32,
    margin: { left: 18, right: 18 },
    theme: "striped",
    headStyles: { fillColor: [24, 24, 27] },
    styles: { fontSize: 9 },
    head: [["Description", "Category", "Amount"]],
    body: [[d.description + (d.period_start ? `\n${periodLabel(d)}` : ""), CATEGORY_LABEL[d.category], moneyDoc(d.amount, d.currency, true)]],
    columnStyles: { 2: { halign: "right" } },
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  y = (doc as any).lastAutoTable.finalY + 8;
  doc.setFont("helvetica", "bold");
  doc.setFontSize(11);
  doc.text(payment ? "Amount paid" : "Total", 120, y);
  doc.text(moneyDoc(payment?.amount ?? d.amount, d.currency, true), W - 18, y, { align: "right" });
  y += 12;
  doc.setFontSize(9);
  if (!payment && d.kind === "invoice" && d.status === "open") {
    doc.text("How to pay", 18, y);
    doc.setFont("helvetica", "normal");
    const lines = ["Online: Settings → Billing → Pay now (card / M-Pesa via Paystack)"];
    if (v.vendor.bank_name) lines.push(`Bank: ${v.vendor.bank_name}${v.vendor.account_masked ? `, Account: ${v.vendor.account_masked}` : ""}${v.vendor.account_name ? ` (${v.vendor.account_name})` : ""}`);
    lines.push(`Use ${d.number} as the payment reference.`);
    if (v.vendor.instructions) lines.push(v.vendor.instructions);
    lines.forEach((l, i) => doc.text(l, 18, y + 5 + i * 4.5));
  } else if (payment) {
    doc.setFont("helvetica", "normal");
    doc.text("Thank you for your payment.", 18, y);
  }
  doc.save(`${number}.pdf`);
}
