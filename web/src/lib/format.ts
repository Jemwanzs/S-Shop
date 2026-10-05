/** Display helpers. Amounts are rendered whole-shilling with grouping (legacy style). */

type Num = number | string | null | undefined;

export const toNum = (v: Num): number => (v === null || v === undefined || v === "" ? 0 : Number(v));

const whole = new Intl.NumberFormat("en-KE", { maximumFractionDigits: 0 });
const two = new Intl.NumberFormat("en-KE", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

/**
 * Display currency for app screens (User preferences). Amounts are recorded in the business currency (KES);
 * figures on screen are converted with the current rate and shown without a symbol — the active currency is
 * shown once in the profile instead of beside every number.
 */
const display = { code: "KES", rate: 1 };
export function setDisplayCurrency(code: string, rate: number) {
  display.code = code;
  display.rate = rate > 0 ? rate : 1;
}
export const displayCurrency = () => display.code;

/** Screen figure in the user's display currency, no symbol. Foreign currencies always show cents. */
export function money(v: Num, decimals = false): string {
  const n = toNum(v) * display.rate;
  return (decimals || display.code !== "KES" ? two : whole).format(n);
}

/** Document figure (receipts, PDFs, messages to customers): business currency with its symbol. */
export function moneyDoc(v: Num, currency = "KSh", decimals = false): string {
  return `${currency} ${(decimals ? two : whole).format(toNum(v))}`;
}

export function amount(v: Num, decimals = false): string {
  return (decimals ? two : whole).format(toNum(v));
}

export function signed(v: Num): string {
  const n = toNum(v);
  return `${n > 0 ? "+" : n < 0 ? "−" : ""}${whole.format(Math.abs(n))}`;
}

export function count(v: Num): string {
  return whole.format(toNum(v));
}

export function compact(v: Num): string {
  return new Intl.NumberFormat("en-KE", { notation: "compact", maximumFractionDigits: 1 }).format(toNum(v));
}

export function date(v?: string | null): string {
  if (!v) return "—";
  const d = new Date(v.length === 10 ? `${v}T12:00:00` : v);
  return d.toLocaleDateString("en-GB", { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function dateTime(v?: string | null): string {
  if (!v) return "—";
  return new Date(v).toLocaleString("en-GB", { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" });
}

export function time(v?: string | null): string {
  if (!v) return "";
  return new Date(v).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

export function ago(v?: string | null): string {
  if (!v) return "";
  const s = (Date.now() - new Date(v).getTime()) / 1000;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  if (s < 86400 * 7) return `${Math.floor(s / 86400)}d ago`;
  return date(v);
}

/** 254712345678 → 0712 345 678 */
export function phone(m?: string | null): string {
  if (!m) return "";
  if (m.startsWith("254") && m.length === 12) return `0${m.slice(3, 6)} ${m.slice(6, 9)} ${m.slice(9)}`;
  return m;
}

/** Privacy mask used in lists: +254 798 ***04 */
export function maskPhone(m?: string | null): string {
  if (!m) return "";
  if (m.startsWith("254") && m.length >= 12) return `+254 ${m.slice(3, 6)} ***${m.slice(-2)}`;
  return m.length > 5 ? `${m.slice(0, 3)}***${m.slice(-2)}` : m;
}

export function initials(name?: string | null): string {
  return (name ?? "?").trim().split(/\s+/).slice(0, 2).map((p) => p[0]?.toUpperCase() ?? "").join("") || "?";
}

export const titleCase = (s: string) => s.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());

export function todayIso(): string {
  const d = new Date();
  return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
}

export function methodLabel(m: string): string {
  return ({ mpesa: "M-Pesa", cash: "Cash", credit: "Credit Sale", legacy: "Legacy import" } as Record<string, string>)[m] ?? titleCase(m);
}
