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
use super::site_seo::{self, absolute, clip, esc, Cat};
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
        &format!("SELECT t.id, t.slug, t.name, t.currency, {} AS has_logo, t.status AS tenant_status, w.status, w.billing_suspended,
                w.draft, w.published, w.version, w.published_at, d.domain
         FROM tenants t LEFT JOIN websites w ON w.tenant_id = t.id
         LEFT JOIN website_domains d ON d.tenant_id = t.id AND d.status = 'active' AND d.is_primary
         WHERE ($1::uuid IS NOT NULL AND t.id = $1) OR ($1::uuid IS NULL AND t.slug = current_slug($2))", crate::settings::logo_kept_sql("t", "website")),
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
    /// Staff preview of one campaign (roadmap 85), whatever its status.
    campaign: Option<Uuid>,
}

pub fn media_url(id: Uuid) -> String {
    format!("/api/site/media/{id}")
}

/// The configuration as the public may see it: unpublished testimonials (and every sample), inactive services and
/// promotions and the per-product settings stay on the server. Testimonials (roadmap 90): *Show testimonials* off sends
/// none at all (section, page and menu link disappear; each testimonial keeps its own setting); otherwise only shown,
/// genuine ones. The staff preview also shows the samples switched on for it, marked as samples.
fn public_config(c: &SiteConfig, preview: bool) -> Value {
    let mut c = c.clone();
    if !c.testimonials.show {
        c.testimonials.items.clear();
    }
    c.testimonials.items.retain(|t| t.published && (!t.sample || preview));
    c.services.items.retain(|s| s.active);
    c.promotions.retain(|p| p.active);
    c.products.items.clear();
    // Roadmap 157: the business's search plans and unshown locations stay on the server (locations go out separately).
    c.seo.topics.clear();
    c.seo.primary_category.clear();
    c.seo.service_area.clear();
    c.seo.local = Default::default();
    c.seo.google_verification.clear();
    c.seo.bing_verification.clear();
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
            if s.preview && q.campaign.is_some() {
                data["campaign"] = super::campaigns::public_campaign(&state, &s, q.campaign).await?;
            }
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
    /// Roadmap 131: None = no selling price yet.
    price: Option<Decimal>,
    available: i32,
    available_for_orders: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
    /// Roadmap 130: a second way to ask about a hidden-price product that can be ordered (whatsapp | contact | enquire).
    pub secondary_action: Option<String>,
    /// Roadmap 130: shown instead of a price that is revealed after the order is submitted.
    pub price_note: Option<String>,
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
    pub updated_at: DateTime<Utc>,
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
                GREATEST(COALESCE(sl.on_hand, 0) - COALESCE(sl.reserved, 0), 0) AS available, p.available_for_orders, p.created_at, p.updated_at,
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
        // Roadmap 131: a product without a selling price is "Price on request" — enquired about, never ordered at a price.
        let priced = r.price.is_some();
        let orderable = s.ordering && r.available_for_orders && (in_stock || show_out) && priced;
        let price_visible = price_visible && priced;
        let hidden_action = e.map(|e| e.hidden_action.as_str()).filter(|a| !a.is_empty()).unwrap_or(&cfg.hidden_action).to_string();
        // Roadmap 130: a hidden-price product that can be ordered goes in the cart like any other (its price is shown
        // after the order is submitted); the business's way of asking about it stays as a second action.
        let (action, secondary_action) = if price_visible {
            (if orderable { "add_to_cart" } else { "enquire" }.to_string(), None)
        } else if orderable {
            ("add_to_cart".to_string(), (hidden_action != "order").then(|| hidden_action.clone()))
        } else if hidden_action == "order" {
            ("enquire".to_string(), None)
        } else {
            (hidden_action, None)
        };
        let price_note = if !priced {
            Some("Price on Request".to_string())
        } else {
            (!price_visible && action == "add_to_cart").then(|| "Price revealed after order submission".to_string())
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
            price: if price_visible { r.price } else { None },
            compare_at: e.and_then(|e| e.compare_at).filter(|c| price_visible && r.price.is_some_and(|p| *c > p)),
            stock_ok: in_stock,
            badge: if cfg.grid.show_badges { e.map(|e| e.badge.clone()).unwrap_or_default() } else { String::new() },
            in_stock: cfg.grid.show_availability.then_some(in_stock),
            action,
            secondary_action,
            price_note,
            cta_label: e.map(|e| e.cta_label.clone()).unwrap_or_default(),
            photo: photos.first().cloned(),
            photo_thumb: thumb,
            photos,
            featured: e.is_some_and(|e| e.featured),
            sort: e.map_or(i32::MAX, |e| e.sort),
            created_at: r.created_at,
            updated_at: r.updated_at,
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
    for line in &b.items {
        let p = items.iter().find(|p| p.id == line.product_id).ok_or_else(|| bad("A product in your cart is no longer available"))?;
        if p.action != "add_to_cart" {
            return Err(refused("Ask us first", format!("{} cannot be ordered online — please contact us", p.name)));
        }
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
    // Roadmap 130: once recorded, the order's own prices are disclosed in this response (the snapshot stored with it).
    Ok(Json(json!({
        "id": id, "order_no": order_no, "track_token": track_token, "total": total,
        "items": portal::submitted_lines(&state, id).await?,
        "status_note": "Order submitted — payment and delivery are confirmed by the shop.",
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
    /// Roadmap 86: campaign views and clicks.
    campaign_id: Option<Uuid>,
}

async fn event(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<EventBody>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "site_event", 600, std::time::Duration::from_secs(600))?;
    if !matches!(b.kind.as_str(), "visit" | "product_view" | "add_to_cart" | "order_start" | "campaign_view" | "campaign_product" | "campaign_cta") {
        return Err(bad("Unknown event"));
    }
    let Resolved::Live(s) = resolve(&state, &headers, b.slug.as_deref(), false).await? else { return Ok(Json(json!({ "ok": false }))) };
    let product = match b.product_id {
        Some(p) => sqlx::query_scalar::<_, Uuid>("SELECT id FROM products WHERE id = $1 AND tenant_id = $2").bind(p).bind(s.tenant).fetch_optional(&state.db).await?,
        None => None,
    };
    let campaign = super::campaigns::owned(&state, s.tenant, b.campaign_id).await?;
    if b.kind.starts_with("campaign_") && campaign.is_none() {
        return Err(bad("Unknown campaign"));
    }
    sqlx::query("INSERT INTO website_events (tenant_id, kind, visitor, product_id, campaign_id) VALUES ($1, $2, $3, $4, $5)")
        .bind(s.tenant)
        .bind(&b.kind)
        .bind(b.visitor.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(64).collect::<String>())
        .bind(product)
        .bind(campaign)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Pages, SEO, sitemap (roadmap 53) ─────────────────────────────

/// Same policy as the rest of S'Shop, except that pages may be framed by the same origin (the live preview in the
/// Website Management Centre).
const SITE_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' data: https://fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self'; media-src 'self' blob:; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'self'";

fn site_csp(analytics: bool) -> String {
    if !analytics {
        return SITE_CSP.to_string();
    }
    SITE_CSP
        .replace("script-src 'self'", "script-src 'self' https://www.googletagmanager.com")
        .replace("connect-src 'self'", "connect-src 'self' https://*.google-analytics.com https://*.googletagmanager.com")
        .replace("img-src 'self' data: blob:", "img-src 'self' data: blob: https://*.google-analytics.com https://*.googletagmanager.com")
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
    /// website | product | article
    og_type: &'static str,
    site_name: String,
    /// Structured data for this page (Schema.org).
    ld: Vec<Value>,
    /// The page's real content, in the first HTML (roadmap 156). React replaces it when it starts.
    seed: String,
    /// `rel=prev` / `rel=next` of a paged list.
    prev: Option<String>,
    next: Option<String>,
    /// Roadmap 160: meta tags proving ownership to a search engine (name, code), on the home page.
    verification: Vec<(&'static str, String)>,
    /// Roadmap 160: this website uses Google Analytics, so its security policy allows Google's script for those who accept.
    analytics: bool,
}

fn render(state: &AppState, meta: &PageMeta, data: &Value, fonts: &[String], lang: &str) -> axum::response::Response {
    let Some(html) = shell(state) else {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Website files missing").into_response();
    };
    let mut head = String::new();
    head.push_str(&format!("<meta name=\"description\" content=\"{}\" />\n", esc(&meta.description)));
    head.push_str(&format!("<link rel=\"canonical\" href=\"{}\" />\n", esc(&meta.canonical)));
    for (rel, href) in [("prev", &meta.prev), ("next", &meta.next)] {
        if let Some(h) = href {
            head.push_str(&format!("<link rel=\"{rel}\" href=\"{}\" />\n", esc(h)));
        }
    }
    head.push_str(&format!("<meta property=\"og:title\" content=\"{}\" />\n", esc(&meta.title)));
    head.push_str(&format!("<meta property=\"og:description\" content=\"{}\" />\n", esc(&meta.description)));
    head.push_str(&format!("<meta property=\"og:url\" content=\"{}\" />\n<meta property=\"og:type\" content=\"{}\" />\n", esc(&meta.canonical), meta.og_type));
    head.push_str(&format!("<meta property=\"og:site_name\" content=\"{}\" />\n", esc(&meta.site_name)));
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
    for block in &meta.ld {
        head.push_str(&site_seo::ld_block(block));
    }
    for (name, code) in &meta.verification {
        head.push_str(&format!("<meta name=\"{name}\" content=\"{}\" />\n", esc(code)));
    }
    // Published content for the first render (a data block, not a script: the page's JavaScript reads it).
    let data = data.to_string().replace('<', "\\u003c");
    head.push_str(&format!("<script type=\"application/json\" id=\"site-data\">{data}</script>\n"));
    let page = html
        .replacen("<title>Website</title>", &format!("<title>{}</title>", esc(&meta.title)), 1)
        .replacen("<!--SITE_HEAD-->", &head, 1)
        .replacen("<html lang=\"en\">", &format!("<html lang=\"{}\">", esc(lang)), 1)
        .replacen("<div id=\"site-root\"></div>", &format!("<div id=\"site-root\">{}</div>", meta.seed), 1);
    let mut res = (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
            (header::HeaderName::from_static("content-security-policy"), site_csp(meta.analytics)),
            (header::HeaderName::from_static("x-frame-options"), "SAMEORIGIN".to_string()),
        ],
        page,
    )
        .into_response();
    if meta.noindex {
        res.headers_mut().insert(header::HeaderName::from_static("x-robots-tag"), header::HeaderValue::from_static("noindex"));
    }
    res
}

/// Where a website lives: its active custom domain, else `/s/{slug}` on the S'Shop host.
pub fn site_root(state: &AppState, s: &Site) -> String {
    match &s.domain {
        Some(d) => format!("https://{d}"),
        None => format!("{}/s/{}", state.cfg.public_url, s.slug),
    }
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
        "config": public_config(&s.config, s.preview),
        "categories": categories(state, s).await?,
        "show_prices": t.settings.orders.show_prices,
        "ordering": { "enabled": s.ordering, "otp_required": portal::otp_required(state, &t) },
        // Roadmap 84: the holiday / promotional campaign showing now (or null).
        "campaign": super::campaigns::public_campaign(state, s, None).await?,
        "locations": locations(state, s).await?,
        // Roadmap 158: the Articles link appears once there is something to read.
        "articles": super::articles::live_articles(state, s.tenant, 1, 0).await?.1,
    }))
}

/// Roadmap 157: where customers can visit, as the business chose to show it: only its own active branches it switched on.
/// Empty fields follow the branch's own record; nothing is added.
pub async fn locations(state: &AppState, s: &Site) -> AppResult<Vec<site_seo::Location>> {
    let chosen: Vec<&crate::website::LocalBranch> = s.config.seo.local.branches.iter().filter(|b| b.show).collect();
    if !s.config.seo.local.enabled || chosen.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(Uuid, String, String, String, Option<DbJson<crate::settings::Hours>>)> =
        sqlx::query_as("SELECT id, name, location, phone, hours FROM branches WHERE tenant_id = $1 AND is_active").bind(s.tenant).fetch_all(&state.db).await?;
    let or = |a: &str, b: &str| if a.trim().is_empty() { b.trim().to_string() } else { a.trim().to_string() };
    Ok(chosen
        .into_iter()
        .filter_map(|b| {
            let (_, name, location, phone, hours) = rows.iter().find(|r| r.0 == b.branch_id)?;
            Some(site_seo::Location {
                name: or(&b.name, name),
                street_address: or(&b.street_address, location),
                city: b.city.trim().to_string(),
                region: b.region.trim().to_string(),
                country: or(&b.country, &s.config.seo.local.country),
                phone: or(&b.phone, phone),
                maps_url: b.maps_url.trim().to_string(),
                hours: hours.as_ref().map(|h| h.0.clone()),
            })
        })
        .collect())
}

/// The path the root shows (the business's chosen landing page): one address per page, so that path points back to the root.
fn landing_path(c: &SiteConfig) -> &'static str {
    match c.landing.as_str() {
        "products" => "/products",
        "categories" => "/categories",
        "services" => "/services",
        _ => "/home",
    }
}

fn nav_visible(c: &SiteConfig, key: &str) -> bool {
    c.navigation.iter().any(|n| n.key == key && n.visible)
}

/// The categories this website shows, for links and the sitemap.
pub async fn public_cats(state: &AppState, s: &Site) -> AppResult<Vec<Cat>> {
    Ok(categories(state, s)
        .await?
        .iter()
        .filter_map(|c| Some(Cat { id: c["id"].as_str()?.parse().ok()?, name: c["name"].as_str()?.to_string() }))
        .collect())
}

/// One `<url>` of a sitemap.
fn sitemap_url(loc: &str, lastmod: Option<DateTime<Utc>>) -> String {
    let last = lastmod.map(|d| format!("<lastmod>{}</lastmod>", d.format("%Y-%m-%d"))).unwrap_or_default();
    format!("  <url><loc>{}</loc>{last}</url>\n", esc(loc))
}

/// This website's indexable pages. A website kept out of search engines lists none; a page the root already shows is listed
/// once, as the root; hidden sections, checkout and tracking are never listed.
async fn sitemap(state: &AppState, s: &Site) -> AppResult<axum::response::Response> {
    let root = site_root(state, s);
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    if !s.config.seo.noindex {
        let landing = landing_path(&s.config);
        xml.push_str(&sitemap_url(&root, s.published_at));
        for (key, path) in [("about", "/about"), ("products", "/products"), ("categories", "/categories"), ("services", "/services"), ("contact", "/contact")] {
            if path != landing && nav_visible(&s.config, key) {
                xml.push_str(&sitemap_url(&format!("{root}{path}"), s.published_at));
            }
        }
        if nav_visible(&s.config, "testimonials") && site_seo::shown_testimonials(&s.config).next().is_some() {
            xml.push_str(&sitemap_url(&format!("{root}/testimonials"), s.published_at));
        }
        for c in public_cats(state, s).await? {
            xml.push_str(&sitemap_url(&format!("{root}/categories/{}", c.id), s.published_at));
        }
        for p in published(state, s).await? {
            xml.push_str(&sitemap_url(&format!("{root}/products/{}", p.slug), Some(p.updated_at.max(s.published_at.unwrap_or(p.updated_at)))));
        }
        let (articles, total) = super::articles::live_articles(state, s.tenant, 2000, 0).await?;
        if total > 0 {
            xml.push_str(&sitemap_url(&format!("{root}/articles"), articles.first().map(|a| a.updated_at)));
        }
        for a in articles {
            xml.push_str(&sitemap_url(&format!("{root}/articles/{}", a.slug), Some(a.updated_at)));
        }
    }
    xml.push_str("</urlset>\n");
    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], xml).into_response())
}

