//! Platform administration (platform owner only — PLATFORM_ADMIN_EMAILS): the directory of every business on this
//! S'Shop installation with its billing position (roadmap 34), activity across businesses (35), activation status
//! (36), opening a business to work inside it (audited in both businesses) and the Pablo Niche demo business.
//! Tenant administrators never reach these endpoints: `require_platform_admin` checks the email allow-list.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::access::require_platform_admin;
use super::{Counted, Page, Paged, Period};
use crate::audit::{self, Entry};
use crate::auth::{hash_pin, issue_acting_token, issue_token, Ctx, STAFF_TOKEN_HOURS};
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/platform/tenants", get(list))
        .route("/platform/tenants/{id}", get(detail))
        .route("/platform/tenants/{id}/open", post(open))
        .route("/platform/tenants/{id}/status", post(set_status))
        .route("/platform/tenants/{id}/users/{user_id}/reset-pin", post(reset_pin))
        .route("/platform/activity", get(activity))
        .route("/platform/demo", get(demo_status).post(demo_start))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct TenantRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub is_demo: bool,
    pub status: String,
    /// customer | platform (the platform owner's own, protected business).
    pub ownership: String,
    pub billing_suspended: bool,
    pub status_reason: String,
    pub status_changed_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub users: i64,
    pub branches: i64,
    pub sales: i64,
    pub last_sale_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub admin_name: Option<String>,
    pub admin_email: Option<String>,
    pub admin_phone: Option<String>,
    #[sqlx(skip)]
    pub billing: crate::billing::Summary,
}

const TENANT_ROW: &str = "SELECT t.id, t.name, t.slug, t.is_demo, t.status, t.ownership, t.billing_suspended, t.status_reason, t.status_changed_at, t.activated_at, t.created_at,
        t.phone, t.email, t.address,
        (SELECT COUNT(*) FROM users u WHERE u.tenant_id = t.id AND u.is_active) AS users,
        (SELECT COUNT(*) FROM branches b WHERE b.tenant_id = t.id AND b.is_active) AS branches,
        (SELECT COUNT(*) FROM sales s WHERE s.tenant_id = t.id) AS sales,
        (SELECT max(created_at) FROM sales s WHERE s.tenant_id = t.id) AS last_sale_at,
        (SELECT max(last_login_at) FROM users u WHERE u.tenant_id = t.id) AS last_login_at,
        a.name AS admin_name, a.email AS admin_email, a.phone AS admin_phone
 FROM tenants t
 LEFT JOIN LATERAL (SELECT u.name, u.email, u.phone FROM users u JOIN roles r ON r.id = u.role_id
                    WHERE u.tenant_id = t.id AND u.is_active AND '*' = ANY(r.permissions) ORDER BY u.created_at LIMIT 1) a ON true";

/// Every business (or one), each with its billing position.
pub async fn tenant_rows(state: &AppState, id: Option<Uuid>) -> AppResult<Vec<TenantRow>> {
    let mut rows: Vec<TenantRow> = sqlx::query_as(&format!("{TENANT_ROW} WHERE $1::uuid IS NULL OR t.id = $1 ORDER BY t.is_demo, t.created_at"))
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let mut conn = state.db.acquire().await?;
    for r in &mut rows {
        r.billing = crate::billing::summary(&mut conn, r.id).await?;
    }
    Ok(rows)
}

/// Directory of every business with contacts, activation and billing position (roadmap 34).
async fn list(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let rows = tenant_rows(&state, None).await?;
    Ok(Json(json!({ "items": rows, "home_tenant_id": ctx.acting_from.unwrap_or(ctx.tenant_id), "current_tenant_id": ctx.tenant_id })))
}

