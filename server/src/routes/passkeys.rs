//! Face ID & biometric sign-in (roadmap 132) with passkeys (WebAuthn), verified by `webauthn-rs`.
//!
//! - **No biometric data, ever**: the phone or computer checks the face, fingerprint or screen lock itself and signs a
//!   one-time challenge; S'Shop stores only the passkey's public key and receives only that signature.
//! - **Belongs to the person** (their sign-in account — never a linked row in another business). Setting one up needs a
//!   full sign-in *and* the full PIN re-entered.
//! - **Challenges** are random, single use and expire in 5 minutes; the origin, the RP ID (S'Shop's domain) and user
//!   verification are checked on every ceremony, and the signature counter is kept to spot a cloned passkey.
//! - **Limited sessions**, like the Quick PIN: sensitive actions still ask for the full PIN (`Ctx::require_full`).
//! - **Within rules**: the platform (on/off) and the business (on/off, which roles), re-checked at every sign-in.
//! - **Independent of the Quick PIN**; ends when the person removes it, when the full PIN changes or is reset, or when
//!   they sign out everywhere.

use std::sync::OnceLock;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use webauthn_rs::prelude::{
    Passkey, PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential, Url, Webauthn, WebauthnBuilder,
};

use super::quickpin::{check_full_pin, device_name, identity, platform_security, tenant_rules};
use crate::audit::{self, Entry};
use crate::auth::{client_meta, Ctx};
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::state::AppState;

/// How long a registration or sign-in challenge may be answered.
const CHALLENGE_MINUTES: i32 = 5;
/// Passkeys one person may keep (phones, computers, security keys).
const MAX_PASSKEYS: i64 = 10;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/passkeys", get(status))
        .route("/auth/passkeys/register/start", post(register_start))
        .route("/auth/passkeys/register/finish", post(register_finish))
        .route("/auth/passkeys/{id}", delete(remove))
        .route("/auth/passkeys/disable", post(disable))
        .route("/auth/passkey/login/start", post(login_start))
        .route("/auth/passkey/login/finish", post(login_finish))
}

/// The relying party: S'Shop's own domain (`WEBAUTHN_RP_ID`, else the host of `PUBLIC_URL`). A passkey made here only
/// ever works here — the browser refuses it on any other site.
fn webauthn(state: &AppState) -> AppResult<&'static Webauthn> {
    static W: OnceLock<Option<Webauthn>> = OnceLock::new();
    W.get_or_init(|| {
        let origin = Url::parse(&state.cfg.public_url).ok()?;
        let rp_id = std::env::var("WEBAUTHN_RP_ID").ok().filter(|v| !v.trim().is_empty()).or_else(|| origin.host_str().map(str::to_string))?;
        let local = rp_id == "localhost";
        let built = WebauthnBuilder::new(&rp_id, &origin)
            .map(|b| b.rp_name("S'Shop").allow_subdomains(false).allow_any_port(local))
            .and_then(|b| b.build());
        match built {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::error!(error = %e, rp_id, "passkeys unavailable: the WebAuthn relying party could not be set up");
                None
            }
        }
    })
    .as_ref()
    .ok_or_else(|| refused("Face ID & biometrics unavailable", "Biometric sign-in is not set up on this server"))
}

/// The relying party's domain, for the app to know where passkeys work (it is in every challenge anyway).
pub fn rp_id(state: &AppState) -> Option<String> {
    webauthn(state).ok().and_then(|w| w.get_allowed_origins().first().and_then(|o| o.host_str().map(str::to_string)))
}

/// Why this person may not use biometric sign-in, if they may not (platform off, business off, role not included).
async fn not_allowed(state: &AppState, identity: Uuid) -> AppResult<Option<&'static str>> {
    if !platform_security(&state.db).await?.passkeys_enabled {
        return Ok(Some("Face ID & biometric sign-in is switched off on S'Shop"));
    }
    let (tenant, role): (Uuid, Uuid) = sqlx::query_as("SELECT tenant_id, role_id FROM users WHERE id = $1").bind(identity).fetch_one(&state.db).await?;
    let t = tenant_rules(&state.db, tenant).await?;
    if !t.passkeys {
        return Ok(Some("Your business does not use Face ID & biometric sign-in"));
    }
    if !t.role_ids.is_empty() && !t.role_ids.contains(&role) {
        return Ok(Some("Face ID & biometric sign-in is not available for your role"));
    }
    Ok(None)
}

