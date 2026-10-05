//! Business profile, settings, branches, users, roles & workflow configuration.

use axum::body::Bytes;
use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::{hash_pin, validate_pin, Ctx};
use crate::error::{bad, rule, AppError, AppResult};
use crate::perms;
use crate::settings::{Hours, TenantSettings};
use crate::state::AppState;
use crate::util::slugify;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings).put(put_settings))
        .route("/settings/profile", put(put_profile))
        .route("/settings/logo", post(upload_logo))
        .route("/settings/workflows/{action}", put(put_workflow))
        .route("/public/{slug}/logo", get(logo))
        .route("/branches", get(list_branches).post(create_branch))
        .route("/branches/{id}", put(update_branch))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{id}", put(update_user))
        .route("/users/{id}/reset-pin", post(reset_user_pin))
        .route("/roles", get(list_roles).post(create_role))
        .route("/roles/{id}", put(update_role))
        .route("/permissions", get(permission_catalogue))
}

// ───────────────────────────── Settings ─────────────────────────────

async fn get_settings(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let (name, slug, tagline, phone, email, address, currency, timezone, has_logo, settings): (
        String, String, String, String, String, String, String, String, bool, Value,
    ) = sqlx::query_as(
        "SELECT name, slug, tagline, phone, email, address, currency, timezone, logo IS NOT NULL, settings FROM tenants WHERE id = $1",
    )
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let settings: TenantSettings = serde_json::from_value(settings).unwrap_or_default();

    let mut conn = state.db.acquire().await?;
    let workflows = workflow::rules(&mut conn, ctx.tenant_id).await?;
    let actions: Vec<Value> = workflow::ACTIONS
        .iter()
        .map(|a| {
            let rule = workflows.iter().find(|w| w.action == a.key);
            json!({
                "action": a.key, "label": a.label, "uses_amount": a.uses_amount, "uses_category": a.uses_category,
                "enabled": rule.map(|r| r.enabled).unwrap_or(false),
                "levels": rule.map(|r| json!(r.levels)).unwrap_or_else(|| json!([{ "approver_type": "admin" }])),
                "conditions": rule.map(|r| json!(r.conditions)).unwrap_or_else(|| json!({})),
                "min_amount": rule.and_then(|r| r.min_amount),
            })
        })
        .collect();

    Ok(Json(json!({
        "profile": {
            "name": name, "slug": slug, "tagline": tagline, "phone": phone, "email": email, "address": address,
            "currency": currency, "timezone": timezone,
            "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")),
            "portal_url": format!("{}/order/{slug}", state.cfg.public_url),
        },
        "settings": settings,
        "workflows": actions,
        "integrations": {
            "mpesa_stk": state.cfg.mpesa.is_some(),
            "mpesa_environment": state.cfg.mpesa.as_ref().map(|m| if m.base_url.contains("sandbox") { "sandbox" } else { "production" }),
            "whatsapp": state.cfg.whatsapp.is_some(),
            "whatsapp_webhook_url": format!("{}/api/webhooks/whatsapp", state.cfg.public_url),
        }
    })))
}