#[derive(Serialize, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    name: String,
    email: String,
    phone: String,
    role: String,
    is_admin: bool,
    is_active: bool,
    last_login_at: Option<DateTime<Utc>>,
    failed_attempts: i32,
    locked_until: Option<DateTime<Utc>>,
    failed_last_7d: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct BranchRow {
    id: Uuid,
    name: String,
    code: String,
    location: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct OnboardingRow {
    contact_name: String,
    email: String,
    phone: String,
    location: String,
    business_type: String,
    branches: Option<i32>,
    message: String,
    created_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
    decided_by_name: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct StatusChange {
    created_at: DateTime<Utc>,
    by_name: Option<String>,
    action: String,
    after: Option<Value>,
}

/// One business in full: profile, admins and users, branches and locations, onboarding, status history and billing.
async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let tenant = tenant_rows(&state, Some(id)).await?.pop().ok_or(AppError::NotFound("Business"))?;
    let users: Vec<UserRow> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, u.phone, r.name AS role, '*' = ANY(r.permissions) AS is_admin, u.is_active, u.last_login_at,
                u.failed_attempts, u.locked_until,
                (SELECT COUNT(*) FROM audit_log a WHERE a.tenant_id = u.tenant_id AND a.user_id = u.id AND a.module = 'auth'
                    AND a.action = 'login_failed' AND a.created_at > now() - interval '7 days') AS failed_last_7d
         FROM users u JOIN roles r ON r.id = u.role_id WHERE u.tenant_id = $1 ORDER BY ('*' = ANY(r.permissions)) DESC, u.created_at",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let branches: Vec<BranchRow> = sqlx::query_as(
        "SELECT id, name, code, location, latitude, longitude, is_active, created_at FROM branches WHERE tenant_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let onboarding: Option<OnboardingRow> = sqlx::query_as(
        "SELECT a.contact_name, a.email, a.phone, a.location, a.business_type, a.branches, a.message, a.created_at, a.decided_at,
                u.name AS decided_by_name
         FROM access_requests a LEFT JOIN users u ON u.id = a.decided_by WHERE a.tenant_id = $1 ORDER BY a.created_at LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let history: Vec<StatusChange> = sqlx::query_as(
        "SELECT a.created_at, u.name AS by_name, a.action, a.after FROM audit_log a LEFT JOIN users u ON u.id = a.user_id
         WHERE a.tenant_id = $1 AND ((a.module = 'platform' AND a.action IN ('deactivate_business', 'reactivate_business'))
                OR (a.module = 'billing' AND a.action IN ('billing_suspended', 'billing_restored', 'trial_ended', 'plan_updated')))
         ORDER BY a.created_at DESC LIMIT 20",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let billing = super::billing::platform_view(&state, id).await?;
    Ok(Json(json!({
        "tenant": tenant, "users": users, "branches": branches, "onboarding": onboarding, "status_history": history,
        "billing": billing, "is_home": id == ctx.acting_from.unwrap_or(ctx.tenant_id),
    })))
}

/// Writes a platform action to the audit trail of the business concerned and of the platform owner's own business
/// (the platform audit trail).
pub async fn record_platform(tx: &mut sqlx::PgConnection, ctx: &Ctx, tenant: Uuid, e: impl Fn() -> Entry<'static>) -> AppResult<()> {
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    let targets = if tenant == home { vec![home] } else { vec![tenant, home] };
    for t in targets {
        let mut c = ctx.clone();
        c.tenant_id = t;
        audit::record(tx, &c, e()).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct StatusBody {
    /// "active" | "deactivated"
    status: String,
    #[serde(default)]
    reason: String,
}

/// Deactivate / reactivate a business (roadmap 36). Deactivation keeps all data; it blocks sign-in, ends every
/// session issued before it, switches off the ordering link and therefore blocks new transactions.
async fn set_status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<StatusBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let reason: String = b.reason.trim().chars().take(500).collect();
    if !matches!(b.status.as_str(), "active" | "deactivated") {
        return Err(bad("Status must be active or deactivated"));
    }
    if b.status == "deactivated" && reason.chars().count() < 5 {
        return Err(refused("Reason required", "Give the reason for deactivating this business"));
    }
    let mut tx = state.db.begin().await?;
    let (name, current, ownership): (String, String, String) = sqlx::query_as("SELECT name, status, ownership FROM tenants WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Business"))?;
    if current == b.status {
        return Err(rule(if current == "active" { "This business is already active" } else { "This business is already deactivated" }));
    }
    if b.status == "deactivated" {
        // The platform owner's own business (any business with a platform administrator) can never be switched off.
        let emails: Vec<String> = sqlx::query_scalar("SELECT lower(email) FROM users WHERE tenant_id = $1 AND is_active")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
        if ownership == "platform" || id == ctx.acting_from.unwrap_or(ctx.tenant_id) || emails.iter().any(|e| state.cfg.platform_admins.contains(e)) {
            return Err(refused("Not allowed", "The platform owner's own business cannot be deactivated"));
        }
        sqlx::query("UPDATE tenants SET status = 'deactivated', status_reason = $2, status_changed_at = now(), sessions_valid_after = now() WHERE id = $1")
            .bind(id)
            .bind(&reason)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE tenants SET status = 'active', status_reason = $2, status_changed_at = now() WHERE id = $1")
            .bind(id)
            .bind(&reason)
            .execute(&mut *tx)
            .await?;
    }
    let action = if b.status == "deactivated" { "deactivate_business" } else { "reactivate_business" };
    let after = json!({ "business": name, "status": b.status, "reason": reason, "by": ctx.name });
    record_platform(&mut tx, &ctx, id, || Entry::new("platform", action, "tenant", id).before(json!({ "status": current })).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "status": b.status })))
}

