//! Tenant accounts, secure business access and several businesses per tenant (roadmap 70–73).
//!
//! - **Tenant accounts** (platform owner): Platform → Tenants groups a customer's businesses (`tenants` rows) under one
//!   account with its primary administrator, totals, billing position (per business) and status. Activating or
//!   deactivating a tenant applies to every business it owns; platform-owned businesses are protected.
//! - **Support access** (platform owner ↔ tenant): entering another business needs a support session — fresh PIN,
//!   reason, scope (view / full), a time limit and, when the tenant asks for it, the tenant's approval. Either side ends
//!   it at any time; every step is audited in both businesses. The platform owner never uses a tenant's PIN.
//! - **Several businesses, one sign-in**: a person's access to another business of the same tenant is a linked user
//!   row there; *Switch business* moves between them without signing in again, never into another tenant.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::access::require_platform_admin;
use super::platform::{record_platform, tenant_rows, TenantRow};
use crate::audit::{self, Entry};
use crate::auth::{issue_acting_token, issue_token, verify_pin, Ctx, STAFF_TOKEN_HOURS};
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Platform owner
        .route("/platform/accounts", get(accounts))
        .route("/platform/accounts/{id}", get(account).put(update_account))
        .route("/platform/accounts/{id}/status", post(account_status))
        .route("/platform/accounts/{id}/businesses", post(add_business))
        .route("/platform/tenants/{id}/account", post(move_business))
        .route("/platform/tenants/{id}/support", post(request_support))
        .route("/platform/support", get(platform_sessions))
        .route("/platform/support/{id}/start", post(start_support))
        .route("/platform/support/{id}/end", post(end_support))
        // Tenant administrators
        .route("/support-access", get(tenant_sessions))
        .route("/support-access/policy", put(set_policy))
        .route("/support-access/{id}/{decision}", post(decide))
        // Everyone: several businesses of one tenant
        .route("/auth/switch-business", post(switch_business))
        .route("/users/linkable", get(linkable))
        .route("/users/link", post(link_user))
}

// ── 70. Tenant accounts ─────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct AccountRow {
    id: Uuid,
    name: String,
    notes: String,
    created_at: DateTime<Utc>,
    primary_user_id: Option<Uuid>,
    admin_name: Option<String>,
    admin_email: Option<String>,
    admin_phone: Option<String>,
    businesses: i64,
    branches: i64,
    users: i64,
    deactivated: i64,
    platform_owned: bool,
    is_demo: bool,
    last_login_at: Option<DateTime<Utc>>,
    #[sqlx(skip)]
    status: String,
    #[sqlx(skip)]
    billing_status: String,
    #[sqlx(skip)]
    next_due: Option<chrono::NaiveDate>,
    #[sqlx(skip)]
    outstanding: rust_decimal::Decimal,
    #[sqlx(skip)]
    business_names: Vec<String>,
}

const ACCOUNT_ROW: &str = "SELECT a.id, a.name, a.notes, a.created_at, a.primary_user_id, p.name AS admin_name, p.email AS admin_email, p.phone AS admin_phone,
        (SELECT COUNT(*) FROM tenants t WHERE t.account_id = a.id) AS businesses,
        (SELECT COUNT(*) FROM branches b JOIN tenants t ON t.id = b.tenant_id WHERE t.account_id = a.id AND b.is_active) AS branches,
        (SELECT COUNT(DISTINCT COALESCE(u.login_user_id, u.id)) FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE t.account_id = a.id AND u.is_active) AS users,
        (SELECT COUNT(*) FROM tenants t WHERE t.account_id = a.id AND t.status = 'deactivated') AS deactivated,
        COALESCE((SELECT bool_or(t.ownership = 'platform') FROM tenants t WHERE t.account_id = a.id), false) AS platform_owned,
        COALESCE((SELECT bool_and(t.is_demo) FROM tenants t WHERE t.account_id = a.id), false) AS is_demo,
        (SELECT max(u.last_login_at) FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE t.account_id = a.id) AS last_login_at
 FROM tenant_accounts a LEFT JOIN users p ON p.id = a.primary_user_id";

/// How pressing a billing position is (the tenant shows its most pressing business).
fn billing_rank(s: &str) -> u8 {
    match s {
        "suspended" => 9,
        "overdue" => 8,
        "grace" => 7,
        "payment_due" | "maintenance_due" => 6,
        "trial" => 5,
        "not_set" => 4,
        "active" | "one_off_paid" => 3,
        "free" => 2,
        _ => 1,
    }
}

