//! Public website API (roadmap 53–54, 57): what a business's own website shows and how it takes orders.
//!
//! The business is decided by the server only: a verified, active custom domain (the `Host` of the request) maps to
//! its business; on the S'Shop host a website is addressed as `/s/{slug}`. A custom domain never serves another
//! business, whatever the request asks for. Nothing unpublished leaves the server (draft only for preview by the
//! business's own website editors), and hidden prices are removed here — not merely hidden in the page.
//!
//! Orders go into the existing orders engine (`orders::create_order`, source `website`) with the same customers,
//! verification, branch, stock, payments and loyalty as the ordering link.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as DbJson;
use uuid::Uuid;

use super::orders::{announce_new, create_order, OrderLine};
use super::portal::{self, IdentifyBody, SessionBody};
use crate::auth::PortalCustomer;
use crate::error::{bad, refused, AppError, AppResult};
use crate::state::AppState;
use crate::website::{ProductCfg, SiteConfig};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/site", get(site))
        .route("/site/products", get(products))
        .route("/site/products/{slug}", get(product))
        .route("/site/media/{id}", get(media))
        .route("/site/identify", post(identify))
        .route("/site/session", post(session))
        .route("/site/orders", post(place_order))
        .route("/site/events", post(event))
        .route("/site/whoami", get(whoami))
}

/// Lets the domain check confirm that a custom domain really reaches this server (routing and certificate).
async fn whoami(headers: HeaderMap) -> Json<Value> {
    Json(json!({ "service": "sshop", "host": request_host(&headers) }))
}

// ───────────────────────────── Which business, which version ─────────────────────────────

pub fn request_host(headers: &HeaderMap) -> String {
    let h = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default().trim().to_lowercase();
    h.split(':').next().unwrap_or_default().trim_end_matches('.').to_string()
}

/// The business whose verified, active custom domain this host is.
pub async fn domain_tenant(state: &AppState, host: &str) -> AppResult<Option<Uuid>> {
    Ok(domain_match(state, host).await?.map(|m| m.0))
}

/// Roadmap 74: the active domain this host belongs to — the domain itself, or its `www.` / bare twin (redirected to
/// the domain as registered, provided the twin also reaches this server).
pub async fn domain_match(state: &AppState, host: &str) -> AppResult<Option<(Uuid, String)>> {
    if host.is_empty() {
        return Ok(None);
    }
    Ok(sqlx::query_as(
        "SELECT tenant_id, domain FROM website_domains WHERE status = 'active' AND (domain = $1 OR domain = 'www.' || $1 OR 'www.' || domain = $1)
         ORDER BY domain = $1 DESC LIMIT 1",
    )
    .bind(host)
    .fetch_optional(&state.db)
    .await?)
}

pub struct Site {
    pub tenant: Uuid,
    pub slug: String,
    pub name: String,
    pub currency: String,
    pub has_logo: bool,
    pub config: SiteConfig,
    pub preview: bool,
    pub domain: Option<String>,
    pub version: i32,
    pub published_at: Option<DateTime<Utc>>,
    /// The business takes website orders (Orders module, business active).
    pub ordering: bool,
}

pub enum Resolved {
    Live(Box<Site>),
    /// Website switched off, suspended or not published yet: the business name only, for a "temporarily unavailable" page.
    Unavailable { name: String, slug: String, has_logo: bool },
}

#[derive(sqlx::FromRow)]
struct SiteDb {
    id: Uuid,
    slug: String,
    name: String,
    currency: String,
    has_logo: bool,
    tenant_status: String,
    status: Option<String>,
    billing_suspended: Option<bool>,
    draft: Option<DbJson<SiteConfig>>,
    published: Option<DbJson<SiteConfig>>,
    version: Option<i32>,
    published_at: Option<DateTime<Utc>>,
    domain: Option<String>,
}

/// Does this request carry a staff session of the same business allowed to preview its website?
async fn preview_allowed(state: &AppState, headers: &HeaderMap, tenant: Uuid) -> bool {
    let Some(token) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) else {
        return false;
    };
    let Ok(claims) = crate::auth::read_token(&state.cfg.jwt_secret, token.trim(), "staff") else { return false };
    if claims.tid != tenant {
        return false;
    }
    let perms: Option<Vec<String>> = sqlx::query_scalar(
        "SELECT effective_permissions(r.permissions, u.extra_permissions) FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1 AND u.tenant_id = $2 AND u.is_active",
    )
    .bind(claims.sub)
    .bind(claims.home.unwrap_or(claims.tid))
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    if claims.home.is_some() {
        // The platform owner inside the business: only while the support session is live (roadmap 71).
        return sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM support_sessions WHERE id = $1 AND user_id = $2 AND tenant_id = $3 AND status = 'active' AND expires_at > now())",
        )
        .bind(claims.sid)
        .bind(claims.sub)
        .bind(tenant)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);
    }
    perms.is_some_and(|p| p.iter().any(|x| x == "*" || x.starts_with("website.")))
}