/// The platform owner resets a business user's PIN (typically a locked-out administrator): a one-time PIN is
/// returned to pass on and the lock is cleared.
async fn reset_pin(State(state): State<AppState>, ctx: Ctx, Path((id, user_id)): Path<(Uuid, Uuid)>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let pin = super::access::temporary_pin();
    let mut tx = state.db.begin().await?;
    let user: Option<(String, String)> = sqlx::query_as(
        "UPDATE users SET pin_hash = $3, failed_attempts = 0, locked_until = NULL WHERE id = $1 AND tenant_id = $2 RETURNING name, email",
    )
    .bind(user_id)
    .bind(id)
    .bind(hash_pin(&pin)?)
    .fetch_optional(&mut *tx)
    .await?;
    let (name, email) = user.ok_or(AppError::NotFound("User"))?;
    let after = json!({ "user": name, "email": email, "by": ctx.name });
    record_platform(&mut tx, &ctx, id, || Entry::new("platform", "reset_pin", "user", user_id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "name": name, "email": email, "temporary_pin": pin })))
}

/// Activities the platform owner can filter by (roadmap 35), mapped onto the existing audit trail.
const ACTIVITIES: &[(&str, &str, &[&str])] = &[
    ("login", "auth", &["login"]),
    ("login_failed", "auth", &["login_failed"]),
    ("sale", "sales", &["create", "offline_sync", "exchange"]),
    ("stock_count", "stock", &["adjust"]),
    ("stock_receive", "stock", &["receive"]),
    ("transfer", "transfers", &["create", "submit", "dispatch", "receive", "cancel"]),
    ("pin_reset", "platform", &["reset_pin"]),
    ("platform", "platform", &["open_business", "deactivate_business", "reactivate_business", "approve_access"]),
    ("billing", "billing", &["payment_received", "invoice_issued", "quotation_issued", "quotation_accepted", "document_void", "plan_updated", "payment_started",
                             "billing_suspended", "billing_restored", "trial_ended"]),
];

#[derive(Deserialize)]
struct ActivityQuery {
    tenant_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    user_id: Option<Uuid>,
    activity: Option<String>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

#[derive(Serialize, sqlx::FromRow)]
struct ActivityRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    tenant_id: Uuid,
    business: String,
    user_id: Option<Uuid>,
    user_name: Option<String>,
    user_email: Option<String>,
    branch_name: Option<String>,
    module: String,
    action: String,
    entity_type: String,
    after: Option<Value>,
    ip: String,
    location: Option<Value>,
}

/// Activity across businesses — sign-ins, failed sign-ins, sales, stock counts, transfers, platform and billing
/// actions — filtered by business, branch, user, activity and date range.
async fn activity(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ActivityQuery>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let tz = crate::util::parse_tz(crate::billing::PLATFORM_TZ);
    let (from, to) = q.period.resolve(crate::billing::today(), "week");
    let (start, end) = crate::util::local_range(from, to, tz);
    let pairs: Vec<(String, String)> = match q.activity.as_deref().filter(|a| !a.is_empty() && *a != "all") {
        Some(a) => {
            let (_, m, acts) = ACTIVITIES.iter().find(|(k, _, _)| *k == a).ok_or_else(|| bad("Unknown activity"))?;
            acts.iter().map(|x| (m.to_string(), x.to_string())).collect()
        }
        None => ACTIVITIES.iter().flat_map(|(_, m, acts)| acts.iter().map(|x| (m.to_string(), x.to_string()))).collect(),
    };
    let (modules, actions): (Vec<String>, Vec<String>) = pairs.into_iter().unzip();
    const FILTER: &str = "JOIN unnest($1::text[], $2::text[]) AS f(module, action) ON f.module = a.module AND f.action = a.action
         WHERE a.created_at >= $3 AND a.created_at < $4
           AND ($5::uuid IS NULL OR a.tenant_id = $5) AND ($6::uuid IS NULL OR a.branch_id = $6) AND ($7::uuid IS NULL OR a.user_id = $7)";
    let rows: Vec<Counted<ActivityRow>> = sqlx::query_as(&format!(
        "SELECT COUNT(*) OVER() AS total_count, a.id, a.created_at, a.tenant_id, t.name AS business, a.user_id, u.name AS user_name,
                u.email AS user_email, b.name AS branch_name, a.module, a.action, a.entity_type, a.after, a.ip, a.location
         FROM audit_log a JOIN tenants t ON t.id = a.tenant_id LEFT JOIN users u ON u.id = a.user_id LEFT JOIN branches b ON b.id = a.branch_id
         {FILTER} ORDER BY a.created_at DESC LIMIT $8 OFFSET $9"
    ))
    .bind(&modules)
    .bind(&actions)
    .bind(start)
    .bind(end)
    .bind(q.tenant_id)
    .bind(q.branch_id)
    .bind(q.user_id)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    let counts: Vec<(String, String, i64)> = sqlx::query_as(&format!("SELECT a.module, a.action, COUNT(*) FROM audit_log a {FILTER} GROUP BY 1, 2"))
        .bind(&modules)
        .bind(&actions)
        .bind(start)
        .bind(end)
        .bind(q.tenant_id)
        .bind(q.branch_id)
        .bind(q.user_id)
        .fetch_all(&state.db)
        .await?;
    let mut totals = serde_json::Map::new();
    for (key, m, acts) in ACTIVITIES {
        let n: i64 = counts.iter().filter(|(cm, ca, _)| cm == m && acts.contains(&ca.as_str())).map(|c| c.2).sum();
        totals.insert(key.to_string(), json!(n));
    }
    let page: Paged<ActivityRow> = rows.into();
    Ok(Json(json!({
        "items": page.items, "total": page.total, "from": from, "to": to, "totals": totals,
        "activities": ACTIVITIES.iter().map(|a| a.0).collect::<Vec<_>>(),
    })))
}

