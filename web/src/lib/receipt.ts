/** Digital receipts (roadmap 65–67): one stored snapshot → the same 50 mm receipt on screen, as PDF, image, print, email
 * and WhatsApp. The server freezes the snapshot when the receipt is issued; this file only draws it. */
import mark from "@/assets/sshop-mark.png";
import { api } from "./api";

export interface ReceiptLine { name: string; qty: number; price: string; total: string }
export interface ReceiptSnapshot {
  kind: "original" | "adjustment";
  title: string;
  number: string;
  receipt_no: string;
  at: string;
  business_date: string;
  status: string;
  currency: string;
  timezone: string;
  font: "sans" | "thermal";
  business: { name: string; logo: string | null; phone: string | null };
  branch: { name: string | null; phone: string | null };
  customer: string | null;
  served_by: string | null;
  footer: string;
  signed_by: string;
  items?: ReceiptLine[];
  totals?: { subtotal: string; discount: string; redeemed_points: number; redeemed_value: string; total: string; paid: string; balance: string | null; method: string };
  payments?: { method: string; amount: string; reference: string | null }[];
  points_earned?: number;
  notes?: string;
  adjustment?: {
    reference: string;
    kind: string;
    reason: string;
    original_receipt: string;
    original_at: string;
    returned: { name: string; qty: number; total: string }[];
    remaining: { name: string; qty: number; price: string }[];
    original_total: string;
    refund: string;
    refund_method: string;
    refunded_total: string;
    customer_credit: string;
    net_sale_value: string;
    exchange: { receipt_no: string; total: string; difference: string } | null;
    approved_by: string | null;
    loyalty: { original: number; reversed: number; unrecovered: number; net: number } | null;
  };
}
export interface ReceiptRecord { id: string; kind: "original" | "adjustment"; number: string; created_at: string; snapshot: ReceiptSnapshot }