pub async fn resolve(state: &AppState, headers: &HeaderMap, slug: Option<&str>, want_preview: bool) -> AppResult<Resolved> {
    let host = request_host(headers);
    // A verified custom domain decides the business on its own; only the S'Shop host addresses websites by slug.
    let by_domain = domain_tenant(state, &host).await?;
    let row: Option<SiteDb> = sqlx::query_as(
        "SELECT t.id, t.slug, t.name, t.currency, t.logo IS NOT NULL AS has_logo, t.status AS tenant_status, w.status, w.billing_suspended,
                w.draft, w.published, w.version, w.published_at, d.domain
         FROM tenants t LEFT JOIN websites w ON w.tenant_id = t.id
         LEFT JOIN website_domains d ON d.tenant_id = t.id AND d.status = 'active' AND d.is_primary
         WHERE ($1::uuid IS NOT NULL AND t.id = $1) OR ($1::uuid IS NULL AND t.slug = $2)",
    )
    .bind(by_domain)
    .bind(slug.unwrap_or_default())
    .fetch_optional(&state.db)
    .await?;
    let r = row.ok_or(AppError::NotFound("Website"))?;
    let service_on = r.status.as_deref() == Some("active") && r.tenant_status == "active";
    let preview = want_preview && service_on && preview_allowed(state, headers, r.id).await;
    let config = if preview { r.draft.map(|d| d.0) } else { r.published.map(|p| p.0) };
    let live = service_on && !r.billing_suspended.unwrap_or(false);
    match config {
        Some(config) if live || preview => {
            let access = {
                let mut conn = state.db.acquire().await?;
                crate::billing::access(&mut conn, r.id).await?
            };
            let ordering = !access.suspended() && access.modules().is_none_or(|m| m.iter().any(|x| x == "orders"));
            Ok(Resolved::Live(Box::new(Site {
                tenant: r.id,
                slug: r.slug,
                name: r.name,
                currency: r.currency,
                has_logo: r.has_logo,
                config,
                preview,
                domain: r.domain,
                version: r.version.unwrap_or(0),
                published_at: r.published_at,
                ordering,
            })))
        }
        _ => Ok(Resolved::Unavailable { name: r.name, slug: r.slug, has_logo: r.has_logo }),
    }
}

async fn live(state: &AppState, headers: &HeaderMap, slug: Option<&str>, preview: bool) -> AppResult<Site> {
    match resolve(state, headers, slug, preview).await? {
        Resolved::Live(s) => Ok(*s),
        Resolved::Unavailable { .. } => Err(refused("Website unavailable", "This website is temporarily unavailable")),
    }
}

#[derive(Deserialize, Default)]
struct SiteQuery {
    slug: Option<String>,
    #[serde(default)]
    preview: bool,
}

pub fn media_url(id: Uuid) -> String {
    format!("/api/site/media/{id}")
}

/// The configuration as the public may see it: unpublished testimonials (and every sample), inactive services and
/// promotions and the per-product settings stay on the server.
fn public_config(c: &SiteConfig) -> Value {
    let mut c = c.clone();
    c.testimonials.items.retain(|t| t.published && !t.sample);
    c.services.items.retain(|s| s.active);
    c.promotions.retain(|p| p.active);
    c.products.items.clear();
    serde_json::to_value(c).unwrap_or_default()
}

async fn site(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<SiteQuery>) -> AppResult<Json<Value>> {
    match resolve(&state, &headers, q.slug.as_deref(), q.preview).await? {
        Resolved::Unavailable { name, slug, has_logo } => Ok(Json(json!({
            "available": false, "business": { "name": name, "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")) },
        }))),
        Resolved::Live(s) => {
            let mut data = site_data(&state, &s).await?;
            data["preview"] = json!(s.preview);
            Ok(Json(data))
        }
    }
}

// ───────────────────────────── Products ─────────────────────────────

#[derive(sqlx::FromRow)]
struct ProductDb {
    id: Uuid,
    name: String,
    nickname: String,
    code: String,
    description: String,
    category_id: Option<Uuid>,
    category_name: Option<String>,
    price: Decimal,
    available: i32,
    available_for_orders: bool,
    created_at: DateTime<Utc>,
    sold: i64,
    photos: Vec<Uuid>,
}

#[derive(Serialize, Clone)]
pub struct PubProduct {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    /// None when the price is not shown for this product.
    pub price: Option<Decimal>,
    pub badge: String,
    /// Stock shown only when the website shows availability.
    pub in_stock: Option<bool>,
    /// add_to_cart | enquire | contact | whatsapp
    pub action: String,
    pub cta_label: String,
    pub photo: Option<String>,
    pub photo_thumb: Option<String>,
    pub photos: Vec<String>,
    pub featured: bool,
    #[serde(skip)]
    pub sort: i32,
    #[serde(skip)]
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub sold: i64,
    #[serde(skip)]
    pub search: String,
    /// Real availability, for the "in stock" filter (shown to visitors only when the website shows availability).
    #[serde(skip)]
    pub stock_ok: bool,
    /// Roadmap 80: the website's "was" price, only with a visible, lower current price.
    pub compare_at: Option<Decimal>,
    pub seo_title: String,
    pub seo_description: String,
}