/// The S'Shop host's own sitemap: an index of the websites that live on it. A website on its own domain declares its sitemap
/// there, and one kept out of search engines, switched off or suspended is not listed.
async fn sitemap_index(state: &AppState) -> AppResult<axum::response::Response> {
    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT t.slug FROM websites w JOIN tenants t ON t.id = w.tenant_id
          WHERE w.status = 'active' AND NOT COALESCE(w.billing_suspended, false) AND w.published IS NOT NULL AND t.status = 'active'
            AND COALESCE((w.published->'seo'->>'noindex')::boolean, false) = false
            AND NOT EXISTS (SELECT 1 FROM website_domains d WHERE d.tenant_id = t.id AND d.status = 'active' AND d.is_primary)
          ORDER BY t.slug",
    )
    .fetch_all(&state.db)
    .await?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for slug in slugs {
        xml.push_str(&format!("  <sitemap><loc>{}</loc></sitemap>\n", esc(&format!("{}/s/{slug}/sitemap.xml", state.cfg.public_url))));
    }
    xml.push_str("</sitemapindex>\n");
    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], xml).into_response())
}

fn page_number(query: &str) -> usize {
    query.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix("page=")).and_then(|v| v.parse().ok()).filter(|n| *n >= 1).unwrap_or(1)
}

