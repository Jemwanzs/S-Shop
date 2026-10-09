/** Website building blocks: product cards and grids, categories, testimonials, CTA links. */
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { Link } from "react-router-dom";
import {
  Check, Clock, Coffee, Gift, Heart, Home, Mail, MapPin, MessageCircle, Package, Palette, Phone, Plus, Ruler, Scissors, Shield, Shirt,
  Sparkles, Star, Truck, Wrench, type LucideIcon,
} from "lucide-react";
import type { Category, Cta, Product, SiteData, Testimonial } from "./types";
import { cartStore, ctaHref, money, track, waLink } from "./lib";

export const SERVICE_ICONS: Record<string, LucideIcon> = {
  truck: Truck, gift: Gift, sparkles: Sparkles, scissors: Scissors, wrench: Wrench, shield: Shield, heart: Heart, star: Star,
  clock: Clock, phone: Phone, package: Package, home: Home, ruler: Ruler, palette: Palette, shirt: Shirt, coffee: Coffee,
};

export function CtaLink({ cta, className = "btn", children }: { cta: Cta; className?: string; children?: ReactNode }) {
  const h = ctaHref(cta.target);
  const body = children ?? cta.label;
  if (h.to) return <Link className={className} to={h.to}>{body}</Link>;
  if (h.href) return <a className={className} href={h.href} target={h.href.startsWith("https:") ? "_blank" : undefined} rel="noopener noreferrer">{body}</a>;
  return null;
}

export function gridVars(data: SiteData, kind: "products" | "categories"): CSSProperties {
  if (kind === "categories") {
    const c = data.config.categories;
    return { "--cm": c.mobile, "--ct": c.tablet, "--cd": c.desktop } as CSSProperties;
  }
  const g = data.config.products.grid;
  return { "--cm": g.mobile, "--ct": g.tablet, "--cd": g.desktop } as CSSProperties;
}

/** What a visitor can do with a product whose price is hidden, or that cannot be ordered online. */
export function productAction(data: SiteData, p: Product): { label: string; to?: string; href?: string } | null {
  const contact = data.config.contact;
  const phone = contact.phone || data.business.phone || "";
  const wa = contact.whatsapp || data.config.social.whatsapp || phone;
  const text = `Hello, I'm interested in ${p.name}`;
  switch (p.action) {
    case "add_to_cart":
      return null;
    case "whatsapp":
      return wa ? { label: p.cta_label || "Ask on WhatsApp", href: waLink(wa, text) } : { label: p.cta_label || "Contact us", to: `/contact?about=${encodeURIComponent(p.name)}` };
    case "contact":
      return phone ? { label: p.cta_label || "Call to order", href: `tel:${phone.replace(/\s/g, "")}` } : { label: p.cta_label || "Contact us", to: "/contact" };
    default:
      return { label: p.cta_label || "Enquire", to: `/contact?about=${encodeURIComponent(p.name)}` };
  }
}

export function addToCart(p: Product, qty = 1) {
  cartStore.add({ id: p.id, slug: p.slug, name: p.name, price: p.price, photo: p.photo_thumb }, qty);
  track("add_to_cart", p.id);
  // Roadmap 77: a short confirmation with a way to the cart (shown by the site shell).
  window.dispatchEvent(new CustomEvent("sshop:added", { detail: { name: p.name, qty } }));
}

/** A product photo that fades in when loaded and falls back to a branded placeholder when missing or broken —
 * never a broken-image icon, never an unrelated stock photo (roadmap 75). */
export function Photo({ src, alt, name, sizes, priority }: { src: string | null | undefined; alt: string; name: string; sizes?: string; priority?: boolean }) {
  const [state, setState] = useState<"load" | "ok" | "err">(src ? "load" : "err");
  useEffect(() => setState(src ? "load" : "err"), [src]);
  if (!src || state === "err") {
    return (
      <span className="ph" aria-hidden>
        <span className="ph-mark">{(name.trim()[0] ?? "•").toUpperCase()}</span>
      </span>
    );
  }
  return (
    <img
      src={src}
      alt={alt}
      sizes={sizes}
      loading={priority ? "eager" : "lazy"}
      decoding="async"
      className={state === "ok" ? "ok" : ""}
      ref={(el) => { if (el?.complete && el.naturalWidth > 0 && state === "load") setState("ok"); }}
      onLoad={() => setState("ok")}
      onError={() => setState("err")}
    />
  );
}

