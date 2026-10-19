/** Roadmap 145: business workspace addresses — `https://s-shop.store/{business-slug}/…`.
 *
 * Every screen of the signed-in workspace lives under the business's slug (`/pablo-niche/products`). The app's links
 * keep naming paths the short way (`/products`); these wrappers of React Router add the slug of the business the person
 * is signed in to, at the moment of navigating, so no internal link can drop or swap it. Public addresses — sign-in,
 * the ordering link, tracking, receipts and the business website (`/s/{slug}`, served by the server) — never get it.
 * `useLocation` returns the path without the slug, so screens compare paths as before.
 *
 * The slug in the address is never proof of anything: the server decides the business from the session token on every
 * request, and the workspace gate (App) sends a person who types another business's slug back to their own. */
import { forwardRef, useCallback, useMemo } from "react";
import * as RR from "react-router-dom";
import type { Profile } from "./types";

export * from "react-router-dom";

let current: string | null = null;
/** The business the session is in (set by the session as soon as the profile is known). */
export const workspace = {
  get slug() {
    return current;
  },
  set(slug: string | null) {
    current = slug;
  },
};

/** First path segments that are not workspace screens (public or pre-workspace pages). */
const PUBLIC = ["login", "select-branch", "request-access", "forgot", "set-pin", "access-status", "order", "track", "r", "s", "api"];

/** First segments of workspace screens — an address that starts with one is an old (unprefixed) bookmark. */
export const SCREENS = new Set([
  "my", "leaderboards", "more", "notifications", "pos", "sales", "credit", "orders", "products", "stock", "transfers",
  "customers", "loyalty", "expenses", "reports", "approvals", "settings", "audit", "dashboard",
]);

/** `/products?x` → `/{slug}/products?x` for the current business; public and already-prefixed paths unchanged. */
export function inWorkspace(to: string, slug = current): string {
  if (!slug || !to.startsWith("/") || to.startsWith("//")) return to;
  const first = to.slice(1).split(/[/?#]/)[0];
  if (first === slug || PUBLIC.includes(first)) return to;
  return to === "/" ? `/${slug}` : `/${slug}${to.startsWith("/?") || to.startsWith("/#") ? to.slice(1) : to}`;
}

/** The path inside the workspace (`/pablo-niche/products` → `/products`). */
export function outOfWorkspace(pathname: string, slug = current): string {
  if (!slug) return pathname;
  if (pathname === `/${slug}`) return "/";
  return pathname.startsWith(`/${slug}/`) ? pathname.slice(slug.length + 1) : pathname;
}

function prefixTo(to: RR.To): RR.To {
  if (typeof to === "string") return inWorkspace(to);
  return to.pathname ? { ...to, pathname: inWorkspace(to.pathname) } : to;
}

export const Link = forwardRef<HTMLAnchorElement, RR.LinkProps>(function Link(props, ref) {
  return <RR.Link ref={ref} {...props} to={prefixTo(props.to)} />;
});

export const NavLink = forwardRef<HTMLAnchorElement, RR.NavLinkProps>(function NavLink(props, ref) {
  return <RR.NavLink ref={ref} {...props} to={prefixTo(props.to)} />;
});

export function Navigate(props: RR.NavigateProps) {
  return <RR.Navigate {...props} to={prefixTo(props.to)} />;
}

export function useNavigate(): RR.NavigateFunction {
  const navigate = RR.useNavigate();
  return useCallback(
    ((to: RR.To | number, options?: RR.NavigateOptions) => (typeof to === "number" ? navigate(to) : navigate(prefixTo(to), options))) as RR.NavigateFunction,
    [navigate],
  );
}

/** The real address (slug included) — for the workspace gate and for remembering where to return after sign-in. */
export const useRawLocation = RR.useLocation;

const NEXT = "sshop.next";
/** Every address of the businesses this person may open (current, earlier slugs, the tenant's other businesses). */
export const slugsOf = (p: Profile) => [p.tenant.slug, ...(p.tenant.previous_slugs ?? []), ...(p.businesses ?? []).map((b) => b.slug)];
/** Remember the workspace address someone opened before signing in (a bookmark or shared link). */
export function rememberNext(path: string) {
  try {
    sessionStorage.setItem(NEXT, path);
  } catch {
    /* storage unavailable: they land on their home screen */
  }
}
/** The remembered address, once — only a path on this site (never another site), and only one of the businesses of
 * the person who just signed in (`slugs`) or an old unprefixed one — not what someone else left on a shared device.
 * Where it leads is still decided by the workspace gate: a slug in it grants nothing. */
export function takeNext(slugs: (string | undefined)[]): string | null {
  try {
    const v = sessionStorage.getItem(NEXT);
    sessionStorage.removeItem(NEXT);
    if (!v || !v.startsWith("/") || v.startsWith("//") || v.includes("\\")) return null;
    const first = v.slice(1).split(/[/?#]/)[0];
    return SCREENS.has(first) || slugs.includes(first) ? v : null;
  } catch {
    return null;
  }
}

/** The location with the business slug taken off the path (screens compare `/products`, as before). */
export function useLocation(): RR.Location {
  const location = RR.useLocation();
  return useMemo(() => ({ ...location, pathname: outOfWorkspace(location.pathname) }), [location]);
}
