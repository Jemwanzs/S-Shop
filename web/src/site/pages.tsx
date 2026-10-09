/** Website pages. Everything shown comes from the published configuration and the business's live S'Shop products. */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Check, ChevronLeft, ChevronRight, ImageOff, Loader2, Minus, PackageSearch, Plus, ShoppingBag, Trash2, XCircle } from "lucide-react";
import type { Category, Product, Section, SiteData } from "./types";
import { pageItems } from "@/lib/paging";
import { call, cartStore, customerStore, currentVisitor, hasCta, mediaUrl, money, SiteError, track, type Customer } from "./lib";
import { addToCart, badgeLabel, CategoryTiles, ContactList, CtaLink, GridSkeleton, productAction, ProductGrid, SERVICE_ICONS, Testimonials } from "./parts";

type List = { items: Product[]; total: number };

export function useProducts(query: Record<string, string | number | undefined>, enabled = true) {
  return useQuery({
    queryKey: ["site-products", query],
    queryFn: () => call<List>("/site/products", { query }),
    enabled,
    // Keep the current page on screen while the next one loads (no blank flash between pages).
    placeholderData: (prev) => prev,
    staleTime: 60_000,
  });
}

function useTitle(title: string, data: SiteData) {
  const brand = data.config.brand.name || data.business.name;
  useEffect(() => {
    document.title = title ? `${title} — ${brand}` : data.config.seo.title || brand;
  }, [title, brand, data.config.seo.title]);
}

function Head({ s, more }: { s: { heading: string; subheading?: string }; more?: { to: string; label: string } }) {
  if (!s.heading && !more) return null;
  return (
    <div className="section-head">
      <div>
        {s.heading && <h2>{s.heading}</h2>}
        {s.subheading && <p className="muted">{s.subheading}</p>}
      </div>
      {more && <Link className="more" to={more.to}>{more.label} <ArrowRight size={14} style={{ verticalAlign: "-2px" }} /></Link>}
    </div>
  );
}

// ── Home ──────────────────────────────────────────────────────────────

function ProductSection({ data, s, alt }: { data: SiteData; s: Section; alt: boolean }) {
  const { data: list, isLoading } = useProducts({ section: s.key, limit: 12 });
  if (!isLoading && !list?.items.length) return null;
  return (
    <section className={`section ${alt ? "alt" : ""}`}>
      <div className="wrap">
        <Head s={s} more={{ to: "/products", label: "View all" }} />
        {isLoading ? <GridSkeleton data={data} /> : <ProductGrid data={data} items={list!.items} rail={s.layout === "carousel"} />}
      </div>
    </section>
  );
}

