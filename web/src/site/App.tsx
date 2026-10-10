/** A business's public website: header, pages, cart, footer — themed entirely from its published configuration. */
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { BrowserRouter, Link, NavLink, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Menu, Moon, Search, ShoppingBag, Sun, X } from "lucide-react";
import type { Palette, Product, SiteData } from "./types";
import { CampaignBanner } from "./campaign";
import { call, consent, money, onColor, pagePath, runtime, track } from "./lib";
import {
  AboutPage, CartLines, cartTotal, CategoriesPage, CategoryPage, ContactPage, HomePage, NotFound, OrderPage, ProductPage, ProductsPage, ratio,
  ServicesPage, TestimonialsPage, TrackPage, useCart, useScrollTop,
} from "./pages";

const RADIUS = { none: "0px", small: "6px", medium: "12px", large: "20px" } as const;

function themeVars(p: Palette, data: SiteData): CSSProperties {
  const g = data.config.products.grid;
  const t = data.config.theme;
  return {
    "--bg": p.background, "--surface": p.surface, "--text": p.text, "--muted": p.muted, "--heading": p.heading,
    "--primary": p.primary, "--secondary": p.secondary, "--accent": p.accent,
    "--on-primary": onColor(p.primary), "--on-accent": onColor(p.accent),
    "--heading-font": `"${t.heading_font}"`, "--body-font": `"${t.body_font}"`,
    "--card-r": RADIUS[g.radius] ?? "12px", "--ratio": ratio(g.ratio), "--fit": g.fit, "--lines": g.name_lines,
    // Whole-product photos sit inside the frame with breathing room (roadmap 80).
    "--img-pad": g.fit === "contain" ? "6%" : "0",
    colorScheme: p === data.config.theme.dark ? "dark" : "light",
  } as CSSProperties;
}

function useMode(data: SiteData): ["light" | "dark", (() => void) | null] {
  const modes = data.config.theme.modes;
  const key = `sshop.site.mode.${data.slug}`;
  const [mode, setMode] = useState<"light" | "dark">(() => {
    if (modes !== "both") return modes === "dark" ? "dark" : "light";
    try {
      const saved = localStorage.getItem(key);
      if (saved === "light" || saved === "dark") return saved;
    } catch { /* default below */ }
    return "light";
  });
  if (modes !== "both") return [modes === "dark" ? "dark" : "light", null];
  return [mode, () => setMode((m) => {
    const next = m === "light" ? "dark" : "light";
    try { localStorage.setItem(key, next); } catch { /* not kept */ }
    return next;
  })];
}

function Logo({ data }: { data: SiteData }) {
  const name = data.config.brand.name || data.business.name;
  return (
    <Link to="/" className="brand" aria-label={`${name} — home`}>
      {data.business.logo_url ? <img src={data.business.logo_url} alt="" /> : <span className="mark" aria-hidden>{name[0]}</span>}
      <strong>{name}</strong>
    </Link>
  );
}