async fn finish(state: &AppState, rows: &mut [AccountRow]) -> AppResult<()> {
    let mut conn = state.db.acquire().await?;
    for r in rows.iter_mut() {
        let businesses: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM tenants WHERE account_id = $1 ORDER BY created_at").bind(r.id).fetch_all(&mut *conn).await?;
        let mut worst = String::new();
        for (id, name) in &businesses {
            let s = crate::billing::summary(&mut conn, *id).await?;
            if billing_rank(&s.status) > billing_rank(&worst) {
                worst = s.status.clone();
            }
            r.next_due = match (r.next_due, s.next_due) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            r.outstanding += s.outstanding;
            r.business_names.push(name.clone());
        }
        r.billing_status = worst.clone();
        // Active | Trial | Grace period | Suspended | Deactivated (roadmap 70); platform-owned tenants are never billed.
        r.status = if r.platform_owned {
            "platform".into()
        } else if r.businesses > 0 && r.deactivated == r.businesses {
            "deactivated".into()
        } else {
            match worst.as_str() {
                "suspended" => "suspended",
                "grace" | "overdue" => "grace",
                "trial" => "trial",
                _ => "active",
            }
            .into()
        };
    }
    Ok(())
}

async fn accounts(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let mut rows: Vec<AccountRow> = sqlx::query_as(&format!(
        "{ACCOUNT_ROW} WHERE EXISTS (SELECT 1 FROM tenants t WHERE t.account_id = a.id) ORDER BY is_demo, a.created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    finish(&state, &mut rows).await?;
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    let home_account: Uuid = sqlx::query_scalar("SELECT account_id FROM tenants WHERE id = $1").bind(home).fetch_one(&state.db).await?;
    Ok(Json(json!({ "items": rows, "home_account_id": home_account, "home_tenant_id": home })))
}

#[derive(Serialize, sqlx::FromRow)]
struct AccountUser {
    id: Uuid,
    tenant_id: Uuid,
    business: String,
    name: String,
    email: String,
    phone: String,
    role: String,
    is_admin: bool,
    is_active: bool,
    linked: bool,
    last_login_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, sqlx::FromRow)]
struct SessionRow {
    id: Uuid,
    tenant_id: Uuid,
    business: String,
    user_name: String,
    reason: String,
    scope: String,
    minutes: i32,
    status: String,
    requested_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
    decided_by_name: Option<String>,
    started_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    ended_by_name: Option<String>,
    end_note: String,
}

const SESSION_ROW: &str = "SELECT s.id, s.tenant_id, t.name AS business, u.name AS user_name, s.reason, s.scope, s.minutes,
        CASE WHEN s.status = 'active' AND s.expires_at <= now() THEN 'expired' ELSE s.status END AS status,
        s.requested_at, s.decided_at, d.name AS decided_by_name, s.started_at, s.expires_at, s.ended_at, e.name AS ended_by_name, s.end_note
 FROM support_sessions s JOIN tenants t ON t.id = s.tenant_id JOIN users u ON u.id = s.user_id
 LEFT JOIN users d ON d.id = s.decided_by LEFT JOIN users e ON e.id = s.ended_by";

/// One tenant in full: profile, businesses (each with its billing, website and status), people, support sessions.
async fn account(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let mut rows: Vec<AccountRow> = sqlx::query_as(&format!("{ACCOUNT_ROW} WHERE a.id = $1")).bind(id).fetch_all(&state.db).await?;
    finish(&state, &mut rows).await?;
    let acc = rows.pop().ok_or(AppError::NotFound("Tenant"))?;
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE account_id = $1 ORDER BY created_at").bind(id).fetch_all(&state.db).await?;
    let mut businesses: Vec<TenantRow> = Vec::new();
    let mut websites = serde_json::Map::new();
    let mut policies = serde_json::Map::new();
    for t in &ids {
        businesses.extend(tenant_rows(&state, Some(*t)).await?);
        websites.insert(t.to_string(), super::website::platform_summary(&state, *t).await?);
        policies.insert(t.to_string(), json!(policy(&state, *t).await?));
    }
    let users: Vec<AccountUser> = sqlx::query_as(
        "SELECT u.id, u.tenant_id, t.name AS business, u.name, u.email, u.phone, r.name AS role, '*' = ANY(r.permissions) AS is_admin,
                u.is_active, u.login_user_id IS NOT NULL AS linked, COALESCE(li.last_login_at, u.last_login_at) AS last_login_at
         FROM users u JOIN tenants t ON t.id = u.tenant_id JOIN roles r ON r.id = u.role_id LEFT JOIN users li ON li.id = u.login_user_id
         WHERE t.account_id = $1 ORDER BY t.created_at, ('*' = ANY(r.permissions)) DESC, u.name",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let sessions: Vec<SessionRow> = sqlx::query_as(&format!("{SESSION_ROW} WHERE t.account_id = $1 ORDER BY s.requested_at DESC LIMIT 50"))
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let others: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT a.id, a.name FROM tenant_accounts a WHERE a.id <> $1 AND EXISTS (SELECT 1 FROM tenants t WHERE t.account_id = a.id) ORDER BY a.name")
            .bind(id)
            .fetch_all(&state.db)
            .await?;
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    Ok(Json(json!({
        "account": acc, "businesses": businesses, "users": users, "sessions": sessions, "websites": websites, "policies": policies,
        "home_tenant_id": home, "accounts": others.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct AccountBody {
    name: String,
    #[serde(default)]
    notes: String,
    primary_user_id: Option<Uuid>,
}

async fn update_account(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<AccountBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let name = b.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(bad("Enter the tenant's name (up to 120 characters)"));
    }
    let mut tx = state.db.begin().await?;
    if let Some(u) = b.primary_user_id {
        // The primary administrator signs in with their own account and is a Tenant Administrator of one of its businesses.
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users u JOIN tenants t ON t.id = u.tenant_id JOIN roles r ON r.id = u.role_id
                            WHERE u.id = $1 AND t.account_id = $2 AND u.login_user_id IS NULL AND u.is_active AND '*' = ANY(r.permissions))",
        )
        .bind(u)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if !ok {
            return Err(bad("The primary administrator must be an active administrator of this tenant"));
        }
    }
    let before: Option<Value> = sqlx::query_scalar("SELECT to_jsonb(a) FROM tenant_accounts a WHERE id = $1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    let before = before.ok_or(AppError::NotFound("Tenant"))?;
    sqlx::query("UPDATE tenant_accounts SET name = $2, notes = $3, primary_user_id = COALESCE($4, primary_user_id) WHERE id = $1")
        .bind(id)
        .bind(name)
        .bind(b.notes.trim().chars().take(2000).collect::<String>())
        .bind(b.primary_user_id)
        .execute(&mut *tx)
        .await?;
    let first: Uuid = sqlx::query_scalar("SELECT id FROM tenants WHERE account_id = $1 ORDER BY created_at LIMIT 1").bind(id).fetch_one(&mut *tx).await?;
    let after = json!({ "tenant": name, "primary_user_id": b.primary_user_id, "by": ctx.name });
    record_platform(&mut tx, &ctx, first, || Entry::new("platform", "update_tenant", "tenant_account", id).before(before.clone()).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
    #[serde(default)]
    reason: String,
}

/// Activate / deactivate a whole tenant: every business it owns (platform-owned ones are protected).
async fn account_status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<StatusBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let reason: String = b.reason.trim().chars().take(500).collect();
    if !matches!(b.status.as_str(), "active" | "deactivated") {
        return Err(bad("Status must be active or deactivated"));
    }
    if b.status == "deactivated" && reason.chars().count() < 5 {
        return Err(refused("Reason required", "Give the reason for deactivating this tenant"));
    }
    let mut tx = state.db.begin().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE account_id = $1 ORDER BY created_at FOR UPDATE").bind(id).fetch_all(&mut *tx).await?;
    if ids.is_empty() {
        return Err(AppError::NotFound("Tenant"));
    }
    let mut changed = 0;
    for t in ids {
        if super::platform::change_status(&mut tx, &state, &ctx, t, &b.status, &reason).await? {
            changed += 1;
        }
    }
    if changed == 0 {
        return Err(rule(if b.status == "active" { "Every business of this tenant is already active" } else { "Nothing to deactivate" }));
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "status": b.status, "businesses": changed })))
}

/// Next free ordering-link slug for a business name.
pub async fn unique_slug(conn: &mut sqlx::PgConnection, name: &str) -> AppResult<String> {
    let base = crate::util::slugify(name);
    let base = if base.is_empty() { "shop".to_string() } else { base };
    let mut slug = base.clone();
    let mut n = 2;
    while sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM tenants WHERE slug = $1)").bind(&slug).fetch_one(&mut *conn).await? {
        slug = format!("{base}-{n}");
        n += 1;
    }
    Ok(slug)
}