function HomeSection({ data, s, alt }: { data: SiteData; s: Section; alt: boolean }) {
  const c = data.config;
  const cls = `section ${alt ? "alt" : ""}`;
  switch (s.key) {
    case "hero": {
      const h = c.hero;
      return (
        <section className={`hero ${h.image ? "has-img" : ""}`} data-align={h.align}>
          {h.image && <img src={mediaUrl(h.image)} alt="" fetchPriority="high" />}
          {h.image && h.overlay && <div className="shade" />}
          <div className="wrap">
            <h1>{h.headline || c.brand.name || data.business.name}</h1>
            {h.text && <p>{h.text}</p>}
            {(hasCta(h.primary) || hasCta(h.secondary)) && (
              <div className="ctas">
                {hasCta(h.primary) && <CtaLink cta={h.primary} className={`btn ${h.image ? "light" : ""}`} />}
                {hasCta(h.secondary) && <CtaLink cta={h.secondary} className="btn outline" />}
              </div>
            )}
          </div>
        </section>
      );
    }
    case "categories": {
      const cats = c.categories.limit ? data.categories.slice(0, c.categories.limit) : data.categories;
      if (!cats.length) return null;
      return (
        <section className={cls}>
          <div className="wrap">
            <Head s={s} more={data.categories.length > cats.length ? { to: "/categories", label: "All categories" } : undefined} />
            <CategoryTiles data={data} items={cats} rail={(s.layout || c.categories.style) === "carousel"} />
          </div>
        </section>
      );
    }
    case "featured":
    case "new_arrivals":
    case "popular":
      return <ProductSection data={data} s={s} alt={alt} />;
    case "promotions": {
      const promos = c.promotions.filter((p) => p.active);
      if (!promos.length) return null;
      return (
        <section className={cls}>
          <div className="wrap">
            <Head s={s} />
            <div className="tiles">
              {promos.map((p) => (
                <div key={p.id} className={`promo ${p.image ? "has-img" : ""}`}>
                  {p.image && <img src={mediaUrl(p.image)} alt="" loading="lazy" />}
                  <h3>{p.title}</h3>
                  {p.text && <p>{p.text}</p>}
                  {hasCta(p.cta) && <div><CtaLink cta={p.cta} className="btn light sm" /></div>}
                </div>
              ))}
            </div>
          </div>
        </section>
      );
    }
    case "about":
      if (!c.about.intro) return null;
      return (
        <section className={cls}>
          <div className="wrap split">
            <div className="prose">
              <h2 style={{ fontSize: "clamp(1.35rem, 4.2vw, 2.1rem)" }}>{s.heading || "About Us"}</h2>
              <p className="muted" style={{ fontSize: "1.05em" }}>{c.about.intro}</p>
              {hasCta(c.about.cta) && <div><CtaLink cta={c.about.cta} className="btn outline" /></div>}
            </div>
            {c.about.image && <img src={mediaUrl(c.about.image)} alt="" loading="lazy" />}
          </div>
        </section>
      );
    case "services": {
      const items = c.services.items.filter((x) => x.active).slice(0, 6);
      if (!items.length) return null;
      return (
        <section className={cls}>
          <div className="wrap">
            <Head s={s} more={{ to: "/services", label: "All services" }} />
            <ServiceTiles data={data} short />
          </div>
        </section>
      );
    }
    case "testimonials": {
      const items = c.testimonials.items;
      if (!c.testimonials.show || !items.length) return null;
      return (
        <section className={cls}>
          <div className="wrap">
            <Head s={s} />
            <Testimonials data={data} items={items} />
          </div>
        </section>
      );
    }
    case "cta":
      return (
        <section className="section">
          <div className="wrap">
            <div className="cta-band">
              <h2>{s.heading || "Ready to order?"}</h2>
              {s.subheading && <p>{s.subheading}</p>}
              {s.cta_label && <CtaLink cta={{ label: s.cta_label, target: s.cta_target || "products" }} className="btn light" />}
            </div>
          </div>
        </section>
      );
    case "contact":
      return (
        <section className={cls}>
          <div className="wrap split" style={{ alignItems: "start" }}>
            <div className="prose">
              <h2 style={{ fontSize: "clamp(1.35rem, 4.2vw, 2.1rem)" }}>{s.heading || "Get in Touch"}</h2>
              {c.contact.intro && <p className="muted">{c.contact.intro}</p>}
            </div>
            <ContactList data={data} />
          </div>
        </section>
      );
    default:
      return null;
  }
}

export function HomePage({ data }: { data: SiteData }) {
  useTitle("", data);
  const sections = data.config.sections.filter((s) => s.visible);
  let n = 0;
  return <>{sections.map((s) => <HomeSection key={s.key} data={data} s={s} alt={s.key !== "hero" && s.key !== "cta" && n++ % 2 === 1} />)}</>;
}

// ── Products & categories ─────────────────────────────────────────────

const SORTS: [string, string, boolean][] = [
  ["recommended", "Recommended", false], ["newest", "Newest", false], ["name_asc", "Name A–Z", false], ["name_desc", "Name Z–A", false],
  ["price_asc", "Price: low to high", true], ["price_desc", "Price: high to low", true],
];

/** Roadmap 80: one product listing for all products, categories and search results — search, category, availability,
 * sort, "Showing 1–10 of 143", and pages (or "Load more") counted in products, kept in the URL. */
