//! Website Add-On management (roadmap 51–52): the business's Website Management Centre and the platform owner's
//! approve / decline / activate / disable actions.
//!
//! * `/website` — status, request, draft (edit → preview → publish), discard, history and rollback, product price
//!   visibility, users & access. Each part of the configuration needs its own `website.*` permission; publishing needs
//!   `website.publish`. Everything is audited.
//! * `/platform/tenants/{id}/website` — the platform owner's decisions. Billing uses the `website` service of the
//!   existing engine (`/platform/tenants/{id}/billing-plan` with `service: "website"`).

use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::types::Json as DbJson;
use uuid::Uuid;

use super::access::require_platform_admin;
use super::platform::record_platform;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;
use crate::domains;
use crate::website::{self, SiteConfig};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/website", get(overview))
        .route("/website/request", post(request))
        .route("/website/draft", put(save_draft))
        .route("/website/publish", post(publish))
        .route("/website/discard", post(discard))
        .route("/website/versions", get(versions))
        .route("/website/versions/{version}/restore", post(restore))
        .route("/website/prices", put(set_prices))
        .route("/website/access", get(access_list))
        .route("/website/access/{user_id}", put(access_set))
        .route("/website/media", get(media_list).post(media_upload).layer(DefaultBodyLimit::max(8 * 1024 * 1024)))
        .route("/website/media/{id}", axum::routing::patch(media_update).delete(media_delete))
        .route("/website/domain", get(domain_get).put(domain_set).delete(domain_delete))
        .route("/website/domain/check", post(domain_check))
        .route("/website/domain/primary", put(domain_primary))
        .route("/website/analytics", get(analytics))
        .route("/website/catalogue", get(catalogue))
        .route("/platform/tenants/{id}/website", post(platform_action))
        .route("/platform/tenants/{id}/website/domain", put(platform_domain))
}

pub const PERMS: [&str; 15] = [
    "website.view", "website.content", "website.products", "website.photos", "website.categories", "website.media",
    "website.services", "website.testimonials", "website.design", "website.navigation", "website.seo", "website.domain",
    "website.preview", "website.publish", "website.analytics",
];

#[derive(sqlx::FromRow)]
pub struct SiteRow {
    pub status: String,
    pub status_reason: String,
    pub request_message: String,
    pub requested_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub billing_suspended: bool,
    pub draft: DbJson<SiteConfig>,
    pub draft_updated_at: Option<DateTime<Utc>>,
    pub draft_updated_by_name: Option<String>,
    pub published: Option<DbJson<SiteConfig>>,
    pub version: i32,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by_name: Option<String>,
}

