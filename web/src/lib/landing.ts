/**
 * Roadmap 108 — the page people land on after signing in (and picking their branch): their own choice, else the
 * business default, else the Dashboard — always one they are allowed to open.
 */
import { NAV } from "@/components/layout/nav";
import type { Profile } from "@/lib/types";

/** Mirrors server/src/settings.rs::LANDING_PAGES. */
const LANDING = ["/", "/my", "/pos", "/sales", "/credit", "/orders", "/products", "/stock", "/transfers", "/customers", "/loyalty", "/expenses", "/reports", "/leaderboards"];

export const LANDING_PAGES = NAV.flatMap((g) => g.items).filter((i) => LANDING.includes(i.to));

/** Same rule as the session's `can` (modules of the plan, then the role's permissions). */
export function profileCan(profile: Profile | null | undefined, perm: string): boolean {
  if (!profile) return false;
  const b = profile.billing;
  if (b?.modules) {
    const owner = b.catalogue.find((m) => m.perms.some((p) => perm === p || (p.endsWith(".") && perm.startsWith(p))));
    if (owner && !b.modules.includes(owner.key)) return false;
  }
  return profile.permissions.some((p) => p === "*" || p === perm || (p === "settings.manage" && perm.startsWith("settings.")));
}

/** Can this person open the page at `to`? */
export function canOpen(profile: Profile | null | undefined, to: string): boolean {
  const item = LANDING_PAGES.find((i) => i.to === to);
  return !!item && (item.perms.length === 0 || item.perms.some((p) => profileCan(profile, p)));
}

export function landingPath(profile: Profile | null | undefined): string {
  const want = profile?.user.preferences?.landing || profile?.settings.experience?.landing || "/";
  return canOpen(profile, want) ? want : "/";
}
