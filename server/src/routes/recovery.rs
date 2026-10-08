//! Self-service account recovery and applicant status (roadmap 60–61), and Resend delivery events.
//!
//! * **Forgot PIN / Password** — anyone enters an email; the answer is always the same, whether or not the email is
//!   known. A registered user receives a single-use reset link (30 min); someone who applied for access receives a
//!   link to their request's status instead. Nothing else is revealed.
//! * **Set PIN** — a set-up (welcome) or reset link lets the person choose a new PIN; the link is used up, other open
//!   links stop working, all existing sessions end, and a confirmation email is sent.
//! * **Request status** — only reachable through the emailed link (proof the person owns the address): pending /
//!   approved / rejected, support contacts, and *Resend set-up instructions* for approved administrators who have not
//!   set their PIN yet.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use super::access::SUPPORT_PHONES;
use crate::audit::{self, Entry};
use crate::auth::{client_meta, hash_pin, validate_pin};
use crate::error::{bad, rule, AppError, AppResult};
use crate::mailer::{self, button, esc, layout, p, support_text, Mail};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/forgot", post(forgot))
        .route("/auth/link", post(link_info))
        .route("/auth/set-pin", post(set_pin))
        .route("/auth/request-status", post(request_status))
        .route("/auth/request-status/resend-setup", post(resend_setup))
        .route("/webhooks/resend", post(resend_webhook))
}

const NEUTRAL: &str = "If this email is registered, we'll send you a secure link to reset your PIN or password. If you applied for access, we'll email you your request's status instead.";

#[derive(Deserialize)]
struct ForgotBody {
    email: String,
}

async fn forgot(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<ForgotBody>) -> AppResult<Json<Value>> {
    let (ip, ua) = client_meta(&headers);
    state.limits.check(&ip, "forgot_ip", 10, std::time::Duration::from_secs(900))?;
    let email = b.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 160 {
        return Err(bad("Enter the email you sign in with"));
    }
    // Per address: at most 3 emails per 15 minutes; extra requests get the same answer and send nothing.
    let allowed = state.limits.check(&email, "forgot_email", 3, std::time::Duration::from_secs(900)).is_ok();
    if allowed {
        // Work happens after the response so timing reveals nothing either.
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = forgot_work(&st, &email, &ip, &ua).await {
                tracing::warn!(error = %e, "forgot PIN processing failed");
            }
        });
    }
    Ok(Json(json!({ "ok": true, "message": NEUTRAL, "support_phones": SUPPORT_PHONES })))
}

async fn forgot_work(state: &AppState, email: &str, ip: &str, ua: &str) -> AppResult<()> {
    let user: Option<(Uuid, Uuid, String, bool)> = sqlx::query_as("SELECT id, tenant_id, name, is_active FROM users WHERE lower(email) = $1 AND login_user_id IS NULL")
        .bind(email)
        .fetch_optional(&state.db)
        .await?;
    if let Some((user_id, tenant_id, name, active)) = user {
        if !active {
            return Ok(()); // a deactivated account is restored by its administrator, not by email
        }
        let mut tx = state.db.begin().await?;
        let (token, _) = mailer::issue_token(&mut tx, "reset", Some(user_id), None, None, ip).await?;
        audit::system(&mut tx, tenant_id, Some(user_id), Entry::new("auth", "reset_requested", "user", user_id), ip, ua).await?;
        tx.commit().await?;
        let link = mailer::link(state, "set-pin", &token);
        let first = name.split_whitespace().next().unwrap_or(&name).to_string();
        let mut body = p(&format!("Hello {},", esc(&first)));
        body.push_str(&p("We received a request to reset the PIN / password for your S'Shop account. Use the button below to choose a new one. The link works once and expires in 30 minutes."));
        body.push_str(&button("Reset my PIN", &link));
        body.push_str(&p("If you did not ask for this, you can ignore this email — your current PIN keeps working."));
        mailer::send(
            state,
            Mail {
                kind: "pin_reset",
                to: vec![email.to_string()],
                subject: "Reset your S'Shop PIN".into(),
                html: layout("Your secure link to reset your S'Shop PIN", "Reset your PIN", &body),
                text: format!(
                    "Hello {first},\n\nWe received a request to reset the PIN / password for your S'Shop account. Use this link to choose a new one (works once, expires in 30 minutes):\n{link}\n\nIf you did not ask for this, ignore this email — your current PIN keeps working.\n\n{}",
                    support_text()
                ),
                tenant_id: Some(tenant_id),
                access_request_id: None,
                user_id: Some(user_id),
                created_by: None,
                retry_of: None,
            },
        )
        .await;
        return Ok(());
    }
    // Not a user: an applicant? Send a link to their latest request's status.
    let request: Option<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, contact_name, business_name FROM access_requests WHERE lower(email) = $1 ORDER BY created_at DESC LIMIT 1")
            .bind(email)
            .fetch_optional(&state.db)
            .await?;
    if let Some((request_id, contact, business)) = request {
        let mut conn = state.db.acquire().await?;
        let (token, _) = mailer::issue_token(&mut conn, "status", None, Some(request_id), None, ip).await?;
        drop(conn);
        let link = mailer::link(state, "access-status", &token);
        let first = contact.split_whitespace().next().unwrap_or(&contact).to_string();
        let mut body = p(&format!("Hello {},", esc(&first)));
        body.push_str(&p(&format!("You asked about your S'Shop access request for <b>{}</b>. Open the secure link below to see its current status (valid for 30 minutes).", esc(&business))));
        body.push_str(&button("View my request status", &link));
        mailer::send(
            state,
            Mail {
                kind: "request_status",
                to: vec![email.to_string()],
                subject: "Your S'Shop access request status".into(),
                html: layout("See the status of your S'Shop access request", "Your access request", &body),
                text: format!("Hello {first},\n\nOpen this secure link to see the status of your S'Shop access request for {business} (valid for 30 minutes):\n{link}\n\n{}", support_text()),
                tenant_id: None,
                access_request_id: Some(request_id),
                user_id: None,
                created_by: None,
                retry_of: None,
            },
        )
        .await;
    }
    Ok(())
}