pub async fn load(conn: &mut sqlx::PgConnection, tenant_id: Uuid, lock: bool) -> AppResult<Option<SiteRow>> {
    Ok(sqlx::query_as(&format!(
        "SELECT w.status, w.status_reason, w.request_message, w.requested_at, w.activated_at, w.billing_suspended, w.draft,
                w.draft_updated_at, du.name AS draft_updated_by_name, w.published, w.version, w.published_at, pu.name AS published_by_name
         FROM websites w LEFT JOIN users du ON du.id = w.draft_updated_by LEFT JOIN users pu ON pu.id = w.published_by
         WHERE w.tenant_id = $1 {}",
        if lock { "FOR UPDATE OF w" } else { "" }
    ))
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The website service is active (activated by the platform owner); editing is possible even before the first publish.
fn ensure_active(row: &Option<SiteRow>) -> AppResult<&SiteRow> {
    match row {
        Some(r) if r.status == "active" => Ok(r),
        Some(r) if r.status == "disabled" => Err(refused("Website disabled", "The website service is disabled for this business — contact S'Shop")),
        _ => Err(refused("Website not active", "The website service is not active for this business — request it first")),
    }
}

fn can_any(ctx: &Ctx) -> bool {
    ctx.can("settings.integrations") || PERMS.iter().any(|p| ctx.can(p))
}

async fn business(conn: &mut sqlx::PgConnection, tenant_id: Uuid) -> AppResult<(String, String, String, String, String, String)> {
    Ok(sqlx::query_as("SELECT name, slug, tagline, phone, email, address FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(&mut *conn).await?)
}

/// A fresh starting website for the business.
async fn starter(conn: &mut sqlx::PgConnection, tenant_id: Uuid) -> AppResult<SiteConfig> {
    let (name, _slug, tagline, phone, email, address) = business(conn, tenant_id).await?;
    Ok(website::starter(&website::Business { name: &name, tagline: &tagline, phone: &phone, email: &email, address: &address }))
}

async fn overview(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    if !can_any(&ctx) {
        return Err(AppError::Forbidden("You do not have access to the website".into()));
    }
    let mut conn = state.db.acquire().await?;
    let row = load(&mut conn, ctx.tenant_id, false).await?;
    let (name, slug, ..) = business(&mut conn, ctx.tenant_id).await?;
    let settings: Value = sqlx::query_scalar("SELECT settings FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&mut *conn).await?;
    let show_prices = settings.pointer("/orders/show_prices").and_then(Value::as_bool).unwrap_or(true);
    let billing = crate::billing::summary_of(&mut conn, ctx.tenant_id, "website").await?;
    let domain: Option<(String, String)> = sqlx::query_as("SELECT domain, status FROM website_domains WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?;
    let public_url = match &domain {
        Some((d, s)) if s == "active" => format!("https://{d}"),
        _ => format!("{}/s/{slug}", state.cfg.public_url),
    };
    let Some(r) = row else {
        return Ok(Json(json!({ "status": "none", "business": name, "billing": billing })));
    };
    let has_unpublished = r.published.as_ref().is_none_or(|p| p.0 != r.draft.0);
    let live = r.status == "active" && r.published.is_some() && !r.billing_suspended;
    Ok(Json(json!({
        "status": r.status,
        "status_reason": r.status_reason,
        "request_message": r.request_message,
        "requested_at": r.requested_at,
        "activated_at": r.activated_at,
        "business": name,
        "slug": slug,
        "live": live,
        "billing_suspended": r.billing_suspended,
        "billing": billing,
        "draft": if r.status == "active" { Some(&r.draft.0) } else { None },
        "draft_updated_at": r.draft_updated_at,
        "draft_updated_by": r.draft_updated_by_name,
        "has_unpublished": has_unpublished,
        "version": r.version,
        "published_at": r.published_at,
        "published_by": r.published_by_name,
        "public_url": public_url,
        "preview_url": format!("/s/{slug}?preview=1"),
        "domain": domain.map(|(d, s)| json!({ "domain": d, "status": s })),
        "show_prices": show_prices,
        "fonts": website::FONTS,
    })))
}

#[derive(Deserialize)]
struct RequestBody {
    #[serde(default)]
    message: String,
}

/// *Request Website Service*: the platform owner is emailed and notified in-app; nothing is activated until they decide.
async fn request(State(state): State<AppState>, ctx: Ctx, Json(b): Json<RequestBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.integrations")?;
    let message: String = b.message.trim().chars().take(1000).collect();
    let mut tx = state.db.begin().await?;
    if let Some(r) = load(&mut tx, ctx.tenant_id, true).await? {
        match r.status.as_str() {
            "requested" => return Err(rule("Your website request is already with S'Shop")),
            "active" => return Err(rule("The website service is already active")),
            "disabled" => return Err(refused("Website disabled", "The website service was disabled — contact S'Shop to restore it")),
            _ => {}
        }
        sqlx::query(
            "UPDATE websites SET status = 'requested', status_reason = '', requested_by = $2, requested_at = now(), request_message = $3,
                    decided_by = NULL, decided_at = NULL WHERE tenant_id = $1",
        )
        .bind(ctx.tenant_id)
        .bind(ctx.user_id)
        .bind(&message)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO websites (tenant_id, status, requested_by, requested_at, request_message) VALUES ($1, 'requested', $2, now(), $3)",
        )
        .bind(ctx.tenant_id)
        .bind(ctx.user_id)
        .bind(&message)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(&mut tx, &ctx, Entry::new("website", "request", "website", ctx.tenant_id).after(json!({ "message": message }))).await?;
    let (name, ..) = business(&mut tx, ctx.tenant_id).await?;
    tx.commit().await?;
    let text = format!(
        "{name} requested the S'Shop Website service.\n\nRequested by: {}\nMessage: {}\n\nReview it: {}/settings/businesses/{}",
        ctx.name,
        if message.is_empty() { "—" } else { &message },
        state.cfg.public_url,
        ctx.tenant_id
    );
    notify::to_platform_admins(
        &state,
        Note::new("website_request", format!("Website request: {name}"), format!("Requested by {}", ctx.name), format!("/settings/businesses/{}", ctx.tenant_id)),
        &format!("S'Shop website request: {name}"),
        &text,
    )
    .await;
    Ok(Json(json!({ "ok": true, "status": "requested" })))
}

#[derive(Deserialize)]
struct DraftBody {
    config: SiteConfig,
    /// The draft version the editor started from; a newer save by someone else is refused instead of overwritten.
    #[serde(default)]
    base_updated_at: Option<DateTime<Utc>>,
}

/// Keeps ids stable and unique and product entries one per product.
fn normalise(c: &mut SiteConfig) {
    for p in &mut c.promotions {
        if p.id.is_nil() {
            p.id = Uuid::new_v4();
        }
    }
    for s in &mut c.services.items {
        if s.id.is_nil() {
            s.id = Uuid::new_v4();
        }
    }
    for t in &mut c.testimonials.items {
        if t.id.is_nil() {
            t.id = Uuid::new_v4();
        }
    }
    let mut seen = Vec::new();
    c.products.items.retain(|p| !p.product_id.is_nil() && !seen.contains(&p.product_id) && {
        seen.push(p.product_id);
        true
    });
    for p in &mut c.products.items {
        if p.price.is_empty() {
            p.price = "inherit".into();
        }
    }
}

/// Every configuration change is checked part by part against the user's website permissions, and every image it uses
/// must belong to this business.
async fn check_change(conn: &mut sqlx::PgConnection, ctx: &Ctx, before: &SiteConfig, after: &SiteConfig) -> AppResult<Vec<String>> {
    let parts = website::changed_parts(before, after);
    for p in &parts {
        let perm = website::permission_for(p);
        if !ctx.can(perm) {
            return Err(AppError::Forbidden(format!("You do not have permission to change the website's {p}")));
        }
    }
    let ids = website::media_ids(after);
    if !ids.is_empty() {
        let mine: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM website_media WHERE tenant_id = $1 AND id = ANY($2) AND NOT archived")
            .bind(ctx.tenant_id)
            .bind(&ids)
            .fetch_one(&mut *conn)
            .await?;
        if mine as usize != ids.len() {
            return Err(bad("An image is not in this website's media library (or was archived)"));
        }
    }
    let products: Vec<Uuid> = after.products.items.iter().map(|p| p.product_id).collect();
    if !products.is_empty() {
        let mine: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE tenant_id = $1 AND id = ANY($2)")
            .bind(ctx.tenant_id)
            .bind(&products)
            .fetch_one(&mut *conn)
            .await?;
        if mine as usize != products.len() {
            return Err(bad("A product is not part of this business"));
        }
    }
    Ok(parts)
}

async fn save_draft(State(state): State<AppState>, ctx: Ctx, Json(mut b): Json<DraftBody>) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    let row = load(&mut tx, ctx.tenant_id, true).await?;
    let r = ensure_active(&row)?;
    if let (Some(base), Some(current)) = (b.base_updated_at, r.draft_updated_at) {
        if (current - base).num_milliseconds().abs() > 1 {
            return Err(refused(
                "Changed by someone else",
                format!("{} saved the website draft after you opened it — reload to see their changes", r.draft_updated_by_name.clone().unwrap_or_default()),
            ));
        }
    }
    normalise(&mut b.config);
    website::validate(&b.config)?;
    let parts = check_change(&mut tx, &ctx, &r.draft.0, &b.config).await?;
    if parts.is_empty() {
        return Ok(Json(json!({ "ok": true, "changed": [], "draft_updated_at": r.draft_updated_at })));
    }
    let updated: DateTime<Utc> = sqlx::query_scalar(
        "UPDATE websites SET draft = $2, draft_updated_at = now(), draft_updated_by = $3 WHERE tenant_id = $1 RETURNING draft_updated_at",
    )
    .bind(ctx.tenant_id)
    .bind(DbJson(&b.config))
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "draft_saved", "website", ctx.tenant_id).after(json!({ "parts": parts }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "changed": parts, "draft_updated_at": updated })))
}

/// Problems that keep a draft from going public.
fn publish_blockers(c: &SiteConfig) -> Vec<String> {
    let mut out = Vec::new();
    if c.testimonials.items.iter().any(|t| t.sample && t.published) {
        out.push("Sample testimonials cannot be published — replace them with genuine ones or confirm they are genuine".to_string());
    }
    if c.brand.name.trim().is_empty() {
        out.push("Add the business name".to_string());
    }
    if c.navigation.iter().all(|n| !n.visible) {
        out.push("Show at least one navigation item".to_string());
    }
    out
}

#[derive(Deserialize, Default)]
struct PublishBody {
    #[serde(default)]
    note: String,
}

/// Draft → published: the public website changes only now, as one step. Each publication is kept for rollback.
async fn publish(State(state): State<AppState>, ctx: Ctx, body: Option<Json<PublishBody>>) -> AppResult<Json<Value>> {
    ctx.require("website.publish")?;
    let note: String = body.map(|b| b.0.note).unwrap_or_default().trim().chars().take(200).collect();
    let mut tx = state.db.begin().await?;
    let row = load(&mut tx, ctx.tenant_id, true).await?;
    let r = ensure_active(&row)?;
    website::validate(&r.draft.0)?;
    let blockers = publish_blockers(&r.draft.0);
    if !blockers.is_empty() {
        return Err(refused("Not ready to publish", blockers.join(" · ")));
    }
    if r.published.as_ref().is_some_and(|p| p.0 == r.draft.0) {
        return Err(rule("Nothing new to publish"));
    }
    let changed = match &r.published {
        Some(p) => website::changed_parts(&p.0, &r.draft.0),
        None => vec!["first publication".to_string()],
    };
    let version = r.version + 1;
    sqlx::query("UPDATE websites SET published = draft, version = $2, published_at = now(), published_by = $3 WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .bind(version)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO website_versions (tenant_id, version, config, published_by, note) VALUES ($1, $2, $3, $4, $5)")
        .bind(ctx.tenant_id)
        .bind(version)
        .bind(DbJson(&r.draft.0))
        .bind(ctx.user_id)
        .bind(&note)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "publish", "website", ctx.tenant_id).after(json!({ "version": version, "parts": changed, "note": note })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "version": version, "changed": changed })))
}

