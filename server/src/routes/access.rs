//! Access requests (roadmap 7). There is no open signup: a prospective business submits a request,
//! the platform admins are emailed and notified in-app, and nothing is activated until a platform
//! admin approves it. Approval creates the business (default roles, main branch, settings) and its
//! first administrator with a one-time PIN that the admin passes on.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::{client_meta, Ctx};
use crate::error::{bad, rule, AppError, AppResult};
use crate::integrations::email;
use crate::notify::{self, Note};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/access-requests", post(submit))
        .route("/platform/access-requests", get(list))
        .route("/platform/access-requests/{id}/approve", post(approve))
        .route("/platform/access-requests/{id}/reject", post(reject))
}

pub const SUPPORT_PHONES: [&str; 2] = ["0798 993 404", "0732 968 898"];

#[derive(Deserialize)]
struct RequestBody {
    business_name: String,
    contact_name: String,
    email: String,
    phone: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    business_type: String,
    branches: Option<i32>,
    #[serde(default)]
    message: String,
    /// Honeypot: invisible to people, filled in by bots.
    #[serde(default)]
    website: String,
}

fn clip(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

async fn submit(State(state): State<AppState>, headers: HeaderMap, Json(b): Json<RequestBody>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "access_request", 6, std::time::Duration::from_secs(3600))?;
    let ok = json!({ "ok": true, "support_phones": SUPPORT_PHONES });
    if !b.website.trim().is_empty() {
        return Ok(Json(ok)); // quietly drop bot submissions
    }
    let business_name = clip(&b.business_name, 120);
    let contact_name = clip(&b.contact_name, 120);
    let email = clip(&b.email, 160).to_lowercase();
    let phone = clip(&b.phone, 40);
    if business_name.len() < 2 {
        return Err(bad("Enter your business name"));
    }
    if contact_name.len() < 2 {
        return Err(bad("Enter your name"));
    }
    if !email.contains('@') || !email.contains('.') || email.contains(' ') {
        return Err(bad("Enter a valid email address"));
    }
    if phone.chars().filter(|c| c.is_ascii_digit()).count() < 9 {
        return Err(bad("Enter a valid phone number"));
    }
    if let Some(n) = b.branches {
        if !(1..=500).contains(&n) {
            return Err(bad("Number of branches must be between 1 and 500"));
        }
    }
    let (ip, _) = client_meta(&headers);
    let recent: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_requests WHERE ip = $1 AND ip <> '' AND created_at > now() - interval '1 hour'")
        .bind(&ip)
        .fetch_one(&state.db)
        .await?;
    if recent >= 5 {
        return Err(rule("Too many requests from this connection — please try again later or call us"));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE lower(email) = $1)")
        .bind(&email)
        .fetch_one(&state.db)
        .await?;
    if exists {
        return Err(rule("This email already has access — sign in instead, or ask your administrator to reset your PIN"));
    }
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO access_requests (business_name, contact_name, email, phone, location, business_type, branches, message, ip)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT (lower(email)) WHERE status = 'pending' DO NOTHING RETURNING id",
    )
    .bind(&business_name)
    .bind(&contact_name)
    .bind(&email)
    .bind(&phone)
    .bind(clip(&b.location, 120))
    .bind(clip(&b.business_type, 60))
    .bind(b.branches)
    .bind(clip(&b.message, 1000))
    .bind(&ip)
    .fetch_optional(&state.db)
    .await?;
    // A repeat of a pending request is treated as success (no duplicate review item, no new email).
    if let Some(id) = id {
        let st = state.clone();
        tokio::spawn(async move { announce(&st, id).await });
    }
    Ok(Json(ok))
}