pub fn product_slug(name: &str, id: Uuid) -> String {
    let base = crate::util::slugify(name);
    let short = &id.simple().to_string()[..8];
    if base.is_empty() { short.to_string() } else { format!("{base}-{short}") }
}

/// Every product the website publishes, with its marketing presentation and the price the public may see.
pub async fn published(state: &AppState, s: &Site) -> AppResult<Vec<PubProduct>> {
    let t = portal::tenant_by_id(state, s.tenant).await?;
    let branch = portal::portal_branch(state, &t).await.ok();
    let rows: Vec<ProductDb> = sqlx::query_as(
        "SELECT p.id, p.name, p.nickname, p.code, p.description, p.category_id, c.name AS category_name, p.marked_price AS price,
                GREATEST(COALESCE(sl.on_hand, 0) - COALESCE(sl.reserved, 0), 0) AS available, p.available_for_orders, p.created_at,
                COALESCE((SELECT SUM(si.quantity - si.returned_qty) FROM sale_items si JOIN sales sa ON sa.id = si.sale_id
                          WHERE si.product_id = p.id AND sa.created_at > now() - interval '90 days'), 0)::bigint AS sold,
                ARRAY(SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order) AS photos
         FROM products p
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $2
         WHERE p.tenant_id = $1 AND p.is_active",
    )
    .bind(s.tenant)
    .bind(branch)
    .fetch_all(&state.db)
    .await?;
    let cfg = &s.config.products;
    let entries: HashMap<Uuid, &ProductCfg> = cfg.items.iter().map(|p| (p.product_id, p)).collect();
    let show_prices = t.settings.orders.show_prices;
    let show_out = t.settings.orders.show_out_of_stock;
    let max = cfg.max_photos.clamp(1, 5) as usize;
    let mut out = Vec::new();
    for r in rows {
        let e = entries.get(&r.id).copied();
        let publish = e.map_or(cfg.auto_publish_new, |e| e.published);
        if !publish {
            continue;
        }
        let price_visible = match e.map(|e| e.price.as_str()).unwrap_or("inherit") {
            "show" => true,
            "hide" => false,
            _ => show_prices,
        };
        let in_stock = r.available > 0;
        let orderable = s.ordering && r.available_for_orders && (in_stock || show_out);
        let hidden_action = e.map(|e| e.hidden_action.as_str()).filter(|a| !a.is_empty()).unwrap_or(&cfg.hidden_action).to_string();
        let action = if price_visible {
            if orderable { "add_to_cart".to_string() } else { "enquire".to_string() }
        } else if hidden_action == "order" && orderable {
            "add_to_cart".to_string()
        } else if hidden_action == "order" {
            "enquire".to_string()
        } else {
            hidden_action
        };
        let photos: Vec<String> = match e {
            Some(e) if !e.use_product_photos => e.photos.iter().take(max).map(|m| media_url(*m)).collect(),
            _ => r
                .photos
                .iter()
                .filter(|p| !e.is_some_and(|e| e.hidden_photos.contains(p)))
                .take(max)
                .map(|p| format!("/api/photos/{p}"))
                .collect(),
        };
        let thumb = match e {
            Some(e) if !e.use_product_photos => e.photos.first().map(|m| format!("{}?size=thumb", media_url(*m))),
            _ => photos.first().map(|p| format!("{p}?size=thumb")),
        };
        let name = e.map(|e| e.marketing_name.trim()).filter(|n| !n.is_empty()).unwrap_or(&r.name).to_string();
        let description = e.map(|e| e.marketing_description.trim()).filter(|n| !n.is_empty()).unwrap_or(&r.description).to_string();
        let category_id = e.and_then(|e| e.category_id).or(r.category_id);
        let category_name = if category_id == r.category_id { r.category_name.clone() } else { None };
        out.push(PubProduct {
            id: r.id,
            slug: product_slug(&name, r.id),
            search: format!("{} {} {} {} {}", name, r.name, r.nickname, r.code, r.category_name.clone().unwrap_or_default()).to_lowercase(),
            name,
            description,
            category_id,
            category_name,
            price: price_visible.then_some(r.price),
            compare_at: e.and_then(|e| e.compare_at).filter(|c| price_visible && *c > r.price),
            stock_ok: in_stock,
            badge: if cfg.grid.show_badges { e.map(|e| e.badge.clone()).unwrap_or_default() } else { String::new() },
            in_stock: cfg.grid.show_availability.then_some(in_stock),
            action,
            cta_label: e.map(|e| e.cta_label.clone()).unwrap_or_default(),
            photo: photos.first().cloned(),
            photo_thumb: thumb,
            photos,
            featured: e.is_some_and(|e| e.featured),
            sort: e.map_or(i32::MAX, |e| e.sort),
            created_at: r.created_at,
            sold: r.sold,
            seo_title: e.map(|e| e.seo_title.clone()).unwrap_or_default(),
            seo_description: e.map(|e| e.seo_description.clone()).unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| a.sort.cmp(&b.sort).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

async fn categories(state: &AppState, s: &Site) -> AppResult<Vec<Value>> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM categories WHERE tenant_id = $1 AND is_active ORDER BY name")
        .bind(s.tenant)
        .fetch_all(&state.db)
        .await?;
    let products = published(state, s).await?;
    let cfg: HashMap<Uuid, &crate::website::CategoryCfg> = s.config.categories.items.iter().map(|c| (c.category_id, c)).collect();
    let mut out: Vec<(i32, Value)> = Vec::new();
    for (id, name) in rows {
        let c = cfg.get(&id);
        if c.is_some_and(|c| !c.visible) {
            continue;
        }
        let in_cat: Vec<&PubProduct> = products.iter().filter(|p| p.category_id == Some(id)).collect();
        if in_cat.is_empty() {
            continue;
        }
        let image = c.and_then(|c| c.image).map(media_url).or_else(|| in_cat.iter().find_map(|p| p.photo_thumb.clone()));
        out.push((c.map_or(i32::MAX, |c| c.sort), json!({ "id": id, "name": name, "image": image, "count": in_cat.len() })));
    }
    out.sort_by_key(|x| x.0);
    Ok(out.into_iter().map(|x| x.1).collect())
}

#[derive(Deserialize, Default)]
struct ProductsQuery {
    slug: Option<String>,
    #[serde(default)]
    preview: bool,
    q: Option<String>,
    category: Option<Uuid>,
    /// featured | new_arrivals | popular
    section: Option<String>,
    /// Roadmap 80: recommended | newest | name_asc | name_desc | price_asc | price_desc
    sort: Option<String>,
    /// "in": only products in stock.
    stock: Option<String>,
    /// Paging: products to skip (the page size is `limit`).
    offset: Option<usize>,
    limit: Option<usize>,
    /// Search suggestions: a few results with thumbnails.
    #[serde(default)]
    suggest: bool,
}

async fn products(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<ProductsQuery>) -> AppResult<Json<Value>> {
    let s = live(&state, &headers, q.slug.as_deref(), q.preview).await?;
    let mut items = published(&state, &s).await?;
    if let Some(c) = q.category {
        items.retain(|p| p.category_id == Some(c));
    }
    if let Some(term) = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let words: Vec<String> = term.to_lowercase().split_whitespace().map(str::to_string).collect();
        items.retain(|p| words.iter().all(|w| p.search.contains(w)));
    }
    match q.section.as_deref() {
        Some("featured") => {
            let chosen = s.config.sections.iter().find(|x| x.key == "featured").map(|x| x.product_ids.clone()).unwrap_or_default();
            if chosen.is_empty() {
                items.retain(|p| p.featured);
            } else {
                items.retain(|p| chosen.contains(&p.id));
                items.sort_by_key(|p| chosen.iter().position(|c| *c == p.id));
            }
        }
        Some(key @ ("new_arrivals" | "popular")) => {
            let chosen = s.config.sections.iter().find(|x| x.key == key).map(|x| x.product_ids.clone()).unwrap_or_default();
            if chosen.is_empty() {
                if key == "new_arrivals" {
                    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                } else {
                    items.retain(|p| p.sold > 0);
                    items.sort_by(|a, b| b.sold.cmp(&a.sold));
                }
            } else {
                items.retain(|p| chosen.contains(&p.id));
                items.sort_by_key(|p| chosen.iter().position(|c| *c == p.id));
            }
        }
        _ => {}
    }
    if q.stock.as_deref() == Some("in") {
        items.retain(|p| p.stock_ok);
    }
    // Sorting by real data; prices only where visitors see them (hidden prices go last, never reveal their order).
    match q.sort.as_deref() {
        Some("newest") => items.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        Some("name_asc") => items.sort_by_key(|p| p.name.to_lowercase()),
        Some("name_desc") => items.sort_by_key(|p| std::cmp::Reverse(p.name.to_lowercase())),
        Some("price_asc") => items.sort_by(|a, b| match (a.price, b.price) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }),
        Some("price_desc") => items.sort_by(|a, b| match (a.price, b.price) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }),
        _ => {}
    }
    let total = items.len();
    let limit = if q.suggest { 6 } else { q.limit.unwrap_or(200).min(500) };
    let offset = q.offset.unwrap_or(0).min(total);
    let mut items: Vec<PubProduct> = items.into_iter().skip(offset).collect();
    items.truncate(limit);
    if q.suggest {
        let slim: Vec<Value> = items
            .iter()
            .map(|p| json!({ "id": p.id, "slug": p.slug, "name": p.name, "price": p.price, "photo_thumb": p.photo_thumb, "category_name": p.category_name }))
            .collect();
        return Ok(Json(json!({ "items": slim, "total": total })));
    }
    Ok(Json(json!({ "items": items, "total": total })))
}