/// Every passkey of this person stops working (full PIN changed or reset, signed out everywhere).
pub async fn revoke_all(conn: &mut sqlx::PgConnection, user: Uuid, reason: &str, by: Option<Uuid>) -> AppResult<u64> {
    sqlx::query("DELETE FROM passkey_challenges WHERE user_id = $1").bind(user).execute(&mut *conn).await?;
    Ok(sqlx::query("UPDATE passkeys SET revoked_at = now(), revoked_by = $3, revoke_reason = $2 WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user)
        .bind(reason)
        .bind(by)
        .execute(&mut *conn)
        .await?
        .rows_affected())
}

fn cred_key(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

/// Stores a ceremony's state; only its id goes to the browser. Expired ones are cleared on the way.
async fn save_challenge(state: &AppState, user: Uuid, kind: &str, st: Value, passkey: Option<Uuid>) -> AppResult<Uuid> {
    sqlx::query("DELETE FROM passkey_challenges WHERE expires_at < now()").execute(&state.db).await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO passkey_challenges (user_id, kind, state, passkey_id, expires_at) VALUES ($1, $2, $3, $4, now() + make_interval(mins => $5)) RETURNING id",
    )
    .bind(user)
    .bind(kind)
    .bind(st)
    .bind(passkey)
    .bind(CHALLENGE_MINUTES)
    .fetch_one(&state.db)
    .await?)
}

/// Takes a challenge (single use: it is deleted whether or not the answer is right).
async fn take_challenge(state: &AppState, id: Uuid, kind: &str) -> AppResult<(Uuid, Value, Option<Uuid>)> {
    let row: Option<(Uuid, Value, Option<Uuid>, DateTime<Utc>)> =
        sqlx::query_as("DELETE FROM passkey_challenges WHERE id = $1 AND kind = $2 RETURNING user_id, state, passkey_id, expires_at")
            .bind(id)
            .bind(kind)
            .fetch_optional(&state.db)
            .await?;
    match row {
        Some((user, st, passkey, expires)) if expires > Utc::now() => Ok((user, st, passkey)),
        _ => Err(refused("Request expired", "That request has expired — please try again")),
    }
}

// ── The person's own passkeys ───────────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct PasskeyRow {
    id: Uuid,
    credential_id: String,
    name: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

async fn status(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let passkeys: Vec<PasskeyRow> = sqlx::query_as(
        "SELECT id, credential_id, name, created_at, last_used_at FROM passkeys WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC",
    )
    .bind(ident)
    .fetch_all(&state.db)
    .await?;
    let reason = match webauthn(&state) {
        Err(_) => Some("Biometric sign-in is not set up on this server"),
        Ok(_) => not_allowed(&state, ident).await?,
    };
    Ok(Json(json!({
        "available": reason.is_none(), "reason": reason, "enabled": !passkeys.is_empty(),
        "passkeys": passkeys, "quick_session": ctx.quick, "rp_id": rp_id(&state),
    })))
}

#[derive(Deserialize)]
struct RegisterStart {
    current_pin: String,
}

/// Step 1 of setting up: full sign-in + the full PIN re-entered, then the browser asks the device to create a passkey.
async fn register_start(State(state): State<AppState>, ctx: Ctx, Json(b): Json<RegisterStart>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    let ident = identity(&state, &ctx).await?;
    let w = webauthn(&state)?;
    if let Some(why) = not_allowed(&state, ident).await? {
        return Err(refused("Face ID & biometrics unavailable", why));
    }
    check_full_pin(&state, ident, &b.current_pin).await?;
    let (email, name): (String, String) = sqlx::query_as("SELECT email, name FROM users WHERE id = $1").bind(ident).fetch_one(&state.db).await?;
    let existing: Vec<Value> = sqlx::query_scalar("SELECT passkey FROM passkeys WHERE user_id = $1 AND revoked_at IS NULL").bind(ident).fetch_all(&state.db).await?;
    if existing.len() as i64 >= MAX_PASSKEYS {
        return Err(rule(format!("You can keep up to {MAX_PASSKEYS} devices — remove one first")));
    }
    // Devices already set up are excluded, so the same device is not added twice.
    let exclude: Vec<_> = existing.into_iter().filter_map(|v| serde_json::from_value::<Passkey>(v).ok()).map(|p| p.cred_id().clone()).collect();
    let (options, reg) = w
        .start_passkey_registration(ident, &email, &name, Some(exclude))
        .map_err(|e| {
            tracing::warn!(error = %e, "passkey registration could not start");
            bad("Biometric set-up could not start — please try again")
        })?;
    let id = save_challenge(&state, ident, "register", json!(reg), None).await?;
    Ok(Json(json!({ "challenge_id": id, "options": options })))
}

