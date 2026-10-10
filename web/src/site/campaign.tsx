/** Holiday & promotional campaign on the public website (roadmap 84–86): greeting, occasion decorations, featured
 * products and a button — as a hero, compact banner, top strip or floating card. One campaign at a time, chosen by
 * the server (schedule, priority, both switches on). Dismissible when the business allows it (remembered per campaign
 * version). Decorations are CSS only, quiet, and still when animation is off or the visitor prefers reduced motion. */
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Plus, X } from "lucide-react";
import type { Campaign, Product, SiteData } from "./types";
import { call, ctaHref, currentVisitor, money, runtime } from "./lib";
import { addToCart, Photo } from "./parts";

const DECOR: Record<string, string[]> = {
  christmas: ["❄", "✦", "❄", "●", "❄", "✦", "●", "❄"],
  new_year: ["✦", "•", "✧", "•", "✦", "•", "✧", "✦"],
  valentines: ["♥", "♡", "♥", "♥", "♡", "♥", "♡", "♥"],
  easter: ["◖", "✿", "◗", "✿", "◖", "✿", "◗", "✿"],
  eid: ["☾", "✦", "✧", "☾", "✦", "✧", "✦", "☾"],
  celebration: ["●", "■", "▲", "●", "■", "●", "▲", "■"],
  sale: ["%", "★", "%", "★", "%", "★", "%", "★"],
  elegant: ["✦", "·", "✧", "·", "✦", "·", "✧", "·"],
  minimal: [],
};

function event(kind: "campaign_view" | "campaign_product" | "campaign_cta", c: Campaign, productId?: string) {
  if (runtime.preview) return;
  const visitor = currentVisitor();
  call("/site/events", { body: { kind, visitor, campaign_id: c.id, product_id: productId } }).catch(() => undefined);
}

const dismissKey = (c: Campaign) => `sshop.campaign.${c.id}.${c.version}`;

/** Which placement key the current path belongs to. */
export function pageKey(pathname: string, landing: string): string {
  if (pathname === "/") return landing === "home" ? "home" : landing;
  if (pathname === "/home") return "home";
  if (pathname === "/products") return "products";
  if (pathname.startsWith("/categories")) return "categories";
  return "other";
}

export function CampaignBanner({ data, pathname }: { data: SiteData; pathname: string }) {
  const c = data.campaign;
  const [hidden, setHidden] = useState(() => {
    if (!c) return true;
    try {
      return !!localStorage.getItem(dismissKey(c));
    } catch {
      return false;
    }
  });
  const page = pageKey(pathname, data.config.landing ?? "products");
  const here = !!c && (c.placement.pages.includes("all") || c.placement.pages.includes(page));
  useEffect(() => {
    if (!c || hidden || !here) return;
    // One view per campaign and page visit.
    const key = `sshop.cv.${c.id}.${pathname}`;
    try {
      if (sessionStorage.getItem(key)) return;
      sessionStorage.setItem(key, "1");
    } catch { /* still counted */ }
    event("campaign_view", c);
  }, [c, hidden, here, pathname]);
  if (!c || hidden || !here) return null;
  const d = c.design;
  const dismiss = () => {
    setHidden(true);
    try { localStorage.setItem(dismissKey(c), "1"); } catch { /* not remembered */ }
  };
  const style = {
    ...(d.background ? { "--cg-custom-bg": d.background } : {}),
    ...(d.text_color ? { "--cg-custom-fg": d.text_color } : {}),
    ...(d.background_image ? { "--cg-img": `url(/api/site/media/${d.background_image})` } : {}),
  } as React.CSSProperties;
  const cta = d.cta_label && <CampaignCta c={c} />;
  const close = c.placement.dismissible && (
    <button type="button" className="cg-close" aria-label="Close" onClick={dismiss}><X aria-hidden /></button>
  );
  const marks = d.decorations ? (DECOR[d.template] ?? []) : [];
  const decor = marks.length > 0 && (
    <span className="cg-decor" aria-hidden>{marks.map((m, i) => (
      <i key={i} style={{ "--i": i, left: `${i * 12.5 + 2}%`, top: `${8 + ((i * 37) % 84)}%`, fontSize: `${12 + ((i * 5) % 16)}px` } as React.CSSProperties}>{m}</i>
    ))}</span>
  );
  const cls = `cg cg-${c.placement.display} t-${d.template} h-${d.height} a-${d.align} s-${d.headline_size} ${d.animation === "subtle" ? "anim" : ""} ${d.background_image ? "has-img" : ""} ${d.background ? "custom-bg" : ""} ${d.text_color ? "custom-fg" : ""}`;

  if (c.placement.display === "strip") {
    return (
      <aside className={cls} style={style} aria-label="Announcement">
        {decor}
        <div className="wrap cg-strip-in">
          <strong>{d.headline}</strong>
          {d.promo && <span className="cg-promo">{d.promo}</span>}
          {cta}
        </div>
        {close}
      </aside>
    );
  }
  if (c.placement.display === "card") {
    return (
      <aside className={cls} style={style} aria-label="Greeting">
        {decor}
        <strong className="cg-title">{d.headline}</strong>
        {d.message && <p className="cg-msg">{d.message}</p>}
        {cta}
        {close}
      </aside>
    );
  }
  return (
    <section className={cls} style={style} aria-label={d.headline}>
      {decor}
      <div className="wrap cg-in">
        <div className="cg-copy">
          <h2 className="cg-title">{d.headline}</h2>
          {d.message && <p className="cg-msg">{d.message}</p>}
          {d.promo && <p className="cg-promo">{d.promo}</p>}
          {cta}
        </div>
        {c.products.length > 0 && (
          <div className="cg-products" role="list">
            {c.products.map((p) => <CampaignProduct key={p.id} data={data} c={c} p={p} />)}
          </div>
        )}
      </div>
      {close}
    </section>
  );
}

function CampaignCta({ c }: { c: Campaign }) {
  const d = c.design;
  const h = ctaHref(d.cta_target);
  const cls = `btn ${d.button_style === "outline" ? "outline" : ""} cg-btn`;
  const click = () => event("campaign_cta", c);
  if (h.to) return <Link className={cls} to={h.to} onClick={click}>{d.cta_label}</Link>;
  if (h.href) return <a className={cls} href={h.href} target={h.href.startsWith("https:") ? "_blank" : undefined} rel="noopener noreferrer" onClick={click}>{d.cta_label}</a>;
  return null;
}

function CampaignProduct({ data, c, p }: { data: SiteData; c: Campaign; p: Product }) {
  const [added, setAdded] = useState(false);
  const orderable = p.action === "add_to_cart" && data.ordering.enabled && p.in_stock !== false;
  return (
    <div className="cg-prod" role="listitem">
      <Link to={`/products/${p.slug}`} onClick={() => event("campaign_product", c, p.id)}>
        <span className="cg-img"><Photo src={p.photo_thumb ?? p.photo} alt={p.name} name={p.name} sizes="140px" /></span>
        <span className="cg-name">{p.name}</span>
        {p.price != null && (
          <span className="cg-price">
            {money(data.business.currency, p.price)}
            {p.compare_at && <s>{money(data.business.currency, p.compare_at)}</s>}
          </span>
        )}
      </Link>
      {orderable && (
        <button type="button" className="cg-add" aria-label={`Add ${p.name} to cart`}
          onClick={() => { addToCart(p); setAdded(true); event("campaign_product", c, p.id); }}>
          {added ? "Added" : <><Plus aria-hidden /> Add</>}
        </button>
      )}
    </div>
  );
}
