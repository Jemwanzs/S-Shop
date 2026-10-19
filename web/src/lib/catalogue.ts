/** Roadmap 143: the product catalogue remembers where the person was browsing. The URL holds the filters
 * (`/products?status=active&price=missing&page=2` — refresh-safe and shareable); this session remembers only which
 * catalogue URL to return to and its scroll position, per business, so Back from a product (after editing, saving or
 * cancelling) lands on the same filtered page. Nothing here outlives the browser tab, and another business's
 * catalogue context is never used. */
import { useEffect } from "react";
import { useSession } from "@/lib/session";

const key = (tenant: string, what: "url" | "scroll", extra = "") => `sshop.catalogue.${what}.${tenant}${extra}`;

function read(k: string): string | null {
  try {
    return sessionStorage.getItem(k);
  } catch {
    return null;
  }
}
function write(k: string, v: string) {
  try {
    sessionStorage.setItem(k, v);
  } catch {
    /* private mode: the catalogue simply opens unfiltered */
  }
}

/** The catalogue URL to go back to (its filters and page), or the plain catalogue. */
export function useCatalogueHref(): string {
  const tenant = useSession().profile?.tenant.id;
  const saved = tenant ? read(key(tenant, "url")) : null;
  return saved && saved.startsWith("/products") ? saved : "/products";
}

/** Called by the catalogue: remembers its current URL and, on leaving, its scroll position. */
export function useRememberCatalogue(search: string, ready: boolean) {
  const tenant = useSession().profile?.tenant.id;
  const url = `/products${search}`;
  useEffect(() => {
    if (tenant) write(key(tenant, "url"), url);
  }, [tenant, url]);
  // Restore the scroll position once this page of results is on screen; remember it when leaving.
  useEffect(() => {
    if (!tenant || !ready) return;
    const y = Number(read(key(tenant, "scroll", search)) ?? 0);
    if (y > 0) requestAnimationFrame(() => window.scrollTo(0, y));
    const save = () => write(key(tenant, "scroll", search), String(Math.round(window.scrollY)));
    window.addEventListener("pagehide", save);
    return () => {
      save();
      window.removeEventListener("pagehide", save);
    };
  }, [tenant, search, ready]);
}