#[derive(Deserialize)]
struct RegisterFinish {
    challenge_id: Uuid,
    credential: RegisterPublicKeyCredential,
    #[serde(default)]
    device_name: String,
}

/// Step 2: the device's answer is verified (origin, RP ID, challenge, user verification) and its public key kept.
async fn register_finish(State(state): State<AppState>, ctx: Ctx, headers: HeaderMap, Json(b): Json<RegisterFinish>) -> AppResult<Json<Value>> {
    ctx.require_full()?;
    let ident = identity(&state, &ctx).await?;
    let w = webauthn(&state)?;
    let (owner, st, _) = take_challenge(&state, b.challenge_id, "register").await?;
    if owner != ident {
        return Err(refused("Request expired", "That request has expired — please try again"));
    }
    let reg: PasskeyRegistration = serde_json::from_value(st).map_err(|_| bad("That request has expired — please try again"))?;
    let passkey = w.finish_passkey_registration(&b.credential, &reg).map_err(|e| {
        tracing::warn!(error = %e, "passkey registration refused");
        bad("This device could not be set up for biometric sign-in — please try again")
    })?;
    let (_, ua) = client_meta(&headers);
    let key = cred_key(passkey.cred_id().as_ref());
    let mut tx = state.db.begin().await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO passkeys (user_id, credential_id, passkey, name, user_agent) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (credential_id) DO NOTHING RETURNING id",
    )
    .bind(ident)
    .bind(&key)
    .bind(json!(passkey))
    .bind(device_name(&b.device_name, &ua))
    .bind(ua.chars().take(200).collect::<String>())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = id else { return Err(rule("This device is already set up")) };
    audit::record(&mut tx, &ctx, Entry::new("auth", "passkey_added", "user", ident).after(json!({ "passkey": id, "name": device_name(&b.device_name, &ua) }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "id": id, "credential_id": key })))
}

async fn remove(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE passkeys SET revoked_at = now(), revoked_by = $2, revoke_reason = 'Removed by the person' WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL")
        .bind(id)
        .bind(ident)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Device"));
    }
    audit::record(&mut tx, &ctx, Entry::new("auth", "passkey_removed", "user", ident).after(json!({ "passkey": id }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Switch biometric sign-in off: every device of this person is removed.
async fn disable(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let ident = identity(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let n = revoke_all(&mut tx, ident, "Switched off by the person", Some(ident)).await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "passkeys_disabled", "user", ident).after(json!({ "devices_removed": n }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "devices_removed": n })))
}

// ── Biometric sign-in ───────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoginStart {
    /// The passkey this device set up (remembered by the browser); no email is asked or revealed.
    credential_id: String,
}

#[derive(sqlx::FromRow)]
struct SignInRow {
    passkey_id: Uuid,
    passkey: Value,
    user_id: Uuid,
    tenant_id: Uuid,
    is_active: bool,
    must_change_pin: bool,
    tenant_status: String,
}

/// A live passkey with its person and business, found by `cond` on `p` (credential id or passkey id) = $1.
async fn sign_in_row<T>(state: &AppState, cond: &str, value: T) -> AppResult<Option<SignInRow>>
where
    T: for<'q> sqlx::Encode<'q, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send,
{
    let sql = format!(
        "SELECT p.id AS passkey_id, p.passkey, u.id AS user_id, u.tenant_id, u.is_active, u.must_change_pin, t.status AS tenant_status
         FROM passkeys p JOIN users u ON u.id = p.user_id JOIN tenants t ON t.id = u.tenant_id
         WHERE p.revoked_at IS NULL AND u.login_user_id IS NULL AND {cond} = $1"
    );
    Ok(sqlx::query_as(&sql).bind(value).fetch_optional(&state.db).await?)
}