async fn product(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Query(q): Query<SiteQuery>) -> AppResult<Json<Value>> {
    let s = live(&state, &headers, q.slug.as_deref(), q.preview).await?;
    let items = published(&state, &s).await?;
    let p = find_product(&items, &slug).ok_or(AppError::NotFound("Product"))?.clone();
    let related: Vec<&PubProduct> = items.iter().filter(|x| x.id != p.id && x.category_id.is_some() && x.category_id == p.category_id).take(8).collect();
    Ok(Json(json!({ "product": p, "related": related })))
}

/// A product by its URL slug ("name-1a2b3c4d": the short id decides, the name part may change).
pub fn find_product<'a>(items: &'a [PubProduct], slug: &str) -> Option<&'a PubProduct> {
    let short = slug.rsplit('-').next().unwrap_or(slug);
    items.iter().find(|p| p.slug == slug).or_else(|| items.iter().find(|p| short.len() == 8 && p.id.simple().to_string().starts_with(short)))
}

// ───────────────────────────── Media ─────────────────────────────

#[derive(Deserialize, Default)]
struct MediaQuery {
    size: Option<String>,
}

/// Website images: unguessable ids, never archived ones; cached by browsers and proxies (an id never changes content).
async fn media(State(state): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<MediaQuery>) -> AppResult<impl IntoResponse> {
    let thumb = q.size.as_deref() == Some("thumb");
    let row: Option<(Vec<u8>, String, Option<Vec<u8>>, Option<String>)> =
        sqlx::query_as("SELECT data, mime, thumb, thumb_mime FROM website_media WHERE id = $1 AND NOT archived")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (data, mime, t, tm) = row.ok_or(AppError::NotFound("Image"))?;
    let (body, ty) = match (thumb, t, tm) {
        (true, Some(t), Some(tm)) => (t, tm),
        _ => (data, mime),
    };
    Ok(([(header::CONTENT_TYPE, ty), (header::CACHE_CONTROL, "public, max-age=31536000, immutable".into())], body))
}

