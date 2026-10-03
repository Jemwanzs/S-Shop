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
}

export const marked = (l: CartLine) => toNum(l.product.marked_price);
export const unitDiscount = (l: CartLine) => marked(l) - l.unitPrice;
export const lineTotal = (l: CartLine) => Math.round(l.unitPrice * l.quantity * 100) / 100;
export const exceedsMax = (l: CartLine) => l.product.max_discount !== null && unitDiscount(l) > toNum(l.product.max_discount);

/** Mirrors server/src/loyalty.rs::line_points. */
export function linePoints(l: CartLine, s: Settings): number {
  if (!s.loyalty.enabled || !l.product.loyalty_eligible) return 0;
  const total = lineTotal(l);
  const threshold = l.product.loyalty_threshold !== null ? toNum(l.product.loyalty_threshold) : toNum(s.loyalty.threshold);
  if (threshold <= 0 || total <= 0) return 0;
  const per = l.product.loyalty_points_per ?? s.loyalty.points_per;
  return Math.floor(total / threshold) * per;
}

export function totals(lines: CartLine[], s: Settings, redeemPoints = 0) {
  const gross = lines.reduce((a, l) => a + marked(l) * l.quantity, 0);
  const net = lines.reduce((a, l) => a + lineTotal(l), 0);
  const redeemValue = Math.round(redeemPoints * toNum(s.loyalty.point_value) * 100) / 100;
  const payable = Math.max(0, Math.round((net - redeemValue) * 100) / 100);
  const rawPoints = net >= toNum(s.loyalty.min_spend) ? lines.reduce((a, l) => a + linePoints(l, s), 0) : 0;
  const points = net > 0 && redeemValue > 0 ? Math.floor((rawPoints * payable) / net) : rawPoints;
  return { gross, discount: gross - net, net, redeemValue, payable, points, units: lines.reduce((a, l) => a + l.quantity, 0) };
}