/// Emails and notifies the platform admins about a new request. Best-effort.
async fn announce(state: &AppState, id: Uuid) {
    let row: Result<(String, String, String, String, String, String, Option<i32>, String), _> = sqlx::query_as(
        "SELECT business_name, contact_name, email, phone, location, business_type, branches, message FROM access_requests WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await;
    let Ok((business, contact, email_addr, phone, location, kind, branches, message)) = row else { return };

    if let Some(cfg) = &state.cfg.email {
        if !state.cfg.access_request_notify.is_empty() {
            let text = format!(
                "New access request\n\nBusiness: {business}\nType: {kind}\nBranches: {}\nLocation: {location}\n\nContact: {contact}\nEmail: {email_addr}\nPhone: {phone}\n\nMessage:\n{}\n\nReview and activate: {}/settings/access-requests",
                branches.map(|n| n.to_string()).unwrap_or_else(|| "—".into()),
                if message.is_empty() { "—" } else { &message },
                state.cfg.public_url,
            );
            match email::send(&state.http, cfg, &state.cfg.access_request_notify, &format!("S'Shop access request: {business}"), &text).await {
                Ok(()) => {
                    let _ = sqlx::query("UPDATE access_requests SET email_sent = true WHERE id = $1").bind(id).execute(&state.db).await;
                }
                Err(e) => tracing::warn!(error = %e, "access request email failed"),
            }
        }
    } else {
        tracing::warn!("RESEND_API_KEY not set — access request {id} stored without an email notification");
    }

    // In-app notification for every platform admin, whichever business they sign in to.
    let admins: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT id, tenant_id FROM users WHERE is_active AND lower(email) = ANY($1)")
        .bind(&state.cfg.platform_admins)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (user, tenant) in admins {
        notify::to_users(state, tenant, &[user], Note::new("access_request", format!("Access request: {business}"), format!("{contact} · {phone}"), "/settings/access-requests")).await;
    }
}

// ───────────────────────────── Platform admin ─────────────────────────────

pub async fn require_platform_admin(state: &AppState, ctx: &Ctx) -> AppResult<()> {
    if !ctx.can("*") {
        return Err(AppError::Forbidden("Only platform administrators can review access requests".into()));
    }
    let email: String = sqlx::query_scalar("SELECT lower(email) FROM users WHERE id = $1").bind(ctx.user_id).fetch_one(&state.db).await?;
    if !state.cfg.platform_admins.contains(&email) {
        return Err(AppError::Forbidden("Only platform administrators can review access requests".into()));
    }
    Ok(())
}

pub fn is_platform_admin(state: &AppState, email: &str, permissions: &[String]) -> bool {
    permissions.iter().any(|p| p == "*") && state.cfg.platform_admins.contains(&email.to_lowercase())
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
}

#[derive(sqlx::FromRow, serde::Serialize)]
struct RequestRow {
    id: Uuid,
    business_name: String,
    contact_name: String,
    email: String,
    phone: String,
    location: String,
    business_type: String,
    branches: Option<i32>,
    message: String,
    status: String,
    tenant_id: Option<Uuid>,
    decided_by_name: Option<String>,
    decided_at: Option<DateTime<Utc>>,
    decision_note: String,
    email_sent: bool,
    created_at: DateTime<Utc>,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let status = q.status.unwrap_or_else(|| "pending".into());
    let rows: Vec<RequestRow> = sqlx::query_as(
        "SELECT a.id, a.business_name, a.contact_name, a.email, a.phone, a.location, a.business_type, a.branches, a.message,
                a.status, a.tenant_id, u.name AS decided_by_name, a.decided_at, a.decision_note, a.email_sent, a.created_at
         FROM access_requests a LEFT JOIN users u ON u.id = a.decided_by
         WHERE $1 = 'all' OR a.status = $1 ORDER BY a.created_at DESC LIMIT 200",
    )
    .bind(&status)
    .fetch_all(&state.db)
    .await?;
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_requests WHERE status = 'pending'").fetch_one(&state.db).await?;
    Ok(Json(json!({ "items": rows, "pending": pending, "email_configured": state.cfg.email.is_some() })))
}

/// One-time PIN for the new administrator: 8 characters without look-alikes (0/O, 1/l/I).
pub fn temporary_pin() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..8).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

async fn approve(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let (status, business, contact, email_addr, phone, location): (String, String, String, String, String, String) = sqlx::query_as(
        "SELECT status, business_name, contact_name, email, phone, location FROM access_requests WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("Access request"))?;
    if status != "pending" {
        return Err(rule(format!("This request was already {status}")));
    }
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE lower(email) = lower($1))")
        .bind(&email_addr)
        .fetch_one(&mut *tx)
        .await?;
    if taken {
        return Err(rule("A user with this email already exists — reject this request or ask them to sign in"));
    }

    // Unique slug for the business's ordering link.
    let base = crate::util::slugify(&business);
    let base = if base.is_empty() { "shop".to_string() } else { base };
    let mut slug = base.clone();
    let mut n = 2;
    while sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM tenants WHERE slug = $1)").bind(&slug).fetch_one(&mut *tx).await? {
        slug = format!("{base}-{n}");
        n += 1;
    }

    let tenant_id = crate::bootstrap::seed_tenant(&mut tx, &business, &slug).await.map_err(AppError::Other)?;
    sqlx::query("UPDATE tenants SET phone = $2, email = $3, address = $4 WHERE id = $1")
        .bind(tenant_id)
        .bind(&phone)
        .bind(&email_addr)
        .bind(&location)
        .execute(&mut *tx)
        .await?;
    let pin = temporary_pin();
    crate::bootstrap::create_admin(&mut tx, tenant_id, &contact, &email_addr, &pin).await.map_err(AppError::Other)?;
    sqlx::query("UPDATE access_requests SET status = 'approved', tenant_id = $2, decided_by = $3, decided_at = now() WHERE id = $1")
        .bind(id)
        .bind(tenant_id)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("platform", "approve_access", "access_request", id).after(json!({ "business": business, "email": email_addr, "tenant_id": tenant_id, "slug": slug })),
    )
    .await?;
    tx.commit().await?;

    let sign_in = format!("{}/login", state.cfg.public_url);
    let message = format!(
        "Hello {contact}, your S'Shop access for {business} is ready.\n\nSign in: {sign_in}\nEmail: {email_addr}\nTemporary PIN: {pin}\n\nPlease change your PIN after signing in (More → Change PIN).\nHelp: {} / {}",
        SUPPORT_PHONES[0], SUPPORT_PHONES[1],
    );
    Ok(Json(json!({
        "ok": true, "tenant_id": tenant_id, "slug": slug, "email": email_addr, "phone": phone,
        "temporary_pin": pin, "sign_in_url": sign_in, "message": message,
    })))
}

#[derive(Deserialize)]
struct RejectBody {
    #[serde(default)]
    note: String,
}

async fn reject(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RejectBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let updated: Option<String> = sqlx::query_scalar(
        "UPDATE access_requests SET status = 'rejected', decided_by = $2, decided_at = now(), decision_note = $3
         WHERE id = $1 AND status = 'pending' RETURNING business_name",
    )
    .bind(id)
    .bind(ctx.user_id)
    .bind(clip(&b.note, 500))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(business) = updated else { return Err(rule("This request is no longer pending")) };
    audit::record(&mut tx, &ctx, Entry::new("platform", "reject_access", "access_request", id).after(json!({ "business": business, "note": b.note })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