async fn put_settings(State(state): State<AppState>, ctx: Ctx, Json(body): Json<TenantSettings>) -> AppResult<Json<TenantSettings>> {
    // One document, but each area needs its own permission: only changed areas are checked.
    {
        let mut conn = state.db.acquire().await?;
        let current = serde_json::to_value(crate::settings::load(&mut conn, ctx.tenant_id).await?).unwrap_or_default();
        let incoming = serde_json::to_value(&body).unwrap_or_default();
        let mut changed = false;
        for (section, perm) in [
            ("product", "settings.products"), ("stock", "settings.stock"), ("sales", "settings.sales"), ("orders", "settings.orders"),
            ("customers", "settings.customers"), ("loyalty", "settings.customers"), ("expenses", "settings.expenses"),
            ("reports", "settings.reports"), ("notifications", "settings.integrations"), ("workspace", "settings.workspace"),
        ] {
            if current.get(section) != incoming.get(section) {
                ctx.require(perm)?;
                changed = true;
            }
        }
        if !changed && !ctx.permissions.iter().any(|p| p == "*" || p.starts_with("settings.")) {
            return Err(AppError::Forbidden("You do not have permission for this action".into()));
        }
    }
    if body.product.max_photos == 0 || body.product.max_photos > 20 {
        return Err(bad("Product photos must be between 1 and 20"));
    }
    if body.loyalty.threshold <= Decimal::ZERO {
        return Err(bad("Loyalty spend threshold must be greater than zero"));
    }
    if !(0..=100).contains(&body.loyalty.referral_bonus_percent) {
        return Err(bad("Referral bonus must be between 0 and 100%"));
    }
    for (who, t) in [("Product", &body.reports.medals.products), ("Staff", &body.reports.medals.staff)] {
        let set: Vec<Decimal> = [t.gold, t.silver, t.bronze].into_iter().filter(|v| *v != Decimal::ZERO).collect();
        if set.iter().any(|v| *v < Decimal::ZERO) {
            return Err(bad(format!("{who} medal targets cannot be negative")));
        }
        if set.windows(2).any(|w| w[0] <= w[1]) {
            return Err(bad(format!("{who} medal targets must go down from Gold to Silver to Bronze")));
        }
    }
    if !["delivered", "completed"].contains(&body.orders.sale_on_status.as_str()) {
        return Err(bad("Orders can become sales at Delivered or Completed"));
    }
    for st in &body.orders.statuses {
        if !crate::settings::ORDER_STATUSES.iter().any(|(k, ..)| *k == st.key) {
            return Err(bad(format!("Unknown order status: {}", st.key)));
        }
        if st.label.trim().is_empty() {
            return Err(bad("Every order status needs a name"));
        }
    }
    if body.sales.payment_methods.iter().all(|m| !m.enabled) {
        return Err(bad("Enable at least one payment method"));
    }
    if let Some(b) = body.orders.default_branch_id {
        ctx.ensure_branch(b)?;
    }
    body.workspace.hours.validate().map_err(bad)?;
    if let Some(a) = body.workspace.location.areas.iter().find(|a| !crate::geo::AREAS.contains(&a.as_str())) {
        return Err(bad(format!("Unknown location area: {a}")));
    }
    let mut tx = state.db.begin().await?;
    let before: Value = sqlx::query_scalar("SELECT settings FROM tenants WHERE id = $1 FOR UPDATE")
        .bind(ctx.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let after = serde_json::to_value(&body).map_err(|e| AppError::Other(e.into()))?;
    sqlx::query("UPDATE tenants SET settings = $2 WHERE id = $1")
        .bind(ctx.tenant_id)
        .bind(&after)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "update", "tenant", ctx.tenant_id).before(before).after(&after)).await?;
    crate::settings::apply_day_shifts(&mut tx, ctx.tenant_id, &body).await?;
    tx.commit().await?;
    Ok(Json(body))
}

#[derive(Deserialize, Serialize)]
struct ProfileBody {
    name: String,
    slug: String,
    tagline: String,
    phone: String,
    email: String,
    address: String,
    currency: String,
    timezone: String,
}

async fn put_profile(State(state): State<AppState>, ctx: Ctx, Json(mut b): Json<ProfileBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.business")?;
    b.name = b.name.trim().to_string();
    b.slug = slugify(&b.slug);
    if b.name.is_empty() || b.slug.is_empty() {
        return Err(bad("Business name and link are required"));
    }
    if b.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(bad("Unknown time zone"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE tenants SET name=$2, slug=$3, tagline=$4, phone=$5, email=$6, address=$7, currency=$8, timezone=$9 WHERE id=$1",
    )
    .bind(ctx.tenant_id)
    .bind(&b.name)
    .bind(&b.slug)
    .bind(b.tagline.trim())
    .bind(b.phone.trim())
    .bind(b.email.trim())
    .bind(b.address.trim())
    .bind(b.currency.trim())
    .bind(&b.timezone)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "update_profile", "tenant", ctx.tenant_id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "slug": b.slug })))
}

