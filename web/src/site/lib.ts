/** Website runtime: where the site lives, API calls, cart, customer session, analytics consent. */
import type { CartLine, Cta, SiteData } from "./types";

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function store(key: string, value: unknown) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage unavailable — the cart lasts for this page only */
  }
}

/** The website's base path: "" on its own domain, "/s/{slug}" on the S'Shop host. */
export function basePath(slug: string): string {
  const prefix = `/s/${slug}`;
  return location.pathname === prefix || location.pathname.startsWith(prefix + "/") ? prefix : "";
}

export const runtime = {
  slug: "",
  /** Staff preview (Website Management Centre): the draft, read with the staff session. */
  preview: false,
  previewToken: null as string | null,
};

export class SiteError extends Error {
  constructor(message: string, public status: number, public title?: string) {
    super(message);
  }
}

export async function call<T>(path: string, opts: { method?: string; body?: unknown; token?: string | null; query?: Record<string, string | number | boolean | undefined> } = {}): Promise<T> {
  const q = new URLSearchParams();
  // On a custom domain the server knows the business from the host; the slug is only needed on the S'Shop host.
  if (basePath(runtime.slug)) q.set("slug", runtime.slug);
  if (runtime.preview) q.set("preview", "true");
  for (const [k, v] of Object.entries(opts.query ?? {})) if (v !== undefined && v !== "") q.set(k, String(v));
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  const token = opts.token ?? (runtime.preview ? runtime.previewToken : null);
  if (token) headers.Authorization = `Bearer ${token}`;
  let body = opts.body;
  if (body && typeof body === "object" && basePath(runtime.slug)) body = { slug: runtime.slug, ...(body as object) };
  const res = await fetch(`/api${path}${q.toString() ? `?${q}` : ""}`, { method: opts.method ?? (body ? "POST" : "GET"), headers, body: body ? JSON.stringify(body) : undefined });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) {
    const e = data?.error ?? {};
    throw new SiteError(e.message ?? "Something went wrong — please try again", res.status, e.title);
  }
  return data as T;
}

// ── Cart ──────────────────────────────────────────────────────────────

const cartKey = () => `sshop.site.cart.${runtime.slug}`;
type Listener = () => void;
const listeners = new Set<Listener>();
let cart: CartLine[] = [];

export const cartStore = {
  load() {
    cart = readJson<CartLine[]>(cartKey(), []).filter((l) => l && l.id && l.qty > 0);
  },
  get: () => cart,
  subscribe(fn: Listener) {
    listeners.add(fn);
    return () => void listeners.delete(fn);
  },
  set(next: CartLine[]) {
    cart = next.filter((l) => l.qty > 0);
    store(cartKey(), cart);
    listeners.forEach((l) => l());
  },
  add(line: Omit<CartLine, "qty">, qty = 1) {
    const hit = cart.find((l) => l.id === line.id);
    cartStore.set(hit ? cart.map((l) => (l.id === line.id ? { ...l, ...line, qty: Math.min(99, l.qty + qty) } : l)) : [...cart, { ...line, qty }]);
  },
  qty(id: string, qty: number) {
    cartStore.set(cart.map((l) => (l.id === id ? { ...l, qty: Math.max(0, Math.min(99, qty)) } : l)));
  },
  clear() {
    cartStore.set([]);
  },
};

// ── Customer session (same S'Shop customer as the ordering link) ─────

export interface Customer { token: string; first_name: string; mobile: string }
const customerKey = () => `sshop.site.customer.${runtime.slug}`;
export const customerStore = {
  get: () => readJson<Customer | null>(customerKey(), null),
  set: (c: Customer | null) => store(customerKey(), c),
};

// ── Analytics: anonymous, and only after consent when the business asks for it ──

const consentKey = () => `sshop.site.consent.${runtime.slug}`;
export const consent = {
  get: (): "accepted" | "declined" | null => readJson(consentKey(), null),
  set: (v: "accepted" | "declined") => store(consentKey(), v),
};

let needsConsent = false;
export function configureAnalytics(data: SiteData) {
  needsConsent = data.config.cookies.analytics;
}

function visitorId(): string | null {
  if (needsConsent) {
    if (consent.get() !== "accepted") return null;
    let id = readJson<string | null>("sshop.site.visitor", null);
    if (!id) {
      id = crypto.randomUUID?.() ?? Math.random().toString(36).slice(2);
      store("sshop.site.visitor", id);
    }
    return id;
  }
  // Without analytics cookies: an id for this tab only, nothing kept on the device.
  try {
    let id = sessionStorage.getItem("sshop.site.tab");
    if (!id) {
      id = crypto.randomUUID?.() ?? Math.random().toString(36).slice(2);
      sessionStorage.setItem("sshop.site.tab", id);
    }
    return id;
  } catch {
    return "";
  }
}

export function track(kind: "visit" | "product_view" | "add_to_cart" | "order_start", productId?: string) {
  if (runtime.preview) return;
  const visitor = visitorId();
  if (visitor === null) return;
  call("/site/events", { body: { kind, visitor, product_id: productId } }).catch(() => undefined);
}
export const currentVisitor = () => visitorId() ?? "";

// ── Links ─────────────────────────────────────────────────────────────

const PAGE_PATHS: Record<string, string> = {
  home: "/", about: "/about", products: "/products", categories: "/categories", services: "/services",
  testimonials: "/testimonials", contact: "/contact", order: "/order",
};

/** A call-to-action target: a page key, a product/category path, or an external https:/tel:/mailto: link. */
export function ctaHref(target: string): { to?: string; href?: string } {
  const t = (target || "").trim();
  if (!t) return {};
  if (PAGE_PATHS[t]) return { to: PAGE_PATHS[t] };
  if (t.startsWith("/products/") || t.startsWith("/categories/")) return { to: t };
  if (/^(https:|tel:|mailto:)/.test(t)) return { href: t };
  return {};
}
export const pagePath = (key: string) => PAGE_PATHS[key] ?? "/";
export const hasCta = (c: Cta | undefined) => !!c && !!c.label.trim() && !!(ctaHref(c.target).to || ctaHref(c.target).href);

export function waLink(number: string, text?: string): string {
  let d = number.replace(/\D/g, "");
  if (d.startsWith("0")) d = "254" + d.slice(1);
  return `https://wa.me/${d}${text ? `?text=${encodeURIComponent(text)}` : ""}`;
}

export function money(currency: string, v: string | number | null | undefined): string {
  const n = typeof v === "number" ? v : Number(v ?? 0);
  return `${currency} ${n.toLocaleString(undefined, { maximumFractionDigits: 2 })}`;
}

/** WCAG text colour for a filled colour (mirrors the server's on_color). */
export function onColor(hex: string): string {
  const lum = (h: string) => {
    const m = /^#?([0-9a-f]{6})$/i.exec(h);
    if (!m) return 0;
    const n = parseInt(m[1], 16);
    const ch = (x: number) => {
      const c = x / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * ch((n >> 16) & 255) + 0.7152 * ch((n >> 8) & 255) + 0.0722 * ch(n & 255);
  };
  const contrast = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  const bg = lum(hex);
  return contrast(1, bg) >= contrast(lum("#111111"), bg) ? "#FFFFFF" : "#111111";
}

/** A website media library image (`size=thumb` for the small version). */
export const mediaUrl = (id: string, thumb = false) => `/api/site/media/${id}${thumb ? "?size=thumb" : ""}`;
