import type { PosProduct, Settings } from "@/lib/types";
import { toNum } from "@/lib/format";

/**
 * One pricing model: the selling price is the truth; the discount is derived
 * (marked − selling). Entering a discount simply sets the selling price, so a
 * discount can never be counted twice.
 */
export interface CartLine {
  key: string;
  product: PosProduct;
  quantity: number;
  unitPrice: number;
  barcode?: string;
  /** Roadmap 115: this line's own sale date (yyyy-mm-dd); unset = the sale's date. */
  saleDate?: string;
}

// Roadmap 131: a product without a selling price has no marked price to discount from — the typed price is its price.
export const marked = (l: CartLine) => (l.product.marked_price == null ? l.unitPrice : toNum(l.product.marked_price));
export const unitDiscount = (l: CartLine) => marked(l) - l.unitPrice;
export const lineTotal = (l: CartLine) => Math.round(l.unitPrice * l.quantity * 100) / 100;
export const exceedsMax = (l: CartLine) => l.product.max_discount !== null && unitDiscount(l) > toNum(l.product.max_discount);

/**
 * Mirrors server/src/loyalty.rs::sale_points (roadmap 106): points on the sale's eligible amount per earning rate —
 * net payable by default (or gross), reduced by the part paid with points unless that earns too — then shared back
 * over the lines (largest remainder). Returns the total and each line's share by cart key.
 */
export function salePoints(lines: CartLine[], s: Settings, share = 1): { total: number; perLine: Map<string, number> } {
  const perLine = new Map<string, number>();
  if (!s.loyalty.enabled) return { total: 0, perLine };
  const groups = new Map<string, { threshold: number; per: number; items: { key: string; amount: number }[] }>();
  for (const l of lines) {
    const threshold = l.product.loyalty_threshold !== null ? toNum(l.product.loyalty_threshold) : toNum(s.loyalty.threshold);
    const per = l.product.loyalty_points_per ?? s.loyalty.points_per;
    const amount = (s.loyalty.earn_base === "gross" ? marked(l) * l.quantity : lineTotal(l)) * share;
    if (!l.product.loyalty_eligible || amount <= 0 || threshold <= 0 || per <= 0) continue;
    const k = `${threshold}:${per}`;
    const g = groups.get(k) ?? { threshold, per, items: [] };
    g.items.push({ key: l.key, amount });
    groups.set(k, g);
  }
  let total = 0;
  for (const g of groups.values()) {
    const amount = g.items.reduce((a, i) => a + i.amount, 0);
    const blocks = amount / g.threshold;
    const points = (s.loyalty.rounding === "nearest" ? Math.floor(blocks + 0.5 + 1e-9) : Math.floor(blocks + 1e-9)) * g.per;
    total += points;
    let given = 0;
    const rest = g.items.map((i) => {
      const exact = (points * i.amount) / amount;
      const whole = Math.floor(exact + 1e-9);
      perLine.set(i.key, whole);
      given += whole;
      return { key: i.key, frac: exact - whole };
    });
    rest.sort((a, b) => b.frac - a.frac).slice(0, Math.max(0, points - given)).forEach((r) => perLine.set(r.key, (perLine.get(r.key) ?? 0) + 1));
  }
  return { total, perLine };
}

/** The most points may pay on this sale: the business's share, never above 50 % (server-enforced). */
export function redeemCap(net: number, s: Settings) {
  const pct = Math.min(50, Math.max(0, s.loyalty.max_redemption_percent ?? 50));
  const value = Math.round(net * pct) / 100;
  const pv = toNum(s.loyalty.point_value);
  return { pct, value, points: pv > 0 ? Math.floor(value / pv + 1e-9) : 0 };
}

export function totals(lines: CartLine[], s: Settings, redeemPoints = 0) {
  const gross = lines.reduce((a, l) => a + marked(l) * l.quantity, 0);
  const net = lines.reduce((a, l) => a + lineTotal(l), 0);
  const redeemValue = Math.round(redeemPoints * toNum(s.loyalty.point_value) * 100) / 100;
  const payable = Math.max(0, Math.round((net - redeemValue) * 100) / 100);
  const share = redeemValue > 0 && !s.loyalty.earn_on_redeemed && net > 0 ? payable / net : 1;
  const points = net >= toNum(s.loyalty.min_spend) ? salePoints(lines, s, share).total : 0;
  return { gross, discount: gross - net, net, redeemValue, payable, points, cap: redeemCap(net, s), units: lines.reduce((a, l) => a + l.quantity, 0) };
}
