//! Holiday & promotional campaigns on business websites (roadmap 84–86).
//!
//! A campaign: greeting and promotion copy, an occasion design (template, colours, decorations, animation, height,
//! button), featured products (automatic best sellers from real sales, or a manual list), placement (pages, display
//! style, dismissible) and a schedule. Status is derived on every read from `published`, the start / end times and
//! `archived_at` — draft · scheduled · live · expired · archived — so a campaign appears and disappears on time with
//! no background job. The website shows at most one campaign at a time (highest priority, then the latest start).
//!
//! Off by default: the platform (feature on/off, limits) and the business (*Holiday & promotional banners*) must both
//! allow it. Editing needs `website.content`; switching the feature, publishing, unpublishing and archiving need
//! `website.publish`. Everything is audited. Insights come from the existing anonymous website analytics.

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::access::require_platform_admin;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/website/campaigns", get(list).post(create))
        .route("/website/campaigns/settings", put(set_enabled))
        .route("/website/campaigns/{id}", put(update).delete(remove))
        .route("/website/campaigns/{id}/duplicate", post(duplicate))
        .route("/website/campaigns/{id}/{action}", post(lifecycle))
        .route("/platform/campaigns", get(platform_get).put(platform_put))
}

pub const OCCASIONS: [&str; 15] = [
    "christmas", "new_year", "valentines", "easter", "eid", "diwali", "mothers_day", "fathers_day", "black_friday", "cyber_monday",
    "national", "anniversary", "appreciation", "back_to_school", "custom",
];
const TEMPLATES: [&str; 9] = ["christmas", "new_year", "valentines", "easter", "eid", "celebration", "sale", "elegant", "minimal"];
const PAGES: [&str; 4] = ["home", "products", "categories", "all"];
const DISPLAYS: [&str; 4] = ["hero", "compact", "strip", "card"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Design {
    pub template: String,
    pub headline: String,
    pub message: String,
    /// Optional promotional line ("Up to 20% off selected gifts").
    pub promo: String,
    pub cta_label: String,
    /// A website page, /products/…, /categories/… or an https:// / tel: / mailto: link.
    pub cta_target: String,
    /// left | center
    pub align: String,
    /// md | lg | xl
    pub headline_size: String,
    /// "" (template default) or #RRGGBB.
    pub text_color: String,
    pub background: String,
    /// An image from the business's media library.
    pub background_image: Option<Uuid>,
    pub decorations: bool,
    /// off | subtle (always off for visitors who ask for reduced motion)
    pub animation: String,
    /// compact | standard | tall
    pub height: String,
    /// solid | outline
    pub button_style: String,
}
impl Default for Design {
    fn default() -> Self {
        Self {
            template: "celebration".into(),
            headline: String::new(),
            message: String::new(),
            promo: String::new(),
            cta_label: "Shop now".into(),
            cta_target: "products".into(),
            align: "center".into(),
            headline_size: "lg".into(),
            text_color: String::new(),
            background: String::new(),
            background_image: None,
            decorations: true,
            animation: "subtle".into(),
            height: "standard".into(),
            button_style: "solid".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Products {
    /// auto (best sellers from completed sales) | manual | none
    pub mode: String,
    pub product_ids: Vec<Uuid>,
    /// Best sellers over the last 30 / 90 / 365 days, or all time; an empty period falls back to all time.
    pub period_days: u16,
    pub category_id: Option<Uuid>,
    pub limit: u8,
}
impl Default for Products {
    fn default() -> Self {
        Self { mode: "auto".into(), product_ids: Vec::new(), period_days: 90, category_id: None, limit: 5 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Placement {
    pub pages: Vec<String>,
    pub display: String,
    pub dismissible: bool,
}
impl Default for Placement {
    fn default() -> Self {
        Self { pages: vec!["home".into(), "products".into()], display: "hero".into(), dismissible: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PlatformCampaigns {
    pub enabled: bool,
    /// Campaigns a business may keep (not archived).
    pub max_campaigns: u16,
    /// Featured products per campaign.
    pub max_products: u8,
}
impl Default for PlatformCampaigns {
    fn default() -> Self {
        Self { enabled: true, max_campaigns: 20, max_products: 12 }
    }
}

pub async fn platform_rules(db: &sqlx::PgPool) -> AppResult<PlatformCampaigns> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = 'campaigns'").fetch_optional(db).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}

#[derive(sqlx::FromRow, Clone)]
struct Row {
    id: Uuid,
    name: String,
    occasion: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    published: bool,
    published_at: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
    priority: i32,
    design: sqlx::types::Json<Design>,
    products: sqlx::types::Json<Products>,
    placement: sqlx::types::Json<Placement>,
    version: i32,
    updated_at: DateTime<Utc>,
}
const COLS: &str = "id, name, occasion, starts_at, ends_at, published, published_at, archived_at, priority, design, products, placement, version, updated_at";

fn status(r: &Row, now: DateTime<Utc>) -> &'static str {
    if r.archived_at.is_some() {
        "archived"
    } else if !r.published {
        "draft"
    } else if now < r.starts_at {
        "scheduled"
    } else if now >= r.ends_at {
        "expired"
    } else {
        "live"
    }
}

async fn tz_of(state: &AppState, tenant: Uuid) -> AppResult<chrono_tz::Tz> {
    let tz: String = sqlx::query_scalar("SELECT timezone FROM tenants WHERE id = $1").bind(tenant).fetch_one(&state.db).await?;
    Ok(crate::util::parse_tz(&tz))
}

/// "2026-12-20T08:00" in the business's timezone → an instant.
fn parse_local(v: &str, tz: chrono_tz::Tz, what: &str) -> AppResult<DateTime<Utc>> {
    let n = NaiveDateTime::parse_from_str(v.trim(), "%Y-%m-%dT%H:%M").map_err(|_| bad(format!("{what}: choose a date and time")))?;
    tz.from_local_datetime(&n).earliest().map(|d| d.with_timezone(&Utc)).ok_or_else(|| bad(format!("{what}: that time does not exist in your timezone")))
}

fn local(d: DateTime<Utc>, tz: chrono_tz::Tz) -> String {
    d.with_timezone(&tz).format("%Y-%m-%dT%H:%M").to_string()
}

fn view(r: &Row, tz: chrono_tz::Tz, now: DateTime<Utc>) -> Value {
    json!({
        "id": r.id, "name": r.name, "occasion": r.occasion, "status": status(r, now), "priority": r.priority,
        "starts_local": local(r.starts_at, tz), "ends_local": local(r.ends_at, tz), "starts_at": r.starts_at, "ends_at": r.ends_at,
        "published_at": r.published_at, "archived_at": r.archived_at, "design": r.design.0, "products": r.products.0, "placement": r.placement.0,
        "version": r.version, "updated_at": r.updated_at,
    })
}

// ── Validation ──────────────────────────────────────────────────────────────────────────────────────────────────────

fn hex_ok(v: &str) -> bool {
    v.is_empty() || (v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit()))
}

fn len(v: &str, max: usize, what: &str) -> AppResult<()> {
    if v.chars().count() > max {
        return Err(bad(format!("{what} is too long (max {max} characters)")));
    }
    Ok(())
}

async fn validate(state: &AppState, tenant: Uuid, b: &CampaignBody, max_products: u8) -> AppResult<()> {
    let name = b.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(bad("Give the campaign a name (up to 80 characters)"));
    }
    if !OCCASIONS.contains(&b.occasion.as_str()) {
        return Err(bad("Choose an occasion"));
    }
    if !(-100..=100).contains(&b.priority) {
        return Err(bad("Priority must be between -100 and 100"));
    }
    let d = &b.design;
    if !TEMPLATES.contains(&d.template.as_str()) {
        return Err(bad("Choose a banner design"));
    }
    if d.headline.trim().is_empty() {
        return Err(bad("Write the greeting headline"));
    }
    len(&d.headline, 90, "Headline")?;
    len(&d.message, 260, "Message")?;
    len(&d.promo, 120, "Promotional line")?;
    len(&d.cta_label, 30, "Button text")?;
    if !crate::website::link_ok(&d.cta_target) {
        return Err(bad("Button: the link must be a page (products, about, contact …) or start with https://, tel: or mailto:"));
    }
    for (v, what) in [(d.align.as_str(), ["left", "center"].as_slice()), (d.headline_size.as_str(), &["md", "lg", "xl"]), (d.animation.as_str(), &["off", "subtle"]),
        (d.height.as_str(), &["compact", "standard", "tall"]), (d.button_style.as_str(), &["solid", "outline"])]
    {
        if !what.contains(&v) {
            return Err(bad("Choose one of the offered design options"));
        }
    }
    if !hex_ok(&d.text_color) || !hex_ok(&d.background) {
        return Err(bad("Colours must look like #A1B2C3"));
    }
    if let Some(m) = d.background_image {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_media WHERE id = $1 AND tenant_id = $2 AND NOT archived)").bind(m).bind(tenant).fetch_one(&state.db).await?;
        if !ok {
            return Err(bad("Choose a background image from your media library"));
        }
    }
    let p = &b.products;
    if !["auto", "manual", "none"].contains(&p.mode.as_str()) {
        return Err(bad("Choose how products are featured"));
    }
    let max = max_products.max(1);
    if p.limit == 0 || p.limit > max {
        return Err(bad(format!("Feature 1–{max} products")));
    }
    if ![30, 90, 365, 0].contains(&p.period_days) {
        return Err(bad("Best sellers: last 30, 90 or 365 days, or all time"));
    }
    if p.product_ids.len() > max as usize {
        return Err(bad(format!("Feature at most {max} products")));
    }
    if p.mode == "manual" && p.product_ids.is_empty() {
        return Err(bad("Choose the products to feature"));
    }
    if !p.product_ids.is_empty() {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE tenant_id = $1 AND id = ANY($2)").bind(tenant).bind(&p.product_ids).fetch_one(&state.db).await?;
        if n != p.product_ids.len() as i64 {
            return Err(bad("Only this business's products can be featured"));
        }
    }
    if let Some(c) = p.category_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM categories WHERE id = $1 AND tenant_id = $2)").bind(c).bind(tenant).fetch_one(&state.db).await?;
        if !ok {
            return Err(bad("Unknown category"));
        }
    }
    let pl = &b.placement;
    if pl.pages.is_empty() || pl.pages.iter().any(|x| !PAGES.contains(&x.as_str())) {
        return Err(bad("Choose where the banner appears"));
    }
    if !DISPLAYS.contains(&pl.display.as_str()) {
        return Err(bad("Choose a display style"));
    }
    Ok(())
}

/// The website service must be active (the campaign belongs to it).
async fn ensure_site(state: &AppState, tenant: Uuid) -> AppResult<()> {
    let mut conn = state.db.acquire().await?;
    super::website::ensure_active(&super::website::load(&mut conn, tenant, false).await?)?;
    Ok(())
}

// ── Management ──────────────────────────────────────────────────────────────────────────────────────────────────────

fn can_view(ctx: &Ctx) -> bool {
    ctx.can("website.content") || ctx.can("website.publish") || ctx.can("website.analytics") || ctx.can("settings.integrations")
}

async fn list(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    if !can_view(&ctx) {
        return Err(AppError::Forbidden("You do not have permission for this action".into()));
    }
    ensure_site(&state, ctx.tenant_id).await?;
    let tz = tz_of(&state, ctx.tenant_id).await?;
    let rows: Vec<Row> = sqlx::query_as(&format!("SELECT {COLS} FROM website_campaigns WHERE tenant_id = $1 ORDER BY archived_at IS NOT NULL, starts_at DESC"))
        .bind(ctx.tenant_id)
        .fetch_all(&state.db)
        .await?;
    let enabled: bool = sqlx::query_scalar("SELECT campaigns_enabled FROM websites WHERE tenant_id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let now = Utc::now();
    let mut items = Vec::new();
    for r in &rows {
        let mut v = view(r, tz, now);
        v["insights"] = insights(&state, ctx.tenant_id, r.id).await?;
        items.push(v);
    }
    let live = active_row(&state, ctx.tenant_id, None).await?.map(|r| r.id);
    Ok(Json(json!({ "enabled": enabled, "platform": platform_rules(&state.db).await?, "items": items, "showing": live, "timezone": tz.name() })))
}

#[derive(Deserialize)]
struct EnabledBody {
    enabled: bool,
}

async fn set_enabled(State(state): State<AppState>, ctx: Ctx, Json(b): Json<EnabledBody>) -> AppResult<Json<Value>> {
    ctx.require("website.publish")?;
    ensure_site(&state, ctx.tenant_id).await?;
    if b.enabled && !platform_rules(&state.db).await?.enabled {
        return Err(refused("Not available", "Holiday & promotional banners are switched off on S'Shop"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE websites SET campaigns_enabled = $2 WHERE tenant_id = $1").bind(ctx.tenant_id).bind(b.enabled).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "campaigns_enabled", "website", ctx.tenant_id).after(json!({ "enabled": b.enabled }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "enabled": b.enabled })))
}

#[derive(Deserialize)]
struct CampaignBody {
    name: String,
    occasion: String,
    starts_local: String,
    ends_local: String,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    design: Design,
    #[serde(default)]
    products: Products,
    #[serde(default)]
    placement: Placement,
    /// Optimistic concurrency: the version the editor started from.
    version: Option<i32>,
}

async fn window(state: &AppState, tenant: Uuid, b: &CampaignBody) -> AppResult<(DateTime<Utc>, DateTime<Utc>)> {
    let tz = tz_of(state, tenant).await?;
    let s = parse_local(&b.starts_local, tz, "Start")?;
    let e = parse_local(&b.ends_local, tz, "End")?;
    if e <= s {
        return Err(bad("The end must be after the start"));
    }
    if e - s > Duration::days(366) {
        return Err(bad("A campaign can run for at most a year"));
    }
    Ok((s, e))
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CampaignBody>) -> AppResult<Json<Value>> {
    ctx.require("website.content")?;
    ensure_site(&state, ctx.tenant_id).await?;
    let rules = platform_rules(&state.db).await?;
    validate(&state, ctx.tenant_id, &b, rules.max_products).await?;
    let (s, e) = window(&state, ctx.tenant_id, &b).await?;
    let mut tx = state.db.begin().await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM website_campaigns WHERE tenant_id = $1 AND archived_at IS NULL").bind(ctx.tenant_id).fetch_one(&mut *tx).await?;
    if n >= rules.max_campaigns as i64 {
        return Err(rule(format!("Up to {} campaigns — archive one first", rules.max_campaigns)));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO website_campaigns (tenant_id, name, occasion, starts_at, ends_at, priority, design, products, placement, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(&b.occasion)
    .bind(s)
    .bind(e)
    .bind(b.priority)
    .bind(sqlx::types::Json(&b.design))
    .bind(sqlx::types::Json(&b.products))
    .bind(sqlx::types::Json(&b.placement))
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "campaign_created", "campaign", id).after(json!({ "name": b.name.trim(), "occasion": b.occasion }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn load_row(conn: &mut sqlx::PgConnection, tenant: Uuid, id: Uuid, lock: bool) -> AppResult<Row> {
    sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM website_campaigns WHERE id = $1 AND tenant_id = $2 {}", if lock { "FOR UPDATE" } else { "" }))
        .bind(id)
        .bind(tenant)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Campaign"))
}

/// Edit (a live campaign stays live with the new content; editing never publishes).
async fn update(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<CampaignBody>) -> AppResult<Json<Value>> {
    ctx.require("website.content")?;
    ensure_site(&state, ctx.tenant_id).await?;
    validate(&state, ctx.tenant_id, &b, platform_rules(&state.db).await?.max_products).await?;
    let (s, e) = window(&state, ctx.tenant_id, &b).await?;
    let mut tx = state.db.begin().await?;
    let before = load_row(&mut tx, ctx.tenant_id, id, true).await?;
    if before.archived_at.is_some() {
        return Err(rule("Restore is not possible for archived campaigns — duplicate it instead"));
    }
    if b.version.is_some_and(|v| v != before.version) {
        return Err(refused("Changed meanwhile", "Someone else saved this campaign in the meantime — reload it to see their changes"));
    }
    if before.published && !ctx.can("website.publish") {
        return Err(AppError::Forbidden("Only people who can publish may change a published campaign".into()));
    }
    sqlx::query(
        "UPDATE website_campaigns SET name = $3, occasion = $4, starts_at = $5, ends_at = $6, priority = $7, design = $8, products = $9, placement = $10,
                version = version + 1, updated_at = now() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(&b.occasion)
    .bind(s)
    .bind(e)
    .bind(b.priority)
    .bind(sqlx::types::Json(&b.design))
    .bind(sqlx::types::Json(&b.products))
    .bind(sqlx::types::Json(&b.placement))
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("website", "campaign_updated", "campaign", id).before(json!({ "name": before.name, "starts_at": before.starts_at, "ends_at": before.ends_at })).after(json!({ "name": b.name.trim(), "starts_at": s, "ends_at": e })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn duplicate(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("website.content")?;
    ensure_site(&state, ctx.tenant_id).await?;
    let rules = platform_rules(&state.db).await?;
    let mut tx = state.db.begin().await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM website_campaigns WHERE tenant_id = $1 AND archived_at IS NULL").bind(ctx.tenant_id).fetch_one(&mut *tx).await?;
    if n >= rules.max_campaigns as i64 {
        return Err(rule(format!("Up to {} campaigns — archive one first", rules.max_campaigns)));
    }
    let src = load_row(&mut tx, ctx.tenant_id, id, false).await?;
    // A copy is a draft; its dates move to the future when the original's have passed.
    let (s, e) = if src.ends_at <= Utc::now() {
        let len = src.ends_at - src.starts_at;
        let s = Utc::now() + Duration::days(7);
        (s, s + len)
    } else {
        (src.starts_at, src.ends_at)
    };
    let new_id: Uuid = sqlx::query_scalar(
        "INSERT INTO website_campaigns (tenant_id, name, occasion, starts_at, ends_at, priority, design, products, placement, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(format!("{} (copy)", src.name).chars().take(80).collect::<String>())
    .bind(&src.occasion)
    .bind(s)
    .bind(e)
    .bind(src.priority)
    .bind(&src.design)
    .bind(&src.products)
    .bind(&src.placement)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "campaign_duplicated", "campaign", new_id).after(json!({ "from": id }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "id": new_id })))
}

/// publish | unpublish | archive
async fn lifecycle(State(state): State<AppState>, ctx: Ctx, Path((id, action)): Path<(Uuid, String)>) -> AppResult<Json<Value>> {
    ctx.require("website.publish")?;
    ensure_site(&state, ctx.tenant_id).await?;
    let mut tx = state.db.begin().await?;
    let r = load_row(&mut tx, ctx.tenant_id, id, true).await?;
    let now = Utc::now();
    let (sql, audit_action) = match action.as_str() {
        "publish" => {
            if r.archived_at.is_some() {
                return Err(rule("An archived campaign cannot be published — duplicate it instead"));
            }
            if r.ends_at <= now {
                return Err(rule("This campaign has ended — change its dates first"));
            }
            if r.published {
                return Err(rule("Already published"));
            }
            ("UPDATE website_campaigns SET published = true, published_at = now(), published_by = $3 WHERE id = $1 AND tenant_id = $2", "campaign_published")
        }
        "unpublish" => {
            if !r.published {
                return Err(rule("Not published"));
            }
            ("UPDATE website_campaigns SET published = false WHERE id = $1 AND tenant_id = $2 AND $3::uuid IS NOT NULL", "campaign_unpublished")
        }
        "archive" => {
            if r.archived_at.is_some() {
                return Err(rule("Already archived"));
            }
            ("UPDATE website_campaigns SET archived_at = now(), published = false WHERE id = $1 AND tenant_id = $2 AND $3::uuid IS NOT NULL", "campaign_archived")
        }
        _ => return Err(AppError::NotFound("Action")),
    };
    sqlx::query(sql).bind(id).bind(ctx.tenant_id).bind(ctx.user_id).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("website", audit_action, "campaign", id).after(json!({ "name": r.name, "status_before": status(&r, now) }))).await?;
    tx.commit().await?;
    let tz = tz_of(&state, ctx.tenant_id).await?;
    let mut conn = state.db.acquire().await?;
    let fresh = load_row(&mut conn, ctx.tenant_id, id, false).await?;
    Ok(Json(json!({ "ok": true, "campaign": view(&fresh, tz, Utc::now()) })))
}

/// Only drafts that were never published can be deleted; others are archived (their insights are kept).
async fn remove(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("website.content")?;
    let mut tx = state.db.begin().await?;
    let r = load_row(&mut tx, ctx.tenant_id, id, true).await?;
    if r.published || r.published_at.is_some() {
        return Err(rule("A campaign that has been published is archived, not deleted"));
    }
    sqlx::query("DELETE FROM website_campaigns WHERE id = $1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "campaign_deleted", "campaign", id).after(json!({ "name": r.name }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Insights ────────────────────────────────────────────────────────────────────────────────────────────────────────

/// Views and clicks from the website's anonymous analytics; an order is attributed when the same visitor clicked a
/// product or the button of this campaign within the 7 days before ordering (orders cancelled later are excluded).
async fn insights(state: &AppState, tenant: Uuid, id: Uuid) -> AppResult<Value> {
    let (views, viewers, product_clicks, cta_clicks): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE kind = 'campaign_view'), COUNT(DISTINCT visitor) FILTER (WHERE kind = 'campaign_view' AND visitor <> ''),
                COUNT(*) FILTER (WHERE kind = 'campaign_product'), COUNT(*) FILTER (WHERE kind = 'campaign_cta')
         FROM website_events WHERE tenant_id = $1 AND campaign_id = $2",
    )
    .bind(tenant)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let (orders, sales): (i64, Option<Decimal>) = sqlx::query_as(
        "SELECT COUNT(*), SUM(o.total) FROM orders o
         WHERE o.tenant_id = $1 AND o.status <> 'cancelled' AND o.id IN (
             SELECT oc.order_id FROM website_events oc
             WHERE oc.tenant_id = $1 AND oc.kind = 'order_complete' AND oc.visitor <> '' AND oc.order_id IS NOT NULL
               AND EXISTS (SELECT 1 FROM website_events c WHERE c.tenant_id = $1 AND c.campaign_id = $2 AND c.kind IN ('campaign_product', 'campaign_cta')
                           AND c.visitor = oc.visitor AND c.at <= oc.at AND c.at > oc.at - interval '7 days'))",
    )
    .bind(tenant)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(json!({ "views": views, "viewers": viewers, "product_clicks": product_clicks, "cta_clicks": cta_clicks, "orders": orders, "sales": sales.unwrap_or_default() }))
}

// ── Public website ──────────────────────────────────────────────────────────────────────────────────────────────────

/// The campaign the website shows now: both switches on, published, within its window, not archived — highest
/// priority, then the latest start. A staff preview may ask for a specific campaign whatever its status.
async fn active_row(state: &AppState, tenant: Uuid, preview: Option<Uuid>) -> AppResult<Option<Row>> {
    if let Some(id) = preview {
        let mut conn = state.db.acquire().await?;
        return Ok(load_row(&mut conn, tenant, id, false).await.ok());
    }
    if !platform_rules(&state.db).await?.enabled {
        return Ok(None);
    }
    Ok(sqlx::query_as(&format!(
        "SELECT {COLS} FROM website_campaigns c WHERE c.tenant_id = $1 AND c.published AND c.archived_at IS NULL AND c.starts_at <= now() AND c.ends_at > now()
           AND EXISTS (SELECT 1 FROM websites w WHERE w.tenant_id = $1 AND w.campaigns_enabled)
         ORDER BY c.priority DESC, c.starts_at DESC LIMIT 1"
    ))
    .bind(tenant)
    .fetch_optional(&state.db)
    .await?)
}

/// Best sellers from completed sales (net of returns), over the period, else all time — never invented.
async fn best_sellers(state: &AppState, tenant: Uuid, days: u16, category: Option<Uuid>) -> AppResult<Vec<Uuid>> {
    let q = "SELECT si.product_id FROM sale_items si JOIN sales s ON s.id = si.sale_id JOIN products p ON p.id = si.product_id
             WHERE s.tenant_id = $1 AND s.status <> 'cancelled' AND ($2::int = 0 OR s.created_at > now() - make_interval(days => $2::int))
               AND ($3::uuid IS NULL OR p.category_id = $3)
             GROUP BY si.product_id HAVING SUM(si.quantity - si.returned_qty) > 0 ORDER BY SUM(si.quantity - si.returned_qty) DESC LIMIT 60";
    let ids: Vec<Uuid> = sqlx::query_scalar(q).bind(tenant).bind(days as i32).bind(category).fetch_all(&state.db).await?;
    if ids.is_empty() && days != 0 {
        return Ok(sqlx::query_scalar(q).bind(tenant).bind(0).bind(category).fetch_all(&state.db).await?);
    }
    Ok(ids)
}

/// The campaign for the public website, with its featured products resolved from what the website publishes
/// (prices, stock and ordering rules exactly as everywhere else on the website).
pub async fn public_campaign(state: &AppState, s: &super::site::Site, preview: Option<Uuid>) -> AppResult<Value> {
    let Some(r) = active_row(state, s.tenant, if s.preview { preview } else { None }).await? else { return Ok(Value::Null) };
    let published = super::site::published(state, s).await?;
    let p = &r.products.0;
    let limit = p.limit.max(1) as usize;
    let ids: Vec<Uuid> = match p.mode.as_str() {
        "manual" => p.product_ids.clone(),
        "auto" => best_sellers(state, s.tenant, p.period_days, p.category_id).await?,
        _ => Vec::new(),
    };
    let products: Vec<&super::site::PubProduct> = ids.iter().filter_map(|id| published.iter().find(|x| x.id == *id)).take(limit).collect();
    Ok(json!({
        "id": r.id, "version": r.version, "occasion": r.occasion, "design": r.design.0, "placement": r.placement.0,
        "products": products, "ends_at": r.ends_at,
    }))
}

/// A campaign id from a public event, only when it belongs to this business.
pub async fn owned(state: &AppState, tenant: Uuid, id: Option<Uuid>) -> AppResult<Option<Uuid>> {
    match id {
        Some(id) => Ok(sqlx::query_scalar("SELECT id FROM website_campaigns WHERE id = $1 AND tenant_id = $2").bind(id).bind(tenant).fetch_optional(&state.db).await?),
        None => Ok(None),
    }
}

/// Media still used by a campaign that is not archived cannot be deleted from the library.
pub async fn uses_media(conn: &mut sqlx::PgConnection, tenant: Uuid, media: Uuid) -> AppResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_campaigns WHERE tenant_id = $1 AND archived_at IS NULL AND design->>'background_image' = $2::text)")
        .bind(tenant)
        .bind(media)
        .fetch_one(&mut *conn)
        .await?)
}

// ── Platform ────────────────────────────────────────────────────────────────────────────────────────────────────────

async fn platform_get(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<PlatformCampaigns>> {
    require_platform_admin(&state, &ctx).await?;
    Ok(Json(platform_rules(&state.db).await?))
}

async fn platform_put(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PlatformCampaigns>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if !(1..=100).contains(&b.max_campaigns) || !(1..=24).contains(&b.max_products) {
        return Err(bad("Campaigns per business 1–100, featured products 1–24"));
    }
    let before = platform_rules(&state.db).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO platform_settings (key, value, updated_by) VALUES ('campaigns', $1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(json!(b))
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("platform", "campaign_settings", "platform_settings", ctx.tenant_id).before(json!(before)).after(json!(b))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_times_follow_the_business_timezone() {
        let tz: chrono_tz::Tz = "Africa/Nairobi".parse().unwrap();
        let at = parse_local("2026-12-24T18:00", tz, "Start").unwrap();
        assert_eq!(at.to_rfc3339(), "2026-12-24T15:00:00+00:00");
        assert_eq!(local(at, tz), "2026-12-24T18:00");
        assert!(parse_local("24/12/2026", tz, "Start").is_err());
    }

    #[test]
    fn colours_validated() {
        assert!(hex_ok("") && hex_ok("#A1b2C3"));
        assert!(!hex_ok("red") && !hex_ok("#12345") && !hex_ok("#12345g"));
    }
}