export function ProductCard({ data, p }: { data: SiteData; p: Product }) {
  const g = data.config.products.grid;
  const [added, setAdded] = useState(false);
  useEffect(() => {
    if (!added) return;
    const t = setTimeout(() => setAdded(false), 1400);
    return () => clearTimeout(t);
  }, [added]);
  const act = productAction(data, p);
  const orderable = p.action === "add_to_cart" && data.ordering.enabled && p.in_stock !== false;
  // Roadmap 76: image → name → availability, then an action row of its own (price + Add): nothing floats over the
  // name or the price, and cards in a row line up whatever the name length.
  const quick = g.quick_add && orderable;
  const off = p.price != null && p.compare_at ? Math.round((1 - Number(p.price) / Number(p.compare_at)) * 100) : 0;
  const second = p.photos.length > 1 ? `${p.photos[1]}${p.photos[1].includes("?") ? "&" : "?"}size=thumb` : null;
  return (
    <article className={`card ${g.card} ${g.shadow ? "shadow" : ""}`}>
      <Link to={`/products/${p.slug}`} className="card-link">
        <div className="img">
          <span className="badges">
            {g.show_badges && p.badge && <span className="badge">{badgeLabel(p.badge)}</span>}
            {off > 0 && <span className="badge sale">−{off}%</span>}
            {p.in_stock === false && <span className="badge muted">Out of stock</span>}
          </span>
          <Photo src={p.photo_thumb ?? p.photo} alt={p.name} name={p.name} sizes="(min-width: 1024px) 25vw, (min-width: 640px) 33vw, 50vw" />
          {/* Desktop hover: the next photo, when there is one (roadmap 80). */}
          {second && <img className="alt" src={second} alt="" aria-hidden loading="lazy" decoding="async" />}
        </div>
        <div className="body">
          <span className="name">{p.name}</span>
          {g.show_availability && p.in_stock != null && <span className={`stock ${p.in_stock ? "" : "out"}`}><i aria-hidden />{p.in_stock ? "In stock" : "Out of stock"}</span>}
        </div>
      </Link>
      {(p.price != null || quick) && (
        <div className="foot">
          {p.price != null ? (
            <span className="prices">
              <span className="price">{money(data.business.currency, p.price)}</span>
              {p.compare_at && <s className="was" aria-label={`Was ${money(data.business.currency, p.compare_at)}`}>{money(data.business.currency, p.compare_at)}</s>}
            </span>
          ) : <span />}
          {quick && (
            <button type="button" className={`add ${added ? "done" : ""}`} aria-label={`Add ${p.name} to cart`} onClick={() => { addToCart(p); setAdded(true); }}>
              {added ? <Check aria-hidden /> : <Plus aria-hidden />}<span className="lbl">{added ? "Added" : "Add"}</span>
            </button>
          )}
        </div>
      )}
      {act && g.card !== "compact" && (
        <div className="cta">
          {act.to ? <Link className="btn sm outline block" to={act.to}>{act.label}</Link> : <a className="btn sm outline block" href={act.href} target="_blank" rel="noopener noreferrer">{act.label}</a>}
        </div>
      )}
    </article>
  );
}

export function badgeLabel(b: string) {
  return ({ new: "New", featured: "Featured", offer: "Offer" } as Record<string, string>)[b] ?? b;
}

export function ProductGrid({ data, items, rail }: { data: SiteData; items: Product[]; rail?: boolean }) {
  return (
    <div className={`${rail ? "rail" : "grid"} products ${data.config.products.grid.mobile >= 3 ? "m3" : ""}`} style={gridVars(data, "products")}>
      {items.map((p) => <ProductCard key={p.id} data={data} p={p} />)}
    </div>
  );
}

export function GridSkeleton({ data, n = 4 }: { data: SiteData; n?: number }) {
  return (
    <div className="grid" style={gridVars(data, "products")} aria-hidden>
      {Array.from({ length: n }, (_, i) => <div key={i} className="skel" style={{ aspectRatio: "3 / 4" }} />)}
    </div>
  );
}

