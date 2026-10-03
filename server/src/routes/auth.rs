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
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    profile: Profile,
}

async fn login(State(state): State<AppState>, headers: HeaderMap, Json(body): Json<LoginBody>) -> AppResult<Json<LoginResponse>> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || body.pin.is_empty() {
        return Err(bad("Enter your email and PIN"));
    }
    let invalid = || AppError::BadRequest("Invalid email or PIN".into());

    let row: Option<LoginRow> = sqlx::query_as(
        "SELECT id, tenant_id, pin_hash, is_active, failed_attempts, locked_until FROM users WHERE lower(email) = $1",
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

    if !verify_pin(&body.pin, &user.pin_hash) {
        let attempts = user.failed_attempts + 1;
        let lock = (attempts >= MAX_ATTEMPTS).then(|| Utc::now() + Duration::minutes(LOCK_MINUTES));
        sqlx::query("UPDATE users SET failed_attempts = $2, locked_until = $3 WHERE id = $1")
            .bind(user.id)
            .bind(if lock.is_some() { 0 } else { attempts })
            .bind(lock)
            .execute(&state.db)
            .await?;
        return Err(invalid());
    }

    sqlx::query("UPDATE users SET failed_attempts = 0, locked_until = NULL, last_login_at = now() WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;

    let (ip, ua) = client_meta(&headers);
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
    let profile = load_profile(&state, user.id, user.tenant_id).await?;
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
}

async fn load_profile(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> AppResult<Profile> {
    let (name, email, role, permissions, all_branches): (String, String, String, Vec<String>, bool) = sqlx::query_as(
        "SELECT u.name, u.email, r.name, r.permissions, u.all_branches FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    let is_admin = permissions.iter().any(|p| p == "*");

    let branches: Vec<(Uuid, String, String, String)> = if all_branches || is_admin {
        sqlx::query_as("SELECT id, name, code, location FROM branches WHERE tenant_id = $1 AND is_active ORDER BY created_at")
            .bind(tenant_id)
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as(
            "SELECT b.id, b.name, b.code, b.location FROM user_branches ub JOIN branches b ON b.id = ub.branch_id
             WHERE ub.user_id = $1 AND b.is_active ORDER BY b.created_at",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await?
    };

    let (tname, slug, tagline, currency, has_logo, settings): (String, String, String, String, bool, Value) = sqlx::query_as(
        "SELECT name, slug, tagline, currency, logo IS NOT NULL, settings FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await?;
    let settings: crate::settings::TenantSettings = serde_json::from_value(settings).unwrap_or_default();

    Ok(Profile {
        user: serde_json::json!({ "id": user_id, "name": name, "email": email, "role": role, "all_branches": all_branches || is_admin }),
        tenant: serde_json::json!({
            "id": tenant_id, "name": tname, "slug": slug, "tagline": tagline, "currency": currency,
            "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")),
        }),
        branches: branches
            .into_iter()
            .map(|(id, name, code, location)| serde_json::json!({ "id": id, "name": name, "code": code, "location": location }))
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
    Ok(Json(load_profile(&state, ctx.user_id, ctx.tenant_id).await?))
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
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET pin_hash = $2 WHERE id = $1")
        .bind(ctx.user_id)
        .bind(hash_pin(&body.new_pin)?)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "change_pin", "user", ctx.user_id)).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