/// Unpublished changes are thrown away: the draft becomes the published website again (or a fresh start).
async fn discard(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    if !PERMS.iter().filter(|p| **p != "website.view" && **p != "website.analytics").any(|p| ctx.can(p)) {
        return Err(AppError::Forbidden("You do not have permission to change the website".into()));
    }
    let mut tx = state.db.begin().await?;
    let row = load(&mut tx, ctx.tenant_id, true).await?;
    let r = ensure_active(&row)?;
    let base = match &r.published {
        Some(p) => p.0.clone(),
        None => starter(&mut tx, ctx.tenant_id).await?,
    };
    if base == r.draft.0 {
        return Err(rule("There are no unpublished changes"));
    }
    let parts = website::changed_parts(&r.draft.0, &base);
    sqlx::query("UPDATE websites SET draft = $2, draft_updated_at = now(), draft_updated_by = $3 WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .bind(DbJson(&base))
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "discard", "website", ctx.tenant_id).after(json!({ "parts": parts }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "discarded": parts })))
}

async fn versions(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    if !can_any(&ctx) {
        return Err(AppError::Forbidden("You do not have access to the website".into()));
    }
    let rows: Vec<(i32, DateTime<Utc>, Option<String>, String)> = sqlx::query_as(
        "SELECT v.version, v.published_at, u.name, v.note FROM website_versions v LEFT JOIN users u ON u.id = v.published_by
         WHERE v.tenant_id = $1 ORDER BY v.version DESC LIMIT 50",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "items": rows.into_iter().map(|(v, at, by, note)| json!({ "version": v, "published_at": at, "published_by": by, "note": note })).collect::<Vec<_>>(),
    })))
}

/// Rollback: an earlier publication becomes the live website again (as a new publication, so nothing is lost).
async fn restore(State(state): State<AppState>, ctx: Ctx, Path(version): Path<i32>) -> AppResult<Json<Value>> {
    ctx.require("website.publish")?;
    let mut tx = state.db.begin().await?;
    let row = load(&mut tx, ctx.tenant_id, true).await?;
    let r = ensure_active(&row)?;
    let cfg: DbJson<SiteConfig> = sqlx::query_scalar("SELECT config FROM website_versions WHERE tenant_id = $1 AND version = $2")
        .bind(ctx.tenant_id)
        .bind(version)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Website version"))?;
    // Media archived since then would leave holes: refuse rather than publish a broken page.
    let ids = website::media_ids(&cfg.0);
    if !ids.is_empty() {
        let ok: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM website_media WHERE tenant_id = $1 AND id = ANY($2) AND NOT archived")
            .bind(ctx.tenant_id)
            .bind(&ids)
            .fetch_one(&mut *tx)
            .await?;
        if ok as usize != ids.len() {
            return Err(refused("Cannot restore", "That version uses images that were archived since — restore them in the media library first"));
        }
    }
    let new_version = r.version + 1;
    sqlx::query(
        "UPDATE websites SET published = $2, draft = $2, version = $3, published_at = now(), published_by = $4,
                draft_updated_at = now(), draft_updated_by = $4 WHERE tenant_id = $1",
    )
    .bind(ctx.tenant_id)
    .bind(&cfg)
    .bind(new_version)
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO website_versions (tenant_id, version, config, published_by, note) VALUES ($1, $2, $3, $4, $5)")
        .bind(ctx.tenant_id)
        .bind(new_version)
        .bind(&cfg)
        .bind(ctx.user_id)
        .bind(format!("Restored version {version}"))
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "rollback", "website", ctx.tenant_id).after(json!({ "restored": version, "version": new_version })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "version": new_version })))
}

#[derive(Deserialize)]
struct PricesBody {
    show_prices: bool,
}

/// Website-wide price visibility is the same setting as the ordering link's *Show product prices* — one setting, so the
/// two can never contradict each other. It applies at once (it is not part of the draft).
async fn set_prices(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PricesBody>) -> AppResult<Json<Value>> {
    ctx.require("website.products")?;
    let mut tx = state.db.begin().await?;
    let before: Option<bool> = sqlx::query_scalar("SELECT (settings #>> '{orders,show_prices}')::boolean FROM tenants WHERE id = $1 FOR UPDATE")
        .bind(ctx.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE tenants SET settings = jsonb_set(jsonb_set(settings, '{orders}', COALESCE(settings->'orders', '{}'::jsonb)), '{orders,show_prices}', to_jsonb($2::boolean)) WHERE id = $1")
        .bind(ctx.tenant_id)
        .bind(b.show_prices)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("settings", "update", "settings", ctx.tenant_id)
            .before(json!({ "orders.show_prices": before.unwrap_or(true) }))
            .after(json!({ "orders.show_prices": b.show_prices, "via": "website" })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "show_prices": b.show_prices })))
}

/// Users & Access: existing S'Shop users and the website permissions they hold through their role or individually.
async fn access_list(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require_any(&["users.manage", "website.view"])?;
    let rows: Vec<(Uuid, String, String, String, Vec<String>, Vec<String>, bool)> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, r.name, r.permissions, u.extra_permissions, u.is_active
         FROM users u JOIN roles r ON r.id = u.role_id WHERE u.tenant_id = $1 ORDER BY u.is_active DESC, u.name",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "permissions": PERMS,
        "can_manage": ctx.can("users.manage"),
        "users": rows.into_iter().map(|(id, name, email, role, role_perms, extra, active)| {
            let all = role_perms.iter().any(|p| p == "*");
            json!({
                "id": id, "name": name, "email": email, "role": role, "is_active": active, "administrator": all,
                "from_role": if all { PERMS.iter().map(|p| p.to_string()).collect::<Vec<_>>() } else { role_perms.into_iter().filter(|p| p.starts_with("website.")).collect() },
                "individual": extra,
            })
        }).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct AccessBody {
    permissions: Vec<String>,
}

/// Gives a user website permissions individually (on top of their role). Only website permissions can be given this way,
/// so website access never opens sales, stock or finance.
async fn access_set(State(state): State<AppState>, ctx: Ctx, Path(user_id): Path<Uuid>, Json(b): Json<AccessBody>) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    let mut perms: Vec<String> = Vec::new();
    for p in b.permissions {
        if !PERMS.contains(&p.as_str()) {
            return Err(bad(format!("Not a website permission: {p}")));
        }
        if !perms.contains(&p) {
            perms.push(p);
        }
    }
    let mut tx = state.db.begin().await?;
    let before: Option<Vec<String>> = sqlx::query_scalar("SELECT extra_permissions FROM users WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(user_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    let before = before.ok_or(AppError::NotFound("User"))?;
    // Only the website part changes; the user's other access exceptions (roadmap 64) stay as they are.
    let mut all: Vec<String> = before.iter().filter(|p| !p.starts_with("website.")).cloned().collect();
    all.extend(perms.iter().cloned());
    sqlx::query("UPDATE users SET extra_permissions = $3 WHERE id = $1 AND tenant_id = $2")
        .bind(user_id)
        .bind(ctx.tenant_id)
        .bind(&all)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "access", "user", user_id).before(json!({ "permissions": before })).after(json!({ "permissions": perms })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "permissions": perms })))
}

