//! Quick Login PIN (roadmap 83): a 4–6 digit PIN for fast sign-in on devices the person registered after a full
//! email + PIN sign-in.
//!
//! - **Belongs to the person** (their sign-in account — never a linked row in another business), salted hash only.
//! - **Device-bound**: registering a device returns a random secret once; only its SHA-256 is stored. A Quick PIN without
//!   a registered, unexpired, unrevoked device never signs anyone in.
//! - **Within rules**: the platform (on/off, minimum length, session length, attempts, device lifetime) and the business
//!   (on/off, which roles) — re-checked at every Quick sign-in.
//! - **Limited sessions**: Quick sessions are marked; payments, roles and access, security settings and platform
//!   administration need a full sign-in (`Ctx::require_full`).
//! - **Ends** when the full PIN changes or is reset, when the person, a device or every device is revoked, after too many
//!   wrong attempts, or when the device expires. Nobody — administrators or the platform — can see a Quick PIN.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::access::require_platform_admin;
use crate::audit::{self, Entry};
use crate::auth::{client_meta, hash_pin, verify_pin, Ctx};
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::settings::QuickPinPolicy;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/quick-pin", get(status).put(set_pin).delete(disable))
        .route("/auth/quick-pin/devices", post(trust_device))
        .route("/auth/quick-pin/devices/{id}", delete(revoke_device))
        .route("/auth/quick-pin/devices/revoke-all", post(revoke_all))
        .route("/auth/quick-login", post(quick_login))
        .route("/users/{id}/quick-pin/reset", post(admin_reset))
        .route("/security/quick-pin", get(tenant_policy).put(set_tenant_policy))
        .route("/platform/security", get(platform_get).put(platform_put))
        .route("/auth/activity", post(activity))
        .route("/auth/logout", post(logout))
        .route("/security/sessions", get(session_policy).put(set_session_policy))
}

// ── Rules ───────────────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PlatformSecurity {
    pub quick_pin_enabled: bool,
    /// 4 | 5 | 6 — the shortest Quick PIN allowed (6 is the longest).
    pub quick_pin_min_length: u8,
    /// How long a Quick sign-in lasts (1–24 h).
    pub quick_session_hours: u8,
    /// Wrong Quick PINs before a 15-minute lock (3–10); twice that and the device must sign in fully again.
    pub max_attempts: u8,
    /// A trusted device's lifetime (7–365 days).
    pub device_days: u16,
    /// Roadmap 87: automatic logout after inactivity — the default for businesses, the range they may choose from,
    /// the warning before logout and the longest any session may last.
    pub idle_default_minutes: u16,
    pub idle_min_minutes: u16,
    pub idle_max_minutes: u16,
    pub warning_seconds: u16,
    pub max_session_hours: u8,
}
impl Default for PlatformSecurity {
    fn default() -> Self {
        Self {
            quick_pin_enabled: true,
            quick_pin_min_length: 4,
            quick_session_hours: 12,
            max_attempts: 5,
            device_days: 90,
            idle_default_minutes: 5,
            idle_min_minutes: 1,
            idle_max_minutes: 120,
            warning_seconds: 30,
            max_session_hours: 12,
        }
    }
}

pub async fn platform_security(db: &sqlx::PgPool) -> AppResult<PlatformSecurity> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = 'security'").fetch_optional(db).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}

async fn tenant_rules(db: &sqlx::PgPool, tenant: Uuid) -> AppResult<QuickPinPolicy> {
    let v: Option<Value> = sqlx::query_scalar("SELECT settings->'security'->'quick_pin' FROM tenants WHERE id = $1").bind(tenant).fetch_one(db).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}

/// Why this person may not use a Quick PIN, if they may not (platform off, business off, role not included).
async fn not_allowed(state: &AppState, identity: Uuid) -> AppResult<Option<&'static str>> {
    let p = platform_security(&state.db).await?;
    if !p.quick_pin_enabled {
        return Ok(Some("Quick PIN sign-in is switched off on S'Shop"));
    }
    let (tenant, role): (Uuid, Uuid) = sqlx::query_as("SELECT tenant_id, role_id FROM users WHERE id = $1").bind(identity).fetch_one(&state.db).await?;
    let t = tenant_rules(&state.db, tenant).await?;
    if !t.enabled {
        return Ok(Some("Your business does not use Quick PIN sign-in"));
    }
    if !t.role_ids.is_empty() && !t.role_ids.contains(&role) {
        return Ok(Some("Quick PIN sign-in is not available for your role"));
    }
    Ok(None)
}

