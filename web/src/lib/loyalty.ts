import type { Settings } from "@/lib/types";

/** Can points be redeemed right now? `null` = yes, otherwise why not. Mirrors server/src/settings.rs::redemption_open. */
export function redemptionBlocked(l: Settings["loyalty"], now = Date.now()): string | null {
  if (!l.enabled || !l.redemption_enabled) return "Points redemption is turned off";
  if (!l.redemption_window_open) return "The points redemption window is closed";
  if (l.redemption_opens_at && now < Date.parse(l.redemption_opens_at)) return "The points redemption window has not opened yet";
  if (l.redemption_closes_at && now >= Date.parse(l.redemption_closes_at)) return "The points redemption window has closed";
  return null;
}