/// The account may sign in at all (active person, active business, no one-time PIN to replace, rules allow it).
async fn usable(state: &AppState, r: &SignInRow) -> AppResult<()> {
    if !r.is_active || r.must_change_pin || r.tenant_status != "active" {
        return Err(full_needed());
    }
    if let Some(why) = not_allowed(state, r.user_id).await? {
        return Err(refused("Face ID & biometrics unavailable", why));
    }
    Ok(())
}

fn full_needed() -> AppError {
    refused("Full sign-in needed", "Biometric sign-in is not available on this device — sign in with your email and PIN")
}

async fn login_start(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<LoginStart>) -> AppResult<Json<Value>> {
    let (ip, _) = client_meta(&headers);
    state.limits.check(&ip, "passkey_login", 30, std::time::Duration::from_secs(300))?;
    let w = webauthn(&state)?;
    let key: String = b.credential_id.trim().chars().take(512).collect();
    let r = sign_in_row(&state, "p.credential_id", key).await?.ok_or_else(full_needed)?;
    usable(&state, &r).await?;
    let passkey: Passkey = serde_json::from_value(r.passkey.clone()).map_err(|_| full_needed())?;
    let (options, auth) = w.start_passkey_authentication(&[passkey]).map_err(|e| {
        tracing::warn!(error = %e, "passkey sign-in could not start");
        bad("Biometric sign-in could not start — please try again")
    })?;
    let id = save_challenge(&state, r.user_id, "login", json!(auth), Some(r.passkey_id)).await?;
    Ok(Json(json!({ "challenge_id": id, "options": options })))
}

#[derive(Deserialize)]
struct LoginFinish {
    challenge_id: Uuid,
    credential: PublicKeyCredential,
}

async fn login_finish(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<LoginFinish>) -> AppResult<Json<Value>> {
    let (ip, ua) = client_meta(&headers);
    state.limits.check(&ip, "passkey_login", 30, std::time::Duration::from_secs(300))?;
    let w = webauthn(&state)?;
    let (user, st, passkey_id) = take_challenge(&state, b.challenge_id, "login").await?;
    let passkey_id = passkey_id.ok_or_else(full_needed)?;
    let r = sign_in_row(&state, "p.id", passkey_id).await?.filter(|r| r.user_id == user).ok_or_else(full_needed)?;
    usable(&state, &r).await?;
    let auth: PasskeyAuthentication = serde_json::from_value(st).map_err(|_| full_needed())?;
    let result = match w.finish_passkey_authentication(&b.credential, &auth) {
        Ok(res) if res.user_verified() => res,
        outcome => {
            if let Err(e) = &outcome {
                tracing::warn!(error = %e, "passkey sign-in refused");
            }
            let mut tx = state.db.begin().await?;
            audit::system(&mut tx, r.tenant_id, Some(r.user_id), Entry::new("auth", "passkey_login_failed", "user", r.user_id).after(json!({ "passkey": r.passkey_id })), &ip, &ua).await?;
            tx.commit().await?;
            return Err(bad("Face ID / biometric check failed — try again or sign in with your PIN"));
        }
    };
    // The signature counter moves on (a cloned passkey is refused by the verification above).
    let mut passkey: Passkey = serde_json::from_value(r.passkey.clone()).map_err(|_| full_needed())?;
    let changed = passkey.update_credential(&result).unwrap_or(false);
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE passkeys SET last_used_at = now(), passkey = CASE WHEN $2 THEN $3 ELSE passkey END WHERE id = $1")
        .bind(r.passkey_id)
        .bind(changed)
        .bind(json!(passkey))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1").bind(r.user_id).execute(&mut *tx).await?;
    audit::system(&mut tx, r.tenant_id, Some(r.user_id), Entry::new("auth", "login", "user", r.user_id).after(json!({ "method": "passkey", "passkey": r.passkey_id })), &ip, &ua).await?;
    tx.commit().await?;
    let token = crate::sessions::issue(&state, r.user_id, r.tenant_id, crate::sessions::Kind::Quick).await?;
    let mut profile = super::auth::load_profile(&state, r.user_id, r.tenant_id, None).await?;
    profile.quick = true;
    Ok(Json(json!({ "token": token, "profile": profile, "quick": true })))
}