function SearchPanel({ data, onClose }: { data: SiteData; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [items, setItems] = useState<Pick<Product, "id" | "slug" | "name" | "price" | "photo_thumb" | "category_name">[]>([]);
  const navigate = useNavigate();
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  useEffect(() => {
    const term = q.trim();
    if (term.length < 2) { setItems([]); return; }
    const timer = setTimeout(() => {
      call<{ items: typeof items }>("/site/products", { query: { q: term, suggest: "true" } }).then((r) => setItems(r.items)).catch(() => setItems([]));
    }, 180);
    return () => clearTimeout(timer);
  }, [q]);
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);
  return (
    <>
      <div className="drawer-bg" onClick={onClose} />
      <div className="search-panel" role="dialog" aria-modal="true" aria-label="Search">
        <form className="row" role="search" onSubmit={(e) => { e.preventDefault(); if (q.trim()) { navigate(`/products?q=${encodeURIComponent(q.trim())}`); onClose(); } }}>
          <label className="sr" htmlFor="sq">Search products</label>
          <input ref={input} id="sq" className="field" type="search" placeholder="Search products" value={q} onChange={(e) => setQ(e.target.value)} autoComplete="off" />
          <button type="button" className="icon-btn" aria-label="Close search" onClick={onClose}><X /></button>
        </form>
        {items.length > 0 && (
          <div className="suggest" role="listbox" aria-label="Suggestions">
            {items.map((p) => (
              <Link key={p.id} to={`/products/${p.slug}`} onClick={onClose} role="option" aria-selected={false}>
                {p.photo_thumb ? <img src={p.photo_thumb} alt="" /> : <span className="ph" />}
                <span style={{ minWidth: 0, flex: 1 }}>
                  <strong style={{ display: "block", color: "var(--heading)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.name}</strong>
                  {p.category_name && <span className="muted" style={{ fontSize: ".85em" }}>{p.category_name}</span>}
                </span>
                {p.price != null && <span style={{ fontWeight: 600 }}>{money(data.business.currency, p.price)}</span>}
              </Link>
            ))}
          </div>
        )}
        {q.trim().length >= 2 && !items.length && <p className="muted" style={{ textAlign: "center", marginTop: 12 }}>No matches yet — press Enter to search all products.</p>}
      </div>
    </>
  );
}

function CartDrawer({ data, onClose }: { data: SiteData; onClose: () => void }) {
  const lines = useCart();
  const total = cartTotal(lines);
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);
  return (
    <>
      <div className="drawer-bg" onClick={onClose} />
      <aside className="drawer right" role="dialog" aria-modal="true" aria-label="Your cart">
        <div className="drawer-head"><h2>Your cart</h2><button className="icon-btn" aria-label="Close cart" onClick={onClose}><X /></button></div>
        <div className="drawer-body">
          {lines.length ? <CartLines data={data} compact /> : <div className="empty"><ShoppingBag aria-hidden /><p>Your cart is empty.</p></div>}
        </div>
        {lines.length > 0 && (
          <div className="drawer-foot">
            {total != null ? <div className="total"><span>Total</span><span>{money(data.business.currency, total)}</span></div> : <p className="hint">Some prices are confirmed by the shop before payment.</p>}
            <Link className="btn block" to="/order" onClick={onClose}>Checkout</Link>
            <button className="btn ghost" onClick={onClose}>Continue shopping</button>
          </div>
        )}
      </aside>
    </>
  );
}

/** Re-runs a short animation on the element whenever `value` changes (cart badge bump). */
function useBump(value: number) {
  const [n, setN] = useState(0);
  const first = useRef(true);
  useEffect(() => {
    if (first.current) { first.current = false; return; }
    setN((x) => x + 1);
  }, [value]);
  return n > 0 ? `bump-${n % 2}` : "";
}

function Header({ data, onSearch, onCart, toggleMode, mode }: { data: SiteData; onSearch: () => void; onCart: () => void; toggleMode: (() => void) | null; mode: string }) {
  const [menu, setMenu] = useState(false);
  const lines = useCart();
  const count = lines.reduce((a, l) => a + l.qty, 0);
  const bump = useBump(count);
  const nav = data.config.navigation.filter((n) => n.visible && n.key !== "order");
  const { pathname } = useLocation();
  useEffect(() => setMenu(false), [pathname]);
  return (
    <header className="hdr">
      <div className="wrap">
        <button className="icon-btn menu-btn" aria-label="Menu" aria-expanded={menu} onClick={() => setMenu(true)}><Menu /></button>
        <Logo data={data} />
        <nav className="nav" aria-label="Main">
          {nav.map((n) => <NavLink key={n.key} to={pagePath(n.key)} end={n.key === "home"} className={({ isActive }) => (isActive ? "on" : "")}>{n.label}</NavLink>)}
        </nav>
        <button className="icon-btn" aria-label="Search products" onClick={onSearch}><Search /></button>
        {toggleMode && <button className="icon-btn" aria-label={mode === "dark" ? "Light mode" : "Dark mode"} onClick={toggleMode}>{mode === "dark" ? <Sun /> : <Moon />}</button>}
        {data.ordering.enabled && (
          <button className="icon-btn" aria-label={`Cart, ${count} items`} onClick={onCart}><ShoppingBag />{count > 0 && <span key={bump} className={`dot ${bump ? "bump" : ""}`}>{count}</span>}</button>
        )}
      </div>
      {menu && (
        <>
          <div className="drawer-bg" onClick={() => setMenu(false)} />
          <aside className="drawer left" role="dialog" aria-modal="true" aria-label="Menu">
            <div className="drawer-head"><Logo data={data} /><button className="icon-btn" aria-label="Close menu" onClick={() => setMenu(false)}><X /></button></div>
            <div className="drawer-body">
              <nav aria-label="Main">{data.config.navigation.filter((n) => n.visible).map((n) => <Link key={n.key} to={pagePath(n.key)}>{n.label}</Link>)}</nav>
            </div>
          </aside>
        </>
      )}
    </header>
  );
}

const SOCIAL: [keyof SiteData["config"]["social"], string][] = [
  ["instagram", "IG"], ["facebook", "FB"], ["tiktok", "TT"], ["x", "X"], ["linkedin", "IN"], ["youtube", "YT"],
];

function Footer({ data }: { data: SiteData }) {
  const c = data.config;
  const name = c.brand.name || data.business.name;
  const socials = SOCIAL.filter(([k]) => /^https:\/\//.test(c.social[k] ?? ""));
  return (
    <footer className="ftr">
      <div className="wrap">
        <div className="cols">
          <div>
            <h3>{name}</h3>
            {c.footer.description && <p className="muted">{c.footer.description}</p>}
            {socials.length > 0 && (
              <div className="socials">{socials.map(([k, label]) => <a key={k} href={c.social[k]} target="_blank" rel="noopener noreferrer" aria-label={k}>{label}</a>)}</div>
            )}
          </div>
          <div>
            <h3>Explore</h3>
            <ul>{c.navigation.filter((n) => n.visible).map((n) => <li key={n.key}><Link to={pagePath(n.key)}>{n.label}</Link></li>)}</ul>
          </div>
          {((c.contact.show_phone && c.contact.phone) || (c.contact.show_email && c.contact.email) || (c.contact.show_location && c.contact.location)) && <div>
            <h3>Contact</h3>
            <ul>
              {c.contact.show_phone && c.contact.phone && <li><a href={`tel:${c.contact.phone.replace(/\s/g, "")}`}>{c.contact.phone}</a></li>}
              {c.contact.show_email && c.contact.email && <li><a href={`mailto:${c.contact.email}`} style={{ overflowWrap: "anywhere" }}>{c.contact.email}</a></li>}
              {c.contact.show_location && c.contact.location && <li className="muted">{c.contact.location}</li>}
            </ul>
          </div>}
        </div>
        {c.footer.policies && <p className="muted" style={{ marginTop: 24, whiteSpace: "pre-line", fontSize: ".9em" }}>{c.footer.policies}</p>}
        <div className="base">
          <span>© {new Date().getFullYear()} {name}</span>
          {c.cookies.privacy_policy && /^https:\/\//.test(c.cookies.privacy_policy) && <a href={c.cookies.privacy_policy} target="_blank" rel="noopener noreferrer">Privacy policy</a>}
          {c.footer.show_attribution && <a href="https://s-shop.store" target="_blank" rel="noopener noreferrer">Powered by S'Shop</a>}
        </div>
      </div>
    </footer>
  );
}

function Consent({ data }: { data: SiteData }) {
  const [answer, setAnswer] = useState(() => consent.get());
  if (!data.config.cookies.analytics || answer || runtime.preview) return null;
  const choose = (v: "accepted" | "declined") => {
    consent.set(v);
    setAnswer(v);
    if (v === "accepted") track("visit");
  };
  return (
    <div className="consent" role="dialog" aria-label="Cookie consent">
      <p>We use a small anonymous cookie to understand which products visitors like. Your cart works either way.{" "}
        {/^https:\/\//.test(data.config.cookies.privacy_policy) && <a href={data.config.cookies.privacy_policy} target="_blank" rel="noopener noreferrer">Privacy policy</a>}
      </p>
      <div className="acts">
        <button className="btn ghost sm" onClick={() => choose("declined")}>Decline</button>
        <button className="btn sm" onClick={() => choose("accepted")}>Accept</button>
      </div>
    </div>
  );
}

function Shell({ data }: { data: SiteData }) {
  const [mode, toggleMode] = useMode(data);
  const palette = mode === "dark" ? data.config.theme.dark : data.config.theme.light;
  const vars = useMemo(() => themeVars(palette, data), [palette, data]);
  const [search, setSearch] = useState(false);
  const [cart, setCart] = useState(false);
  const lines = useCart();
  const { pathname } = useLocation();
  useScrollTop();
  useEffect(() => {
    document.body.style.background = palette.background;
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", palette.background);
  }, [palette.background]);
  useEffect(() => {
    track("visit");
  }, []);
  const count = lines.reduce((a, l) => a + l.qty, 0);
  const motion = data.config.theme.motion ?? "subtle";
  // Roadmap 77: "Added to cart" confirmation with a way to the cart; sections fade in as they scroll into view.
  const [added, setAdded] = useState<string | null>(null);
  useEffect(() => {
    let timer = 0;
    const on = (e: Event) => {
      setAdded((e as CustomEvent<{ name: string }>).detail.name);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setAdded(null), 2600);
    };
    window.addEventListener("sshop:added", on);
    return () => { window.removeEventListener("sshop:added", on); window.clearTimeout(timer); };
  }, []);
  useEffect(() => {
    if (motion === "off" || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver((entries) => entries.forEach((en) => {
      if (en.isIntersecting) { en.target.classList.add("in"); io.unobserve(en.target); }
    }), { rootMargin: "0px 0px -8% 0px", threshold: 0.06 });
    const t = window.setTimeout(() => {
      document.querySelectorAll("#main section:not(.hero)").forEach((el) => { el.classList.add("reveal"); io.observe(el); });
    }, 30);
    return () => { window.clearTimeout(t); io.disconnect(); };
  }, [pathname, motion]);
  const floatBump = useBump(count);
  return (
    <div className="site" style={vars} data-style={data.config.theme.style} data-scale={data.config.theme.scale} data-motion={motion}>
      <a className="skip" href="#main">Skip to content</a>
      {runtime.preview && <div className="preview-bar">Preview — this is your unpublished draft. Visitors still see the published website.</div>}
      <Header data={data} onSearch={() => setSearch(true)} onCart={() => setCart(true)} toggleMode={toggleMode} mode={mode} />
      <CampaignBanner data={data} pathname={pathname} />
      <main id="main" key={pathname} className="page-in">
        <Routes>
          <Route path="/" element={<Landing data={data} />} />
          <Route path="/home" element={<HomePage data={data} />} />
          <Route path="/products" element={<ProductsPage data={data} />} />
          <Route path="/products/:slug" element={<ProductPage data={data} />} />
          <Route path="/categories" element={<CategoriesPage data={data} />} />
          <Route path="/categories/:id" element={<CategoryPage data={data} />} />
          <Route path="/services" element={<ServicesPage data={data} />} />
          <Route path="/about" element={<AboutPage data={data} />} />
          <Route path="/contact" element={<ContactPage data={data} />} />
          <Route path="/testimonials" element={<TestimonialsPage data={data} />} />
          <Route path="/order" element={<OrderPage data={data} />} />
          <Route path="/orders" element={<OrderPage data={data} />} />
          <Route path="/cart" element={<OrderPage data={data} />} />
          <Route path="/track/:token" element={<TrackPage data={data} />} />
          <Route path="*" element={<NotFound data={data} />} />
        </Routes>
      </main>
      <Footer data={data} />
      {count > 0 && data.ordering.enabled && pathname !== "/order" && !cart && (
        <button key={floatBump} className={`btn float-cart ${floatBump ? "bump" : ""}`} onClick={() => setCart(true)} aria-label={`Cart, ${count} items`}><ShoppingBag /> {count}</button>
      )}
      {added && !cart && (
        <div className="added-toast" role="status" aria-live="polite">
          <span>Added to cart · {added}</span>
          {data.ordering.enabled && <button type="button" onClick={() => { setAdded(null); setCart(true); }}>View cart</button>}
        </div>
      )}
      {search && <SearchPanel data={data} onClose={() => setSearch(false)} />}
      {cart && <CartDrawer data={data} onClose={() => setCart(false)} />}
      <Consent data={data} />
    </div>
  );
}

/** Roadmap 80: the page visitors see first (Products by default); every page stays reachable from the menu. */
function Landing({ data }: { data: SiteData }) {
  switch (data.config.landing ?? "products") {
    case "products":
      return <ProductsPage data={data} />;
    case "categories":
      return <CategoriesPage data={data} />;
    case "services":
      return <ServicesPage data={data} />;
    default:
      return <HomePage data={data} />;
  }
}

const client = new QueryClient({ defaultOptions: { queries: { refetchOnWindowFocus: false, retry: 1 } } });

export default function SiteApp({ data, base }: { data: SiteData; base: string }) {
  return (
    <QueryClientProvider client={client}>
      <BrowserRouter basename={base || "/"}>
        <Shell data={data} />
      </BrowserRouter>
    </QueryClientProvider>
  );
}

export function Unavailable({ data }: { data: Pick<SiteData, "business"> }) {
  useEffect(() => {
    document.title = `${data.business.name} — temporarily unavailable`;
  }, [data.business.name]);
  return (
    <div className="center-page">
      <div>
        {data.business.logo_url && <img src={data.business.logo_url} alt="" />}
        <h1 style={{ margin: "0 0 8px", fontSize: "1.6rem" }}>{data.business.name}</h1>
        <p style={{ margin: 0, opacity: 0.75 }}>This website is temporarily unavailable. Please check back soon.</p>
      </div>
    </div>
  );
}