#[derive(Deserialize)]
struct TokenBody {
    token: String,
}

/// What a set-up / reset link is for (before the person types a new PIN). The email is masked.
async fn link_info(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<TokenBody>) -> AppResult<Json<Value>> {
    state.limits.check(&client_meta(&headers).0, "auth_link", 60, std::time::Duration::from_secs(900))?;
    let mut conn = state.db.acquire().await?;
    let t = mailer::find_token(&mut conn, &b.token, &["setup", "reset"], false).await?;
    let (name, email, business): (String, String, String) =
        sqlx::query_as("SELECT u.name, u.email, t.name FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = $1")
            .bind(t.user_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(Json(json!({
        "kind": t.kind, "name": name.split_whitespace().next().unwrap_or(&name), "email": mailer::mask_email(&email), "business": business,
        "expires_at": t.expires_at,
    })))
}

#[derive(Deserialize)]
struct SetPinBody {
    token: String,
    pin: String,
}

async fn set_pin(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<SetPinBody>) -> AppResult<Json<Value>> {
    let (ip, ua) = client_meta(&headers);
    state.limits.check(&ip, "auth_set_pin", 20, std::time::Duration::from_secs(900))?;
    validate_pin(&b.pin)?;
    let mut tx = state.db.begin().await?;
    let t = mailer::find_token(&mut tx, &b.token, &["setup", "reset"], true).await?;
    let user_id = t.user_id.ok_or(AppError::NotFound("User"))?;
    let (tenant_id, name, email, active): (Uuid, String, String, bool) =
        sqlx::query_as("SELECT tenant_id, name, email, is_active FROM users WHERE id = $1 FOR UPDATE").bind(user_id).fetch_one(&mut *tx).await?;
    if !active {
        return Err(rule("This account is deactivated — contact your administrator"));
    }
    sqlx::query(
        "UPDATE users SET pin_hash = $2, must_change_pin = false, pin_expires_at = NULL, pin_changed_at = now(), sessions_valid_after = now(),
                failed_attempts = 0, locked_until = NULL WHERE id = $1",
    )
    .bind(user_id)
    .bind(hash_pin(&b.pin)?)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE auth_tokens SET used_at = now() WHERE id = $1").bind(t.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE auth_tokens SET revoked_at = now() WHERE user_id = $1 AND used_at IS NULL AND revoked_at IS NULL").bind(user_id).execute(&mut *tx).await?;
    let action = if t.kind == "setup" { "setup_completed" } else { "reset_completed" };
    audit::system(&mut tx, tenant_id, Some(user_id), Entry::new("auth", action, "user", user_id), &ip, &ua).await?;
    tx.commit().await?;

    let first = name.split_whitespace().next().unwrap_or(&name).to_string();
    let sign_in = format!("{}/login", state.cfg.public_url);
    let mut body = p(&format!("Hello {},", esc(&first)));
    body.push_str(&p("The PIN / password for your S'Shop account was just changed. Other devices have been signed out."));
    body.push_str(&button("Sign in", &sign_in));
    body.push_str(&p("<b>Didn't do this?</b> Use “Forgot PIN / Password?” on the sign-in page straight away and contact S'Shop Support."));
    mailer::send(
        &state,
        Mail {
            kind: "pin_changed",
            to: vec![email.clone()],
            subject: "Your S'Shop PIN was changed".into(),
            html: layout("Your S'Shop PIN was changed", "PIN changed", &body),
            text: format!(
                "Hello {first},\n\nThe PIN / password for your S'Shop account was just changed. Other devices have been signed out.\n\nSign in: {sign_in}\n\nDidn't do this? Use “Forgot PIN / Password?” on the sign-in page straight away and contact S'Shop Support.\n\n{}",
                support_text()
            ),
            tenant_id: Some(tenant_id),
            access_request_id: None,
            user_id: Some(user_id),
            created_by: None,
            retry_of: None,
        },
    )
    .await;
    Ok(Json(json!({ "ok": true, "email": email })))
}