#[derive(Deserialize)]
struct NewBusiness {
    name: String,
}

/// Another business for a tenant (roadmap 72): its own branches, products, stock, sales and reports. The tenant's
/// primary administrator administers it with their existing sign-in; billing starts unset, like any new business.
async fn add_business(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<NewBusiness>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let name = b.name.trim();
    if name.chars().count() < 2 || name.chars().count() > 120 {
        return Err(bad("Enter the business name"));
    }
    let mut tx = state.db.begin().await?;
    let acc: Option<(Option<Uuid>, bool)> = sqlx::query_as(
        "SELECT a.primary_user_id, COALESCE((SELECT bool_or(t.ownership = 'platform') FROM tenants t WHERE t.account_id = a.id), false)
         FROM tenant_accounts a WHERE a.id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (primary, platform_owned) = acc.ok_or(AppError::NotFound("Tenant"))?;
    let primary = primary.ok_or_else(|| rule("Choose the tenant's primary administrator first"))?;
    let slug = unique_slug(&mut tx, name).await?;
    let tenant_id = crate::bootstrap::seed_business(&mut tx, name, &slug, id).await.map_err(AppError::Other)?;
    if platform_owned {
        sqlx::query("UPDATE tenants SET ownership = 'platform' WHERE id = $1").bind(tenant_id).execute(&mut *tx).await?;
    }
    let admin_role: Uuid = sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND is_system").bind(tenant_id).fetch_one(&mut *tx).await?;
    link_row(&mut tx, tenant_id, primary, admin_role, true, &[]).await?;
    let after = json!({ "business": name, "slug": slug, "tenant_account": id, "by": ctx.name });
    record_platform(&mut tx, &ctx, tenant_id, || Entry::new("platform", "add_business", "tenant", tenant_id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "tenant_id": tenant_id, "slug": slug })))
}