/// Serves a website page (`/` … on a custom domain, `/s/{slug}/…` on the S'Shop host). The server fills in its title,
/// description, canonical URL, social tags, structured data and content (roadmap 53, 156), so search engines and link previews
/// read the real page. Pages that do not exist answer 404, and one page has one address.
async fn page(state: &AppState, headers: &HeaderMap, slug: Option<&str>, rest: &str, query: &str) -> AppResult<axum::response::Response> {
    let resolved = resolve(state, headers, slug, false).await?;
    let s = match resolved {
        Resolved::Unavailable { name, slug, has_logo } => {
            let meta = PageMeta {
                title: format!("{name} — temporarily unavailable"),
                description: format!("{name}'s website is temporarily unavailable."),
                image: None,
                canonical: format!("{}/s/{slug}", state.cfg.public_url),
                noindex: true,
                og_type: "website",
                site_name: name.clone(),
                ld: Vec::new(),
                seed: String::new(),
                prev: None,
                next: None,
                verification: Vec::new(),
                analytics: false,
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
            return Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], site_seo::robots(&format!("{root}/sitemap.xml"), &[])).into_response());
        }
        _ => {}
    }
    // An earlier address of the business, or a trailing slash: forward to the one address.
    let trimmed = if rest.len() > 1 { rest.trim_end_matches('/') } else { rest };
    if slug.is_some_and(|x| x != s.slug) || trimmed != rest {
        let q = if query.is_empty() { String::new() } else { query.to_string() };
        return Ok(axum::response::Redirect::permanent(&format!("{root}{}{q}", if trimmed == "/" { "" } else { trimmed })).into_response());
    }
    let c = &s.config;
    let brand = if c.brand.name.trim().is_empty() { s.name.clone() } else { c.brand.name.clone() };
    let site_title = if c.seo.title.trim().is_empty() { brand.clone() } else { c.seo.title.clone() };
    let site_desc = [c.seo.description.as_str(), c.brand.tagline.as_str(), c.hero.text.as_str()].into_iter().find(|x| !x.trim().is_empty()).map(str::to_string).unwrap_or_else(|| format!("{brand}: products, prices and how to reach us."));
    let share = c.seo.share_image.or(c.hero.image).map(|m| absolute(&root, &media_url(m)));
    let cats = public_cats(state, &s).await?;
    let items = published(state, &s).await?;
    let logo = c.brand.logo.map(media_url).or_else(|| s.has_logo.then(|| format!("/api/public/{}/logo", s.slug)));
    let places = locations(state, &s).await?;
    let (_, article_count) = super::articles::live_articles(state, s.tenant, 1, 0).await?;
    let ctx = site_seo::Ctx { root: &root, brand: &brand, currency: &s.currency, config: c, cats: &cats, logo, places: &places, articles: article_count };
    let page_title = |label: &str| format!("{label} | {brand}");
    let landing = landing_path(c);
    // The root shows the business's chosen landing page (roadmap 80), under the root's address.
    let route = if rest == "/" { landing } else { rest };
    let at_landing = route == landing;
    let number = page_number(query);
    let mut meta = PageMeta {
        title: site_title.clone(),
        description: clip(&site_desc, 160),
        image: share,
        canonical: if at_landing { root.clone() } else { format!("{root}{route}") },
        noindex: false,
        og_type: "website",
        site_name: brand.clone(),
        ld: Vec::new(),
        seed: String::new(),
        prev: None,
        next: None,
        verification: Vec::new(),
        analytics: !c.seo.ga4_id.is_empty(),
    };
    let per = (c.products.per_page.clamp(1, 100)) as usize;
    let mut missing = false;
    let paged_list = |meta: &mut PageMeta, title: &str, base: &str, all: Vec<&PubProduct>| {
        let last = all.len().div_ceil(per).max(1);
        if number > last {
            return true;
        }
        let shown: Vec<&PubProduct> = all.iter().skip((number - 1) * per).take(per).copied().collect();
        let address = |n: usize| {
            let page_root = if base == landing { root.clone() } else { format!("{root}{base}") };
            if n > 1 { format!("{page_root}?page={n}") } else { page_root }
        };
        meta.canonical = address(number);
        meta.prev = (number > 1).then(|| address(number - 1));
        meta.next = (number < last).then(|| address(number + 1));
        if number > 1 {
            meta.title = format!("{} — page {number}", meta.title);
            meta.description = clip(&format!("Page {number} of {last}. {}", meta.description), 160);
        }
        meta.seed = ctx.products_page(title, if base == landing { "/" } else { base }, &shown, number, last);
        false
    };
    match route {
        "/home" => {
            if rest != "/" {
                meta.title = page_title("Home");
            }
            let mut featured: Vec<&PubProduct> = items.iter().filter(|p| p.featured).collect();
            if featured.is_empty() {
                featured = items.iter().collect();
            }
            featured.truncate(per);
            meta.seed = ctx.home_page(&featured);
        }
        "/about" => {
            meta.title = page_title("About Us");
            if !c.about.intro.trim().is_empty() {
                meta.description = clip(&c.about.intro, 160);
            }
            meta.seed = ctx.about_page();
        }
        "/products" => {
            if rest != "/" {
                meta.title = page_title("Products");
            }
            missing = paged_list(&mut meta, "Products", "/products", items.iter().collect());
        }
        "/categories" => {
            if rest != "/" {
                meta.title = page_title("Categories");
            }
            meta.seed = ctx.categories_page();
        }
        "/services" => {
            if rest != "/" {
                meta.title = page_title("Services");
            }
            meta.seed = ctx.services_page();
        }
        "/contact" => {
            meta.title = page_title("Contact");
            if !c.contact.intro.trim().is_empty() {
                meta.description = clip(&c.contact.intro, 160);
            }
            meta.seed = ctx.contact_page();
        }
        "/testimonials" => {
            if site_seo::shown_testimonials(c).next().is_none() {
                missing = true;
            } else {
                meta.title = page_title("Testimonials");
                meta.seed = ctx.testimonials_page();
            }
        }
        "/order" | "/orders" | "/cart" => {
            meta.title = page_title("Your order");
            meta.noindex = true;
        }
        p if p.starts_with("/products/") => {
            let wanted = &p["/products/".len()..];
            match find_product(&items, wanted) {
                Some(prod) => {
                    // A product renamed keeps working at its earlier address, which forwards to the current one.
                    if prod.slug != wanted {
                        let to = format!("{root}/products/{}", prod.slug);
                        return Ok(axum::response::Redirect::permanent(&to).into_response());
                    }
                    meta.title = if prod.seo_title.trim().is_empty() { format!("{} | {brand}", prod.name) } else { prod.seo_title.clone() };
                    let d = if prod.seo_description.trim().is_empty() { prod.description.clone() } else { prod.seo_description.clone() };
                    meta.description = if d.trim().is_empty() { clip(&format!("{} from {brand}.", prod.name), 160) } else { clip(&d, 160) };
                    if let Some(img) = &prod.photo {
                        meta.image = Some(absolute(&root, img));
                    }
                    meta.canonical = format!("{root}/products/{}", prod.slug);
                    meta.og_type = "product";
                    meta.seed = ctx.product_page(prod);
                    meta.ld.push(ctx.product(prod, &meta.canonical));
                    let mut steps = vec![(brand.clone(), "/".to_string()), ("Products".to_string(), "/products".to_string())];
                    if let (Some(id), Some(name)) = (prod.category_id, &prod.category_name) {
                        steps.push((name.clone(), format!("/categories/{id}")));
                    }
                    steps.push((prod.name.clone(), format!("/products/{}", prod.slug)));
                    meta.ld.push(ctx.breadcrumbs(&steps));
                }
                None => missing = true,
            }
        }
        p if p.starts_with("/categories/") => {
            let found = p["/categories/".len()..].parse::<Uuid>().ok().and_then(|id| cats.iter().find(|x| x.id == id));
            match found {
                Some(cat) => {
                    let own = c.categories.items.iter().find(|x| x.category_id == cat.id);
                    meta.title = own.map(|x| x.seo_title.trim()).filter(|x| !x.is_empty()).map_or_else(|| page_title(&cat.name), str::to_string);
                    meta.description = own.map(|x| x.seo_description.trim()).filter(|x| !x.is_empty()).map_or_else(|| clip(&format!("{} at {brand}. {site_desc}", cat.name), 160), |x| clip(x, 160));
                    let in_cat: Vec<&PubProduct> = items.iter().filter(|x| x.category_id == Some(cat.id)).collect();
                    let base = format!("/categories/{}", cat.id);
                    missing = paged_list(&mut meta, &cat.name, &base, in_cat);
                    meta.ld.push(ctx.breadcrumbs(&[(brand.clone(), "/".to_string()), ("Categories".to_string(), "/categories".to_string()), (cat.name.clone(), base.clone())]));
                }
                None => missing = true,
            }
        }
        // Roadmap 158: the business's articles.
        "/articles" => {
            const PER: i64 = 12;
            let (rows, total) = super::articles::live_articles(state, s.tenant, PER, (number as i64 - 1) * PER).await?;
            let last = (total as usize).div_ceil(PER as usize).max(1);
            if total == 0 || number > last {
                missing = true;
            } else {
                meta.title = page_title("Articles");
                meta.description = clip(&format!("Articles and guides from {brand}."), 160);
                let address = |n: usize| if n > 1 { format!("{root}/articles?page={n}") } else { format!("{root}/articles") };
                meta.canonical = address(number);
                meta.prev = (number > 1).then(|| address(number - 1));
                meta.next = (number < last).then(|| address(number + 1));
                if number > 1 {
                    meta.title = format!("{} — page {number}", meta.title);
                    meta.description = clip(&format!("Page {number} of {last}. {}", meta.description), 160);
                }
                meta.seed = ctx.articles_page(&rows, number, last);
            }
        }
        p if p.starts_with("/articles/") => {
            let wanted = &p["/articles/".len()..];
            match super::articles::live_article(state, s.tenant, wanted).await? {
                // An earlier address forwards to the current one.
                Some(a) if a.slug != wanted => {
                    let to = format!("{root}/articles/{}", a.slug);
                    return Ok(axum::response::Redirect::permanent(&to).into_response());
                }
                Some(a) => {
                    meta.title = if a.seo_title.trim().is_empty() { format!("{} | {brand}", a.title) } else { a.seo_title.clone() };
                    meta.description = clip(&a.description, 160);
                    if let Some(m) = a.cover_media {
                        meta.image = Some(absolute(&root, &media_url(m)));
                    }
                    meta.canonical = format!("{root}/articles/{}", a.slug);
                    meta.og_type = "article";
                    meta.seed = ctx.article_page(&a);
                    meta.ld.push(ctx.article_ld(&a, &meta.canonical));
                    meta.ld.push(ctx.breadcrumbs(&[(brand.clone(), "/".to_string()), ("Articles".to_string(), "/articles".to_string()), (a.title.clone(), format!("/articles/{}", a.slug))]));
                }
                None => missing = true,
            }
        }
        p if p.starts_with("/track/") => {
            meta.title = page_title("Your order");
            meta.noindex = true;
        }
        _ => missing = true,
    }
    if missing {
        meta.title = page_title("Page not found");
        meta.noindex = true;
        meta.ld.clear();
        meta.prev = None;
        meta.next = None;
        meta.seed = ctx.not_found();
    }
    if rest == "/" && !missing {
        meta.ld.push(ctx.organization());
        meta.ld.push(ctx.website());
        if !c.seo.google_verification.is_empty() {
            meta.verification.push(("google-site-verification", c.seo.google_verification.clone()));
        }
        if !c.seo.bing_verification.is_empty() {
            meta.verification.push(("msvalidate.01", c.seo.bing_verification.clone()));
        }
    }
    // The places customers can visit, on the page that shows them.
    if route == "/contact" && !missing {
        meta.ld.extend(ctx.branches_ld());
    }
    // The business chose to stay out of search engines (Website → SEO): every page says so.
    if c.seo.noindex {
        meta.noindex = true;
    }
    let data = site_data(state, &s).await?;
    let fonts = vec![c.theme.heading_font.clone(), c.theme.body_font.clone()];
    let lang = if c.seo.language.trim().is_empty() { "en" } else { c.seo.language.trim() };
    let mut res = render(state, &meta, &data, &fonts, lang);
    if missing {
        *res.status_mut() = axum::http::StatusCode::NOT_FOUND;
    }
    Ok(res)
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
    // Roadmap 156: the S'Shop host's own files for crawlers (a custom domain answers with its website's, below). On any other
    // host reaching the app, the app itself is not indexed either.
    if matched.is_none() && matches!(path.as_str(), "/robots.txt" | "/sitemap.xml") {
        let root = state.cfg.public_url.trim_end_matches('/');
        return if path == "/sitemap.xml" {
            sitemap_index(&state).await.unwrap_or_else(IntoResponse::into_response)
        } else {
            ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], site_seo::robots(&format!("{root}/sitemap.xml"), &[])).into_response()
        };
    }
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
                "SELECT d.domain FROM website_domains d JOIN tenants t ON t.id = d.tenant_id WHERE t.slug = current_slug($1) AND d.status = 'active' AND d.is_primary",
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
        return app_page(&state, &host, &path, next.run(req).await);
    };
    let headers = req.headers().clone();
    match page(&state, &headers, slug.as_deref(), &rest, &query).await {
        Ok(res) => res,
        Err(AppError::NotFound(_)) => {
            let res = next.run(req).await;
            app_page(&state, &host, &path, res)
        }
        Err(e) => e.into_response(),
    }
}