export function ProductsPage({ data, categoryId }: { data: SiteData; categoryId?: string }) {
  const [params, setParams] = useSearchParams();
  const q = params.get("q") ?? "";
  const cat = categoryId ?? params.get("category") ?? "";
  const sort = params.get("sort") ?? "recommended";
  const stock = params.get("stock") === "in" ? "in" : "";
  const page = Math.max(1, Number(params.get("page")) || 1);
  const cfg = data.config.products;
  const paged = cfg.pagination !== false;
  const per = Math.min(100, Math.max(1, cfg.per_page ?? 10));
  const [more, setMore] = useState(1);
  const category = data.categories.find((c) => c.id === cat);
  useTitle(category?.name ?? "Products", data);
  const head = useRef<HTMLDivElement>(null);
  const filterKey = `${q}|${cat}|${sort}|${stock}`;
  useEffect(() => setMore(1), [filterKey]);
  const limit = paged ? per : per * more;
  const offset = paged ? (page - 1) * per : 0;
  const { data: list, isLoading, isFetching } = useProducts({ q: q || undefined, category: cat || undefined, sort: sort === "recommended" ? undefined : sort, stock: stock || undefined, limit, offset });
  const total = list?.total ?? 0;
  const pages = Math.max(1, Math.ceil(total / per));
  /** Changing a filter or the sort starts again at page 1; the page lives in the URL. */
  const update = (patch: Record<string, string>, keepPage = false) => {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (v) next.set(k, v);
      else next.delete(k);
    }
    if (!keepPage) next.delete("page");
    setParams(next, { replace: !keepPage });
  };
  const goPage = (n: number) => {
    update({ page: n > 1 ? String(n) : "" }, true);
    head.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  const sorts = SORTS.filter(([, , price]) => !price || data.show_prices);
  const showing = total ? `Showing ${offset + 1}–${Math.min(total, offset + (list?.items.length ?? 0))} of ${total} ${total === 1 ? "product" : "products"}` : "";
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap stack">
        <div ref={head} className="section-head" style={{ marginBottom: 0, scrollMarginTop: 80 }}>
          <div>
            <h1 style={{ fontSize: "clamp(1.6rem, 5vw, 2.4rem)" }}>{category?.name ?? (q ? `Results for “${q}”` : "Products")}</h1>
            {list && <p className="muted" aria-live="polite">{showing || "0 products"}</p>}
          </div>
        </div>
        <form role="search" onSubmit={(e) => { e.preventDefault(); const v = (new FormData(e.currentTarget).get("q") as string).trim(); update({ q: v }); }}>
          <label className="sr" htmlFor="pq">Search products</label>
          <input id="pq" name="q" type="search" className="field" placeholder="Search products" defaultValue={q} key={q} />
        </form>
        {data.categories.length > 0 && (
          <nav className="chips" aria-label="Categories">
            <Link className={`chip ${!cat ? "on" : ""}`} to={{ pathname: "/products", search: new URLSearchParams({ ...(sort !== "recommended" ? { sort } : {}), ...(stock ? { stock } : {}) }).toString() }}>All</Link>
            {data.categories.map((c) => (
              <Link key={c.id} className={`chip ${cat === c.id ? "on" : ""}`}
                to={{ pathname: `/categories/${c.id}`, search: new URLSearchParams({ ...(sort !== "recommended" ? { sort } : {}), ...(stock ? { stock } : {}) }).toString() }}>{c.name}</Link>
            ))}
          </nav>
        )}
        <div className="toolbar">
          {cfg.grid.show_availability && (
            <label className="check">
              <input type="checkbox" checked={!!stock} onChange={(e) => update({ stock: e.target.checked ? "in" : "" })} /> In stock only
            </label>
          )}
          <label className="sort">
            <span className="sr">Sort by</span>
            <select className="field" value={sort} onChange={(e) => update({ sort: e.target.value === "recommended" ? "" : e.target.value })}>
              {sorts.map(([k, l]) => <option key={k} value={k}>{l}</option>)}
            </select>
          </label>
        </div>
        {isLoading ? <GridSkeleton data={data} n={Math.min(per, 8)} /> : list!.items.length ? (
          <div style={{ opacity: isFetching && paged ? 0.6 : 1, transition: "opacity .2s" }}><ProductGrid data={data} items={list!.items} /></div>
        ) : (
          <div className="empty"><PackageSearch aria-hidden /><p>No products found{q && <> for “{q}”</>}.</p>{(q || cat || stock) && <Link className="btn outline sm" to="/products">See all products</Link>}</div>
        )}
        {paged && pages > 1 && <Pager page={Math.min(page, pages)} pages={pages} onPage={goPage} />}
        {!paged && list && list.items.length < total && (
          <div style={{ display: "grid", justifyItems: "center", gap: 8 }}>
            <button type="button" className="btn outline" disabled={isFetching} onClick={() => setMore((m) => m + 1)}>{isFetching ? "Loading…" : "Load more"}</button>
          </div>
        )}
      </div>
    </section>
  );
}

