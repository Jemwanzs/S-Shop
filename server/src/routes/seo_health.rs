//! SEO health for one business (roadmap 160): what it can act on, worked out from its own live website. Every number is counted
//! from its data. There are no scores, rankings or traffic estimates: search-engine figures appear only in the search engine's
//! own tools, which the business connects itself.

use std::collections::{HashMap, HashSet};

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::articles::ArticleRow;
use super::site::{self, Resolved};
use super::site_seo::currency_code;
use crate::auth::Ctx;
use crate::error::AppResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/website/seo/health", get(health))
}

/// One thing to look at. `tab` says where to fix it.
fn issue(level: &str, code: &str, message: &str, count: usize, tab: &str, items: Vec<String>) -> Value {
    json!({ "level": level, "code": code, "message": message, "count": count, "tab": tab, "items": items.into_iter().take(20).collect::<Vec<_>>() })
}

/// The website paths an article links to (`](/path)`), each once.
pub fn internal_links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("](/") {
        let after = &rest[i + 2..];
        let Some(end) = after.find(')') else { break };
        let path = after[..end].split(['?', '#']).next().unwrap_or_default().trim_end_matches('/').to_string();
        if !path.is_empty() && !out.contains(&path) {
            out.push(path);
        }
        rest = &after[end..];
    }
    out
}