// ───────────────────────────── Ordering ─────────────────────────────

#[derive(Deserialize)]
struct WithSlug<T> {
    slug: Option<String>,
    #[serde(flatten)]
    body: T,
}

async fn ordering_tenant(state: &AppState, headers: &HeaderMap, slug: Option<&str>) -> AppResult<(Site, portal::Tenant)> {
    let s = live(state, headers, slug, false).await?;
    if !s.ordering {
        return Err(refused("Ordering unavailable", "This shop is not taking online orders at the moment"));
    }
    let t = portal::tenant_by_id(state, s.tenant).await?;
    Ok((s, t))
}

async fn identify(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<WithSlug<IdentifyBody>>) -> AppResult<Json<Value>> {
    let (_, t) = ordering_tenant(&state, &headers, b.slug.as_deref()).await?;
    portal::identify_for(&state, &headers, &t, b.body).await.map(Json)
}

async fn session(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<WithSlug<SessionBody>>) -> AppResult<Json<Value>> {
    let (_, t) = ordering_tenant(&state, &headers, b.slug.as_deref()).await?;
    portal::session_for(&state, &headers, &t, b.body).await.map(Json)
}

#[derive(Deserialize)]
struct OrderBody {
    slug: Option<String>,
    items: Vec<OrderLine>,
    #[serde(default)]
    delivery_location: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    visitor: String,
}

/// Website cart → the existing orders engine (source `website`). Only products the website publishes and that can be
/// ordered are accepted; a product whose price is hidden can be ordered only when the business allows it, and the
/// shop confirms the price before any payment.
async fn place_order(State(state): State<AppState>, headers: HeaderMap, c: PortalCustomer, Json(b): Json<OrderBody>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "portal_order", 20, std::time::Duration::from_secs(600))?;
    let (s, t) = ordering_tenant(&state, &headers, b.slug.as_deref()).await?;
    if c.tenant_id != s.tenant {
        return Err(AppError::Unauthorized);
    }
    if b.delivery_location.trim().is_empty() {
        return Err(bad("Tell us where to deliver"));
    }
    let items = published(&state, &s).await?;
    let mut hidden_price = false;
    for line in &b.items {
        let p = items.iter().find(|p| p.id == line.product_id).ok_or_else(|| bad("A product in your cart is no longer available"))?;
        if p.action != "add_to_cart" {
            return Err(refused("Ask us first", format!("{} cannot be ordered online — please contact us", p.name)));
        }
        hidden_price |= p.price.is_none();
    }
    let branch = portal::portal_branch(&state, &t).await?;
    let mut tx = state.db.begin().await?;
    let (id, order_no, track_token, total) =
        create_order(&mut tx, s.tenant, branch, c.customer_id, &b.items, &b.delivery_location, &b.notes, "website", None).await?;
    let (name, mobile): (String, String) = sqlx::query_as("SELECT first_name, mobile FROM customers WHERE id = $1").bind(c.customer_id).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO website_events (tenant_id, kind, visitor, order_id) VALUES ($1, 'order_complete', $2, $3)")
        .bind(s.tenant)
        .bind(b.visitor.chars().take(64).collect::<String>())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    announce_new(&state, s.tenant, branch, id, &order_no, &name, total).await;
    let site_url = match &s.domain {
        Some(d) => format!("https://{d}"),
        None => format!("{}/s/{}", state.cfg.public_url, s.slug),
    };
    if t.settings.orders.notify_customer_whatsapp {
        crate::notify::whatsapp(
            &state,
            s.tenant,
            mobile,
            format!("Hi {name}! 🎉 We've received your {} order {order_no}. We are now processing it.\nTrack it here: {site_url}/track/{track_token}", s.name),
        );
    }
    let show_total = t.settings.orders.show_prices && !hidden_price;
    Ok(Json(json!({
        "id": id, "order_no": order_no, "track_token": track_token,
        "total": show_total.then_some(total),
        "price_note": (!show_total).then_some("We will confirm the final price with you before any payment."),
    })))
}