/// The applicant's request status — reachable only through the emailed link.
async fn request_status(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<TokenBody>) -> AppResult<Json<Value>> {
    state.limits.check(&client_meta(&headers).0, "auth_link", 60, std::time::Duration::from_secs(900))?;
    let mut conn = state.db.acquire().await?;
    let t = mailer::find_token(&mut conn, &b.token, &["status"], false).await?;
    let (status, business, contact, admin): (String, String, String, Option<Uuid>) =
        sqlx::query_as("SELECT status, business_name, contact_name, admin_user_id FROM access_requests WHERE id = $1")
            .bind(t.access_request_id)
            .fetch_one(&mut *conn)
            .await?;
    // Approved: has the administrator set their own PIN yet?
    let awaiting_setup = match admin {
        Some(u) => sqlx::query_scalar::<_, bool>("SELECT must_change_pin FROM users WHERE id = $1 AND is_active").bind(u).fetch_optional(&mut *conn).await?.unwrap_or(false),
        None => false,
    };
    let state_key = match (status.as_str(), awaiting_setup) {
        ("approved", true) => "approved_setup",
        ("approved", false) => "approved_active",
        (s, _) => if s == "rejected" { "rejected" } else { "pending" },
    };
    Ok(Json(json!({
        "status": state_key, "business": business, "name": contact.split_whitespace().next().unwrap_or(&contact),
        "support_phones": SUPPORT_PHONES, "sign_in_url": format!("{}/login", state.cfg.public_url),
    })))
}

/// Approved but not set up: send the set-up email again (rate-limited per request).
async fn resend_setup(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<TokenBody>) -> AppResult<Json<Value>> {
    state.limits.check(&client_meta(&headers).0, "auth_link", 60, std::time::Duration::from_secs(900))?;
    let request_id = {
        let mut conn = state.db.acquire().await?;
        mailer::find_token(&mut conn, &b.token, &["status"], false).await?.access_request_id.ok_or(AppError::NotFound("Access request"))?
    };
    state.limits.check(&request_id.to_string(), "applicant_resend_setup", 3, std::time::Duration::from_secs(3600))?;
    let out = super::access::resend_setup_for_applicant(&state, request_id).await?;
    Ok(Json(json!({ "ok": true, "sent": out.status == "sent" })))
}

// ───────────────────────────── Resend delivery events ─────────────────────────────

/// Resend (Svix) webhook: `svix-id`, `svix-timestamp`, `svix-signature: v1,<base64 HMAC-SHA256>` over
/// `{id}.{timestamp}.{body}` with the base64 secret after `whsec_`. Unsigned or stale events are refused.
async fn resend_webhook(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> AppResult<Json<Value>> {
    let secret = state.cfg.email.as_ref().and_then(|c| c.webhook_secret.clone()).ok_or(AppError::NotFound("Webhook"))?;
    let h = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let (id, ts, sigs) = (h("svix-id"), h("svix-timestamp"), h("svix-signature"));
    if !verify_svix(&secret, &id, &ts, &body, &sigs, chrono::Utc::now().timestamp()) {
        return Err(AppError::Unauthorized);
    }
    let v: Value = serde_json::from_slice(&body).map_err(|_| bad("Unreadable event"))?;
    if let (Some(kind), Some(email_id)) = (v["type"].as_str(), v["data"]["email_id"].as_str()) {
        mailer::apply_event(&state, email_id, kind).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub fn verify_svix(secret: &str, id: &str, ts: &str, body: &[u8], sigs: &str, now: i64) -> bool {
    let Ok(t) = ts.parse::<i64>() else { return false };
    if (now - t).abs() > 300 || id.is_empty() {
        return false;
    }
    let key = secret.strip_prefix("whsec_").unwrap_or(secret);
    let Ok(key) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, key) else { return false };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&key) else { return false };
    mac.update(format!("{id}.{ts}.").as_bytes());
    mac.update(body);
    let expected = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, mac.finalize().into_bytes());
    sigs.split_whitespace().filter_map(|s| s.strip_prefix("v1,")).any(|s| constant_eq(s.as_bytes(), expected.as_bytes()))
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svix_signatures() {
        let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let key = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, "MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw").unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        mac.update(b"msg_1.1700000000.{\"a\":1}");
        let sig = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, mac.finalize().into_bytes());
        let header = format!("v1,bogus v1,{sig}");
        assert!(verify_svix(secret, "msg_1", "1700000000", b"{\"a\":1}", &header, 1_700_000_100));
        assert!(!verify_svix(secret, "msg_1", "1700000000", b"{\"a\":2}", &header, 1_700_000_100));
        assert!(!verify_svix(secret, "msg_1", "1700000000", b"{\"a\":1}", &header, 1_700_009_999));
    }
}