async fn health(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("website.seo")?;
    let slug: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let s = match site::resolve(&state, &HeaderMap::new(), Some(&slug), false).await? {
        Resolved::Live(s) => *s,
        Resolved::Unavailable { .. } => return Ok(Json(json!({ "live": false }))),
    };
    let c = &s.config;
    let root = site::site_root(&state, &s);
    let brand = if c.brand.name.trim().is_empty() { s.name.clone() } else { c.brand.name.clone() };
    let items = site::published(&state, &s).await?;
    let (articles, live_articles) = super::articles::live_articles(&state, s.tenant, 2000, 0).await?;
    let cats = site::public_cats(&state, &s).await?;
    let mut issues = Vec::new();

    if c.seo.noindex {
        issues.push(issue("high", "noindex", "Search engines are asked to stay away from this whole website.", 1, "indexing", vec![]));
    }
    let domain: Option<(String, String, bool)> = sqlx::query_as("SELECT domain, status, is_primary FROM website_domains WHERE tenant_id = $1").bind(s.tenant).fetch_optional(&state.db).await?;
    if let Some((d, status, primary)) = &domain {
        if status != "active" {
            issues.push(issue("medium", "domain_pending", "Your domain is not active yet, so search engines use your S'Shop address.", 1, "indexing", vec![d.clone()]));
        } else if !primary {
            issues.push(issue("medium", "domain_not_main", "Your domain shows the same website but is not its main address, so search engines are told to use the S'Shop address.", 1, "indexing", vec![d.clone()]));
        }
    }
    if c.seo.title.trim().is_empty() {
        issues.push(issue("high", "no_title", "The website has no search title of its own.", 1, "general", vec![]));
    }
    if c.seo.description.trim().is_empty() {
        issues.push(issue("high", "no_description", "The website has no description for search results.", 1, "general", vec![]));
    }
    if c.seo.share_image.is_none() && c.hero.image.is_none() {
        issues.push(issue("low", "no_share_image", "No picture for when your link is shared.", 1, "general", vec![]));
    }
    if c.seo.primary_category.trim().is_empty() {
        issues.push(issue("low", "no_category", "Say what your business sells or does.", 1, "general", vec![]));
    }
    let no_photo: Vec<String> = items.iter().filter(|p| p.photos.is_empty()).map(|p| p.name.clone()).collect();
    if !no_photo.is_empty() {
        issues.push(issue("medium", "products_no_photo", "Products without a photo.", no_photo.len(), "products", no_photo));
    }
    let no_text: Vec<String> = items.iter().filter(|p| p.seo_description.trim().is_empty() && p.description.trim().is_empty()).map(|p| p.name.clone()).collect();
    if !no_text.is_empty() {
        issues.push(issue("medium", "products_no_description", "Products with nothing to show in search results: no description.", no_text.len(), "products", no_text));
    }
    // Two pages competing under the same title.
    let mut titles: HashMap<String, Vec<String>> = HashMap::new();
    for p in &items {
        let t = if p.seo_title.trim().is_empty() { format!("{} | {brand}", p.name) } else { p.seo_title.clone() };
        titles.entry(t.to_lowercase()).or_default().push(p.name.clone());
    }
    for a in &articles {
        let t = if a.seo_title.trim().is_empty() { format!("{} | {brand}", a.title) } else { a.seo_title.clone() };
        titles.entry(t.to_lowercase()).or_default().push(a.title.clone());
    }
    let dup: Vec<String> = titles.values().filter(|v| v.len() > 1).flatten().cloned().collect();
    if !dup.is_empty() {
        issues.push(issue("medium", "duplicate_titles", "Pages with the same search title.", dup.len(), "products", dup));
    }
    // Links inside articles that lead nowhere.
    let mut known: HashSet<String> = ["/products", "/categories", "/about", "/contact", "/services", "/articles", "/testimonials"].into_iter().map(str::to_string).collect();
    known.extend(items.iter().map(|p| format!("/products/{}", p.slug)));
    known.extend(cats.iter().map(|k| format!("/categories/{}", k.id)));
    known.extend(articles.iter().map(|a: &ArticleRow| format!("/articles/{}", a.slug)));
    let broken: Vec<String> = articles
        .iter()
        .flat_map(|a| internal_links(&a.body).into_iter().filter(|l| !known.contains(l)).map(|l| format!("{}: {l}", a.title)).collect::<Vec<_>>())
        .collect();
    if !broken.is_empty() {
        issues.push(issue("high", "broken_links", "Links in your articles that lead nowhere (a product may have been hidden or renamed).", broken.len(), "articles", broken));
    }
    let article_no_alt: Vec<String> = articles.iter().filter(|a| a.cover_media.is_some() && a.cover_alt.trim().is_empty()).map(|a| a.title.clone()).collect();
    if !article_no_alt.is_empty() {
        issues.push(issue("low", "article_no_alt", "Article pictures with no description for people who cannot see them.", article_no_alt.len(), "articles", article_no_alt));
    }
    // Structured data: what we leave out, and why.
    if currency_code(&s.currency).is_none() && items.iter().any(|p| p.price.is_some()) {
        issues.push(issue("medium", "currency_unknown", "Your currency is not an international code, so search engines are not told your prices. Use a code such as USD in Business profile.", 1, "general", vec![s.currency.clone()]));
    }
    let local = &c.seo.local;
    if local.enabled && local.phone.trim().is_empty() && !(c.contact.show_phone && !c.contact.phone.trim().is_empty()) {
        issues.push(issue("low", "local_no_phone", "Customers can visit, but no public phone is shown for search engines.", 1, "local", vec![]));
    }

    let now = Utc::now();
    let recent_articles = articles.iter().filter(|a| a.published_at.is_some_and(|d: DateTime<Utc>| d > now - chrono::Duration::days(30))).count();
    let counts: Vec<(String, i64)> = sqlx::query_as("SELECT status, COUNT(*) FROM website_articles WHERE tenant_id = $1 GROUP BY status").bind(s.tenant).fetch_all(&state.db).await?;
    let visits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM website_events WHERE tenant_id = $1 AND kind = 'visit' AND at >= now() - interval '30 days'").bind(s.tenant).fetch_one(&state.db).await?;
    let pages = 1 + cats.len() + items.len() + usize::from(live_articles > 0) + articles.len();
    Ok(Json(json!({
        "live": true,
        "public_url": root,
        "indexing": !c.seo.noindex,
        "sitemap_url": format!("{root}/sitemap.xml"),
        "pages": if c.seo.noindex { 0 } else { pages },
        "products": items.len(),
        "categories": cats.len(),
        "articles": { "published_30_days": recent_articles, "by_status": counts.into_iter().collect::<HashMap<_, _>>() },
        "visits_30_days": visits,
        "issues": issues,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_links_are_found_once_without_their_query() {
        let body = "See [a](/products/a) and [a again](/products/a?x=1) or [b](/categories/b/) but not [c](https://x.example/d) nor [unfinished](/oops";
        assert_eq!(internal_links(body), vec!["/products/a", "/categories/b"]);
    }
}