// ───────────────────────────── Analytics events (roadmap 57) ─────────────────────────────

#[derive(Deserialize)]
struct EventBody {
    slug: Option<String>,
    kind: String,
    #[serde(default)]
    visitor: String,
    product_id: Option<Uuid>,
}

async fn event(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<EventBody>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "site_event", 600, std::time::Duration::from_secs(600))?;
    if !matches!(b.kind.as_str(), "visit" | "product_view" | "add_to_cart" | "order_start") {
        return Err(bad("Unknown event"));
    }
    let Resolved::Live(s) = resolve(&state, &headers, b.slug.as_deref(), false).await? else { return Ok(Json(json!({ "ok": false }))) };
    let product = match b.product_id {
        Some(p) => sqlx::query_scalar::<_, Uuid>("SELECT id FROM products WHERE id = $1 AND tenant_id = $2").bind(p).bind(s.tenant).fetch_optional(&state.db).await?,
        None => None,
    };
    sqlx::query("INSERT INTO website_events (tenant_id, kind, visitor, product_id) VALUES ($1, $2, $3, $4)")
        .bind(s.tenant)
        .bind(&b.kind)
        .bind(b.visitor.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(64).collect::<String>())
        .bind(product)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Pages, SEO, sitemap (roadmap 53) ─────────────────────────────

/// Same policy as the rest of S'Shop, except that pages may be framed by the same origin (the live preview in the
/// Website Management Centre).
const SITE_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' data: https://fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self'; media-src 'self' blob:; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'self'";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn clip(s: &str, n: usize) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= n { t } else { format!("{}…", t.chars().take(n - 1).collect::<String>()) }
}

/// The website app shell (`site.html` from the web build), read once.
fn shell(state: &AppState) -> Option<&'static str> {
    static SHELL: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SHELL.get_or_init(|| std::fs::read_to_string(format!("{}/site.html", state.cfg.web_dir)).ok()).as_deref()
}

struct PageMeta {
    title: String,
    description: String,
    image: Option<String>,
    canonical: String,
    noindex: bool,
}

fn render(state: &AppState, meta: &PageMeta, data: &Value, fonts: &[String], lang: &str) -> axum::response::Response {
    let Some(html) = shell(state) else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Website files missing").into_response();
    };
    let mut head = String::new();
    head.push_str(&format!("<meta name=\"description\" content=\"{}\" />\n", esc(&meta.description)));
    head.push_str(&format!("<link rel=\"canonical\" href=\"{}\" />\n", esc(&meta.canonical)));
    head.push_str(&format!("<meta property=\"og:title\" content=\"{}\" />\n", esc(&meta.title)));
    head.push_str(&format!("<meta property=\"og:description\" content=\"{}\" />\n", esc(&meta.description)));
    head.push_str(&format!("<meta property=\"og:url\" content=\"{}\" />\n<meta property=\"og:type\" content=\"website\" />\n", esc(&meta.canonical)));
    head.push_str("<meta name=\"twitter:card\" content=\"summary_large_image\" />\n");
    if let Some(img) = &meta.image {
        head.push_str(&format!("<meta property=\"og:image\" content=\"{}\" />\n", esc(img)));
    }
    if meta.noindex {
        head.push_str("<meta name=\"robots\" content=\"noindex\" />\n");
    }
    // The business's own icon (the website is its brand, not S'Shop's).
    if let Some(logo) = data.pointer("/business/logo_url").and_then(Value::as_str) {
        head.push_str(&format!("<link rel=\"icon\" href=\"{0}\" />\n<link rel=\"apple-touch-icon\" href=\"{0}\" />\n", esc(logo)));
    }
    let mut families: Vec<String> = fonts.iter().filter(|f| crate::website::FONTS.contains(&f.as_str())).cloned().collect();
    families.dedup();
    if !families.is_empty() {
        let q: Vec<String> = families.iter().map(|f| format!("family={}:wght@400;500;600;700", f.replace(' ', "+"))).collect();
        head.push_str(&format!(
            "<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin />\n<link href=\"https://fonts.googleapis.com/css2?{}&display=swap\" rel=\"stylesheet\" />\n",
            q.join("&")
        ));
    }
    // Published content for the first render (a data block, not a script: the page's JavaScript reads it).
    let data = data.to_string().replace('<', "\\u003c");
    head.push_str(&format!("<script type=\"application/json\" id=\"site-data\">{data}</script>\n"));
    let page = html
        .replacen("<title>Website</title>", &format!("<title>{}</title>", esc(&meta.title)), 1)
        .replacen("<!--SITE_HEAD-->", &head, 1)
        .replacen("<html lang=\"en\">", &format!("<html lang=\"{}\">", esc(lang)), 1);
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
            (header::HeaderName::from_static("content-security-policy"), SITE_CSP.to_string()),
            (header::HeaderName::from_static("x-frame-options"), "SAMEORIGIN".to_string()),
        ],
        page,
    )
        .into_response()
}