export function CategoryTiles({ data, items, rail }: { data: SiteData; items: Category[]; rail?: boolean }) {
  const c = data.config.categories;
  return (
    <div className={rail ? "rail" : "grid"} style={gridVars(data, "categories")}>
      {items.map((cat) => (
        <Link key={cat.id} to={`/categories/${cat.id}`} className={`cat ${c.card} ${c.show_images ? "" : "noimg"}`}>
          {c.show_images && <div className="img">{cat.image ? <img src={cat.image} alt="" loading="lazy" /> : <span aria-hidden>{cat.name[0]}</span>}</div>}
          <strong>{cat.name}</strong>
        </Link>
      ))}
    </div>
  );
}

function Stars({ n }: { n: number }) {
  if (!n) return null;
  return <div className="stars" aria-label={`${n} out of 5 stars`}>{Array.from({ length: n }, (_, i) => <Star key={i} aria-hidden />)}</div>;
}

function Quote({ t, hidden }: { t: Testimonial; hidden?: boolean }) {
  return (
    <figure className="quote" aria-hidden={hidden || undefined} style={{ margin: 0 }}>
      <Stars n={t.rating} />
      <blockquote style={{ margin: 0 }}>“{t.quote}”</blockquote>
      <figcaption className="who">
        {t.photo ? <img src={`/api/site/media/${t.photo}?size=thumb`} alt="" loading="lazy" /> : <span className="av" aria-hidden>{t.name[0]}</span>}
        <span><strong style={{ color: "var(--heading)" }}>{t.name}</strong>{t.position && <span className="muted" style={{ display: "block", fontSize: ".86em" }}>{t.position}</span>}</span>
      </figcaption>
    </figure>
  );
}

const SPEED = { slow: 70, normal: 45, fast: 25 } as const;

/** Testimonials scroll right to left on their own (paused on hover or keyboard focus; still for reduced motion). */
export function Testimonials({ data, items }: { data: SiteData; items: Testimonial[] }) {
  const cfg = data.config.testimonials;
  if (!items.length) return null;
  if (!cfg.auto_scroll || items.length < 2) {
    return <div className="static-quotes">{items.map((t) => <Quote key={t.id} t={t} />)}</div>;
  }
  const seconds = SPEED[cfg.speed] * Math.max(1, items.length / 4);
  return (
    <div className="marquee" tabIndex={0} aria-label="Customer testimonials">
      <div className="track" style={{ "--speed": `${seconds}s` } as CSSProperties}>
        {items.map((t) => <Quote key={t.id} t={t} />)}
        {/* The second copy makes the loop seamless; screen readers skip it. */}
        {items.map((t) => <div key={`${t.id}-copy`} aria-hidden="true" style={{ display: "contents" }}><Quote t={t} hidden /></div>)}
      </div>
    </div>
  );
}

export function ContactList({ data }: { data: SiteData }) {
  const c = data.config.contact;
  const map = c.map_url || (c.location ? `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(c.location)}` : "");
  return (
    <div className="contact-list">
      {c.show_phone && c.phone && <a href={`tel:${c.phone.replace(/\s/g, "")}`}><Phone aria-hidden /><span><strong>Call</strong><br /><span className="muted">{c.phone}</span></span></a>}
      {c.show_whatsapp && c.whatsapp && <a href={waLink(c.whatsapp)} target="_blank" rel="noopener noreferrer"><MessageCircle aria-hidden /><span><strong>WhatsApp</strong><br /><span className="muted">{c.whatsapp}</span></span></a>}
      {c.show_email && c.email && <a href={`mailto:${c.email}`}><Mail aria-hidden /><span><strong>Email</strong><br /><span className="muted" style={{ overflowWrap: "anywhere" }}>{c.email}</span></span></a>}
      {c.show_location && c.location && <a href={map} target="_blank" rel="noopener noreferrer"><MapPin aria-hidden /><span><strong>Visit us</strong><br /><span className="muted">{c.location}</span></span></a>}
      {c.show_hours && c.hours && <div><Clock aria-hidden /><span><strong>Opening hours</strong><br /><span className="muted" style={{ whiteSpace: "pre-line" }}>{c.hours}</span></span></div>}
    </div>
  );
}