/// 4–6 digits (at least the platform minimum), not one repeated digit, not a straight run like 1234 / 9876.
fn validate_quick(pin: &str, min: u8) -> AppResult<()> {
    let min = min.clamp(4, 6) as usize;
    if !pin.chars().all(|c| c.is_ascii_digit()) || pin.len() < min || pin.len() > 6 {
        return Err(bad(format!("The Quick PIN must be {min}–6 digits")));
    }
    let d: Vec<i32> = pin.bytes().map(|b| (b - b'0') as i32).collect();
    let steps: Vec<i32> = d.windows(2).map(|w| w[1] - w[0]).collect();
    if steps.iter().all(|s| *s == 0) || steps.iter().all(|s| *s == 1) || steps.iter().all(|s| *s == -1) {
        return Err(bad("Choose a Quick PIN that is harder to guess (not 1111 or 1234)"));
    }
    Ok(())
}

/// The person's sign-in account behind this session (a linked row's identity).
async fn identity(state: &AppState, ctx: &Ctx) -> AppResult<Uuid> {
    if ctx.support.is_some() {
        return Err(rule("Not available during a support session"));
    }
    Ok(sqlx::query_scalar("SELECT COALESCE(login_user_id, id) FROM users WHERE id = $1").bind(ctx.user_id).fetch_one(&state.db).await?)
}

/// The full PIN, re-entered: proves it is the person before a Quick PIN or a device is set up.
async fn check_full_pin(state: &AppState, ident: Uuid, pin: &str) -> AppResult<()> {
    state.limits.check(&ident.to_string(), "quick_pin_setup", 10, std::time::Duration::from_secs(600))?;
    let hash: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE id = $1").bind(ident).fetch_one(&state.db).await?;
    if !verify_pin(pin, &hash) {
        return Err(bad("Your current PIN is incorrect"));
    }
    Ok(())
}

/// Ends Quick PIN on every trusted device of this person (PIN changed / reset, Quick PIN off, revoke all).
pub async fn revoke_devices(conn: &mut sqlx::PgConnection, user: Uuid, reason: &str, by: Option<Uuid>) -> AppResult<u64> {
    Ok(sqlx::query("UPDATE trusted_devices SET revoked_at = now(), revoked_by = $3, revoke_reason = $2 WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user)
        .bind(reason)
        .bind(by)
        .execute(&mut *conn)
        .await?
        .rows_affected())
}

fn device_secret() -> String {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw)
}

fn device_name(given: &str, ua: &str) -> String {
    let g: String = given.trim().chars().take(60).collect();
    if !g.is_empty() {
        return g;
    }
    let os = ["Android", "iPhone", "iPad", "Windows", "Mac OS", "Linux"].into_iter().find(|o| ua.contains(o)).unwrap_or("Device");
    let browser = ["Edg", "Chrome", "Firefox", "Safari"].into_iter().find(|b| ua.contains(b)).map(|b| if b == "Edg" { "Edge" } else { b }).unwrap_or("browser");
    format!("{browser} on {}", if os == "Mac OS" { "Mac" } else { os })
}

async fn register_device(tx: &mut sqlx::PgConnection, ident: Uuid, name: &str, ua: &str, days: u16) -> AppResult<(Uuid, String)> {
    let secret = device_secret();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO trusted_devices (user_id, token_hash, name, user_agent, expires_at) VALUES ($1, $2, $3, $4, now() + make_interval(days => $5::int)) RETURNING id",
    )
    .bind(ident)
    .bind(crate::mailer::hash_token(&secret))
    .bind(device_name(name, ua))
    .bind(ua.chars().take(200).collect::<String>())
    .bind(days as i32)
    .fetch_one(&mut *tx)
    .await?;
    Ok((id, secret))
}

// ── The person's own Quick PIN and devices ──────────────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct DeviceRow {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
}