#[derive(Deserialize)]
struct PlatformBody {
    /// activate | decline | disable
    action: String,
    #[serde(default)]
    reason: String,
}

/// The platform owner's decisions on the website service. Activating starts the business with a ready-made draft;
/// disabling takes the public website offline ("temporarily unavailable") but keeps configuration, content, media,
/// domain, products and analytics.
async fn platform_action(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<PlatformBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let reason: String = b.reason.trim().chars().take(500).collect();
    if matches!(b.action.as_str(), "decline" | "disable") && reason.chars().count() < 3 {
        return Err(refused("Reason required", "Give the reason"));
    }
    let mut tx = state.db.begin().await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tenants WHERE id = $1)").bind(id).fetch_one(&mut *tx).await?;
    if !exists {
        return Err(AppError::NotFound("Business"));
    }
    let row = load(&mut tx, id, true).await?;
    let current = row.as_ref().map_or("none", |r| r.status.as_str()).to_string();
    let next = match (b.action.as_str(), current.as_str()) {
        ("activate", "active") => return Err(rule("The website service is already active")),
        ("activate", _) => "active",
        ("decline", "requested") => "declined",
        ("decline", _) => return Err(rule("Only a pending request can be declined")),
        ("disable", "active") => "disabled",
        ("disable", _) => return Err(rule("Only an active website can be disabled")),
        _ => return Err(bad("Choose activate, decline or disable")),
    };
    if row.is_none() {
        let draft = starter(&mut tx, id).await?;
        sqlx::query("INSERT INTO websites (tenant_id, status, draft, draft_updated_at) VALUES ($1, 'requested', $2, now())")
            .bind(id)
            .bind(DbJson(&draft))
            .execute(&mut *tx)
            .await?;
    } else if next == "active" && row.as_ref().is_some_and(|r| r.draft.0 == SiteConfig::default()) {
        let draft = starter(&mut tx, id).await?;
        sqlx::query("UPDATE websites SET draft = $2, draft_updated_at = now() WHERE tenant_id = $1").bind(id).bind(DbJson(&draft)).execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE websites SET status = $2, status_reason = $3, decided_by = $4, decided_at = now(),
                activated_at = CASE WHEN $2 = 'active' THEN COALESCE(activated_at, now()) ELSE activated_at END
         WHERE tenant_id = $1",
    )
    .bind(id)
    .bind(next)
    .bind(&reason)
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    let after = json!({ "from": current, "to": next, "reason": reason });
    record_platform(&mut tx, &ctx, id, || Entry::new("website", "service", "website", id).after(after.clone())).await?;
    let (name, ..) = business(&mut tx, id).await?;
    tx.commit().await?;
    let (title, body) = match next {
        "active" => ("Your website service is active".to_string(), "Set it up in Settings → Integrations → Website".to_string()),
        "declined" => ("Website request declined".to_string(), reason.clone()),
        _ => ("Website service disabled".to_string(), format!("Your website shows “temporarily unavailable” — {reason}")),
    };
    notify::to_permission(&state, id, None, "settings.integrations", Note::new("website_service", title, body, "/settings/website")).await;
    tracing::info!(business = %name, %next, "website service");
    Ok(Json(json!({ "ok": true, "status": next })))
}

// ───────────────────────────── Media library (roadmap 55) ─────────────────────────────

const MEDIA_KINDS: [&str; 8] = ["logo", "banner", "product", "service", "testimonial", "about", "promotion", "other"];

#[derive(serde::Serialize, sqlx::FromRow)]
struct MediaRow {
    id: Uuid,
    kind: String,
    name: String,
    mime: String,
    width: i32,
    height: i32,
    bytes: i32,
    quality: String,
    warnings: Vec<String>,
    archived: bool,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct MediaQuery {
    kind: Option<String>,
    #[serde(default)]
    archived: bool,
}

fn can_upload(ctx: &Ctx) -> bool {
    ["website.media", "website.photos", "website.design", "website.content", "website.services", "website.testimonials", "website.categories"]
        .iter()
        .any(|p| ctx.can(p))
}

async fn media_list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<MediaQuery>) -> AppResult<Json<Value>> {
    if !can_any(&ctx) {
        return Err(AppError::Forbidden("You do not have access to the website".into()));
    }
    let rows: Vec<MediaRow> = sqlx::query_as(
        "SELECT id, kind, name, mime, width, height, bytes, quality, warnings, archived, created_at FROM website_media
         WHERE tenant_id = $1 AND archived = $2 AND ($3::text IS NULL OR kind = $3) ORDER BY created_at DESC LIMIT 500",
    )
    .bind(ctx.tenant_id)
    .bind(q.archived)
    .bind(&q.kind)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "items": rows })))
}

/// Upload one image: optimised in the browser (a display size and a small thumbnail); the server checks the real
/// format and size and grades it (✓ good, ⚠ warning, ✕ refused).
async fn media_upload(State(state): State<AppState>, ctx: Ctx, mut mp: Multipart) -> AppResult<Json<Value>> {
    if !can_upload(&ctx) {
        return Err(AppError::Forbidden("You do not have permission to add website images".into()));
    }
    let (mut kind, mut name, mut blurry, mut upload_ref) = (String::from("other"), String::new(), false, None::<Uuid>);
    let (mut data, mut thumb) = (None, None);
    let unreadable = || bad("The image could not be read. Try again.");
    while let Some(field) = mp.next_field().await.map_err(|_| unreadable())? {
        match field.name() {
            Some("kind") => kind = field.text().await.map_err(|_| unreadable())?,
            Some("name") => name = field.text().await.map_err(|_| unreadable())?.trim().chars().take(120).collect(),
            Some("blurry") => blurry = field.text().await.map_err(|_| unreadable())?.trim() == "true",
            Some("upload_ref") => upload_ref = field.text().await.ok().and_then(|t| Uuid::parse_str(t.trim()).ok()),
            Some("file") => data = Some(field.bytes().await.map_err(|_| unreadable())?),
            Some("thumb") => thumb = Some(field.bytes().await.map_err(|_| unreadable())?),
            _ => {}
        }
    }
    if !MEDIA_KINDS.contains(&kind.as_str()) {
        return Err(bad("Unknown image type"));
    }
    let data = data.ok_or_else(|| bad("No image uploaded"))?;
    if data.len() > 5 * 1024 * 1024 {
        return Err(refused("Cannot upload", "The image is over 5 MB even after optimising — use a smaller one"));
    }
    let (mime, w, h, warnings) = website::assess(&kind, &data, blurry).map_err(|m| refused("Cannot upload", m))?;
    let thumb = match thumb {
        Some(t) if t.len() <= 400 * 1024 && website::sniff(&t).is_some() => Some(t),
        Some(_) => return Err(bad("The thumbnail could not be read")),
        None => None,
    };
    let mut tx = state.db.begin().await?;
    let row = load(&mut tx, ctx.tenant_id, false).await?;
    ensure_active(&row)?;
    if let Some(r) = upload_ref {
        let already: Option<Uuid> = sqlx::query_scalar("SELECT id FROM website_media WHERE tenant_id = $1 AND upload_ref = $2")
            .bind(ctx.tenant_id)
            .bind(r)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(id) = already {
            return Ok(Json(json!({ "id": id, "duplicate": true })));
        }
    }
    let quality = if warnings.is_empty() { "good" } else { "warning" };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO website_media (tenant_id, kind, name, mime, width, height, bytes, data, thumb, thumb_mime, quality, warnings, upload_ref, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&kind)
    .bind(&name)
    .bind(mime)
    .bind(w as i32)
    .bind(h as i32)
    .bind(data.len() as i32)
    .bind(data.to_vec())
    .bind(thumb.as_ref().map(|t| t.to_vec()))
    .bind(thumb.as_ref().and_then(|t| website::sniff(t)))
    .bind(quality)
    .bind(&warnings)
    .bind(upload_ref)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "media_upload", "website_media", id).after(json!({ "kind": kind, "name": name, "quality": quality })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "width": w, "height": h, "quality": quality, "warnings": warnings })))
}