/// Where a website lives: its active custom domain, else `/s/{slug}` on the S'Shop host.
fn site_root(state: &AppState, s: &Site) -> String {
    match &s.domain {
        Some(d) => format!("https://{d}"),
        None => format!("{}/s/{}", state.cfg.public_url, s.slug),
    }
}

fn absolute(root: &str, url: &str) -> String {
    if url.starts_with("http") {
        return url.to_string();
    }
    // root may carry /s/{slug}; images live at the host root.
    let host_root = root.split("/s/").next().unwrap_or(root);
    format!("{host_root}{url}")
}

async fn site_data(state: &AppState, s: &Site) -> AppResult<Value> {
    let t = portal::tenant_by_id(state, s.tenant).await?;
    let (phone, tagline): (String, String) = sqlx::query_as("SELECT phone, tagline FROM tenants WHERE id = $1").bind(s.tenant).fetch_one(&state.db).await?;
    let logo_url = match s.config.brand.logo {
        Some(m) => Some(media_url(m)),
        None => s.has_logo.then(|| format!("/api/public/{}/logo", s.slug)),
    };
    Ok(json!({
        "available": true, "preview": false, "slug": s.slug, "domain": s.domain, "version": s.version,
        "business": { "name": s.name, "tagline": tagline, "phone": phone, "currency": s.currency, "logo_url": logo_url },
        "config": public_config(&s.config),
        "categories": categories(state, s).await?,
        "show_prices": t.settings.orders.show_prices,
        "ordering": { "enabled": s.ordering, "otp_required": portal::otp_required(state, &t) },
    }))
}

async fn sitemap(state: &AppState, s: &Site) -> AppResult<axum::response::Response> {
    let root = site_root(state, s);
    let lastmod = s.published_at.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
    let mut urls = vec![root.clone()];
    for (key, path) in [("about", "/about"), ("products", "/products"), ("services", "/services"), ("contact", "/contact")] {
        if s.config.navigation.iter().any(|n| n.key == key && n.visible) {
            urls.push(format!("{root}{path}"));
        }
    }
    for p in published(state, s).await? {
        urls.push(format!("{root}/products/{}", p.slug));
    }
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for u in urls {
        xml.push_str(&format!("  <url><loc>{}</loc>{}</url>\n", esc(&u), if lastmod.is_empty() { String::new() } else { format!("<lastmod>{lastmod}</lastmod>") }));
    }
    xml.push_str("</urlset>\n");
    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], xml).into_response())
}