async fn status(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let p = platform_security(&state.db).await?;
    let (set_at,): (Option<DateTime<Utc>>,) = sqlx::query_as("SELECT quick_pin_set_at FROM users WHERE id = $1 AND quick_pin_hash IS NOT NULL")
        .bind(ident)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or((None,));
    let devices: Vec<DeviceRow> = sqlx::query_as(
        "SELECT id, name, created_at, last_used_at, expires_at FROM trusted_devices WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now() ORDER BY created_at DESC",
    )
    .bind(ident)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "available": not_allowed(&state, ident).await?.is_none(),
        "reason": not_allowed(&state, ident).await?,
        "enabled": set_at.is_some(), "set_at": set_at, "min_length": p.quick_pin_min_length.clamp(4, 6),
        "devices": devices, "quick_session": ctx.quick,
    })))
}

#[derive(Deserialize)]
struct SetBody {
    current_pin: String,
    quick_pin: String,
    #[serde(default)]
    device_name: String,
}

/// Create or change the Quick PIN (full sign-in + current full PIN) and trust this device.
async fn set_pin(State(state): State<AppState>, ctx: Ctx, headers: HeaderMap, Json(b): Json<SetBody>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    let ident = identity(&state, &ctx).await?;
    if let Some(why) = not_allowed(&state, ident).await? {
        return Err(refused("Quick PIN unavailable", why));
    }
    check_full_pin(&state, ident, &b.current_pin).await?;
    let p = platform_security(&state.db).await?;
    validate_quick(&b.quick_pin, p.quick_pin_min_length)?;
    if verify_pin(&b.quick_pin, &sqlx::query_scalar::<_, String>("SELECT pin_hash FROM users WHERE id = $1").bind(ident).fetch_one(&state.db).await?) {
        return Err(bad("Use a Quick PIN that differs from your full PIN"));
    }
    let (_, ua) = client_meta(&headers);
    let mut tx = state.db.begin().await?;
    let changed: bool = sqlx::query_scalar("SELECT quick_pin_hash IS NOT NULL FROM users WHERE id = $1").bind(ident).fetch_one(&mut *tx).await?;
    sqlx::query("UPDATE users SET quick_pin_hash = $2, quick_pin_set_at = now(), quick_failed = 0, quick_locked_until = NULL WHERE id = $1")
        .bind(ident)
        .bind(hash_pin(&b.quick_pin)?)
        .execute(&mut *tx)
        .await?;
    // A changed Quick PIN starts over: other devices must be trusted again.
    if changed {
        revoke_devices(&mut tx, ident, "Quick PIN changed", Some(ident)).await?;
    }
    let (device, secret) = register_device(&mut tx, ident, &b.device_name, &ua, p.device_days).await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", if changed { "quick_pin_changed" } else { "quick_pin_enabled" }, "user", ident).after(json!({ "device": device }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "device_id": device, "device_token": secret })))
}

#[derive(Deserialize)]
struct TrustBody {
    current_pin: String,
    #[serde(default)]
    device_name: String,
}

/// Trust another device for the existing Quick PIN (full sign-in on that device + current full PIN).
async fn trust_device(State(state): State<AppState>, ctx: Ctx, headers: HeaderMap, Json(b): Json<TrustBody>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    let ident = identity(&state, &ctx).await?;
    if let Some(why) = not_allowed(&state, ident).await? {
        return Err(refused("Quick PIN unavailable", why));
    }
    check_full_pin(&state, ident, &b.current_pin).await?;
    let has: bool = sqlx::query_scalar("SELECT quick_pin_hash IS NOT NULL FROM users WHERE id = $1").bind(ident).fetch_one(&state.db).await?;
    if !has {
        return Err(rule("Create your Quick PIN first"));
    }
    let p = platform_security(&state.db).await?;
    let (_, ua) = client_meta(&headers);
    let mut tx = state.db.begin().await?;
    let (device, secret) = register_device(&mut tx, ident, &b.device_name, &ua, p.device_days).await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "device_trusted", "user", ident).after(json!({ "device": device }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "device_id": device, "device_token": secret })))
}