/// Roadmap 156: the S'Shop app is private to the people using it. Its pages tell crawlers to stay out, except S'Shop's own
/// front page on its own address (a second address reaching the app is never a competing copy).
fn app_page(state: &AppState, host: &str, path: &str, mut res: axum::response::Response) -> axum::response::Response {
    let own_host = state.cfg.public_url.split("://").nth(1).unwrap_or_default().split(['/', ':']).next().unwrap_or_default().eq_ignore_ascii_case(host);
    let html = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
    if html && !(own_host && path == "/") {
        res.headers_mut().insert(header::HeaderName::from_static("x-robots-tag"), header::HeaderValue::from_static("noindex, nofollow"));
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::website::Testimonial;

    #[test]
    fn page_numbers_come_only_from_the_page_parameter() {
        assert_eq!(page_number(""), 1);
        assert_eq!(page_number("?page=3"), 3);
        assert_eq!(page_number("?sort=name_asc&page=2"), 2);
        assert_eq!(page_number("?page=0"), 1);
        assert_eq!(page_number("?page=abc"), 1);
    }

    fn item(published: bool, sample: bool) -> Testimonial {
        Testimonial { id: Uuid::new_v4(), name: "N".into(), quote: "Q".into(), published, sample, ..Default::default() }
    }

    #[test]
    fn testimonials_reach_the_public_only_when_shown_and_genuine() {
        // Roadmap 90: global switch, individual switch, samples preview-only.
        let mut c = SiteConfig::default();
        c.testimonials.items = vec![item(true, false), item(false, false), item(true, true), item(false, true)];
        let n = |v: Value| v["testimonials"]["items"].as_array().unwrap().len();
        assert_eq!(n(public_config(&c, false)), 1, "public: shown genuine only");
        assert_eq!(n(public_config(&c, true)), 2, "preview: plus samples switched on for it");
        c.testimonials.show = false;
        assert_eq!(n(public_config(&c, false)), 0, "global switch off: none");
        assert_eq!(n(public_config(&c, true)), 0);
        assert_eq!(c.testimonials.items.len(), 4, "individual settings untouched");
    }
}