#[derive(Deserialize)]
struct MoveBody {
    account_id: Uuid,
}

/// Group an existing business under another tenant (e.g. two approved requests from the same customer). People linked
/// from the old tenant lose access to it at once (links never cross tenants).
async fn move_business(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<MoveBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    let mut tx = state.db.begin().await?;
    let row: Option<(String, Uuid, String)> = sqlx::query_as("SELECT name, account_id, ownership FROM tenants WHERE id = $1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    let (name, from, ownership) = row.ok_or(AppError::NotFound("Business"))?;
    if from == b.account_id {
        return Err(rule("The business already belongs to this tenant"));
    }
    let target: Option<bool> = sqlx::query_scalar("SELECT COALESCE(bool_or(t.ownership = 'platform'), false) FROM tenant_accounts a LEFT JOIN tenants t ON t.account_id = a.id WHERE a.id = $1 GROUP BY a.id")
        .bind(b.account_id)
        .fetch_optional(&mut *tx)
        .await?;
    let target_platform = target.ok_or(AppError::NotFound("Tenant"))?;
    if id == home || target_platform != (ownership == "platform") {
        return Err(refused("Not allowed", "Platform-owned and customer businesses cannot share a tenant"));
    }
    sqlx::query("UPDATE tenants SET account_id = $2 WHERE id = $1").bind(id).bind(b.account_id).execute(&mut *tx).await?;
    // The primary administrator of the old tenant may have been in this business: keep the old tenant consistent.
    sqlx::query(
        "UPDATE tenant_accounts a SET primary_user_id = NULL WHERE a.id = $1
           AND NOT EXISTS (SELECT 1 FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = a.primary_user_id AND t.account_id = a.id)",
    )
    .bind(from)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM tenant_accounts a WHERE a.id = $1 AND NOT EXISTS (SELECT 1 FROM tenants t WHERE t.account_id = a.id)").bind(from).execute(&mut *tx).await?;
    let after = json!({ "business": name, "from": from, "to": b.account_id, "by": ctx.name });
    record_platform(&mut tx, &ctx, id, || Entry::new("platform", "move_business", "tenant", id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ── 71. Support access ──────────────────────────────────────────────────────────────────────────────────────────────

pub const POLICIES: [&str; 2] = ["notify", "approval"];
const MINUTES: [i32; 6] = [15, 30, 60, 120, 240, 480];

/// The business's support-access policy: `notify` (default — support may enter; administrators are told at once) or
/// `approval` (an administrator approves every request first).
async fn policy(state: &AppState, tenant: Uuid) -> AppResult<String> {
    let p: Option<String> = sqlx::query_scalar("SELECT settings->'security'->>'support_access' FROM tenants WHERE id = $1").bind(tenant).fetch_one(&state.db).await?;
    Ok(p.filter(|p| POLICIES.contains(&p.as_str())).unwrap_or_else(|| "notify".into()))
}

/// Fresh authentication: the platform owner's own PIN, re-entered for this request (rate-limited).
async fn fresh_pin(state: &AppState, ctx: &Ctx, pin: &str) -> AppResult<()> {
    state.limits.check(&ctx.user_id.to_string(), "support_pin", 20, std::time::Duration::from_secs(600))?;
    let hash: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE id = $1 AND login_user_id IS NULL").bind(ctx.user_id).fetch_one(&state.db).await?;
    if !verify_pin(pin, &hash) {
        return Err(bad("Your PIN is incorrect"));
    }
    Ok(())
}

async fn notify_admins(state: &AppState, tenant: Uuid, note: Note) {
    notify::to_permission(state, tenant, None, "users.manage", note).await;
}

#[derive(Deserialize)]
struct SupportBody {
    pin: String,
    reason: String,
    scope: String,
    minutes: i32,
}

/// The session token and profile for an active support session.
async fn enter(state: &AppState, ctx: &Ctx, home: Uuid, tenant: Uuid, sid: Uuid, until: DateTime<Utc>) -> AppResult<Value> {
    let token = issue_acting_token(&state.cfg.jwt_secret, ctx.user_id, home, tenant, sid, until)?;
    let profile = super::auth::load_profile(state, ctx.user_id, tenant, Some(home)).await?;
    Ok(json!({ "status": "active", "id": sid, "token": token, "profile": profile, "expires_at": until }))
}

/// Open a business on behalf of the platform (roadmap 71). Platform-owned businesses open at once (still with PIN,
/// reason and time limit); a customer's business follows its policy.
async fn request_support(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SupportBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if ctx.support.is_some() {
        return Err(rule("End the current support session first"));
    }
    let home = ctx.tenant_id;
    if id == home {
        return Err(rule("This is your own business — no support access is needed"));
    }
    let reason = b.reason.trim();
    if reason.chars().count() < 10 || reason.chars().count() > 500 {
        return Err(bad("Give the reason for this access (at least 10 characters)"));
    }
    if !matches!(b.scope.as_str(), "view" | "full") {
        return Err(bad("Choose view-only or full access"));
    }
    if !MINUTES.contains(&b.minutes) {
        return Err(bad("Choose how long the access lasts"));
    }
    fresh_pin(&state, &ctx, &b.pin).await?;
    let business: Option<(String, String)> = sqlx::query_as("SELECT name, ownership FROM tenants WHERE id = $1").bind(id).fetch_optional(&state.db).await?;
    let (name, ownership) = business.ok_or(AppError::NotFound("Business"))?;
    let needs_approval = ownership != "platform" && policy(&state, id).await? == "approval";

    let mut tx = state.db.begin().await?;
    // An earlier session still open for this business: requested / approved ones are replaced by the new request.
    sqlx::query(
        "UPDATE support_sessions SET status = CASE WHEN status = 'active' AND expires_at <= now() THEN 'expired' ELSE 'ended' END,
                ended_at = now(), ended_by = $3, end_note = 'Replaced by a new request'
         WHERE tenant_id = $1 AND user_id = $2 AND status IN ('requested', 'approved', 'active')",
    )
    .bind(id)
    .bind(ctx.user_id)
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    let until = Utc::now() + Duration::minutes(b.minutes as i64);
    let sid: Uuid = sqlx::query_scalar(
        "INSERT INTO support_sessions (tenant_id, user_id, home_tenant_id, reason, scope, minutes, status, started_at, expires_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(id)
    .bind(ctx.user_id)
    .bind(home)
    .bind(reason)
    .bind(&b.scope)
    .bind(b.minutes)
    .bind(if needs_approval { "requested" } else { "active" })
    .bind((!needs_approval).then(Utc::now))
    .bind((!needs_approval).then_some(until))
    .fetch_one(&mut *tx)
    .await?;
    let action = if needs_approval { "support_requested" } else { "support_started" };
    let after = json!({ "business": name, "reason": reason, "scope": b.scope, "minutes": b.minutes, "by": ctx.name, "session": sid });
    record_platform(&mut tx, &ctx, id, || Entry::new("platform", action, "support_session", sid).after(after.clone())).await?;
    tx.commit().await?;

    let scope = if b.scope == "view" { "view-only" } else { "full" };
    if needs_approval {
        notify_admins(
            &state,
            id,
            Note::new("support_request", "S'Shop support asks to access your business", format!("{} · {scope} · {} min — {reason}", ctx.name, b.minutes), "/settings/security"),
        )
        .await;
        return Ok(Json(json!({ "status": "requested", "id": sid })));
    }
    if ownership != "platform" {
        notify_admins(
            &state,
            id,
            Note::new("support_started", "S'Shop support opened your business", format!("{} · {scope} · until {} UTC — {reason}", ctx.name, until.format("%H:%M")), "/settings/security"),
        )
        .await;
    }
    Ok(Json(enter(&state, &ctx, home, id, sid, until).await?))
}

#[derive(Deserialize)]
struct PinBody {
    pin: String,
}

/// Start a session the business approved (within 24 hours of approval), with fresh authentication.
async fn start_support(State(state): State<AppState>, ctx: Ctx, Path(sid): Path<Uuid>, Json(b): Json<PinBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if ctx.support.is_some() {
        return Err(rule("End the current support session first"));
    }
    fresh_pin(&state, &ctx, &b.pin).await?;
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, Uuid, String, i32, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        "SELECT s.tenant_id, s.home_tenant_id, s.status, s.minutes, s.decided_at, t.name FROM support_sessions s JOIN tenants t ON t.id = s.tenant_id
         WHERE s.id = $1 AND s.user_id = $2 FOR UPDATE OF s",
    )
    .bind(sid)
    .bind(ctx.user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (tenant, home, status, minutes, decided_at, name) = row.ok_or(AppError::NotFound("Support session"))?;
    if status != "approved" {
        return Err(rule(match status.as_str() {
            "requested" => "The business has not approved this request yet",
            "denied" => "The business declined this request",
            _ => "This support session is no longer available",
        }));
    }
    if decided_at.is_some_and(|d| d < Utc::now() - Duration::hours(24)) {
        sqlx::query("UPDATE support_sessions SET status = 'expired', ended_at = now(), end_note = 'Not started within 24 hours of approval' WHERE id = $1")
            .bind(sid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(rule("The approval has lapsed (not used within 24 hours) — request access again"));
    }
    let until = Utc::now() + Duration::minutes(minutes as i64);
    sqlx::query("UPDATE support_sessions SET status = 'active', started_at = now(), expires_at = $2 WHERE id = $1").bind(sid).bind(until).execute(&mut *tx).await?;
    let after = json!({ "business": name, "session": sid, "until": until, "by": ctx.name });
    record_platform(&mut tx, &ctx, tenant, || Entry::new("platform", "support_started", "support_session", sid).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(enter(&state, &ctx, home, tenant, sid, until).await?))
}

/// End (or withdraw) a support session. From inside it, the platform owner gets their own session back.
async fn end_support(State(state): State<AppState>, ctx: Ctx, Path(sid): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT s.tenant_id, s.home_tenant_id, s.status, t.name FROM support_sessions s JOIN tenants t ON t.id = s.tenant_id WHERE s.id = $1 AND s.user_id = $2 FOR UPDATE OF s",
    )
    .bind(sid)
    .bind(ctx.user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (tenant, home, status, name) = row.ok_or(AppError::NotFound("Support session"))?;
    if matches!(status.as_str(), "requested" | "approved" | "active") {
        sqlx::query("UPDATE support_sessions SET status = 'ended', ended_at = now(), ended_by = $2, end_note = 'Ended by S''Shop support' WHERE id = $1")
            .bind(sid)
            .bind(ctx.user_id)
            .execute(&mut *tx)
            .await?;
        let after = json!({ "business": name, "session": sid, "by": ctx.name });
        let mut c = ctx.clone();
        c.tenant_id = home;
        c.acting_from = None;
        record_platform(&mut tx, &c, tenant, || Entry::new("platform", "support_ended", "support_session", sid).after(after.clone())).await?;
    }
    tx.commit().await?;
    let token = issue_token(&state.cfg.jwt_secret, ctx.user_id, home, "staff", Duration::hours(STAFF_TOKEN_HOURS))?;
    let profile = super::auth::load_profile(&state, ctx.user_id, home, None).await?;
    Ok(Json(json!({ "ok": true, "token": token, "profile": profile })))
}

#[derive(Deserialize)]
struct SessionsQuery {
    account_id: Option<Uuid>,
}

/// The platform owner's support sessions (optionally for one tenant).
async fn platform_sessions(State(state): State<AppState>, ctx: Ctx, Query(q): Query<SessionsQuery>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let rows: Vec<SessionRow> = sqlx::query_as(&format!(
        "{SESSION_ROW} WHERE s.user_id = $1 AND ($2::uuid IS NULL OR t.account_id = $2) ORDER BY s.requested_at DESC LIMIT 50"
    ))
    .bind(ctx.user_id)
    .bind(q.account_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "items": rows })))
}

/// Tenant administrators: the policy and every support request / session for their business.
async fn tenant_sessions(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    let rows: Vec<SessionRow> = sqlx::query_as(&format!("{SESSION_ROW} WHERE s.tenant_id = $1 ORDER BY s.requested_at DESC LIMIT 50"))
        .bind(ctx.tenant_id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({ "policy": policy(&state, ctx.tenant_id).await?, "items": rows })))
}

#[derive(Deserialize)]
struct PolicyBody {
    policy: String,
}

/// Only the business's own administrators decide its policy — never the platform from inside a support session.
async fn set_policy(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PolicyBody>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    if !ctx.is_admin() || ctx.support.is_some() {
        return Err(AppError::Forbidden("Only the business's administrators decide on support access".into()));
    }
    if !POLICIES.contains(&b.policy.as_str()) {
        return Err(bad("Unknown policy"));
    }
    let mut tx = state.db.begin().await?;
    let before = policy(&state, ctx.tenant_id).await?;
    sqlx::query("UPDATE tenants SET settings = jsonb_set(COALESCE(settings, '{}'::jsonb), '{security}', COALESCE(settings->'security', '{}'::jsonb) || jsonb_build_object('support_access', $2::text)) WHERE id = $1")
        .bind(ctx.tenant_id)
        .bind(&b.policy)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "support_policy", "tenant", ctx.tenant_id).before(json!({ "policy": before })).after(json!({ "policy": b.policy }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "policy": b.policy })))
}