/// Switch Quick PIN off: the PIN is deleted and every trusted device revoked.
async fn disable(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET quick_pin_hash = NULL, quick_pin_set_at = NULL, quick_failed = 0, quick_locked_until = NULL WHERE id = $1")
        .bind(ident)
        .execute(&mut *tx)
        .await?;
    let n = revoke_devices(&mut tx, ident, "Quick PIN switched off", Some(ident)).await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "quick_pin_disabled", "user", ident).after(json!({ "devices_revoked": n }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn revoke_device(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE trusted_devices SET revoked_at = now(), revoked_by = $3, revoke_reason = 'Removed by the person' WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL")
        .bind(id)
        .bind(ident)
        .bind(ident)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Device"));
    }
    audit::record(&mut tx, &ctx, Entry::new("auth", "device_revoked", "user", ident).after(json!({ "device": id }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Sign out everywhere: every trusted device needs a full sign-in again and every session (this one too) ends.
async fn revoke_all(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let n = revoke_devices(&mut tx, ident, "Signed out everywhere", Some(ident)).await?;
    sqlx::query("UPDATE users SET sessions_valid_after = now() WHERE id = $1").bind(ident).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "signed_out_everywhere", "user", ident).after(json!({ "devices_revoked": n }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "devices_revoked": n })))
}

// ── Quick sign-in ───────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct QuickLogin {
    device_token: String,
    pin: String,
}

#[derive(sqlx::FromRow)]
struct QuickRow {
    device_id: Uuid,
    user_id: Uuid,
    tenant_id: Uuid,
    quick_pin_hash: Option<String>,
    quick_failed: i32,
    quick_locked_until: Option<DateTime<Utc>>,
    is_active: bool,
    must_change_pin: bool,
    tenant_status: String,
}

async fn quick_login(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<QuickLogin>) -> AppResult<Json<Value>> {
    let (ip, ua) = client_meta(&headers);
    state.limits.check(&ip, "quick_login", 30, std::time::Duration::from_secs(300))?;
    let full = || refused("Full sign-in needed", "Quick PIN is not available on this device — sign in with your email and PIN");
    let row: Option<QuickRow> = sqlx::query_as(
        "SELECT d.id AS device_id, u.id AS user_id, u.tenant_id, u.quick_pin_hash, u.quick_failed, u.quick_locked_until, u.is_active, u.must_change_pin,
                t.status AS tenant_status
         FROM trusted_devices d JOIN users u ON u.id = d.user_id JOIN tenants t ON t.id = u.tenant_id
         WHERE d.token_hash = $1 AND d.revoked_at IS NULL AND d.expires_at > now() AND u.login_user_id IS NULL",
    )
    .bind(crate::mailer::hash_token(&b.device_token))
    .fetch_optional(&state.db)
    .await?;
    let r = row.ok_or_else(full)?;
    let Some(hash) = r.quick_pin_hash.clone() else { return Err(full()) };
    if !r.is_active || r.must_change_pin || r.tenant_status != "active" {
        return Err(full());
    }
    if let Some(why) = not_allowed(&state, r.user_id).await? {
        return Err(refused("Quick PIN unavailable", why));
    }
    if let Some(until) = r.quick_locked_until.filter(|u| *u > Utc::now()) {
        let mins = (until - Utc::now()).num_minutes() + 1;
        return Err(AppError::Forbidden(format!("Too many attempts. Try again in {mins} minute(s), or use your full PIN.")));
    }
    let p = platform_security(&state.db).await?;
    let max = p.max_attempts.clamp(3, 10) as i32;
    if !verify_pin(&b.pin, &hash) {
        let attempts = r.quick_failed + 1;
        let mut tx = state.db.begin().await?;
        // Too many in a row: a 15-minute lock; twice the limit: this device must sign in fully again.
        let lock = (attempts % max == 0).then(|| Utc::now() + Duration::minutes(15));
        sqlx::query("UPDATE users SET quick_failed = $2, quick_locked_until = COALESCE($3, quick_locked_until) WHERE id = $1")
            .bind(r.user_id)
            .bind(attempts)
            .bind(lock)
            .execute(&mut *tx)
            .await?;
        let revoked = attempts >= max * 2;
        if revoked {
            sqlx::query("UPDATE trusted_devices SET revoked_at = now(), revoke_reason = 'Too many wrong Quick PINs' WHERE id = $1").bind(r.device_id).execute(&mut *tx).await?;
            sqlx::query("UPDATE users SET quick_failed = 0, quick_locked_until = NULL WHERE id = $1").bind(r.user_id).execute(&mut *tx).await?;
        }
        audit::system(
            &mut tx,
            r.tenant_id,
            Some(r.user_id),
            Entry::new("auth", "quick_login_failed", "user", r.user_id).after(json!({ "attempt": attempts, "locked": lock.is_some(), "device_revoked": revoked })),
            &ip,
            &ua,
        )
        .await?;
        tx.commit().await?;
        if revoked {
            return Err(full());
        }
        return Err(bad("Wrong Quick PIN"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET quick_failed = 0, quick_locked_until = NULL, last_login_at = now() WHERE id = $1").bind(r.user_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE trusted_devices SET last_used_at = now() WHERE id = $1").bind(r.device_id).execute(&mut *tx).await?;
    audit::system(&mut tx, r.tenant_id, Some(r.user_id), Entry::new("auth", "login", "user", r.user_id).after(json!({ "method": "quick_pin", "device": r.device_id })), &ip, &ua).await?;
    tx.commit().await?;
    let token = crate::sessions::issue(&state, r.user_id, r.tenant_id, crate::sessions::Kind::Quick).await?;
    let mut profile = super::auth::load_profile(&state, r.user_id, r.tenant_id, None).await?;
    profile.quick = true;
    Ok(Json(json!({ "token": token, "profile": profile, "quick": true })))
}

// ── Administrators ──────────────────────────────────────────────────────────────────────────────────────────────────

/// A business administrator switches off a person's Quick PIN (it is deleted, devices revoked). Nobody can see it.
async fn admin_reset(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    ctx.require("users.manage")?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query(
        "UPDATE users SET quick_pin_hash = NULL, quick_pin_set_at = NULL, quick_failed = 0, quick_locked_until = NULL
         WHERE id = $1 AND tenant_id = $2 AND login_user_id IS NULL",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(rule("Only people who sign in at this business can be reset here"));
    }
    let devices = revoke_devices(&mut tx, id, "Quick PIN reset by an administrator", Some(ctx.user_id)).await?;
    audit::record(&mut tx, &ctx, Entry::new("users", "quick_pin_reset", "user", id).after(json!({ "devices_revoked": devices }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "devices_revoked": devices })))
}

async fn tenant_policy(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    let p = platform_security(&state.db).await?;
    let t = tenant_rules(&state.db, ctx.tenant_id).await?;
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1 AND quick_pin_hash IS NOT NULL AND login_user_id IS NULL")
        .bind(ctx.tenant_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "policy": t, "platform": { "enabled": p.quick_pin_enabled, "min_length": p.quick_pin_min_length, "session_hours": p.quick_session_hours }, "users_with_quick_pin": users })))
}

