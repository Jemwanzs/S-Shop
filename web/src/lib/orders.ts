import type { Settings } from "./types";

/** Main order path, in order. Mirrors server/src/routes/orders.rs FLOW. */
export const ORDER_FLOW = ["new", "confirmed", "preparing", "dispatched", "on_delivery", "delivered", "completed"];
const OPTIONAL = ["preparing", "dispatched", "on_delivery", "completed"];

/** The business's name for a status (Settings → Orders → Order statuses). */
export function orderLabel(s: Settings | undefined, key: string): string {
  const custom = s?.orders.statuses?.find((x) => x.key === key)?.label?.trim();
  return custom || key.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Optional steps can be switched off; core steps and the sale stage are always on. */
export function orderStepEnabled(s: Settings | undefined, key: string): boolean {
  if (!OPTIONAL.includes(key) || key === s?.orders.sale_on_status) return true;
  return s?.orders.statuses?.find((x) => x.key === key)?.enabled ?? true;
}