/// approve | deny a request; revoke an approved or active session (ends it at once).
async fn decide(State(state): State<AppState>, ctx: Ctx, Path((sid, decision)): Path<(Uuid, String)>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    ctx.require("users.manage")?;
    if ctx.support.is_some() {
        return Err(AppError::Forbidden("Support access is decided by the business's own staff".into()));
    }
    let (from, to, action): (&[&str], &str, &str) = match decision.as_str() {
        "approve" => (&["requested"], "approved", "support_approved"),
        "deny" => (&["requested"], "denied", "support_denied"),
        "revoke" => (&["approved", "active"], "ended", "support_revoked"),
        _ => return Err(AppError::NotFound("Action")),
    };
    let mut tx = state.db.begin().await?;
    let row: Option<(String, Uuid, Uuid)> = sqlx::query_as("SELECT status, user_id, home_tenant_id FROM support_sessions WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(sid)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, platform_user, home) = row.ok_or(AppError::NotFound("Support session"))?;
    if !from.contains(&status.as_str()) {
        return Err(rule(format!("This support session is already {status}")));
    }
    if to == "ended" {
        sqlx::query("UPDATE support_sessions SET status = 'ended', ended_at = now(), ended_by = $2, end_note = 'Revoked by the business' WHERE id = $1")
            .bind(sid)
            .bind(ctx.user_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE support_sessions SET status = $2, decided_by = $3, decided_at = now() WHERE id = $1")
            .bind(sid)
            .bind(to)
            .bind(ctx.user_id)
            .execute(&mut *tx)
            .await?;
    }
    let after = json!({ "session": sid, "status": to, "by": ctx.name });
    audit::record(&mut tx, &ctx, Entry::new("platform", action, "support_session", sid).before(json!({ "status": status })).after(after.clone())).await?;
    let mut c = ctx.clone();
    c.tenant_id = home;
    audit::record(&mut tx, &c, Entry::new("platform", action, "support_session", sid).after(after)).await?;
    tx.commit().await?;
    let business: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let (title, body) = match to {
        "approved" => (format!("{business} approved support access"), "Open it from Platform → Tenants within 24 hours.".to_string()),
        "denied" => (format!("{business} declined support access"), format!("Declined by {}", ctx.name)),
        _ => (format!("{business} ended support access"), format!("Revoked by {}", ctx.name)),
    };
    notify::to_users(&state, home, &[platform_user], Note::new("support_decision", title, body, "/settings/tenants")).await;
    state.emit(home, Some(platform_user), "support", json!({ "id": sid, "status": to }));
    Ok(Json(json!({ "ok": true, "status": to })))
}

// ── 72. Several businesses of one tenant ────────────────────────────────────────────────────────────────────────────

/// The businesses a signed-in person can switch to: their own and those of the same tenant they were given access to.
pub async fn businesses_of(state: &AppState, user_id: Uuid, current: Uuid) -> AppResult<Vec<Value>> {
    let rows: Vec<(Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT t.id, t.name, r.id FROM users me JOIN users i ON i.id = COALESCE(me.login_user_id, me.id) JOIN tenants it ON it.id = i.tenant_id
         JOIN users r ON (r.id = i.id OR r.login_user_id = i.id) AND r.is_active
         JOIN tenants t ON t.id = r.tenant_id AND t.status = 'active' AND t.account_id = it.account_id
         WHERE me.id = $1 ORDER BY t.created_at",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|(id, name, _)| json!({ "id": id, "name": name, "current": id == current })).collect())
}

#[derive(Deserialize)]
struct SwitchBody {
    tenant_id: Uuid,
}

/// Switch business within the same tenant without signing in again (roadmap 72).
async fn switch_business(State(state): State<AppState>, ctx: Ctx, Json(b): Json<SwitchBody>) -> AppResult<Json<Value>> {
    if ctx.support.is_some() || ctx.acting_from.is_some() {
        return Err(rule("End the support session first"));
    }
    let target: Option<Uuid> = sqlx::query_scalar(
        "SELECT r.id FROM users me JOIN users i ON i.id = COALESCE(me.login_user_id, me.id) JOIN tenants it ON it.id = i.tenant_id
         JOIN users r ON (r.id = i.id OR r.login_user_id = i.id) AND r.is_active AND r.tenant_id = $2
         JOIN tenants t ON t.id = r.tenant_id AND t.status = 'active' AND t.account_id = it.account_id
         WHERE me.id = $1 AND i.is_active",
    )
    .bind(ctx.user_id)
    .bind(b.tenant_id)
    .fetch_optional(&state.db)
    .await?;
    let user = target.ok_or_else(|| AppError::Forbidden("You do not have access to that business".into()))?;
    let mut tx = state.db.begin().await?;
    let mut c = ctx.clone();
    c.tenant_id = b.tenant_id;
    c.user_id = user;
    audit::record(&mut tx, &c, Entry::new("auth", "switch_business", "user", user).after(json!({ "from": ctx.tenant_id }))).await?;
    sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1").bind(user).execute(&mut *tx).await?;
    tx.commit().await?;
    // A Quick PIN session stays a Quick PIN session in the other business (roadmap 83): never upgraded by switching.
    let token = if ctx.quick {
        let hours = super::quickpin::platform_security(&state.db).await?.quick_session_hours.clamp(1, 24) as i64;
        crate::auth::issue_quick_token(&state.cfg.jwt_secret, user, b.tenant_id, Duration::hours(hours))?
    } else {
        issue_token(&state.cfg.jwt_secret, user, b.tenant_id, "staff", Duration::hours(STAFF_TOKEN_HOURS))?
    };
    let mut profile = super::auth::load_profile(&state, user, b.tenant_id, None).await?;
    profile.quick = ctx.quick;
    Ok(Json(json!({ "token": token, "profile": profile })))
}

/// Creates a person's linked user row in `tenant` (no PIN of its own: they sign in as themselves).
async fn link_row(conn: &mut sqlx::PgConnection, tenant: Uuid, identity: Uuid, role: Uuid, all_branches: bool, branches: &[Uuid]) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, name, email, phone, pin_hash, role_id, all_branches, login_user_id)
         SELECT $1, name, email, phone, '!', $3, $4, id FROM users WHERE id = $2 AND login_user_id IS NULL
         ON CONFLICT (login_user_id, tenant_id) WHERE login_user_id IS NOT NULL
         DO UPDATE SET is_active = true, role_id = EXCLUDED.role_id, all_branches = EXCLUDED.all_branches
         RETURNING id",
    )
    .bind(tenant)
    .bind(identity)
    .bind(role)
    .bind(all_branches)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM user_branches WHERE user_id = $1").bind(id).execute(&mut *conn).await?;
    for b in branches {
        sqlx::query("INSERT INTO user_branches (user_id, branch_id) VALUES ($1, $2)").bind(id).bind(b).execute(&mut *conn).await?;
    }
    Ok(id)
}