/** Previous | 1 … 4 5 6 … 12 | Next on wider screens; ‹ Page 2 of 8 › on phones. */
export function Pager({ page, pages, onPage }: { page: number; pages: number; onPage: (n: number) => void }) {
  return (
    <nav className="pager" aria-label="Pages">
      <button type="button" className="pg nav" disabled={page <= 1} onClick={() => onPage(page - 1)} aria-label="Previous page"><ChevronLeft aria-hidden /><span className="wide">Previous</span></button>
      <span className="pg-count">Page {page} of {pages}</span>
      <span className="pg-nums">
        {pageItems(page, pages).map((n, i) => n === "…"
          ? <span key={`e${i}`} className="pg gap" aria-hidden>…</span>
          : <button key={n} type="button" className={`pg ${n === page ? "on" : ""}`} aria-current={n === page ? "page" : undefined} aria-label={`Page ${n}`} onClick={() => onPage(n)}>{n}</button>)}
      </span>
      <button type="button" className="pg nav" disabled={page >= pages} onClick={() => onPage(page + 1)} aria-label="Next page"><span className="wide">Next</span><ChevronRight aria-hidden /></button>
    </nav>
  );
}

export function CategoryPage({ data }: { data: SiteData }) {
  const { id } = useParams();
  return <ProductsPage data={data} categoryId={id} />;
}

export function CategoriesPage({ data }: { data: SiteData }) {
  useTitle("Categories", data);
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap stack">
        <h1 style={{ fontSize: "clamp(1.6rem, 5vw, 2.4rem)" }}>Categories</h1>
        {data.categories.length ? <CategoryTiles data={data} items={data.categories as Category[]} /> : <div className="empty"><p>No categories yet.</p></div>}
      </div>
    </section>
  );
}

export function ProductPage({ data }: { data: SiteData }) {
  const { slug } = useParams();
  const { data: res, isLoading, error } = useQuery({
    queryKey: ["site-product", slug],
    queryFn: () => call<{ product: Product; related: Product[] }>(`/site/products/${encodeURIComponent(slug!)}`),
    retry: false,
  });
  const p = res?.product;
  useTitle(p?.seo_title || p?.name || "Product", data);
  const [photo, setPhoto] = useState(0);
  const [qty, setQty] = useState(1);
  const [added, setAdded] = useState(false);
  useEffect(() => {
    if (p) track("product_view", p.id);
    setPhoto(0);
    setQty(1);
  }, [p?.id]); // eslint-disable-line react-hooks/exhaustive-deps
  if (isLoading) return <div className="wrap section"><div className="skel" style={{ height: 320 }} /></div>;
  if (error || !p) return <NotFound data={data} what="This product is not available." />;
  const g = data.config.products.grid;
  const act = productAction(data, p);
  const canOrder = p.action === "add_to_cart" && data.ordering.enabled && p.in_stock !== false;
  const photos = p.photos.length ? p.photos : [];
  return (
    <section className="section" style={{ paddingTop: 20 }}>
      <div className="wrap">
        <nav className="crumbs" aria-label="Breadcrumb">
          <Link to="/products">Products</Link>
          {p.category_id && p.category_name && <>/<Link to={`/categories/${p.category_id}`}>{p.category_name}</Link></>}
        </nav>
        <div className="pdp">
          <div className="gallery" style={{ "--ratio": ratio(g.ratio), "--fit": g.fit } as React.CSSProperties}>
            <div className="main">
              {photos[photo] ? <img src={photos[photo]} alt={p.name} /> : <div style={{ height: "100%", display: "grid", placeItems: "center" }} className="muted"><ImageOff aria-hidden /></div>}
            </div>
            {photos.length > 1 && (
              <div className="thumbs" role="tablist" aria-label="Photos">
                {photos.map((u, i) => <button key={u} type="button" role="tab" aria-selected={i === photo} aria-label={`Photo ${i + 1}`} className={i === photo ? "on" : ""} onClick={() => setPhoto(i)}><img src={u} alt="" /></button>)}
              </div>
            )}
          </div>
          <div className="stack">
            {g.show_badges && p.badge && <span className="badge" style={{ position: "static", justifySelf: "start" }}>{badgeLabel(p.badge)}</span>}
            <h1 style={{ fontSize: "clamp(1.5rem, 5vw, 2.3rem)" }}>{p.name}</h1>
            {p.price != null && <p style={{ fontSize: "1.5rem", fontWeight: 700, color: "var(--heading)" }}>{money(data.business.currency, p.price)}</p>}
            {g.show_availability && p.in_stock != null && <p className={p.in_stock ? "muted" : ""} style={p.in_stock ? undefined : { color: "#b91c1c" }}>{p.in_stock ? "In stock" : "Out of stock"}</p>}
            {p.description && <p className="muted" style={{ whiteSpace: "pre-line" }}>{p.description}</p>}
            {canOrder ? (
              <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
                <div className="qty">
                  <button type="button" aria-label="Fewer" onClick={() => setQty(Math.max(1, qty - 1))}><Minus /></button>
                  <span aria-live="polite">{qty}</span>
                  <button type="button" aria-label="More" onClick={() => setQty(Math.min(99, qty + 1))}><Plus /></button>
                </div>
                <button type="button" className="btn" style={{ flex: 1 }} onClick={() => { addToCart(p, qty); setAdded(true); setTimeout(() => setAdded(false), 1500); }}>
                  {added ? <><Check /> Added</> : <><ShoppingBag /> {p.cta_label || "Add to cart"}{p.price != null && <> · {money(data.business.currency, Number(p.price) * qty)}</>}</>}
                </button>
              </div>
            ) : act ? (
              act.to ? <Link className="btn" to={act.to}>{act.label}</Link> : <a className="btn" href={act.href} target="_blank" rel="noopener noreferrer">{act.label}</a>
            ) : p.in_stock === false ? <p className="note">Out of stock — check back soon.</p> : !data.ordering.enabled && <p className="note">Online ordering is not available right now — please contact us.</p>}
            {p.price == null && p.action === "add_to_cart" && <p className="hint">Price on request — we will confirm it with you before any payment.</p>}
          </div>
        </div>
        {res!.related.length > 0 && (
          <div style={{ marginTop: "var(--section)" }}>
            <Head s={{ heading: "You may also like" }} />
            <ProductGrid data={data} items={res!.related} rail />
          </div>
        )}
      </div>
    </section>
  );
}