#[derive(Deserialize)]
struct MediaPatch {
    name: Option<String>,
    archived: Option<bool>,
}

async fn in_use(conn: &mut sqlx::PgConnection, tenant_id: Uuid, id: Uuid) -> AppResult<(bool, bool)> {
    let row = load(conn, tenant_id, false).await?;
    let draft = row.as_ref().is_some_and(|r| website::media_ids(&r.draft.0).contains(&id));
    let published = row.as_ref().and_then(|r| r.published.as_ref()).is_some_and(|p| website::media_ids(&p.0).contains(&id));
    Ok((draft, published))
}

async fn media_update(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<MediaPatch>) -> AppResult<Json<Value>> {
    ctx.require("website.media")?;
    let mut tx = state.db.begin().await?;
    if b.archived == Some(true) {
        let (draft, published) = in_use(&mut tx, ctx.tenant_id, id).await?;
        if draft || published {
            return Err(refused("Image in use", "The website still uses this image — replace it first, then archive it"));
        }
    }
    let name: Option<String> = b.name.map(|n| n.trim().chars().take(120).collect());
    let ok = sqlx::query("UPDATE website_media SET name = COALESCE($3, name), archived = COALESCE($4, archived) WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(&name)
        .bind(b.archived)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if ok == 0 {
        return Err(AppError::NotFound("Image"));
    }
    audit::record(&mut tx, &ctx, Entry::new("website", "media_update", "website_media", id).after(json!({ "name": name, "archived": b.archived })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Deleting is only possible for images no version of the website uses (including earlier publications kept for
/// rollback); otherwise archive it.
async fn media_delete(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("website.media")?;
    let mut tx = state.db.begin().await?;
    let (draft, published) = in_use(&mut tx, ctx.tenant_id, id).await?;
    let in_history: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_versions WHERE tenant_id = $1 AND config::text LIKE '%' || $2 || '%')")
        .bind(ctx.tenant_id)
        .bind(id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    if draft || published || in_history {
        return Err(refused("Image in use", "This image is used by the website or an earlier version of it — archive it instead"));
    }
    let n = sqlx::query("DELETE FROM website_media WHERE id = $1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Image"));
    }
    audit::record(&mut tx, &ctx, Entry::new("website", "media_delete", "website_media", id)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Custom domain (roadmap 56) ─────────────────────────────

#[derive(sqlx::FromRow)]
struct DomainRow {
    domain: String,
    status: String,
    is_primary: bool,
    token: String,
    last_check: Option<DbJson<domains::Check>>,
    checked_at: Option<DateTime<Utc>>,
    verified_at: Option<DateTime<Utc>>,
    active_at: Option<DateTime<Utc>>,
}

async fn domain_row(conn: &mut sqlx::PgConnection, tenant_id: Uuid, lock: bool) -> AppResult<Option<DomainRow>> {
    Ok(sqlx::query_as(&format!(
        "SELECT domain, status, is_primary, token, last_check, checked_at, verified_at, active_at FROM website_domains WHERE tenant_id = $1 {}",
        if lock { "FOR UPDATE" } else { "" }
    ))
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The S'Shop app's own hosts — never a business website.
fn platform_hosts(state: &AppState) -> Vec<String> {
    let host = state.cfg.public_url.split("://").nth(1).unwrap_or_default().split(['/', ':']).next().unwrap_or_default().to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    if host.is_empty() {
        Vec::new()
    } else {
        vec![host]
    }
}

fn ownership_record(r: &DomainRow, status: &str) -> domains::Record {
    domains::txt_record(
        &format!("_sshop-verify.{}", r.domain),
        &r.domain,
        format!("sshop-verify={}", r.token),
        status,
        "Proves the domain is yours. Keep it in place.",
    )
}

/// A TXT record that is missing at its name but present with the zone added twice (the full name typed into a
/// provider that appends the domain itself): say exactly how to fix it.
async fn misplaced_txt(state: &AppState, fqdn: &str, domain: &str, value: &str) -> Option<String> {
    let wrong = domains::doubled(fqdn, domain);
    let found = domains::lookup(&state.http, &wrong, "TXT").await.unwrap_or_default();
    found.iter().any(|v| *v == value.to_lowercase()).then(|| {
        format!(
            "Your TXT record was saved as {wrong} — your DNS provider adds {} automatically. Edit the record's Name to just {} and check again.",
            domains::zone(domain),
            domains::host(fqdn, domain)
        )
    })
}

fn domain_message(status: &str) -> &'static str {
    match status {
        "dns_required" => "Add the DNS records below at your domain provider, then check again.",
        "verifying" => "Ownership confirmed. Awaiting platform configuration: S'Shop is connecting your domain to its hosting and has been notified — check again later.",
        "points_elsewhere" => "Your domain still points somewhere else. Change the record below.",
        "ssl_pending" => "Your domain points to S'Shop. The security certificate is being issued (usually within an hour).",
        "active" => "Your website is live on this domain.",
        "misconfigured" => "Your domain stopped reaching S'Shop. Check the DNS records below.",
        _ => "",
    }
}

/// The records to show (with copy buttons) and what to do next.
fn domain_view(state: &AppState, r: &DomainRow) -> Value {
    let check = r.last_check.as_ref().map(|c| c.0.clone()).unwrap_or_default();
    let mut records = check.records.clone();
    if records.is_empty() {
        records.push(ownership_record(r, "pending"));
    }
    json!({
        "domain": r.domain,
        "status": r.status,
        "message": if check.message.is_empty() { domain_message(&r.status).to_string() } else { check.message.clone() },
        "records": records,
        "checked_at": r.checked_at,
        "verified_at": r.verified_at,
        "active_at": r.active_at,
        "url": format!("https://{}", r.domain),
        "automatic": state.cfg.railway.is_some(),
        "is_primary": r.is_primary,
        // Ownership proven, but the hosting side is set up by hand by the platform owner (roadmap 74).
        "awaiting_platform": r.status == "verifying" && state.cfg.railway.is_none() && check.routing_target.is_none(),
    })
}

async fn domain_get(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require_any(&["website.domain", "website.view"])?;
    let mut conn = state.db.acquire().await?;
    let row = domain_row(&mut conn, ctx.tenant_id, false).await?;
    Ok(Json(json!({ "domain": row.map(|r| domain_view(&state, &r)), "automatic": state.cfg.railway.is_some() })))
}

#[derive(Deserialize)]
struct DomainBody {
    domain: String,
}

/// Connects (or changes) the business's domain. Nothing is served on it until it is verified and reaches S'Shop.
async fn domain_set(State(state): State<AppState>, ctx: Ctx, Json(b): Json<DomainBody>) -> AppResult<Json<Value>> {
    ctx.require("website.domain")?;
    let domain = domains::normalise(&b.domain, &platform_hosts(&state)).map_err(bad)?;
    let mut tx = state.db.begin().await?;
    ensure_active(&load(&mut tx, ctx.tenant_id, true).await?)?;
    let before = domain_row(&mut tx, ctx.tenant_id, true).await?;
    if let Some(r) = before.as_ref().filter(|r| r.domain == domain) {
        let view = domain_view(&state, r);
        tx.commit().await?;
        return Ok(Json(json!({ "ok": true, "domain": view })));
    }
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_domains WHERE domain = $1 AND tenant_id <> $2)")
        .bind(&domain)
        .bind(ctx.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    if taken {
        return Err(refused("Domain in use", "This domain is already connected to another business"));
    }
    let old_railway = before.as_ref().and_then(|r| r.last_check.as_ref()).and_then(|c| c.0.railway_id.clone());
    let token = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO website_domains (tenant_id, domain, status, token, created_by) VALUES ($1, $2, 'dns_required', $3, $4)
         ON CONFLICT (tenant_id) DO UPDATE SET domain = $2, status = 'dns_required', token = $3, last_check = NULL, checked_at = NULL,
             verified_at = NULL, active_at = NULL, created_by = $4, created_at = now()",
    )
    .bind(ctx.tenant_id)
    .bind(&domain)
    .bind(&token)
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("website", "domain", "website", ctx.tenant_id)
            .before(json!({ "domain": before.as_ref().map(|r| r.domain.clone()) }))
            .after(json!({ "domain": domain })),
    )
    .await?;
    let row = domain_row(&mut tx, ctx.tenant_id, false).await?.ok_or(AppError::NotFound("Domain"))?;
    tx.commit().await?;
    if let (Some(id), Some(api)) = (old_railway, state.cfg.railway.as_ref()) {
        if let Err(e) = domains::railway_detach(&state.http, api, &id).await {
            tracing::warn!(error = %e, "could not remove the previous custom domain from Railway");
        }
    }
    Ok(Json(json!({ "ok": true, "domain": domain_view(&state, &row) })))
}

/// Re-checks the domain: ownership TXT → attached to S'Shop → routing → certificate → reaches this server.
async fn domain_check(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("website.domain")?;
    state.limits.check(&ctx.user_id.to_string(), "website_domain_check", 20, std::time::Duration::from_secs(600))?;
    let row = {
        let mut conn = state.db.acquire().await?;
        ensure_active(&load(&mut conn, ctx.tenant_id, false).await?)?;
        domain_row(&mut conn, ctx.tenant_id, false).await?.ok_or(AppError::NotFound("Domain"))?
    };
    let mut check = row.last_check.as_ref().map(|c| c.0.clone()).unwrap_or_default();
    check.message = String::new();
    let d = row.domain.clone();
    let was_live = matches!(row.status.as_str(), "active" | "misconfigured");

    // 1. Ownership.
    let own_name = format!("_sshop-verify.{d}");
    let own_value = format!("sshop-verify={}", row.token);
    let txt = domains::lookup(&state.http, &own_name, "TXT").await.map_err(AppError::Upstream)?;
    let owned = txt.iter().any(|v| *v == own_value);
    let misplaced = if owned { None } else { misplaced_txt(&state, &own_name, &d, &own_value).await };
    let mut records = vec![ownership_record(&row, if owned { "ok" } else if misplaced.is_some() { "misplaced" } else { "missing" })];
    let mut first_verified = false;
    let status: &str;
    if !owned {
        status = if was_live { "misconfigured" } else { "dns_required" };
        check.message = misplaced.unwrap_or_else(|| "Add the TXT record below (it can take a few minutes to appear), then check again.".into());
    } else {
        first_verified = row.verified_at.is_none();
        // 2. Attached to this service (automatically when the Railway API is configured).
        let mut routing_ok: Option<bool> = None;
        if let Some(api) = state.cfg.railway.as_ref() {
            // Roadmap 78: attached only after ownership is proven; the token never leaves the server; an existing
            // attachment on this service is reused (unique domain per business is enforced by the database).
            if check.railway_id.is_none() {
                let id = domains::railway_attach(&state.http, api, &d).await.map_err(AppError::Upstream)?;
                check.railway_id = Some(id.clone());
                let mut tx = state.db.begin().await?;
                audit::record(&mut tx, &ctx, Entry::new("website", "domain_attached", "website", ctx.tenant_id).after(json!({ "domain": d, "railway_id": id }))).await?;
                tx.commit().await?;
            }
            let mut extra: Vec<domains::Record> = Vec::new();
            let mut railway_verified = false;
            if let Some(id) = check.railway_id.clone() {
                let rs = domains::railway_status(&state.http, api, &id).await.map_err(AppError::Upstream)?;
                if rs.target.is_some() {
                    check.routing_target = rs.target.clone();
                }
                if rs.verification_token.is_some() {
                    check.railway_txt_value = rs.verification_token.clone();
                }
                if rs.verification_host.is_some() {
                    check.railway_txt_name = rs.verification_host.clone();
                }
                routing_ok = Some(rs.routing_ok);
                railway_verified = rs.verified;
                // Railway's other required records (e.g. certificate challenges), named the way Railway gives them.
                for r in rs.records.iter().filter(|r| r.purpose != "TRAFFIC_ROUTE") {
                    extra.push(domains::Record {
                        kind: r.kind.clone(),
                        name: r.host.clone(),
                        fqdn: r.fqdn.clone(),
                        value: r.value.clone(),
                        status: if r.propagated { "ok" } else { "missing" }.into(),
                        note: "Required by S'Shop's hosting for the security certificate.".into(),
                    });
                }
                if rs.certificate == "failed" {
                    check.message = format!(
                        "The security certificate could not be issued yet{} — check the records below, then test again.",
                        rs.certificate_error.map(|e| format!(" ({e})")).unwrap_or_default()
                    );
                }
            }
            // Railway's own ownership TXT: shown until Railway reports the domain verified.
            if let Some(token) = check.railway_txt_value.clone().filter(|_| !railway_verified) {
                // Railway gives a host label (`_railway-verify`); the lookup needs the full name.
                let name = match check.railway_txt_name.clone() {
                    Some(h) if h == d || h.ends_with(&format!(".{d}")) => h,
                    Some(h) => format!("{}.{d}", h.trim_end_matches('.')),
                    None => format!("_railway-verify.{d}"),
                };
                let value = if token.starts_with("railway-verify=") { token } else { format!("railway-verify={token}") };
                let present = domains::lookup(&state.http, &name, "TXT").await.unwrap_or_default().contains(&value.to_lowercase());
                let misplaced = if present { None } else { misplaced_txt(&state, &name, &d, &value).await };
                check.railway_txt_name = Some(name.clone());
                records.push(domains::txt_record(
                    &name,
                    &d,
                    value,
                    if present { "ok" } else if misplaced.is_some() { "misplaced" } else { "missing" },
                    "Required by S'Shop's hosting to route your domain.",
                ));
                if let Some(m) = misplaced {
                    check.message = m;
                }
            }
            records.extend(extra);
        }
        // 3. Routing.
        let cname = domains::lookup(&state.http, &d, "CNAME").await.unwrap_or_default();
        let points = match &check.routing_target {
            Some(t) => cname.iter().any(|c| c == t) || routing_ok == Some(true),
            None => false,
        };
        let elsewhere = !points && (!cname.is_empty() || !domains::lookup(&state.http, &d, "A").await.unwrap_or_default().is_empty());
        if let Some(t) = check.routing_target.clone() {
            records.push(domains::routing_record(&d, &t, if points { "ok" } else if elsewhere { "wrong" } else { "missing" }));
        }
        // 4. Live: the domain answers from this server over HTTPS.
        let live = domains::reaches_us(&d).await;
        status = if live {
            "active"
        } else if was_live {
            "misconfigured"
        } else if check.routing_target.is_none() {
            "verifying"
        } else if points {
            "ssl_pending"
        } else if elsewhere {
            "points_elsewhere"
        } else {
            "dns_required"
        };
        if live || check.message.is_empty() {
            check.message = domain_message(status).into();
        }
    }
    check.status = status.to_string();
    check.records = records;

    let mut tx = state.db.begin().await?;
    let current = domain_row(&mut tx, ctx.tenant_id, true).await?;
    if current.as_ref().is_none_or(|c| c.domain != d || c.token != row.token) {
        return Err(rule("The domain was changed meanwhile — check again"));
    }
    sqlx::query(
        "UPDATE website_domains SET status = $2, last_check = $3, checked_at = now(),
             verified_at = CASE WHEN $4 THEN COALESCE(verified_at, now()) ELSE verified_at END,
             active_at = CASE WHEN $2 = 'active' THEN COALESCE(active_at, now()) ELSE active_at END
         WHERE tenant_id = $1",
    )
    .bind(ctx.tenant_id)
    .bind(status)
    .bind(DbJson(&check))
    .bind(owned)
    .execute(&mut *tx)
    .await?;
    if status != row.status {
        audit::record(
            &mut tx,
            &ctx,
            Entry::new("website", "domain_status", "website", ctx.tenant_id).before(json!({ "status": row.status })).after(json!({ "domain": d, "status": status })),
        )
        .await?;
    }
    let fresh = domain_row(&mut tx, ctx.tenant_id, false).await?.ok_or(AppError::NotFound("Domain"))?;
    let (name, ..) = business(&mut tx, ctx.tenant_id).await?;
    tx.commit().await?;
    if first_verified && state.cfg.railway.is_none() {
        // Manual set-up: the platform owner attaches the domain on Railway and records the routing target.
        let text = format!(
            "{name} verified {d} for its website. Add it as a custom domain on the S'Shop service in Railway, then record the CNAME target in Platform → {name} → Website."
        );
        notify::to_platform_admins(&state, Note::new("website_domain", format!("Connect {d}"), text.clone(), "/platform"), &format!("Connect {d} for {name}"), &text).await;
    }
    Ok(Json(json!({ "ok": true, "domain": domain_view(&state, &fresh) })))
}

#[derive(Deserialize)]
struct PrimaryBody {
    primary: bool,
}

/// Roadmap 74: the business's own domain is the main address (the S'Shop address forwards to it), or the S'Shop
/// address stays the main one (the domain shows the same website; links and search engines use the S'Shop address).
async fn domain_primary(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PrimaryBody>) -> AppResult<Json<Value>> {
    ctx.require("website.domain")?;
    let mut tx = state.db.begin().await?;
    let row = domain_row(&mut tx, ctx.tenant_id, true).await?.ok_or(AppError::NotFound("Domain"))?;
    sqlx::query("UPDATE website_domains SET is_primary = $2 WHERE tenant_id = $1").bind(ctx.tenant_id).bind(b.primary).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("website", "domain_primary", "website", ctx.tenant_id).before(json!({ "primary": row.is_primary })).after(json!({ "domain": row.domain, "primary": b.primary })),
    )
    .await?;
    let fresh = domain_row(&mut tx, ctx.tenant_id, false).await?.ok_or(AppError::NotFound("Domain"))?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "domain": domain_view(&state, &fresh) })))
}