/// People of the tenant's other businesses who could be given access here.
async fn linkable(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, t.name FROM users u JOIN tenants t ON t.id = u.tenant_id
         WHERE t.account_id = (SELECT account_id FROM tenants WHERE id = $1) AND t.id <> $1 AND t.status = 'active'
           AND u.login_user_id IS NULL AND u.is_active AND NOT (lower(u.email) = ANY($2))
           AND NOT EXISTS (SELECT 1 FROM users x WHERE x.tenant_id = $1 AND x.login_user_id = u.id AND x.is_active)
         ORDER BY t.created_at, u.name",
    )
    .bind(ctx.tenant_id)
    .bind(&state.cfg.platform_admins)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "items": rows.into_iter().map(|(id, name, email, business)| json!({ "id": id, "name": name, "email": email, "business": business })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
struct LinkBody {
    user_id: Uuid,
    role_id: Uuid,
    all_branches: bool,
    #[serde(default)]
    branch_ids: Vec<Uuid>,
}

/// Give a person of another business of this tenant access here, with a role and branches of this business.
async fn link_user(State(state): State<AppState>, ctx: Ctx, Json(b): Json<LinkBody>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    ctx.require("users.manage")?;
    let mut tx = state.db.begin().await?;
    let who: Option<(String, String)> = sqlx::query_as(
        "SELECT u.name, lower(u.email) FROM users u JOIN tenants t ON t.id = u.tenant_id
         WHERE u.id = $1 AND u.login_user_id IS NULL AND u.is_active AND t.id <> $2 AND t.status = 'active'
           AND t.account_id = (SELECT account_id FROM tenants WHERE id = $2)",
    )
    .bind(b.user_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (name, email) = who.ok_or_else(|| bad("Only people of this tenant's other businesses can be added"))?;
    if state.cfg.platform_admins.contains(&email) {
        return Err(refused("Not allowed", "The platform owner enters businesses through support access only"));
    }
    super::admin::validate_role_grant(&mut tx, &ctx, b.role_id, b.all_branches, &b.branch_ids).await?;
    let id = link_row(&mut tx, ctx.tenant_id, b.user_id, b.role_id, b.all_branches, &b.branch_ids).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("users", "link", "user", id).after(json!({ "name": name, "email": email, "role_id": b.role_id, "branches": b.branch_ids, "identity": b.user_id })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}
