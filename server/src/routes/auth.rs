//! Staff sign-in (email + PIN), session profile and PIN changes.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::{client_meta, hash_pin, issue_token, validate_pin, verify_pin, Ctx, STAFF_TOKEN_HOURS};
use crate::error::{bad, AppError, AppResult};
use crate::state::AppState;

const MAX_ATTEMPTS: i32 = 5;
const LOCK_MINUTES: i64 = 15;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
        .route("/auth/change-pin", post(change_pin))
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    pin: String,
}

#[derive(sqlx::FromRow)]
struct LoginRow {
    id: Uuid,
    tenant_id: Uuid,
    pin_hash: String,
    is_active: bool,
    failed_attempts: i32,
    locked_until: Option<DateTime<Utc>>,
    must_change_pin: bool,
    pin_expires_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    profile: Profile,
}

async fn login(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<LoginBody>) -> AppResult<Json<LoginResponse>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "login", 30, std::time::Duration::from_secs(300))?;
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || body.pin.is_empty() {
        return Err(bad("Enter your email and PIN"));
    }
    let invalid = || AppError::BadRequest("Invalid email or PIN".into());

    let row: Option<LoginRow> = sqlx::query_as(
        "SELECT id, tenant_id, pin_hash, is_active, failed_attempts, locked_until, must_change_pin, pin_expires_at FROM users WHERE lower(email) = $1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;
    let Some(user) = row else {
        // Spend comparable time so response timing does not reveal which emails exist.
        static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let dummy = DUMMY.get_or_init(|| hash_pin("timing-equaliser").unwrap_or_default());
        let _ = verify_pin(&body.pin, dummy);
        return Err(invalid());
    };
    if !user.is_active {
        return Err(AppError::Forbidden("Your account has been deactivated".into()));
    }
    if let Some(until) = user.locked_until {
        if until > Utc::now() {
            let mins = (until - Utc::now()).num_minutes() + 1;
            return Err(AppError::Forbidden(format!("Too many attempts. Try again in {mins} minute(s).")));
        }
    }

    let (ip, ua) = client_meta(&headers);
    if !verify_pin(&body.pin, &user.pin_hash) {
        let attempts = user.failed_attempts + 1;
        let lock = (attempts >= MAX_ATTEMPTS).then(|| Utc::now() + Duration::minutes(LOCK_MINUTES));
        let mut tx = state.db.begin().await?;
        sqlx::query("UPDATE users SET failed_attempts = $2, locked_until = $3 WHERE id = $1")
            .bind(user.id)
            .bind(if lock.is_some() { 0 } else { attempts })
            .bind(lock)
            .execute(&mut *tx)
            .await?;
        // Failed sign-ins are part of the activity the business and the platform owner can review.
        audit::system(
            &mut tx,
            user.tenant_id,
            Some(user.id),
            Entry::new("auth", "login_failed", "user", user.id).after(serde_json::json!({ "attempt": attempts, "locked": lock.is_some() })),
            &ip,
            &ua,
        )
        .await?;
        tx.commit().await?;
        return Err(invalid());
    }
    // A one-time PIN works once, for a limited time (the person proved they hold it, so saying so leaks nothing).
    if user.must_change_pin && user.pin_expires_at.is_some_and(|t| t < Utc::now()) {
        return Err(crate::error::refused(
            "One-time PIN expired",
            format!(
                "This one-time PIN has expired. Use “Forgot PIN / Password?” to set a new one, or contact S'Shop support: {} / {}",
                super::access::SUPPORT_PHONES[0],
                super::access::SUPPORT_PHONES[1]
            ),
        ));
    }
    let tenant_status: String = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1").bind(user.tenant_id).fetch_one(&state.db).await?;
    if tenant_status != "active" {
        return Err(crate::error::refused(
            "Business deactivated",
            format!(
                "Access to this business has been suspended. Contact S'Shop support: {} / {}",
                super::access::SUPPORT_PHONES[0],
                super::access::SUPPORT_PHONES[1]
            ),
        ));
    }

    sqlx::query("UPDATE users SET failed_attempts = 0, locked_until = NULL, last_login_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;

    sqlx::query(
        "INSERT INTO audit_log (tenant_id, user_id, module, action, entity_type, entity_id, ip, user_agent)
         VALUES ($1,$2,'auth','login','user',$2,$3,$4)",
    )
    .bind(user.tenant_id)
    .bind(user.id)
    .bind(ip)
    .bind(ua)
    .execute(&state.db)
    .await?;

    let token = issue_token(&state.cfg.jwt_secret, user.id, user.tenant_id, "staff", Duration::hours(STAFF_TOKEN_HOURS))?;
    let profile = load_profile(&state, user.id, user.tenant_id, None).await?;
    Ok(Json(LoginResponse { token, profile }))
}

#[derive(Serialize)]
pub struct Profile {
    user: Value,
    tenant: Value,
    branches: Vec<Value>,
    permissions: Vec<String>,
    settings: Value,
    integrations: Value,
    billing: Value,
    /// Present while a platform admin works inside another business.
    acting: Option<Value>,
}

/// `acting`: the platform admin's own business when they have opened `tenant_id` from the platform.
pub async fn load_profile(state: &AppState, user_id: Uuid, tenant_id: Uuid, acting: Option<Uuid>) -> AppResult<Profile> {
    let (name, email, role, permissions, all_branches, preferences): (String, String, String, Vec<String>, bool, Value) = sqlx::query_as(
        "SELECT u.name, u.email, r.name, r.permissions || u.extra_permissions, u.all_branches, u.preferences FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    let is_admin = permissions.iter().any(|p| p == "*");

    let branches: Vec<(Uuid, String, String, String, Option<Value>, bool, Option<f64>, Option<f64>, i32)> = if all_branches || is_admin {
        sqlx::query_as("SELECT id, name, code, location, hours, geofence_enabled, latitude, longitude, geofence_radius_m FROM branches WHERE tenant_id = $1 AND is_active ORDER BY created_at")
            .bind(tenant_id)
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT b.id, b.name, b.code, b.location, b.hours, b.geofence_enabled, b.latitude, b.longitude, b.geofence_radius_m FROM user_branches ub JOIN branches b ON b.id = ub.branch_id
             WHERE ub.user_id = $1 AND b.is_active ORDER BY b.created_at",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await?
    };

    let (tname, slug, tagline, currency, has_logo, settings, is_demo, timezone): (String, String, String, String, bool, Value, bool, String) = sqlx::query_as(
        "SELECT name, slug, tagline, currency, logo IS NOT NULL, settings, is_demo, timezone FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await?;
    let settings: crate::settings::TenantSettings = serde_json::from_value(settings).unwrap_or_default();
    let access = {
        let mut conn = state.db.acquire().await?;
        crate::billing::access(&mut conn, tenant_id).await?
    };

    Ok(Profile {
        user: serde_json::json!({
            "id": user_id, "name": name, "email": email, "role": role, "all_branches": all_branches || is_admin,
            "platform_admin": super::access::is_platform_admin(state, &email, &permissions),
            "preferences": super::prefs::Preferences::from_stored(preferences),
            // Signed in with a one-time PIN: the app asks for a new PIN before anything else (roadmap 59).
            "must_change_pin": sqlx::query_scalar::<_, bool>("SELECT must_change_pin FROM users WHERE id = $1").bind(user_id).fetch_one(&state.db).await?,
        }),
        tenant: serde_json::json!({
            "id": tenant_id, "name": tname, "slug": slug, "tagline": tagline, "currency": currency,
            "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")), "is_demo": is_demo, "timezone": timezone,
        }),
        // Package and billing access (roadmap 41–45): modules = null means every module. The platform owner acting
        // inside a business is not restricted.
        billing: serde_json::json!({
            "modules": if acting.is_some() { None } else { access.modules() },
            "suspended": acting.is_none() && access.suspended(),
            "ownership": access.ownership,
            "access_mode": access.access_mode,
            "trial_end": access.trial_end,
            "catalogue": crate::billing::modules_catalogue(),
        }),
        acting: match acting {
            Some(home) => {
                let name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(home).fetch_one(&state.db).await?;
                Some(serde_json::json!({ "home_tenant_id": home, "home_tenant_name": name }))
            }
            None => None,
        },
        branches: branches
            .into_iter()
            // Effective trading hours (own, else the business hours) for the open/closed banner at the till.
            .map(|(id, name, code, location, hours, fenced, lat, lng, radius)| serde_json::json!({
                "id": id, "name": name, "code": code, "location": location,
                "hours": crate::settings::effective_hours(hours.as_ref(), &settings), "own_hours": hours.is_some(),
                "geofence": fenced.then(|| serde_json::json!({ "latitude": lat, "longitude": lng, "radius_m": radius })),
            }))
            .collect(),
        permissions,
        settings: serde_json::to_value(settings).unwrap_or_default(),
        integrations: serde_json::json!({
            "mpesa_stk": state.cfg.mpesa.is_some(),
            "whatsapp": state.cfg.whatsapp.is_some(),
        }),
    })
}

async fn me(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Profile>> {
    Ok(Json(load_profile(&state, ctx.user_id, ctx.tenant_id, ctx.acting_from).await?))
}

#[derive(Deserialize)]
struct ChangePin {
    current_pin: String,
    new_pin: String,
}

async fn change_pin(State(state): State<AppState>, ctx: Ctx, Json(body): Json<ChangePin>) -> AppResult<Json<Value>> {
    validate_pin(&body.new_pin)?;
    let hash: String = sqlx::query_scalar("SELECT pin_hash FROM users WHERE id = $1")
        .bind(ctx.user_id)
        .fetch_one(&state.db)
        .await?;
    if !verify_pin(&body.current_pin, &hash) {
        return Err(bad("Current PIN is incorrect"));
    }
    if body.new_pin == body.current_pin {
        return Err(bad("Choose a PIN different from the current one"));
    }
    let mut tx = state.db.begin().await?;
    // Other sessions end; this device continues with the fresh token returned below.
    sqlx::query(
        "UPDATE users SET pin_hash = $2, must_change_pin = false, pin_expires_at = NULL, pin_changed_at = now(), sessions_valid_after = now()
         WHERE id = $1",
    )
    .bind(ctx.user_id)
    .bind(hash_pin(&body.new_pin)?)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE auth_tokens SET revoked_at = now() WHERE user_id = $1 AND kind IN ('setup', 'reset') AND used_at IS NULL AND revoked_at IS NULL")
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "change_pin", "user", ctx.user_id)).await?;
    tx.commit().await?;
    let token = match ctx.acting_from {
        Some(home) => crate::auth::issue_acting_token(&state.cfg.jwt_secret, ctx.user_id, home, ctx.tenant_id)?,
        None => issue_token(&state.cfg.jwt_secret, ctx.user_id, ctx.tenant_id, "staff", Duration::hours(STAFF_TOKEN_HOURS))?,
    };
    Ok(Json(serde_json::json!({ "ok": true, "token": token })))
}