/// Disconnects the domain; the website stays available at its S'Shop address.
async fn domain_delete(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("website.domain")?;
    let mut tx = state.db.begin().await?;
    let row = domain_row(&mut tx, ctx.tenant_id, true).await?.ok_or(AppError::NotFound("Domain"))?;
    sqlx::query("DELETE FROM website_domains WHERE tenant_id = $1").bind(ctx.tenant_id).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("website", "domain_remove", "website", ctx.tenant_id).before(json!({ "domain": row.domain, "status": row.status })),
    )
    .await?;
    tx.commit().await?;
    if let (Some(id), Some(api)) = (row.last_check.and_then(|c| c.0.railway_id), state.cfg.railway.as_ref()) {
        if let Err(e) = domains::railway_detach(&state.http, api, &id).await {
            tracing::warn!(error = %e, "could not remove the custom domain from Railway");
        }
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RoutingBody {
    routing_target: String,
}

/// Manual set-up (no Railway API token): the platform owner records the CNAME target Railway gave for the domain.
async fn platform_domain(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RoutingBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let target = b.routing_target.trim().trim_end_matches('.').to_lowercase();
    if !target.ends_with(".railway.app") || target.contains(['/', ' ', ':']) {
        return Err(bad("Enter the CNAME value Railway shows, like abc123.up.railway.app"));
    }
    let mut tx = state.db.begin().await?;
    let row = domain_row(&mut tx, id, true).await?.ok_or(AppError::NotFound("Domain"))?;
    let mut check = row.last_check.map(|c| c.0).unwrap_or_default();
    check.routing_target = Some(target.clone());
    check.message = String::new();
    sqlx::query("UPDATE website_domains SET last_check = $2 WHERE tenant_id = $1").bind(id).bind(DbJson(&check)).execute(&mut *tx).await?;
    let after = json!({ "domain": row.domain, "routing_target": target });
    record_platform(&mut tx, &ctx, id, || Entry::new("website", "domain_routing", "website", id).after(after.clone())).await?;
    tx.commit().await?;
    notify::to_permission(
        &state,
        id,
        None,
        "website.domain",
        Note::new("website_domain", format!("{} is ready to connect", row.domain), "Add the routing record in Website → Domain, then check again".to_string(), "/settings/website"),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Analytics (roadmap 57) ─────────────────────────────

#[derive(Deserialize)]
struct AnalyticsQuery {
    /// today | 7d | 30d | 90d | custom
    #[serde(default)]
    period: Option<String>,
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
}

/// Website performance for a period (business time zone). Orders and completions come from the orders themselves
/// (source = website), so analytics always reconcile with Orders; sales value is shown only to people who may see reports.
async fn analytics(State(state): State<AppState>, ctx: Ctx, Query(q): Query<AnalyticsQuery>) -> AppResult<Json<Value>> {
    ctx.require("website.analytics")?;
    let today = crate::util::today_in(ctx.tz);
    let period = q.period.unwrap_or_else(|| "30d".into());
    let (from, to) = match period.as_str() {
        "today" => (today, today),
        "7d" => (today - chrono::Duration::days(6), today),
        "30d" => (today - chrono::Duration::days(29), today),
        "90d" => (today - chrono::Duration::days(89), today),
        "custom" => match (q.from, q.to) {
            (Some(f), Some(t)) if f <= t && (t - f).num_days() <= 366 => (f, t.min(today)),
            _ => return Err(bad("Choose a range of up to a year")),
        },
        _ => return Err(bad("Choose today, 7d, 30d, 90d or custom")),
    };
    let (start, end) = crate::util::local_range(from, to, ctx.tz);
    let tz = ctx.tz.name();
    let mut conn = state.db.acquire().await?;

    let (visitors, visits, views, carts, starts): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(DISTINCT visitor) FILTER (WHERE kind = 'visit' AND visitor <> ''),
                COUNT(*) FILTER (WHERE kind = 'visit'),
                COUNT(*) FILTER (WHERE kind = 'product_view'),
                COUNT(*) FILTER (WHERE kind = 'add_to_cart'),
                COUNT(*) FILTER (WHERE kind = 'order_start')
         FROM website_events WHERE tenant_id = $1 AND at >= $2 AND at < $3",
    )
    .bind(ctx.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *conn)
    .await?;
    let (orders, completed, value): (i64, i64, rust_decimal::Decimal) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'completed'), COALESCE(SUM(total) FILTER (WHERE status = 'completed'), 0)
         FROM orders WHERE tenant_id = $1 AND source = 'website' AND created_at >= $2 AND created_at < $3",
    )
    .bind(ctx.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *conn)
    .await?;
    let daily: Vec<(chrono::NaiveDate, i64, i64)> = sqlx::query_as(
        "WITH days AS (SELECT generate_series($4::date, $5::date, interval '1 day')::date AS day)
         SELECT d.day,
                (SELECT COUNT(DISTINCT NULLIF(visitor, '')) FROM website_events e
                  WHERE e.tenant_id = $1 AND e.kind = 'visit' AND e.at >= $2 AND e.at < $3 AND (e.at AT TIME ZONE $6)::date = d.day),
                (SELECT COUNT(*) FROM orders o
                  WHERE o.tenant_id = $1 AND o.source = 'website' AND o.created_at >= $2 AND o.created_at < $3 AND (o.created_at AT TIME ZONE $6)::date = d.day)
         FROM days d ORDER BY d.day",
    )
    .bind(ctx.tenant_id)
    .bind(start)
    .bind(end)
    .bind(from)
    .bind(to)
    .bind(tz)
    .fetch_all(&mut *conn)
    .await?;
    let top: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(
        "SELECT p.id, p.name, COUNT(*) FILTER (WHERE e.kind = 'product_view'), COUNT(*) FILTER (WHERE e.kind = 'add_to_cart')
         FROM website_events e JOIN products p ON p.id = e.product_id AND p.tenant_id = e.tenant_id
         WHERE e.tenant_id = $1 AND e.at >= $2 AND e.at < $3 AND e.kind IN ('product_view', 'add_to_cart')
         GROUP BY p.id, p.name ORDER BY 3 DESC, 4 DESC, p.name LIMIT 10",
    )
    .bind(ctx.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_all(&mut *conn)
    .await?;
    let pct = |n: i64, d: i64| if d > 0 { ((n as f64) * 1000.0 / d as f64).round() / 10.0 } else { 0.0 };
    Ok(Json(json!({
        "period": period,
        "from": from,
        "to": to,
        "visitors": visitors,
        "visits": visits,
        "product_views": views,
        "add_to_carts": carts,
        "order_starts": starts,
        "orders": orders,
        "completed_orders": completed,
        "conversion": pct(orders, visitors),
        "completion": pct(completed, orders),
        "sales_value": if ctx.can("reports.view") { Some(value) } else { None },
        "daily": daily.into_iter().map(|(day, v, o)| json!({ "day": day, "visitors": v, "orders": o })).collect::<Vec<_>>(),
        "top_products": top.into_iter().map(|(id, name, v, c)| json!({ "id": id, "name": name, "views": v, "add_to_carts": c })).collect::<Vec<_>>(),
    })))
}

// ───────────────────────────── Catalogue for the editor ─────────────────────────────

/// The business's products and categories as the website editor needs them (names, public price, photos) — so a
/// website editor works without any stock, sales or product-management permission.
async fn catalogue(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require_any(&["website.products", "website.categories", "website.content", "website.view"])?;
    let products: Vec<(Uuid, String, String, Option<Uuid>, Option<String>, rust_decimal::Decimal, bool, bool, Vec<Uuid>)> = sqlx::query_as(
        "SELECT p.id, p.name, p.code, p.category_id, c.name, p.marked_price, p.is_active, p.available_for_orders,
                ARRAY(SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order)
         FROM products p LEFT JOIN categories c ON c.id = p.category_id
         WHERE p.tenant_id = $1 ORDER BY p.is_active DESC, p.name",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let categories: Vec<(Uuid, String, bool)> = sqlx::query_as("SELECT id, name, is_active FROM categories WHERE tenant_id = $1 ORDER BY name")
        .bind(ctx.tenant_id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({
        "products": products.into_iter().map(|(id, name, code, category_id, category_name, price, active, orderable, photos)| json!({
            "id": id, "name": name, "code": code, "category_id": category_id, "category_name": category_name, "price": price,
            "is_active": active, "available_for_orders": orderable, "photos": photos,
        })).collect::<Vec<_>>(),
        "categories": categories.into_iter().map(|(id, name, active)| json!({ "id": id, "name": name, "is_active": active })).collect::<Vec<_>>(),
    })))
}

/// The website service as the platform owner sees it on a business's page (status, request, publication, domain).
pub async fn platform_summary(state: &AppState, tenant_id: Uuid) -> AppResult<Value> {
    let mut conn = state.db.acquire().await?;
    let Some(r) = load(&mut conn, tenant_id, false).await? else { return Ok(json!({ "status": "none" })) };
    let domain = domain_row(&mut conn, tenant_id, false).await?;
    Ok(json!({
        "status": r.status, "status_reason": r.status_reason, "request_message": r.request_message, "requested_at": r.requested_at,
        "activated_at": r.activated_at, "billing_suspended": r.billing_suspended, "version": r.version, "published_at": r.published_at,
        "domain": domain.map(|d| {
            let check = d.last_check.map(|c| c.0).unwrap_or_default();
            json!({ "domain": d.domain, "status": d.status, "verified_at": d.verified_at, "routing_target": check.routing_target, "automatic": state.cfg.railway.is_some() })
        }),
    }))
}
