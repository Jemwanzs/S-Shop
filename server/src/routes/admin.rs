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
use crate::error::{bad, AppError, AppResult};
use crate::perms;
use crate::settings::TenantSettings;
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
    ctx.require("settings.manage")?;
    if body.product.max_photos == 0 || body.product.max_photos > 20 {
        return Err(bad("Product photos must be between 1 and 20"));
    }
    if body.loyalty.threshold <= Decimal::ZERO {
        return Err(bad("Loyalty spend threshold must be greater than zero"));
    }
    if !(0..=100).contains(&body.loyalty.referral_bonus_percent) {
        return Err(bad("Referral bonus must be between 0 and 100%"));
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
    ctx.require("settings.manage")?;
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
    ctx.require("settings.manage")?;
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
    ctx.require("settings.manage")?;
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
}

async fn list_branches(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Branch>>> {
    let rows = sqlx::query_as(
        "SELECT b.id, b.name, b.code, b.location, b.phone, b.manager_id, m.name AS manager_name, b.is_active,
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
}

async fn create_branch(State(state): State<AppState>, ctx: Ctx, Json(b): Json<BranchBody>) -> AppResult<Json<Value>> {
    ctx.require("branches.manage")?;
    if b.name.trim().is_empty() || b.code.trim().is_empty() {
        return Err(bad("Branch name and code are required"));
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO branches (tenant_id, name, code, location, phone, manager_id) VALUES ($1,$2,upper($3),$4,$5,$6) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(b.code.trim())
    .bind(b.location.as_deref().unwrap_or("").trim())
    .bind(b.phone.as_deref().unwrap_or("").trim())
    .bind(b.manager_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("branches", "create", "branch", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_branch(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<BranchBody>) -> AppResult<Json<Value>> {
    ctx.require("branches.manage")?;
    let mut tx = state.db.begin().await?;
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
        "UPDATE branches SET name=$3, code=upper($4), location=$5, phone=$6, manager_id=$7, is_active=COALESCE($8, is_active)
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
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Branch"));
    }
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
    ctx.require_any(&["users.manage", "settings.manage", "approvals.approve"])?;
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
    let role_ok: Option<Vec<String>> = sqlx::query_scalar("SELECT permissions FROM roles WHERE id = $1 AND tenant_id = $2")
        .bind(b.role_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?;
    let perms = role_ok.ok_or_else(|| bad("Unknown role"))?;
    if perms.iter().any(|p| p == "*") && !ctx.is_admin() {
        return Err(AppError::Forbidden("Only administrators can grant administrator access".into()));
    }
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
    user_count: i64,
}

async fn list_roles(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Role>>> {
    ctx.require_any(&["roles.manage", "users.manage", "settings.manage"])?;
    let rows = sqlx::query_as(
        "SELECT r.id, r.name, r.description, r.permissions, r.is_system,
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
    let (is_system, before): (bool, Value) = sqlx::query_as("SELECT is_system, to_jsonb(r) FROM roles r WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Role"))?;
    if is_system {
        return Err(bad("The Tenant Administrator role cannot be changed"));
    }
    validate_role(&b)?;
    sqlx::query("UPDATE roles SET name=$3, description=$4, permissions=$5 WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.description.as_deref().unwrap_or(""))
        .bind(&b.permissions)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("roles", "update", "role", id).before(before).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn permission_catalogue(_ctx: Ctx) -> Json<&'static [perms::PermGroup]> {
    Json(perms::CATALOGUE)
}