export const ratio = (r: string) => (r === "portrait" ? "3 / 4" : r === "landscape" ? "4 / 3" : "1 / 1");

// ── Content pages ─────────────────────────────────────────────────────

function ServiceTiles({ data, short }: { data: SiteData; short?: boolean }) {
  const items = data.config.services.items.filter((x) => x.active);
  return (
    <div className="tiles">
      {items.map((s) => {
        const Icon = SERVICE_ICONS[s.icon];
        return (
          <article key={s.id} className="tile" id={`service-${s.id}`}>
            {s.image ? <img src={mediaUrl(s.image)} alt="" loading="lazy" /> : Icon && <span className="ic"><Icon aria-hidden /></span>}
            <h3 style={{ fontSize: "1.15rem" }}>{s.name}</h3>
            {s.short && <p className="muted">{s.short}</p>}
            {!short && s.details && <p style={{ whiteSpace: "pre-line" }}>{s.details}</p>}
            {hasCta(s.cta) && <div><CtaLink cta={s.cta} className="btn sm outline" /></div>}
          </article>
        );
      })}
    </div>
  );
}

export function ServicesPage({ data }: { data: SiteData }) {
  useTitle("Services", data);
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap stack">
        <h1 style={{ fontSize: "clamp(1.6rem, 5vw, 2.4rem)" }}>Our Services</h1>
        {data.config.services.intro && <p className="muted" style={{ maxWidth: "60ch" }}>{data.config.services.intro}</p>}
        {data.config.services.items.some((s) => s.active) ? <ServiceTiles data={data} /> : <div className="empty"><p>Services coming soon.</p></div>}
      </div>
    </section>
  );
}

export function AboutPage({ data }: { data: SiteData }) {
  useTitle("About Us", data);
  const a = data.config.about;
  const blocks = [
    a.show_story && a.story && { h: "Our Story", t: a.story },
    a.show_mission && a.mission && { h: "Our Mission", t: a.mission },
    a.show_vision && a.vision && { h: "Our Vision", t: a.vision },
  ].filter(Boolean) as { h: string; t: string }[];
  return (
    <>
      <section className="section" style={{ paddingTop: 28 }}>
        <div className="wrap split">
          <div className="prose">
            <h1 style={{ fontSize: "clamp(1.7rem, 5.5vw, 2.8rem)" }}>About {data.config.brand.name || data.business.name}</h1>
            {a.intro && <p className="muted" style={{ fontSize: "1.08em" }}>{a.intro}</p>}
          </div>
          {a.image && <img src={mediaUrl(a.image)} alt="" />}
        </div>
      </section>
      {(blocks.length > 0 || (a.show_values && a.values.length > 0)) && (
        <section className="section alt">
          <div className="wrap tiles">
            {blocks.map((b) => <div key={b.h} className="tile"><h2 style={{ fontSize: "1.25rem" }}>{b.h}</h2><p style={{ whiteSpace: "pre-line" }}>{b.t}</p></div>)}
            {a.show_values && a.values.length > 0 && (
              <div className="tile"><h2 style={{ fontSize: "1.25rem" }}>Our Values</h2><ul style={{ margin: 0, paddingLeft: 18, display: "grid", gap: 6 }}>{a.values.map((v) => <li key={v}>{v}</li>)}</ul></div>
            )}
          </div>
        </section>
      )}
    </>
  );
}