/// The business's own rule — administrators only, full sign-in, never from a support session.
async fn set_tenant_policy(State(state): State<AppState>, ctx: Ctx, Json(b): Json<QuickPinPolicy>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    if !ctx.is_admin() || ctx.support.is_some() {
        return Err(AppError::Forbidden("Only the business's administrators decide on Quick PIN sign-in".into()));
    }
    let mut tx = state.db.begin().await?;
    for r in &b.role_ids {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM roles WHERE id = $1 AND tenant_id = $2)").bind(r).bind(ctx.tenant_id).fetch_one(&mut *tx).await?;
        if !ok {
            return Err(bad("Unknown role"));
        }
    }
    let before = tenant_rules(&state.db, ctx.tenant_id).await?;
    sqlx::query(
        "UPDATE tenants SET settings = jsonb_set(COALESCE(settings, '{}'::jsonb), '{security}',
             COALESCE(settings->'security', '{}'::jsonb) || jsonb_build_object('quick_pin', $2::jsonb)) WHERE id = $1",
    )
    .bind(ctx.tenant_id)
    .bind(json!(b))
    .execute(&mut *tx)
    .await?;
    // Switched off (or roles narrowed): devices of people no longer allowed are revoked now, not at their next try.
    let revoked = if !b.enabled {
        sqlx::query(
            "UPDATE trusted_devices d SET revoked_at = now(), revoked_by = $2, revoke_reason = 'Quick PIN switched off by the business'
             FROM users u WHERE u.id = d.user_id AND u.tenant_id = $1 AND d.revoked_at IS NULL",
        )
        .bind(ctx.tenant_id)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
    } else if !b.role_ids.is_empty() {
        sqlx::query(
            "UPDATE trusted_devices d SET revoked_at = now(), revoked_by = $3, revoke_reason = 'Role no longer allowed Quick PIN'
             FROM users u WHERE u.id = d.user_id AND u.tenant_id = $1 AND d.revoked_at IS NULL AND NOT (u.role_id = ANY($2))",
        )
        .bind(ctx.tenant_id)
        .bind(&b.role_ids)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
    } else {
        0
    };
    audit::record(&mut tx, &ctx, Entry::new("settings", "quick_pin_policy", "tenant", ctx.tenant_id).before(json!(before)).after(json!({ "policy": b, "devices_revoked": revoked }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "devices_revoked": revoked })))
}

