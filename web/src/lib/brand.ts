/**
 * Roadmap 113 — where the business logo replaces S'Shop's (Settings → Business → Branding & White Labelling).
 *
 * Places that showed the business logo before roadmap 112 (sidebar, receipts, customer pages, website) keep it while
 * branding is off; with branding on, their switch decides. New places (dashboard, POS header, favicon) need branding
 * on and their switch on. The platform owner's own administration always shows S'Shop.
 */
import { useEffect } from "react";
import { useSession } from "@/lib/session";
import type { Profile, Settings } from "@/lib/types";

export type Branding = NonNullable<Settings["branding"]>;
export type BrandPlace = Exclude<keyof Branding, "enabled">;

export const DEFAULT_BRANDING: Branding = {
  enabled: false, login: true, login_marketing: false, sidebar: true, dashboard: true, pos: true, documents: true, customer: true, website: true, favicon: false,
};
const EXISTING: BrandPlace[] = ["sidebar", "documents", "customer", "website"];

/** The (versioned) business logo address for `place`, or null for S'Shop branding. */
export function brandLogo(profile: Profile | null | undefined, place: BrandPlace): string | null {
  const url = profile?.tenant.logo_url;
  if (!url) return null;
  const b = { ...DEFAULT_BRANDING, ...profile.settings?.branding };
  if (EXISTING.includes(place)) return !b.enabled || b[place] ? url : null;
  if (profile.user.platform_admin && !profile.acting) return null;
  return b.enabled && b[place] ? url : null;
}

export function useBrandLogo(place: BrandPlace): string | null {
  const { profile } = useSession();
  return brandLogo(profile, place);
}

/** The sign-in screens' business, known from trusted context only (see Login). */
export interface LoginBrand {
  name: string;
  logo_url: string | null;
  /** The business's own sign-in address may also replace the large S'Shop logo. */
  marketing: boolean;
}

const EXPIRED_BUSINESS = "sshop.expired-business";
/** Remembers (for this tab only) which business's session just expired, so its sign-in can show its logo. */
export function rememberExpiredBusiness(slug: string | undefined) {
  try {
    if (slug) sessionStorage.setItem(EXPIRED_BUSINESS, slug);
  } catch {
    /* the sign-in simply shows S'Shop */
  }
}
export function expiredBusiness(): string | null {
  try {
    return sessionStorage.getItem(EXPIRED_BUSINESS);
  } catch {
    return null;
  }
}

/** Advanced (favicon switch): the browser tab icon and installed-app manifest use the business logo while signed in. */
export function useBrandIcons() {
  const { profile } = useSession();
  const logo = brandLogo(profile, "favicon");
  const slug = profile?.tenant.slug;
  useEffect(() => {
    if (!logo || !slug) return;
    const links = [...document.querySelectorAll<HTMLLinkElement>('link[rel="icon"], link[rel="apple-touch-icon"], link[rel="manifest"]')];
    const before = links.map((l) => [l, l.getAttribute("href"), l.getAttribute("type")] as const);
    for (const l of links) {
      if (l.rel === "manifest") l.setAttribute("href", `/api/public/${encodeURIComponent(slug)}/manifest.webmanifest`);
      else {
        l.setAttribute("href", logo);
        l.removeAttribute("type");
      }
    }
    // Signing out (or switching it off) puts S'Shop's icons back.
    return () => before.forEach(([l, href, type]) => {
      if (href) l.setAttribute("href", href);
      if (type) l.setAttribute("type", type);
    });
  }, [logo, slug]);
}