export function ContactPage({ data }: { data: SiteData }) {
  useTitle("Contact", data);
  const [params] = useSearchParams();
  const about = params.get("about");
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap split" style={{ alignItems: "start" }}>
        <div className="prose">
          <h1 style={{ fontSize: "clamp(1.7rem, 5.5vw, 2.8rem)" }}>Contact Us</h1>
          {data.config.contact.intro && <p className="muted">{data.config.contact.intro}</p>}
          {about && <p className="note">Asking about <strong>{about}</strong>? Call or WhatsApp us and we will help right away.</p>}
        </div>
        <ContactList data={data} />
      </div>
    </section>
  );
}

export function TestimonialsPage({ data }: { data: SiteData }) {
  useTitle("Testimonials", data);
  const items = data.config.testimonials.items;
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap stack">
        <h1 style={{ fontSize: "clamp(1.6rem, 5vw, 2.4rem)" }}>What Our Customers Say</h1>
        {items.length ? <Testimonials data={{ ...data, config: { ...data.config, testimonials: { ...data.config.testimonials, auto_scroll: false } } }} items={items} /> : <div className="empty"><p>No testimonials yet.</p></div>}
      </div>
    </section>
  );
}

export function NotFound({ data, what }: { data: SiteData; what?: string }) {
  useTitle("Not found", data);
  return (
    <section className="section">
      <div className="wrap empty">
        <PackageSearch aria-hidden />
        <h1 style={{ fontSize: "1.5rem" }}>{what ?? "This page does not exist."}</h1>
        <Link className="btn" to="/">Go to the home page</Link>
      </div>
    </section>
  );
}

// ── Cart & checkout (the existing S'Shop orders engine) ───────────────

export function useCart() {
  return useSyncExternalStore(cartStore.subscribe, cartStore.get, cartStore.get);
}

export function CartLines({ data, compact }: { data: SiteData; compact?: boolean }) {
  const lines = useCart();
  return (
    <div>
      {lines.map((l) => (
        <div key={l.id} className="line">
          {l.photo ? <img src={l.photo} alt="" /> : <span className="ph" />}
          <div style={{ minWidth: 0 }}>
            <Link to={`/products/${l.slug}`} className="nm" style={{ display: "block", textDecoration: "none" }}>{l.name}</Link>
            {l.price != null && <span className="muted" style={{ fontSize: ".9em" }}>{money(data.business.currency, l.price)}</span>}
          </div>
          <div className="qty" style={compact ? { transform: "scale(.92)" } : undefined}>
            <button type="button" aria-label={l.qty === 1 ? `Remove ${l.name}` : "Fewer"} onClick={() => cartStore.qty(l.id, l.qty - 1)}>{l.qty === 1 ? <Trash2 /> : <Minus />}</button>
            <span>{l.qty}</span>
            <button type="button" aria-label="More" onClick={() => cartStore.qty(l.id, l.qty + 1)}><Plus /></button>
          </div>
        </div>
      ))}
    </div>
  );
}

export function cartTotal(lines: { price: string | null; qty: number }[]): number | null {
  if (lines.some((l) => l.price == null)) return null;
  return lines.reduce((a, l) => a + Number(l.price) * l.qty, 0);
}

interface Ident { mobile: string; exists: boolean; first_name: string | null; otp_required: boolean }
interface Placed { id: string; order_no: string; track_token: string; total: string | null; price_note: string | null }