/// Serves a website page (`/` … on a custom domain, `/s/{slug}/…` on the S'Shop host) with its title, description,
/// canonical URL and social tags filled in by the server, so search engines and link previews see real content.
async fn page(state: &AppState, headers: &HeaderMap, slug: Option<&str>, rest: &str) -> AppResult<axum::response::Response> {
    let resolved = resolve(state, headers, slug, false).await?;
    let s = match resolved {
        Resolved::Unavailable { name, slug, has_logo } => {
            let meta = PageMeta {
                title: format!("{name} — temporarily unavailable"),
                description: format!("{name}'s website is temporarily unavailable."),
                image: None,
                canonical: format!("{}/s/{slug}", state.cfg.public_url),
                noindex: true,
            };
            let data = json!({ "available": false, "business": { "name": name, "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")) } });
            let mut res = render(state, &meta, &data, &[], "en");
            *res.status_mut() = axum::http::StatusCode::SERVICE_UNAVAILABLE;
            return Ok(res);
        }
        Resolved::Live(s) => *s,
    };
    let root = site_root(state, &s);
    match rest {
        "/sitemap.xml" => return sitemap(state, &s).await,
        "/robots.txt" => {
            return Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], format!("User-agent: *\nAllow: /\nSitemap: {root}/sitemap.xml\n")).into_response());
        }
        _ => {}
    }
    let c = &s.config;
    let brand = if c.brand.name.trim().is_empty() { s.name.clone() } else { c.brand.name.clone() };
    let site_title = if c.seo.title.trim().is_empty() { brand.clone() } else { c.seo.title.clone() };
    let site_desc = [c.seo.description.as_str(), c.brand.tagline.as_str(), c.hero.text.as_str()].into_iter().find(|x| !x.trim().is_empty()).unwrap_or("").to_string();
    let share = c.seo.share_image.or(c.hero.image).map(|m| absolute(&root, &media_url(m)));
    let mut meta = PageMeta { title: site_title.clone(), description: clip(&site_desc, 160), image: share, canonical: format!("{root}{}", if rest == "/" { "" } else { rest }), noindex: false };
    let page_title = |label: &str| format!("{label} — {brand}");
    match rest.trim_end_matches('/') {
        // Roadmap 80: the root shows the business's chosen landing page.
        "" => match c.landing.as_str() {
            "products" => meta.title = page_title("Products"),
            "categories" => meta.title = page_title("Categories"),
            "services" => meta.title = page_title("Services"),
            _ => {}
        },
        "/home" => meta.title = page_title("Home"),
        "/about" => {
            meta.title = page_title("About Us");
            if !c.about.intro.trim().is_empty() {
                meta.description = clip(&c.about.intro, 160);
            }
        }
        "/products" => meta.title = page_title("Products"),
        "/services" => meta.title = page_title("Services"),
        "/contact" => meta.title = page_title("Contact"),
        "/testimonials" => meta.title = page_title("Testimonials"),
        "/order" | "/orders" | "/cart" => {
            meta.title = page_title("Your order");
            meta.noindex = true;
        }
        p if p.starts_with("/products/") => {
            let items = published(state, &s).await?;
            match find_product(&items, &p["/products/".len()..]) {
                Some(prod) => {
                    meta.title = if prod.seo_title.trim().is_empty() { format!("{} — {brand}", prod.name) } else { prod.seo_title.clone() };
                    let d = if prod.seo_description.trim().is_empty() { prod.description.clone() } else { prod.seo_description.clone() };
                    if !d.trim().is_empty() {
                        meta.description = clip(&d, 160);
                    }
                    if let Some(img) = &prod.photo {
                        meta.image = Some(absolute(&root, img));
                    }
                    meta.canonical = format!("{root}/products/{}", prod.slug);
                }
                None => meta.noindex = true,
            }
        }
        p if p.starts_with("/categories/") || p.starts_with("/track/") => meta.noindex = p.starts_with("/track/"),
        _ => meta.noindex = true,
    }
    let data = site_data(state, &s).await?;
    let fonts = vec![c.theme.heading_font.clone(), c.theme.body_font.clone()];
    Ok(render(state, &meta, &data, &fonts, "en"))
}

/// In front of the S'Shop app: requests for a business website (its custom domain, or `/s/{slug}` on the S'Shop host)
/// get the website instead. API, assets and files of the app pass through untouched.
pub async fn host_pages(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = req.uri().path().to_string();
    if path.starts_with("/api/") || path.starts_with("/assets/") || path == "/healthz" {
        return next.run(req).await;
    }
    let host = request_host(req.headers());
    let matched = domain_match(&state, &host).await.ok().flatten();
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    // www ↔ bare twin of a connected domain: one address only (roadmap 74).
    if let Some((_, domain)) = matched.as_ref().filter(|(_, d)| *d != host) {
        return axum::response::Redirect::permanent(&format!("https://{domain}{path}{query}")).into_response();
    }
    let custom = matched.is_some();
    let (slug, rest) = if custom {
        // Static files shipped with the build (icons, fonts …) are served as they are.
        let file = format!("{}{}", state.cfg.web_dir, path);
        // (robots.txt and the sitemap are the website's own, not the app's.)
        if !matches!(path.as_str(), "/" | "/robots.txt" | "/sitemap.xml") && !path.ends_with(".html") && std::path::Path::new(&file).is_file() {
            return next.run(req).await;
        }
        (None, path.clone())
    } else if let Some(after) = path.strip_prefix("/s/") {
        let (slug, rest) = after.split_once('/').map_or((after, "/".to_string()), |(a, b)| (a, format!("/{b}")));
        if slug.is_empty() {
            return next.run(req).await;
        }
        // The business's own domain is its main address: the S'Shop address forwards there (previews stay here).
        if !query.contains("preview=") {
            let primary: Option<String> = sqlx::query_scalar(
                "SELECT d.domain FROM website_domains d JOIN tenants t ON t.id = d.tenant_id WHERE t.slug = $1 AND d.status = 'active' AND d.is_primary",
            )
            .bind(slug)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
            if let Some(d) = primary {
                // Temporary: the business can switch its main address back or remove the domain at any time.
                return axum::response::Redirect::temporary(&format!("https://{d}{rest}{query}")).into_response();
            }
        }
        (Some(slug.to_string()), rest)
    } else {
        return next.run(req).await;
    };
    let headers = req.headers().clone();
    match page(&state, &headers, slug.as_deref(), &rest).await {
        Ok(res) => res,
        Err(AppError::NotFound(_)) => next.run(req).await,
        Err(e) => e.into_response(),
    }
}