async fn platform_get(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<PlatformSecurity>> {
    require_platform_admin(&state, &ctx).await?;
    Ok(Json(platform_security(&state.db).await?))
}

async fn platform_put(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PlatformSecurity>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if !(4..=6).contains(&b.quick_pin_min_length) {
        return Err(bad("The Quick PIN length must be 4, 5 or 6 digits"));
    }
    if !(1..=24).contains(&b.quick_session_hours) || !(3..=10).contains(&b.max_attempts) || !(7..=365).contains(&b.device_days) {
        return Err(bad("Session 1–24 hours, attempts 3–10, trusted devices 7–365 days"));
    }
    if !(1..=60).contains(&b.idle_min_minutes) || !(b.idle_min_minutes..=480).contains(&b.idle_max_minutes)
        || !(b.idle_min_minutes..=b.idle_max_minutes).contains(&b.idle_default_minutes) || !(10..=120).contains(&b.warning_seconds)
        || !(1..=24).contains(&b.max_session_hours)
    {
        return Err(bad("Inactivity: shortest 1–60 min, longest up to 480 min, default between them, warning 10–120 s, longest session 1–24 h"));
    }
    let before = platform_security(&state.db).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO platform_settings (key, value, updated_by) VALUES ('security', $1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(json!(b))
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    let revoked = if !b.quick_pin_enabled && before.quick_pin_enabled {
        sqlx::query("UPDATE trusted_devices SET revoked_at = now(), revoked_by = $1, revoke_reason = 'Quick PIN switched off on S''Shop' WHERE revoked_at IS NULL")
            .bind(ctx.user_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
    } else {
        0
    };
    // Roadmap 87: stricter limits reach open sessions at once (idle ones included); longer ones at the next heartbeat.
    sqlx::query(
        "UPDATE user_sessions SET idle_seconds = LEAST(idle_seconds, $1), expires_at = LEAST(expires_at, created_at + make_interval(hours => $2))
         WHERE ended_at IS NULL",
    )
    .bind((b.idle_max_minutes as i32) * 60)
    .bind(b.max_session_hours as i32)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("platform", "security_settings", "platform_settings", ctx.tenant_id).before(json!(before)).after(json!({ "settings": b, "devices_revoked": revoked }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "devices_revoked": revoked })))
}

// ── Session management (roadmap 87) ─────────────────────────────────────────────────────────────────────────────────

/// The app's heartbeat after genuine input (and *Stay logged in*): extends the session and returns the current limits.
async fn activity(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let (idle, warning) = crate::sessions::refresh_limit(&state, ctx.session_id, ctx.user_id, ctx.acting_from.unwrap_or(ctx.tenant_id)).await?;
    Ok(Json(json!({ "idle_seconds": idle, "warning_seconds": warning })))
}

#[derive(Deserialize, Default)]
struct LogoutBody {
    /// "inactivity" when the app's timer ends the session; otherwise a normal sign-out.
    #[serde(default)]
    reason: String,
}

async fn logout(State(state): State<AppState>, ctx: Ctx, body: Option<Json<LogoutBody>>) -> AppResult<Json<Value>> {
    let reason = if body.is_some_and(|b| b.reason == "inactivity") { "inactivity" } else { "logout" };
    crate::sessions::end(&state, ctx.session_id, ctx.user_id, ctx.tenant_id, reason).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn session_policy(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("users.manage")?;
    let p = platform_security(&state.db).await?;
    let v: Option<Value> = sqlx::query_scalar("SELECT settings->'security'->'session' FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let policy: crate::sessions::SessionPolicy = v.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| crate::sessions::SessionPolicy { idle_minutes: p.idle_default_minutes, warning_seconds: p.warning_seconds, ..Default::default() });
    let (idle, warning) = crate::sessions::effective(&state, ctx.user_id, ctx.acting_from.unwrap_or(ctx.tenant_id)).await?;
    Ok(Json(json!({
        "policy": policy,
        "platform": { "default_minutes": p.idle_default_minutes, "min_minutes": p.idle_min_minutes, "max_minutes": p.idle_max_minutes, "max_session_hours": p.max_session_hours },
        "mine": { "idle_seconds": idle, "warning_seconds": warning },
    })))
}

/// The business's timeout and stricter role timeouts — administrators, full sign-in, never from a support session.
async fn set_session_policy(State(state): State<AppState>, ctx: Ctx, Json(b): Json<crate::sessions::SessionPolicy>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    if !ctx.is_admin() || ctx.support.is_some() {
        return Err(AppError::Forbidden("Only the business's administrators set session rules".into()));
    }
    let p = platform_security(&state.db).await?;
    let (lo, hi) = (p.idle_min_minutes, p.idle_max_minutes);
    if !(lo..=hi).contains(&b.idle_minutes) {
        return Err(bad(format!("Choose between {lo} and {hi} minutes (S'Shop's limits)")));
    }
    if !(10..=120).contains(&b.warning_seconds) {
        return Err(bad("The warning shows 10–120 seconds before logout"));
    }
    let mut seen = Vec::new();
    for r in &b.role_minutes {
        if seen.contains(&r.role_id) {
            return Err(bad("One timeout per role"));
        }
        seen.push(r.role_id);
        if r.minutes < lo || r.minutes > b.idle_minutes {
            return Err(bad("A role's timeout must be stricter (shorter) than the business's, within S'Shop's limits"));
        }
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM roles WHERE id = $1 AND tenant_id = $2)").bind(r.role_id).bind(ctx.tenant_id).fetch_one(&state.db).await?;
        if !ok {
            return Err(bad("Unknown role"));
        }
    }
    let mut tx = state.db.begin().await?;
    let before: Option<Value> = sqlx::query_scalar("SELECT settings->'security'->'session' FROM tenants WHERE id = $1 FOR UPDATE").bind(ctx.tenant_id).fetch_one(&mut *tx).await?;
    sqlx::query(
        "UPDATE tenants SET settings = jsonb_set(COALESCE(settings, '{}'::jsonb), '{security}',
             COALESCE(settings->'security', '{}'::jsonb) || jsonb_build_object('session', $2::jsonb)) WHERE id = $1",
    )
    .bind(ctx.tenant_id)
    .bind(json!(b))
    .execute(&mut *tx)
    .await?;
    // Shorter limits apply to open sessions at once (longer ones at the next heartbeat), stricter role timeouts included.
    sqlx::query("UPDATE user_sessions SET idle_seconds = LEAST(idle_seconds, $2) WHERE tenant_id = $1 AND ended_at IS NULL")
        .bind(ctx.tenant_id)
        .bind((b.idle_minutes as i32) * 60)
        .execute(&mut *tx)
        .await?;
    for rm in &b.role_minutes {
        sqlx::query(
            "UPDATE user_sessions s SET idle_seconds = LEAST(s.idle_seconds, $3) FROM users u
             WHERE s.user_id = u.id AND u.tenant_id = $1 AND u.role_id = $2 AND s.tenant_id = $1 AND s.ended_at IS NULL",
        )
        .bind(ctx.tenant_id)
        .bind(rm.role_id)
        .bind(((rm.minutes.max(lo)) as i32) * 60)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(&mut tx, &ctx, Entry::new("settings", "session_policy", "tenant", ctx.tenant_id).before(json!(before)).after(json!(b))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_pins_validated() {
        assert!(validate_quick("2580", 4).is_ok());
        assert!(validate_quick("258013", 6).is_ok());
        assert!(validate_quick("2580", 5).is_err(), "shorter than the platform minimum");
        assert!(validate_quick("1234567", 4).is_err(), "longer than 6");
        assert!(validate_quick("12a4", 4).is_err(), "digits only");
        assert!(validate_quick("1111", 4).is_err());
        assert!(validate_quick("1234", 4).is_err());
        assert!(validate_quick("9876", 4).is_err());
    }
}