export function OrderPage({ data }: { data: SiteData }) {
  useTitle("Your order", data);
  const lines = useCart();
  const navigate = useNavigate();
  const [customer, setCustomer] = useState<Customer | null>(() => customerStore.get());
  const [mobile, setMobile] = useState("");
  const [ident, setIdent] = useState<Ident | null>(null);
  const [code, setCode] = useState("");
  const [firstName, setFirstName] = useState("");
  const [where, setWhere] = useState("");
  const [notes, setNotes] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [placed, setPlaced] = useState<Placed | null>(null);
  const total = cartTotal(lines);
  useEffect(() => {
    if (lines.length) track("order_start");
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Something went wrong");
      if (e instanceof SiteError && e.status === 401) {
        customerStore.set(null);
        setCustomer(null);
      }
    } finally {
      setBusy(false);
    }
  };

  if (placed) {
    return (
      <section className="section">
        <div className="wrap stack" style={{ maxWidth: 560, textAlign: "center", justifyItems: "center" }}>
          <span style={{ width: 64, height: 64, borderRadius: 999, background: "var(--primary)", color: "var(--on-primary)", display: "grid", placeItems: "center" }}><Check size={30} /></span>
          <h1 style={{ fontSize: "1.8rem" }}>Thank you{customer ? `, ${customer.first_name}` : ""}!</h1>
          <p>Your order <strong>{placed.order_no}</strong> has been received. We are processing it now.</p>
          {placed.total != null ? <p className="total" style={{ justifyContent: "center", gap: 8 }}>Total {money(data.business.currency, placed.total)}</p> : placed.price_note && <p className="note">{placed.price_note}</p>}
          <Link className="btn" to={`/track/${placed.track_token}`}>Track your order</Link>
          <Link className="btn ghost" to="/products">Continue shopping</Link>
        </div>
      </section>
    );
  }

  if (!data.ordering.enabled) {
    return <section className="section"><div className="wrap empty"><ShoppingBag aria-hidden /><h1 style={{ fontSize: "1.4rem" }}>Online ordering is not available right now.</h1><Link className="btn" to="/contact">Contact us</Link></div></section>;
  }
  if (!lines.length) {
    return <section className="section"><div className="wrap empty"><ShoppingBag aria-hidden /><h1 style={{ fontSize: "1.4rem" }}>Your cart is empty</h1><Link className="btn" to="/products">Browse products</Link></div></section>;
  }

  const identify = () => run(async () => {
    const r = await call<Ident>("/site/identify", { body: { mobile } });
    setIdent(r);
    if (r.exists && !r.otp_required) {
      const s = await call<{ token: string; customer: { first_name: string; mobile: string } }>("/site/session", { body: { mobile: r.mobile } });
      const c = { token: s.token, first_name: s.customer.first_name, mobile: s.customer.mobile };
      customerStore.set(c);
      setCustomer(c);
    }
  });
  const startSession = () => run(async () => {
    const s = await call<{ token: string; customer: { first_name: string; mobile: string } }>("/site/session", {
      body: { mobile: ident!.mobile, code: code || undefined, first_name: firstName || undefined },
    });
    const c = { token: s.token, first_name: s.customer.first_name, mobile: s.customer.mobile };
    customerStore.set(c);
    setCustomer(c);
  });
  const place = () => run(async () => {
    const r = await call<Placed>("/site/orders", {
      token: customer!.token,
      body: { items: lines.map((l) => ({ product_id: l.id, quantity: l.qty })), delivery_location: where, notes, visitor: currentVisitor() },
    });
    cartStore.clear();
    setPlaced(r);
    window.scrollTo({ top: 0 });
  });

  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap" style={{ maxWidth: 640 }}>
        <div className="stack">
          <h1 style={{ fontSize: "clamp(1.6rem, 5vw, 2.2rem)" }}>Your order</h1>
          <CartLines data={data} />
          {total != null ? <div className="total"><span>Total</span><span>{money(data.business.currency, total)}</span></div> : <p className="note">We will confirm the final price with you before any payment.</p>}
          {error && <p className="err" role="alert">{error}</p>}
          {!customer ? (
            !ident || (ident.exists && !ident.otp_required) ? (
              <form className="stack" onSubmit={(e) => { e.preventDefault(); identify(); }}>
                <div>
                  <label className="label" htmlFor="m">Your mobile number</label>
                  <input id="m" className="field" inputMode="tel" autoComplete="tel" placeholder="0712 345 678" value={mobile} onChange={(e) => setMobile(e.target.value)} required />
                  <p className="hint">We use it to send your order updates.</p>
                </div>
                <button className="btn block" disabled={busy || mobile.replace(/\D/g, "").length < 9}>{busy ? <Loader2 className="spin" /> : "Continue"}</button>
              </form>
            ) : (
              <form className="stack" onSubmit={(e) => { e.preventDefault(); startSession(); }}>
                {ident.otp_required && (
                  <div>
                    <label className="label" htmlFor="c">Verification code</label>
                    <input id="c" className="field" inputMode="numeric" autoComplete="one-time-code" maxLength={6} value={code} onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))} autoFocus />
                    <p className="hint">We sent a 6-digit code to your WhatsApp.</p>
                  </div>
                )}
                {!ident.exists && (
                  <div>
                    <label className="label" htmlFor="f">First name</label>
                    <input id="f" className="field" autoComplete="given-name" value={firstName} onChange={(e) => setFirstName(e.target.value)} autoFocus={!ident.otp_required} />
                  </div>
                )}
                <button className="btn block" disabled={busy || (ident.otp_required && code.length !== 6) || (!ident.exists && !firstName.trim())}>{busy ? <Loader2 className="spin" /> : "Continue"}</button>
                <button type="button" className="btn ghost" onClick={() => { setIdent(null); setCode(""); }}>Use a different number</button>
              </form>
            )
          ) : (
            <form className="stack" onSubmit={(e) => { e.preventDefault(); place(); }}>
              <p className="muted">Ordering as <strong style={{ color: "var(--heading)" }}>{customer.first_name}</strong> · {customer.mobile} <button type="button" className="btn ghost sm" onClick={() => { customerStore.set(null); setCustomer(null); setIdent(null); }}>Change</button></p>
              <div>
                <label className="label" htmlFor="w">Delivery location</label>
                <input id="w" className="field" autoComplete="street-address" placeholder="Estate, street, building or pick-up" value={where} onChange={(e) => setWhere(e.target.value)} required />
              </div>
              <div>
                <label className="label" htmlFor="n">Notes (optional)</label>
                <textarea id="n" className="field" value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={500} />
              </div>
              <button className="btn block" disabled={busy || !where.trim()}>{busy ? <Loader2 className="spin" /> : <>Place order{total != null && <> · {money(data.business.currency, total)}</>}</>}</button>
            </form>
          )}
          <button type="button" className="btn ghost" onClick={() => navigate("/products")}>Continue shopping</button>
        </div>
      </div>
    </section>
  );
}