const n = (v: string | number | null | undefined) => Number(v ?? 0);
/** Amounts without the currency (shown once, on TOTAL), no needless decimals. */
export const amt = (v: string | number | null | undefined) => {
  const x = n(v);
  return x.toLocaleString("en-KE", { minimumFractionDigits: Number.isInteger(x) ? 0 : 2, maximumFractionDigits: 2 });
};
export const when = (iso: string, tz: string) => {
  try {
    return new Intl.DateTimeFormat("en-GB", { day: "2-digit", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit", hour12: false, timeZone: tz }).format(new Date(iso));
  } catch {
    return new Date(iso).toLocaleString();
  }
};
export const logoUrl = (hash: string | null | undefined) => (hash ? `/api/receipt-assets/${hash}` : null);

/** Rows of the receipt body, in order — used by both the HTML receipt and the PDF so they never differ. */
export type Row =
  | { t: "kv"; k: string; v: string; strong?: boolean; big?: boolean }
  | { t: "item"; name: string; detail: string; total: string; muted?: boolean }
  | { t: "rule" }
  | { t: "heading"; text: string }
  | { t: "note"; text: string };

export function receiptRows(s: ReceiptSnapshot): Row[] {
  const rows: Row[] = [];
  const kv = (k: string, v: string | number | null | undefined, opts: { strong?: boolean; big?: boolean } = {}) => {
    if (v !== null && v !== undefined && v !== "") rows.push({ t: "kv", k, v: String(v), ...opts });
  };
  if (s.kind === "original") {
    for (const i of s.items ?? []) rows.push({ t: "item", name: i.name, detail: `${i.qty} × ${amt(i.price)}`, total: amt(i.total) });
    rows.push({ t: "rule" });
    const tt = s.totals!;
    if (n(tt.discount) > 0) kv("Subtotal", amt(tt.subtotal));
    if (n(tt.discount) > 0) kv("Discount", `−${amt(tt.discount)}`);
    if (n(tt.redeemed_value) > 0) kv(`Points redeemed (${tt.redeemed_points})`, `−${amt(tt.redeemed_value)}`);
    kv("TOTAL", `${s.currency} ${amt(tt.total)}`, { strong: true, big: true });
    kv("Payment", tt.method);
    for (const p of s.payments ?? []) if (p.reference) kv(`${p.method} ref`, p.reference);
    if (n(tt.paid) > 0 && n(tt.paid) !== n(tt.total)) kv("Amount paid", amt(tt.paid));
    else if (n(tt.paid) > 0) kv("Amount paid", amt(tt.paid));
    if (tt.balance && n(tt.balance) > 0) kv("Balance", amt(tt.balance), { strong: true });
    if ((s.points_earned ?? 0) > 0) kv("Loyalty points earned", `+${s.points_earned}`);
    if (s.notes) rows.push({ t: "note", text: s.notes });
  } else {
    const a = s.adjustment!;
    kv("Original receipt", a.original_receipt);
    kv(a.kind === "cancellation" ? "Cancellation ref" : a.exchange ? "Exchange ref" : "Return ref", a.reference);
    kv("Status", s.status, { strong: true });
    rows.push({ t: "rule" });
    rows.push({ t: "heading", text: a.kind === "cancellation" ? "Items reversed" : "Items returned" });
    for (const i of a.returned) rows.push({ t: "item", name: i.name, detail: `${i.qty} returned`, total: `−${amt(i.total)}` });
    if (a.exchange) {
      rows.push({ t: "heading", text: "Replacement" });
      rows.push({ t: "item", name: `Receipt ${a.exchange.receipt_no}`, detail: "new items", total: amt(a.exchange.total) });
    }
    if (a.remaining.length && a.kind !== "cancellation") {
      rows.push({ t: "heading", text: "Kept" });
      for (const i of a.remaining) rows.push({ t: "item", name: i.name, detail: `${i.qty} × ${amt(i.price)}`, total: amt(n(i.price) * i.qty), muted: true });
    }
    rows.push({ t: "rule" });
    kv("Original sale total", amt(a.original_total));
    if (a.exchange) kv(n(a.exchange.difference) >= 0 ? "Exchange difference paid" : "Exchange difference refunded", amt(Math.abs(n(a.exchange.difference))));
    if (n(a.refund) > 0 && !a.exchange) kv(a.refund_method ? `Refunded (${a.refund_method})` : "Refunded", `−${amt(a.refund)}`);
    if (n(a.customer_credit) > 0) kv("Held as customer credit", amt(a.customer_credit));
    kv("NET SALE VALUE", `${s.currency} ${amt(a.net_sale_value)}`, { strong: true, big: true });
    if (a.loyalty) {
      kv("Original points", a.loyalty.original);
      if (a.loyalty.reversed > 0) kv("Points reversed", `−${a.loyalty.reversed}`);
      if (a.loyalty.unrecovered > 0) kv("Points not recoverable", a.loyalty.unrecovered);
      kv("Net loyalty points", a.loyalty.net, { strong: true });
    }
    if (a.approved_by) kv("Approved by", a.approved_by);
    if (a.reason) rows.push({ t: "note", text: `Reason: ${a.reason}` });
  }
  return rows;
}

async function dataUrl(src: string): Promise<{ url: string; w: number; h: number } | null> {
  try {
    const blob = await (await fetch(src)).blob();
    const url = await new Promise<string>((res, rej) => {
      const r = new FileReader();
      r.onload = () => res(String(r.result));
      r.onerror = () => rej(r.error);
      r.readAsDataURL(blob);
    });
    const dims = await new Promise<{ w: number; h: number }>((res) => {
      const img = new Image();
      img.onload = () => res({ w: img.naturalWidth || 1, h: img.naturalHeight || 1 });
      img.onerror = () => res({ w: 1, h: 1 });
      img.src = url;
    });
    return { url, ...dims };
  } catch {
    return null;
  }
}

const W = 50; // mm — maximum receipt width
const M = 2.5; // mm margin

/** The receipt as a 50 mm wide PDF whose height fits the content exactly (two passes: measure, then draw). */
export async function receiptPdf(s: ReceiptSnapshot): Promise<Blob> {
  const { jsPDF } = await import("jspdf");
  const logo = s.business.logo ? await dataUrl(logoUrl(s.business.logo)!) : null;
  const icon = await dataUrl(mark);
  const font = s.font === "thermal" ? "courier" : "helvetica";
  const draw = (doc: InstanceType<typeof jsPDF>) => {
    let y = M + 1;
    const right = W - M;
    const set = (size: number, bold = false) => {
      doc.setFont(font, bold ? "bold" : "normal");
      doc.setFontSize(size);
    };
    const center = (text: string, size: number, bold = false) => {
      set(size, bold);
      for (const line of doc.splitTextToSize(text, W - 2 * M) as string[]) {
        doc.text(line, W / 2, y, { align: "center" });
        y += size * 0.4;
      }
    };
    const rule = () => {
      doc.setLineDashPattern([0.6, 0.6], 0);
      doc.setDrawColor(150);
      doc.setLineWidth(0.15);
      doc.line(M, y, right, y);
      doc.setLineDashPattern([], 0);
      y += 2.2;
    };
    if (logo) {
      const lw = Math.min(14, (14 * logo.w) / logo.h);
      const lh = (lw * logo.h) / logo.w;
      doc.addImage(logo.url, (W - lw) / 2, y, lw, lh);
      y += lh + 2;
    }
    center(s.business.name, 8.5, true);
    y += 0.4;
    if (s.branch.name) center(s.branch.name, 6.5);
    const phone = s.branch.phone || s.business.phone;
    if (phone) center(phone, 6.5);
    y += 1;
    rule();
    center(s.title, 7.5, true);
    y += 0.6;
    const kvLine = (k: string, v: string, size = 6.5, bold = false) => {
      set(size, bold);
      const vw = doc.getTextWidth(v);
      const keyLines = doc.splitTextToSize(k, W - 2 * M - vw - 2) as string[];
      keyLines.forEach((line, i) => doc.text(line, M, y + i * size * 0.4));
      doc.text(v, right, y, { align: "right" });
      y += Math.max(1, keyLines.length) * size * 0.4 + 0.9;
    };
    kvLine(s.kind === "adjustment" ? "Ref:" : "Receipt:", s.number);
    kvLine("Date:", when(s.at, s.timezone));
    if (s.customer) kvLine("Customer:", s.customer);
    rule();
    if (s.kind === "original") {
      set(6, true);
      doc.text("Item", M, y);
      doc.text("Total", right, y, { align: "right" });
      y += 3;
    }
    for (const r of receiptRows(s)) {
      if (r.t === "rule") rule();
      else if (r.t === "heading") {
        set(6.2, true);
        doc.text(r.text.toUpperCase(), M, y);
        y += 3;
      } else if (r.t === "note") {
        set(5.8);
        doc.setTextColor(90);
        for (const line of doc.splitTextToSize(r.text, W - 2 * M) as string[]) {
          doc.text(line, M, y);
          y += 2.5;
        }
        doc.setTextColor(0);
        y += 0.5;
      } else if (r.t === "item") {
        set(6.5, true);
        if (r.muted) doc.setTextColor(110);
        const lines = doc.splitTextToSize(r.name, W - 2 * M) as string[];
        lines.forEach((line) => {
          doc.text(line, M, y);
          y += 2.7;
        });
        set(6.2);
        doc.text(r.detail, M + 1, y);
        doc.text(r.total, right, y, { align: "right" });
        doc.setTextColor(0);
        y += 3.3;
      } else {
        kvLine(r.k, r.v, r.big ? 8 : 6.5, !!r.strong);
        if (r.big) y += 0.6;
      }
    }
    if (s.served_by) {
      rule();
      kvLine("Served by:", s.served_by);
    }
    rule();
    if (s.footer) center(s.footer, 6.5, true);
    y += 0.8;
    set(5.6);
    doc.setTextColor(100);
    doc.text(`Digitally signed by ${s.signed_by}`, W / 2, y, { align: "center" });
    doc.setTextColor(0);
    y += 3;
    if (icon) {
      const iw = 4;
      doc.addImage(icon.url, (W - iw) / 2, y, iw, (iw * icon.h) / icon.w);
      y += (iw * icon.h) / icon.w;
    }
    return y + M;
  };
  const probe = new jsPDF({ unit: "mm", format: [W, 2000] });
  const height = Math.ceil(draw(probe));
  const doc = new jsPDF({ unit: "mm", format: [W, Math.max(height, 40)] });
  draw(doc);
  doc.setProperties({ title: `${s.business.name} ${s.number}`, author: s.business.name, subject: s.title });
  return doc.output("blob");
}

export const fileName = (s: ReceiptSnapshot, ext: "pdf" | "png") => `${s.number.replace(/[^\w-]+/g, "-")}.${ext}`;

export function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

/** Prints the PDF itself (the same 50 mm layout), not the surrounding page. */
export async function printReceipt(s: ReceiptSnapshot) {
  const blob = await receiptPdf(s);
  const url = URL.createObjectURL(blob);
  const frame = document.createElement("iframe");
  frame.style.position = "fixed";
  frame.style.width = "0";
  frame.style.height = "0";
  frame.style.border = "0";
  frame.src = url;
  document.body.appendChild(frame);
  frame.onload = () => {
    try {
      frame.contentWindow?.focus();
      frame.contentWindow?.print();
    } catch {
      window.open(url, "_blank");
    }
    setTimeout(() => {
      frame.remove();
      URL.revokeObjectURL(url);
    }, 60_000);
  };
}

/** The on-screen receipt element as a crisp PNG. */
export async function receiptPng(el: HTMLElement): Promise<Blob> {
  const { default: html2canvas } = await import("html2canvas");
  const canvas = await html2canvas(el, { scale: 4, backgroundColor: "#ffffff", useCORS: true });
  return new Promise<Blob>((res, rej) => canvas.toBlob((b) => (b ? res(b) : rej(new Error("Image failed"))), "image/png"));
}

export const toBase64 = (blob: Blob) =>
  new Promise<string>((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(String(r.result).split(",")[1] ?? "");
    r.onerror = () => rej(r.error);
    r.readAsDataURL(blob);
  });

export const receiptLink = (id: string) => api<{ url: string }>(`/receipts/${id}/link`, { method: "POST" }).then((r) => r.url);

/** Kenyan numbers → 2547…; others kept as digits. */
export function waNumber(mobile: string | null | undefined) {
  const d = (mobile ?? "").replace(/\D/g, "");
  if (d.length === 10 && d.startsWith("0")) return `254${d.slice(1)}`;
  if (d.length === 9 && /^[71]/.test(d)) return `254${d}`;
  return d;
}

export function shareMessage(s: ReceiptSnapshot, link: string) {
  const first = s.customer?.split(" ")[0];
  const what = s.kind === "adjustment" ? "your updated receipt" : "your receipt";
  return `Hello${first ? ` ${first}` : ""}! Thank you for shopping with ${s.business.name}. Here is ${what} ${s.number}: ${link}\nWe appreciate your business!`;
}