async fn upload_logo(State(state): State<AppState>, ctx: Ctx, mut mp: Multipart) -> AppResult<Json<Value>> {
    ctx.require("settings.business")?;
    let field = mp.next_field().await.map_err(|e| bad(e.to_string()))?.ok_or_else(|| bad("No file uploaded"))?;
    let mime = field.content_type().unwrap_or("image/png").to_string();
    if !mime.starts_with("image/") {
        return Err(bad("Logo must be an image"));
    }
    let data: Bytes = field.bytes().await.map_err(|e| bad(e.to_string()))?;
    if data.len() > 2 * 1024 * 1024 {
        return Err(bad("Logo must be under 2 MB"));
    }
    sqlx::query("UPDATE tenants SET logo = $2, logo_mime = $3 WHERE id = $1")
        .bind(ctx.tenant_id)
        .bind(data.to_vec())
        .bind(&mime)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn logo(State(state): State<AppState>, Path(slug): Path<String>) -> AppResult<impl IntoResponse> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as("SELECT logo, logo_mime FROM tenants WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await?;
    match row {
        Some((Some(data), mime)) => Ok((
            [
                (header::CONTENT_TYPE, mime.unwrap_or_else(|| "image/png".into())),
                (header::CACHE_CONTROL, "public, max-age=3600".into()),
            ],
            data,
        )),
        _ => Err(AppError::NotFound("Logo")),
    }
}

#[derive(Deserialize)]
struct WorkflowBody {
    enabled: bool,
    levels: Vec<workflow::Level>,
    min_amount: Option<Decimal>,
    #[serde(default)]
    conditions: workflow::Conditions,
}

const MAX_LEVELS: usize = 5;

async fn put_workflow(State(state): State<AppState>, ctx: Ctx, Path(action): Path<String>, Json(b): Json<WorkflowBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.workflows")?;
    let Some(def) = workflow::ACTIONS.iter().find(|a| a.key == action) else { return Err(AppError::NotFound("Workflow")) };
    if b.levels.is_empty() || b.levels.len() > MAX_LEVELS {
        return Err(bad(format!("A workflow needs 1 to {MAX_LEVELS} approval levels")));
    }
    for (i, l) in b.levels.iter().enumerate() {
        let n = i + 1;
        match l.approver_type.as_str() {
            "role" if l.approver_role_id.is_none() => return Err(bad(format!("Level {n}: choose the approving role"))),
            "user" if l.approver_user_id.is_none() => return Err(bad(format!("Level {n}: choose the approving user"))),
            "role" | "user" | "branch_manager" | "admin" => {}
            _ => return Err(bad(format!("Level {n}: unknown approver type"))),
        }
    }
    if action == "sale.discount" && b.levels.len() > 1 {
        return Err(bad("Counter discount approval uses one supervisor — set a single level"));
    }
    if !def.uses_category && !b.conditions.category_ids.is_empty() {
        return Err(bad("Category conditions apply to expense rules only"));
    }
    let mut tx = state.db.begin().await?;
    let known_roles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE tenant_id = $1 AND id = ANY($2)")
        .bind(ctx.tenant_id)
        .bind(&b.conditions.role_ids)
        .fetch_one(&mut *tx)
        .await?;
    let known_branches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM branches WHERE tenant_id = $1 AND id = ANY($2)")
        .bind(ctx.tenant_id)
        .bind(&b.conditions.branch_ids)
        .fetch_one(&mut *tx)
        .await?;
    let known_categories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM expense_categories WHERE tenant_id = $1 AND id = ANY($2)")
        .bind(ctx.tenant_id)
        .bind(&b.conditions.category_ids)
        .fetch_one(&mut *tx)
        .await?;
    if known_roles as usize != b.conditions.role_ids.len()
        || known_branches as usize != b.conditions.branch_ids.len()
        || known_categories as usize != b.conditions.category_ids.len()
    {
        return Err(bad("A condition refers to an unknown role, branch or category"));
    }
    let levels = serde_json::to_value(&b.levels).map_err(|e| AppError::Other(e.into()))?;
    let conditions = serde_json::to_value(&b.conditions).map_err(|e| AppError::Other(e.into()))?;
    sqlx::query(
        "INSERT INTO workflows (tenant_id, action, enabled, levels, min_amount, conditions) VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (tenant_id, action) DO UPDATE SET enabled=$3, levels=$4, min_amount=$5, conditions=$6",
    )
    .bind(ctx.tenant_id)
    .bind(&action)
    .bind(b.enabled)
    .bind(&levels)
    .bind(b.min_amount)
    .bind(&conditions)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("settings", "workflow", "workflow", ctx.tenant_id).after(json!({
            "action": action, "enabled": b.enabled, "levels": levels, "min_amount": b.min_amount, "conditions": conditions
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Branches ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct Branch {
    id: Uuid,
    name: String,
    code: String,
    location: String,
    phone: String,
    manager_id: Option<Uuid>,
    manager_name: Option<String>,
    is_active: bool,
    user_count: i64,
    /// Own trading hours; null = follows the business hours.
    hours: Option<Value>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    geofence_radius_m: i32,
    geofence_enabled: bool,
}

async fn list_branches(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Branch>>> {
    let rows = sqlx::query_as(
        "SELECT b.id, b.name, b.code, b.location, b.phone, b.manager_id, m.name AS manager_name, b.is_active, b.hours,
                b.latitude, b.longitude, b.geofence_radius_m, b.geofence_enabled,
                (SELECT COUNT(*) FROM users u WHERE u.tenant_id = b.tenant_id AND u.is_active
                   AND (u.all_branches OR EXISTS (SELECT 1 FROM user_branches ub WHERE ub.user_id = u.id AND ub.branch_id = b.id))) AS user_count
         FROM branches b LEFT JOIN users m ON m.id = b.manager_id
         WHERE b.tenant_id = $1 ORDER BY b.is_active DESC, b.created_at",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, Serialize)]
struct BranchBody {
    name: String,
    code: String,
    location: Option<String>,
    phone: Option<String>,
    manager_id: Option<Uuid>,
    is_active: Option<bool>,
    /// Own trading hours (overrides Settings → Workspace): absent = unchanged, null = follow the business.
    #[serde(default, deserialize_with = "present")]
    hours: Option<Option<Hours>>,
    /// Where the branch is, for geofencing (absent = unchanged).
    #[serde(default)]
    geofence: Option<Geofence>,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
struct Geofence {
    latitude: Option<f64>,
    longitude: Option<f64>,
    radius_m: i32,
    enabled: bool,
}

/// The branch location is a security setting: changing it needs the workspace permission.
fn check_geofence(ctx: &Ctx, current: Option<&Geofence>, g: &Option<Geofence>) -> AppResult<()> {
    let Some(g) = g else { return Ok(()) };
    if let (Some(lat), Some(lng)) = (g.latitude, g.longitude) {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lng) {
            return Err(bad("Branch coordinates are out of range"));
        }
    } else if g.latitude.is_some() || g.longitude.is_some() {
        return Err(bad("Enter both latitude and longitude"));
    }
    if !(20..=5000).contains(&g.radius_m) {
        return Err(bad("Radius must be between 20 and 5,000 metres"));
    }
    if g.enabled && g.latitude.is_none() {
        return Err(bad("Set the branch location before turning on geofencing"));
    }
    if current != Some(g) {
        ctx.require("settings.workspace")?;
    }
    Ok(())
}

/// Distinguishes a field sent as `null` (Some(None)) from one left out (None).
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<Hours>>, D::Error> {
    Option::<Hours>::deserialize(d).map(Some)
}

/// Branch hours change the business date of that branch, so changing them needs the workspace permission.
/// Returns the value to store when the hours were sent.
fn check_branch_hours(ctx: &Ctx, current: Option<&Value>, b: &BranchBody) -> AppResult<Option<Option<Value>>> {
    let Some(hours) = &b.hours else { return Ok(None) };
    if let Some(h) = hours {
        h.validate().map_err(bad)?;
    }
    let incoming = hours.as_ref().map(|h| serde_json::to_value(h).unwrap_or_default());
    if incoming.as_ref() != current {
        ctx.require("settings.workspace")?;
    }
    Ok(Some(incoming))
}

async fn create_branch(State(state): State<AppState>, ctx: Ctx, Json(b): Json<BranchBody>) -> AppResult<Json<Value>> {
    ctx.require("branches.manage")?;
    if b.name.trim().is_empty() || b.code.trim().is_empty() {
        return Err(bad("Branch name and code are required"));
    }
    let hours = check_branch_hours(&ctx, None, &b)?.flatten();
    let default_fence = Geofence { latitude: None, longitude: None, radius_m: 150, enabled: false };
    check_geofence(&ctx, Some(&default_fence), &b.geofence)?;
    let fence = b.geofence.clone().unwrap_or(default_fence);
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO branches (tenant_id, name, code, location, phone, manager_id, hours, latitude, longitude, geofence_radius_m, geofence_enabled)
         VALUES ($1,$2,upper($3),$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(b.code.trim())
    .bind(b.location.as_deref().unwrap_or("").trim())
    .bind(b.phone.as_deref().unwrap_or("").trim())
    .bind(b.manager_id)
    .bind(hours)
    .bind(fence.latitude)
    .bind(fence.longitude)
    .bind(fence.radius_m)
    .bind(fence.enabled)
    .fetch_one(&mut *tx)
    .await?;
    let s = crate::settings::load(&mut tx, ctx.tenant_id).await?;
    crate::settings::apply_day_shifts(&mut tx, ctx.tenant_id, &s).await?;
    audit::record(&mut tx, &ctx, Entry::new("branches", "create", "branch", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_branch(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<BranchBody>) -> AppResult<Json<Value>> {
    ctx.require("branches.manage")?;
    let mut tx = state.db.begin().await?;
    let current: Option<(Option<Value>, Option<f64>, Option<f64>, i32, bool)> = sqlx::query_as(
        "SELECT hours, latitude, longitude, geofence_radius_m, geofence_enabled FROM branches WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((current, lat, lng, radius, enabled)) = current else { return Err(AppError::NotFound("Branch")) };
    let hours = check_branch_hours(&ctx, current.as_ref(), &b)?;
    let old_fence = Geofence { latitude: lat, longitude: lng, radius_m: radius, enabled };
    check_geofence(&ctx, Some(&old_fence), &b.geofence)?;
    let fence = b.geofence.clone().unwrap_or(old_fence);
    if b.is_active == Some(false) {
        let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM branches WHERE tenant_id = $1 AND is_active AND id <> $2")
            .bind(ctx.tenant_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if active == 0 {
            return Err(bad("At least one branch must remain active"));
        }
    }
    let n = sqlx::query(
        "UPDATE branches SET name=$3, code=upper($4), location=$5, phone=$6, manager_id=$7, is_active=COALESCE($8, is_active),
                hours=CASE WHEN $10 THEN $9 ELSE hours END,
                latitude=$11, longitude=$12, geofence_radius_m=$13, geofence_enabled=$14
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(b.code.trim())
    .bind(b.location.as_deref().unwrap_or("").trim())
    .bind(b.phone.as_deref().unwrap_or("").trim())
    .bind(b.manager_id)
    .bind(b.is_active)
    .bind(hours.clone().flatten())
    .bind(hours.is_some())
    .bind(fence.latitude)
    .bind(fence.longitude)
    .bind(fence.radius_m)
    .bind(fence.enabled)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Branch"));
    }
    let s = crate::settings::load(&mut tx, ctx.tenant_id).await?;
    crate::settings::apply_day_shifts(&mut tx, ctx.tenant_id, &s).await?;
    audit::record(&mut tx, &ctx, Entry::new("branches", "update", "branch", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Users ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    name: String,
    email: String,
    phone: String,
    role_id: Uuid,
    role_name: String,
    is_active: bool,
    all_branches: bool,
    branch_ids: Vec<Uuid>,
    last_login_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

async fn list_users(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<UserRow>>> {
    ctx.require_any(&["users.manage", "settings.manage", "approvals.approve", "staff.view_others"])?;
    let rows = sqlx::query_as(
        "SELECT u.id, u.name, u.email, u.phone, u.role_id, r.name AS role_name, u.is_active, u.all_branches,
                COALESCE(ARRAY(SELECT branch_id FROM user_branches ub WHERE ub.user_id = u.id), '{}') AS branch_ids,
                u.last_login_at, u.created_at
         FROM users u JOIN roles r ON r.id = u.role_id WHERE u.tenant_id = $1 ORDER BY u.is_active DESC, u.name",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct UserBody {
    name: String,
    email: String,
    phone: Option<String>,
    pin: Option<String>,
    role_id: Uuid,
    all_branches: bool,
    branch_ids: Vec<Uuid>,
    is_active: Option<bool>,
}

async fn validate_user(conn: &mut sqlx::PgConnection, ctx: &Ctx, b: &UserBody) -> AppResult<()> {
    if b.name.trim().is_empty() || !b.email.contains('@') {
        return Err(bad("Name and a valid email are required"));
    }
    let role_ok: Option<(Vec<String>, bool)> = sqlx::query_as("SELECT permissions, is_active FROM roles WHERE id = $1 AND tenant_id = $2")
        .bind(b.role_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?;
    let (perms, role_active) = role_ok.ok_or_else(|| bad("Unknown role"))?;
    if !role_active {
        return Err(bad("This role has been retired — choose another"));
    }
    if perms.iter().any(|p| p == "*") && !ctx.is_admin() {
        return Err(AppError::Forbidden("Only administrators can grant administrator access".into()));
    }
    // Assigning a role is granting its permissions: never more than the assigner holds.
    ensure_grantable(ctx, &perms, &[])?;
    if !b.all_branches && b.branch_ids.is_empty() {
        return Err(bad("Assign at least one branch"));
    }
    for br in &b.branch_ids {
        ctx.ensure_branch(*br)?;
    }
    Ok(())
}

async fn set_branches(conn: &mut sqlx::PgConnection, user_id: Uuid, branch_ids: &[Uuid]) -> AppResult<()> {
    sqlx::query("DELETE FROM user_branches WHERE user_id = $1").bind(user_id).execute(&mut *conn).await?;
    for b in branch_ids {
        sqlx::query("INSERT INTO user_branches (user_id, branch_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(b)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn create_user(State(state): State<AppState>, ctx: Ctx, Json(b): Json<UserBody>) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    let pin = b.pin.clone().ok_or_else(|| bad("Set a login PIN"))?;
    validate_pin(&pin)?;
    let mut tx = state.db.begin().await?;
    validate_user(&mut tx, &ctx, &b).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, name, email, phone, pin_hash, role_id, all_branches)
         VALUES ($1,$2,lower($3),$4,$5,$6,$7) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(b.email.trim())
    .bind(b.phone.as_deref().unwrap_or("").trim())
    .bind(hash_pin(&pin)?)
    .bind(b.role_id)
    .bind(b.all_branches)
    .fetch_one(&mut *tx)
    .await?;
    set_branches(&mut tx, id, &b.branch_ids).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("users", "create", "user", id).after(json!({ "name": b.name, "email": b.email, "role_id": b.role_id, "branches": b.branch_ids })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_user(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<UserBody>) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    if id == ctx.user_id && b.is_active == Some(false) {
        return Err(bad("You cannot deactivate your own account"));
    }
    let mut tx = state.db.begin().await?;
    validate_user(&mut tx, &ctx, &b).await?;

    // Never leave the business without an active administrator.
    let (was_admin,): (bool,) = sqlx::query_as(
        "SELECT '*' = ANY(r.permissions) FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1 AND u.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    if was_admin && !ctx.is_admin() {
        return Err(AppError::Forbidden("Only administrators can change an administrator".into()));
    }
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(u) - 'pin_hash' FROM users u WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query(
        "UPDATE users SET name=$3, email=lower($4), phone=$5, role_id=$6, all_branches=$7, is_active=COALESCE($8, is_active)
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(b.email.trim())
    .bind(b.phone.as_deref().unwrap_or("").trim())
    .bind(b.role_id)
    .bind(b.all_branches)
    .bind(b.is_active)
    .execute(&mut *tx)
    .await?;
    set_branches(&mut tx, id, &b.branch_ids).await?;

    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users u JOIN roles r ON r.id = u.role_id WHERE u.tenant_id = $1 AND u.is_active AND '*' = ANY(r.permissions)",
    )
    .bind(ctx.tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    if admins == 0 {
        return Err(bad("At least one active administrator is required"));
    }
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("users", "update", "user", id)
            .before(before)
            .after(json!({ "name": b.name, "email": b.email, "role_id": b.role_id, "is_active": b.is_active, "branches": b.branch_ids })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ResetPin {
    pin: String,
}

async fn reset_user_pin(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<ResetPin>) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    validate_pin(&b.pin)?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE users SET pin_hash=$3, failed_attempts=0, locked_until=NULL WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(hash_pin(&b.pin)?)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("User"));
    }
    audit::record(&mut tx, &ctx, Entry::new("users", "reset_pin", "user", id)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Roles ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct Role {
    id: Uuid,
    name: String,
    description: String,
    permissions: Vec<String>,
    is_system: bool,
    is_active: bool,
    user_count: i64,
}

async fn list_roles(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Role>>> {
    ctx.require_any(&["roles.manage", "users.manage", "settings.manage"])?;
    let rows = sqlx::query_as(
        "SELECT r.id, r.name, r.description, r.permissions, r.is_system, r.is_active,
                (SELECT COUNT(*) FROM users u WHERE u.role_id = r.id AND u.is_active) AS user_count
         FROM roles r WHERE r.tenant_id = $1 ORDER BY r.is_system DESC, r.name",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, Serialize)]
struct RoleBody {
    name: String,
    description: Option<String>,
    permissions: Vec<String>,
    /// Retire a role (only once no active user holds it).
    is_active: Option<bool>,
}

/// Only administrators may hand out permissions they do not hold themselves; anyone else can keep what a role
/// already had but never add more than they have (no self-promotion through roles).
fn ensure_grantable(ctx: &Ctx, permissions: &[String], already: &[String]) -> AppResult<()> {
    if ctx.is_admin() {
        return Ok(());
    }
    if let Some(p) = permissions.iter().find(|p| !already.contains(p) && !ctx.can(p)) {
        return Err(AppError::Forbidden(format!("You cannot grant a permission you do not have yourself ({p})")));
    }
    Ok(())
}

fn validate_role(b: &RoleBody) -> AppResult<()> {
    if b.name.trim().is_empty() {
        return Err(bad("Role name is required"));
    }
    if let Some(p) = b.permissions.iter().find(|p| !perms::is_known(p)) {
        return Err(bad(format!("Unknown permission: {p}")));
    }
    if b.permissions.iter().any(|p| p == "*") {
        return Err(bad("Full access is reserved for the Tenant Administrator role"));
    }
    Ok(())
}

async fn create_role(State(state): State<AppState>, ctx: Ctx, Json(b): Json<RoleBody>) -> AppResult<Json<Value>> {
    ctx.require("roles.manage")?;
    validate_role(&b)?;
    ensure_grantable(&ctx, &b.permissions, &[])?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO roles (tenant_id, name, description, permissions) VALUES ($1,$2,$3,$4) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.description.as_deref().unwrap_or(""))
        .bind(&b.permissions)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("roles", "create", "role", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_role(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RoleBody>) -> AppResult<Json<Value>> {
    ctx.require("roles.manage")?;
    let mut tx = state.db.begin().await?;
    let (is_system, before, current, users): (bool, Value, Vec<String>, i64) = sqlx::query_as(
        "SELECT is_system, to_jsonb(r), permissions, (SELECT COUNT(*) FROM users u WHERE u.role_id = r.id AND u.is_active)
         FROM roles r WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("Role"))?;
    if is_system {
        return Err(bad("The Tenant Administrator role cannot be changed"));
    }
    validate_role(&b)?;
    ensure_grantable(&ctx, &b.permissions, &current)?;
    if b.is_active == Some(false) && users > 0 {
        return Err(rule(format!("{users} active user(s) still have this role — move them to another role first")));
    }
    sqlx::query("UPDATE roles SET name=$3, description=$4, permissions=$5, is_active=COALESCE($6, is_active) WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.description.as_deref().unwrap_or(""))
        .bind(&b.permissions)
        .bind(b.is_active)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("roles", "update", "role", id).before(before).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn permission_catalogue(_ctx: Ctx) -> Json<&'static [perms::PermGroup]> {
    Json(perms::CATALOGUE)
}