/// Gives the platform admin a session inside another business (full access, re-checked on every request).
/// Opening their own business returns a normal session.
async fn open(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Business"))?;
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    let token = if id == home {
        issue_token(&state.cfg.jwt_secret, ctx.user_id, home, "staff", chrono::Duration::hours(STAFF_TOKEN_HOURS))?
    } else {
        issue_acting_token(&state.cfg.jwt_secret, ctx.user_id, home, id)?
    };
    // Recorded in the business being opened (its own audit trail shows platform access) and at home.
    let mut tx = state.db.begin().await?;
    for tenant in [id, home] {
        let mut c = ctx.clone();
        c.tenant_id = tenant;
        audit::record(&mut tx, &c, Entry::new("platform", "open_business", "tenant", id).after(json!({ "business": name, "by": ctx.name }))).await?;
    }
    tx.commit().await?;
    let profile = super::auth::load_profile(&state, ctx.user_id, id, (id != home).then_some(home)).await?;
    Ok(Json(json!({ "token": token, "profile": profile })))
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct DemoStatus {
    pub running: bool,
    pub step: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub report: Option<crate::demo::DemoReport>,
    pub error: Option<String>,
}

async fn demo_status(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let status = state.demo.lock().map(|s| s.clone()).unwrap_or_default();
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1 AND is_demo").bind(crate::demo::DEMO_SLUG).fetch_optional(&state.db).await?;
    Ok(Json(json!({ "status": status, "tenant_id": exists, "pexels": std::env::var("PEXELS_API_KEY").is_ok() })))
}

#[derive(Deserialize)]
struct DemoBody {
    #[serde(default)]
    reset: bool,
}

/// Builds (or with `reset`, rebuilds) the demo business in the background; poll GET for progress.
async fn demo_start(State(state): State<AppState>, ctx: Ctx, Json(b): Json<DemoBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    {
        let mut s = state.demo.lock().map_err(|_| AppError::Other(anyhow::anyhow!("demo status poisoned")))?;
        if s.running {
            return Err(rule("The demo business is already being built"));
        }
        *s = DemoStatus { running: true, step: "Starting".into(), started_at: Some(Utc::now()), ..Default::default() };
    }
    let mut tx = state.db.begin().await?;
    audit::record(&mut tx, &ctx, Entry::new("platform", if b.reset { "reset_demo" } else { "build_demo" }, "tenant", ctx.tenant_id)).await?;
    tx.commit().await?;
    let st = state.clone();
    tokio::spawn(async move {
        let progress = |step: &str| {
            if let Ok(mut s) = st.demo.lock() {
                s.step = step.to_string();
            }
            tracing::info!(step, "demo seed");
        };
        let result = crate::demo::seed(&st, b.reset, progress).await;
        if let Ok(mut s) = st.demo.lock() {
            s.running = false;
            s.finished_at = Some(Utc::now());
            match result {
                Ok(r) => {
                    s.step = "Done".into();
                    s.report = Some(r);
                }
                Err(e) => {
                    tracing::error!(error = %format!("{e:#}"), "demo seed failed");
                    s.step = "Failed".into();
                    s.error = Some(format!("{e:#}"));
                }
            }
        }
    });
    Ok(Json(json!({ "started": true })))
}