interface TrackData {
  business: { name: string; show_prices: boolean };
  order: { order_no: string; status: string; status_label: string; total?: string; created_at: string; delivery_location: string; terminal: boolean };
  steps: { status: string; label: string; done: boolean; current: boolean }[];
  items: { name: string; quantity: number; line_total?: string }[];
}

export function TrackPage({ data }: { data: SiteData }) {
  useTitle("Track your order", data);
  const { token } = useParams();
  const { data: t, isLoading, error } = useQuery({
    queryKey: ["site-track", token],
    queryFn: () => call<TrackData>(`/portal/track/${token}`, { query: {} }),
    refetchInterval: 60_000,
    retry: false,
  });
  if (isLoading) return <div className="wrap section"><Loader2 className="spin" /></div>;
  if (error || !t) return <NotFound data={data} what="We could not find this order." />;
  const o = t.order;
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap stack" style={{ maxWidth: 560 }}>
        <p className="muted">{o.order_no} · {new Date(o.created_at).toLocaleString()}</p>
        <h1 style={{ fontSize: "1.8rem" }}>{o.status_label}</h1>
        {o.terminal ? <p className="err"><XCircle size={16} style={{ verticalAlign: "-3px" }} /> This order was {o.status}.</p> : (
          <div className="steps">
            {t.steps.map((s, i) => <div key={s.status} className={`s ${s.done || s.current ? "done" : ""}`}><span className="b">{s.done ? <Check /> : i + 1}</span><span style={{ paddingTop: 3, fontWeight: s.current ? 700 : 400 }}>{s.label}</span></div>)}
          </div>
        )}
        {o.delivery_location && <p><strong>Delivering to</strong><br /><span className="muted">{o.delivery_location}</span></p>}
        <div>
          {t.items.map((i) => (
            <div key={i.name} className="line" style={{ gridTemplateColumns: "1fr auto" }}>
              <span>{i.name}{i.quantity > 1 && <span className="muted"> × {i.quantity}</span>}</span>
              {t.business.show_prices && i.line_total != null && <span>{money(data.business.currency, i.line_total)}</span>}
            </div>
          ))}
          {t.business.show_prices && o.total != null && <div className="total" style={{ paddingTop: 12 }}><span>Total</span><span>{money(data.business.currency, o.total)}</span></div>}
        </div>
        <Link className="btn outline" to="/products">Continue shopping</Link>
      </div>
    </section>
  );
}

export function useScrollTop() {
  const { pathname } = useLocation();
  useEffect(() => {
    window.scrollTo({ top: 0 });
  }, [pathname]);
}

export const useCategoryName = (data: SiteData, id: string | null) => useMemo(() => data.categories.find((c) => c.id === id)?.name, [data, id]);
