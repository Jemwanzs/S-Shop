/** A business's articles on its website, and sharing (roadmap 158). The article's HTML comes from the server, which escapes
 * everything and allows only headings, paragraphs, lists, bold and safe links (routes/articles.rs). */
import { useEffect, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Copy, Share2 } from "lucide-react";
import type { Product, SiteData } from "./types";
import { call } from "./lib";
import { ProductGrid } from "./parts";
import { NotFound } from "./pages";

interface ArticleCard { slug: string; title: string; description: string; cover: string | null; cover_alt: string; category: string; author: string; published_at: string | null; updated_at: string }
interface Article extends ArticleCard { html: string; ai_assisted: boolean; reviewed_by: string }

const day = (at: string | null | undefined) => (at ? new Date(at).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" }) : "");

function useTitle(title: string, data: SiteData) {
  const brand = data.config.brand.name || data.business.name;
  useEffect(() => {
    document.title = `${title} — ${brand}`;
  }, [title, brand]);
}

/** Opens each network's own share page: nothing is posted from here, and only the visitor can post. */
export function ShareBar({ title }: { title: string }) {
  const [copied, setCopied] = useState(false);
  const url = location.origin + location.pathname;
  const text = encodeURIComponent(title);
  const link = encodeURIComponent(url);
  const canShare = typeof navigator.share === "function";
  return (
    <div className="share" aria-label="Share">
      {canShare && <button type="button" className="btn ghost" onClick={() => void navigator.share({ title, url }).catch(() => undefined)}><Share2 aria-hidden /> Share</button>}
      <button type="button" className="btn ghost" onClick={() => void navigator.clipboard?.writeText(url).then(() => setCopied(true))}><Copy aria-hidden /> {copied ? "Link copied" : "Copy link"}</button>
      <a className="btn ghost" href={`https://wa.me/?text=${text}%20${link}`} target="_blank" rel="noreferrer">WhatsApp</a>
      <a className="btn ghost" href={`https://www.facebook.com/sharer/sharer.php?u=${link}`} target="_blank" rel="noreferrer">Facebook</a>
      <a className="btn ghost" href={`https://www.linkedin.com/sharing/share-offsite/?url=${link}`} target="_blank" rel="noreferrer">LinkedIn</a>
    </div>
  );
}

export function ArticlesPage({ data }: { data: SiteData }) {
  useTitle("Articles", data);
  const [params, setParams] = useSearchParams();
  const page = Math.max(1, Number(params.get("page")) || 1);
  const per = 12;
  const { data: list, isLoading } = useQuery({ queryKey: ["site-articles", page], queryFn: () => call<{ items: ArticleCard[]; total: number }>("/site/articles", { query: { limit: per, offset: (page - 1) * per } }) });
  const last = Math.max(1, Math.ceil((list?.total ?? 0) / per));
  return (
    <section className="section" style={{ paddingTop: 28 }}>
      <div className="wrap stack">
        <h1 style={{ fontSize: "clamp(1.7rem, 5.5vw, 2.8rem)" }}>Articles</h1>
        {isLoading ? <div className="skel" style={{ height: 240 }} /> : (
          <ul className="article-list">
            {list?.items.map((a) => (
              <li key={a.slug}>
                <Link to={`/articles/${a.slug}`} className="article-card">
                  {a.cover && <img src={`${a.cover}?size=thumb`} alt={a.cover_alt} loading="lazy" decoding="async" />}
                  <h2>{a.title}</h2>
                  <p className="muted">{a.description}</p>
                  <small className="muted">{day(a.published_at)}</small>
                </Link>
              </li>
            ))}
          </ul>
        )}
        {last > 1 && (
          <nav className="pager" aria-label="Pages">
            <button type="button" className="btn ghost" disabled={page <= 1} onClick={() => setParams(page > 2 ? { page: String(page - 1) } : {})}>Previous</button>
            <span className="muted">{page} / {last}</span>
            <button type="button" className="btn ghost" disabled={page >= last} onClick={() => setParams({ page: String(page + 1) })}>Next</button>
          </nav>
        )}
      </div>
    </section>
  );
}

export function ArticlePage({ data }: { data: SiteData }) {
  const { slug = "" } = useParams();
  const { data: res, isLoading, error } = useQuery({
    queryKey: ["site-article", slug],
    queryFn: () => call<{ article: Article; products: Product[]; related: { slug: string; title: string; description: string }[] }>(`/site/articles/${encodeURIComponent(slug)}`),
    retry: false,
  });
  const a = res?.article;
  useTitle(a?.title ?? "Articles", data);
  useEffect(() => {
    if (a) void call(`/site/articles/${encodeURIComponent(a.slug)}/view`, { method: "POST" }).catch(() => undefined);
  }, [a?.slug]); // eslint-disable-line react-hooks/exhaustive-deps
  if (isLoading) return <div className="wrap section"><div className="skel" style={{ height: 320 }} /></div>;
  if (error || !a) return <NotFound data={data} what="This article is not available." />;
  return (
    <section className="section" style={{ paddingTop: 20 }}>
      <article className="wrap article">
        <nav className="crumbs" aria-label="Breadcrumb"><Link to="/articles">Articles</Link></nav>
        <h1 style={{ fontSize: "clamp(1.7rem, 5.5vw, 2.8rem)" }}>{a.title}</h1>
        <p className="muted">{[a.author, day(a.published_at)].filter(Boolean).join(" · ")}</p>
        {a.cover && <img className="article-cover" src={a.cover} alt={a.cover_alt} fetchPriority="high" />}
        <div className="prose article-body" dangerouslySetInnerHTML={{ __html: a.html }} />
        {a.ai_assisted && <p className="muted"><small>Drafted with AI assistance{a.reviewed_by ? ` and reviewed by ${a.reviewed_by}` : " and reviewed by our team"}.</small></p>}
        <ShareBar title={a.title} />
      </article>
      {res!.products.length > 0 && (
        <div className="wrap" style={{ marginTop: "var(--section)" }}>
          <h2>Mentioned in this article</h2>
          <ProductGrid data={data} items={res!.products} />
        </div>
      )}
      {res!.related.length > 0 && (
        <div className="wrap stack" style={{ marginTop: "var(--section)" }}>
          <h2>More to read</h2>
          <ul className="article-list">
            {res!.related.map((r) => <li key={r.slug}><Link to={`/articles/${r.slug}`} className="article-card"><h3>{r.title}</h3><p className="muted">{r.description}</p></Link></li>)}
          </ul>
        </div>
      )}
    </section>
  );
}
