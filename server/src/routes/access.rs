//! Access requests (roadmap 7, 58–59). There is no open signup: a prospective business submits a request, the
//! platform admins are emailed and notified in-app and the applicant gets an acknowledgement; nothing is activated
//! until a platform admin approves it. Approval creates the business (default roles, main branch, settings) and its
//! first administrator, who receives a welcome email with a single-use set-up link. A one-time PIN is also shown once
//! to the platform admin (it expires and must be replaced at first sign-in). Every email is logged and can be retried.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::{client_meta, hash_pin, Ctx};
use crate::error::{bad, rule, AppError, AppResult};
use crate::mailer::{self, button, esc, layout, p, receipt, support_text, text_rows, Mail, Outcome};
use crate::notify::{self, Note};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/access-requests", post(submit))
        .route("/platform/access-requests", get(list))
        .route("/platform/access-requests/{id}", get(details))
        .route("/platform/access-requests/{id}/approve", post(approve))
        .route("/platform/access-requests/{id}/reject", post(reject))
        .route("/platform/access-requests/{id}/resend-welcome", post(resend_welcome))
        .route("/platform/access-requests/{id}/issue-pin", post(issue_pin))
        .route("/platform/emails/{id}/retry", post(retry_email))
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
    estimated_users: Option<i32>,
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
    if let Some(n) = b.estimated_users {
        if !(1..=100_000).contains(&n) {
            return Err(bad("Estimated users must be between 1 and 100,000"));
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
        return Err(rule("This email already has access — sign in instead, or use “Forgot PIN / Password?” on the sign-in page"));
    }
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO access_requests (business_name, contact_name, email, phone, location, business_type, branches, estimated_users, message, ip)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (lower(email)) WHERE status = 'pending' DO NOTHING RETURNING id",
    )
    .bind(&business_name)
    .bind(&contact_name)
    .bind(&email)
    .bind(&phone)
    .bind(clip(&b.location, 120))
    .bind(clip(&b.business_type, 60))
    .bind(b.branches)
    .bind(b.estimated_users)
    .bind(clip(&b.message, 1000))
    .bind(&ip)
    .fetch_optional(&state.db)
    .await?;
    // A repeat of a pending request is treated as success (no duplicate review item, no new emails).
    if let Some(id) = id {
        let st = state.clone();
        tokio::spawn(async move { announce(&st, id).await });
    }
    Ok(Json(ok))
}

#[derive(sqlx::FromRow, serde::Serialize, Clone)]
struct RequestRow {
    id: Uuid,
    business_name: String,
    contact_name: String,
    email: String,
    phone: String,
    location: String,
    business_type: String,
    branches: Option<i32>,
    estimated_users: Option<i32>,
    message: String,
    status: String,
    tenant_id: Option<Uuid>,
    admin_user_id: Option<Uuid>,
    decided_by_name: Option<String>,
    decided_at: Option<DateTime<Utc>>,
    decision_note: String,
    public_reason: String,
    email_sent: bool,
    created_at: DateTime<Utc>,
}

const REQUEST_SELECT: &str = "SELECT a.id, a.business_name, a.contact_name, a.email, a.phone, a.location, a.business_type, a.branches,
        a.estimated_users, a.message, a.status, a.tenant_id, a.admin_user_id, u.name AS decided_by_name, a.decided_at, a.decision_note,
        a.public_reason, a.email_sent, a.created_at
     FROM access_requests a LEFT JOIN users u ON u.id = a.decided_by";

async fn load_request(state: &AppState, id: Uuid) -> AppResult<RequestRow> {
    sqlx::query_as(&format!("{REQUEST_SELECT} WHERE a.id = $1")).bind(id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound("Access request"))
}

fn kenya_time(at: DateTime<Utc>) -> String {
    at.with_timezone(&crate::util::parse_tz(crate::billing::PLATFORM_TZ)).format("%d %b %Y, %H:%M (EAT)").to_string()
}

fn request_rows(r: &RequestRow) -> Vec<(&'static str, String)> {
    vec![
        ("Business", r.business_name.clone()),
        ("Applicant", r.contact_name.clone()),
        ("Email", r.email.clone()),
        ("Mobile", r.phone.clone()),
        ("Business type", if r.business_type.is_empty() { "—".into() } else { r.business_type.clone() }),
        ("Branches", r.branches.map_or("—".into(), |n| n.to_string())),
        ("Estimated users", r.estimated_users.map_or("—".into(), |n| n.to_string())),
        ("Location", if r.location.is_empty() { "—".into() } else { r.location.clone() }),
        ("Requested", kenya_time(r.created_at)),
    ]
}

/// Email to the platform owner(s) about a new request.
fn owner_mail(state: &AppState, r: &RequestRow) -> Mail {
    let rows = request_rows(r);
    let review = format!("{}/settings/access-requests", state.cfg.public_url);
    let mut body = p("A business has asked to use S'Shop. Nothing is activated until you approve it.");
    body.push_str(&receipt("Access request", &rows));
    if !r.message.is_empty() {
        body.push_str(&p(&format!("<b>Message:</b> {}", esc(&r.message))));
    }
    body.push_str(&button("Review request", &review));
    Mail {
        kind: "access_request_received",
        to: state.cfg.access_request_notify.clone(),
        subject: format!("New S'Shop access request: {}", r.business_name),
        html: layout(&format!("{} would like to use S'Shop", r.business_name), "New access request", &body),
        text: format!("New access request\n\n{}\n\nMessage: {}\n\nReview: {review}", text_rows(&rows), if r.message.is_empty() { "—" } else { &r.message }),
        tenant_id: None,
        access_request_id: Some(r.id),
        user_id: None,
        created_by: None,
        retry_of: None,
    }
}

/// Acknowledgement to the applicant.
fn ack_mail(r: &RequestRow) -> Mail {
    let first = r.contact_name.split_whitespace().next().unwrap_or(&r.contact_name).to_string();
    let rows = vec![("Business", r.business_name.clone()), ("Email", r.email.clone()), ("Received", kenya_time(r.created_at)), ("Status", "Under review".to_string())];
    let mut body = p(&format!("Hello {},", esc(&first)));
    body.push_str(&p("Thank you for your interest in S'Shop. We have received your access request and our team is reviewing it."));
    body.push_str(&receipt("Request received", &rows));
    body.push_str(&p("You will receive another email as soon as a decision has been made. There is no need to submit the request again."));
    Mail {
        kind: "access_request_ack",
        to: vec![r.email.clone()],
        subject: "We received your S'Shop access request".into(),
        html: layout("Your request is under review", "Request received", &body),
        text: format!(
            "Hello {first},\n\nThank you for your interest in S'Shop. We have received your access request and our team is reviewing it.\n\n{}\n\nYou will receive another email once a decision has been made.\n\n{}",
            text_rows(&rows),
            support_text()
        ),
        tenant_id: None,
        access_request_id: Some(r.id),
        user_id: None,
        created_by: None,
        retry_of: None,
    }
}

/// Emails the platform admins and the applicant, and notifies the platform admins in-app. Runs after the response.
async fn announce(state: &AppState, id: Uuid) {
    let Ok(r) = load_request(state, id).await else { return };
    if !state.cfg.access_request_notify.is_empty() {
        let out = mailer::send(state, owner_mail(state, &r)).await;
        if out.status == "sent" {
            let _ = sqlx::query("UPDATE access_requests SET email_sent = true WHERE id = $1").bind(id).execute(&state.db).await;
        }
    }
    mailer::send(state, ack_mail(&r)).await;

    // In-app notification for every platform admin, whichever business they sign in to.
    let admins: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT id, tenant_id FROM users WHERE is_active AND lower(email) = ANY($1)")
        .bind(&state.cfg.platform_admins)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (user, tenant) in admins {
        notify::to_users(
            state,
            tenant,
            &[user],
            Note::new("access_request", format!("Access request: {}", r.business_name), format!("{} · {}", r.contact_name, r.phone), "/settings/access-requests"),
        )
        .await;
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
struct Activation {
    user_id: Uuid,
    name: String,
    is_active: bool,
    last_login_at: Option<DateTime<Utc>>,
    must_change_pin: bool,
    pin_expires_at: Option<DateTime<Utc>>,
    pin_changed_at: Option<DateTime<Utc>>,
    tenant_status: String,
}

/// active (credentials set) · awaiting_setup (one-time PIN or link not used yet) · pin_expired · deactivated
fn activation_status(a: &Activation) -> &'static str {
    if !a.is_active || a.tenant_status != "active" {
        "deactivated"
    } else if !a.must_change_pin {
        "active"
    } else if a.pin_expires_at.is_some_and(|t| t < Utc::now()) {
        "pin_expired"
    } else {
        "awaiting_setup"
    }
}

async fn activation(state: &AppState, user_id: Option<Uuid>) -> AppResult<Option<Activation>> {
    let Some(uid) = user_id else { return Ok(None) };
    Ok(sqlx::query_as(
        "SELECT u.id AS user_id, u.name, u.is_active, u.last_login_at, u.must_change_pin, u.pin_expires_at, u.pin_changed_at, t.status AS tenant_status
         FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = $1",
    )
    .bind(uid)
    .fetch_optional(&state.db)
    .await?)
}

/// The latest email of each kind for these requests: (request, kind) → status.
async fn latest_emails(state: &AppState, ids: &[Uuid]) -> AppResult<Vec<(Uuid, String, String, DateTime<Utc>)>> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT ON (access_request_id, kind) access_request_id, kind, status, created_at FROM email_log
         WHERE access_request_id = ANY($1) ORDER BY access_request_id, kind, created_at DESC",
    )
    .bind(ids)
    .fetch_all(&state.db)
    .await?)
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let status = q.status.unwrap_or_else(|| "pending".into());
    let rows: Vec<RequestRow> = sqlx::query_as(&format!("{REQUEST_SELECT} WHERE $1 = 'all' OR a.status = $1 ORDER BY a.created_at DESC LIMIT 200"))
        .bind(&status)
        .fetch_all(&state.db)
        .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let emails = latest_emails(&state, &ids).await?;
    let admins: Vec<Activation> = sqlx::query_as(
        "SELECT u.id AS user_id, u.name, u.is_active, u.last_login_at, u.must_change_pin, u.pin_expires_at, u.pin_changed_at, t.status AS tenant_status
         FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = ANY($1)",
    )
    .bind(rows.iter().filter_map(|r| r.admin_user_id).collect::<Vec<_>>())
    .fetch_all(&state.db)
    .await?;
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_requests WHERE status = 'pending'").fetch_one(&state.db).await?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let mut v = serde_json::to_value(&r).unwrap_or_default();
            let mine: serde_json::Map<String, Value> =
                emails.iter().filter(|e| e.0 == r.id).map(|e| (e.1.clone(), json!({ "status": e.2, "at": e.3 }))).collect();
            v["emails"] = Value::Object(mine);
            v["activation"] = json!(r.admin_user_id.and_then(|u| admins.iter().find(|a| a.user_id == u)).map(activation_status));
            v
        })
        .collect();
    Ok(Json(json!({ "items": items, "pending": pending, "email_configured": state.cfg.email.is_some(), "support_phones": SUPPORT_PHONES })))
}

/// One-time PIN: 8 characters without look-alikes (0/O, 1/l/I).
pub fn temporary_pin() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..8).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

/// Replaces a user's PIN with a one-time PIN: expires, must be replaced at first sign-in, older sessions end.
pub async fn set_one_time_pin(conn: &mut sqlx::PgConnection, user_id: Uuid, pin: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE users SET pin_hash = $2, must_change_pin = true, pin_expires_at = now() + make_interval(hours => $3::int),
                failed_attempts = 0, locked_until = NULL, sessions_valid_after = now() WHERE id = $1",
    )
    .bind(user_id)
    .bind(hash_pin(pin)?)
    .bind(mailer::TEMP_PIN_HOURS as i32)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The WhatsApp message for the new administrator — deliberately without the PIN (a reusable chat message is not a
/// safe place for a credential; the email link or the PIN shown once is).
fn whatsapp_message(state: &AppState, contact: &str, business: &str, email: &str) -> String {
    let first = contact.split_whitespace().next().unwrap_or(contact);
    format!(
        "*Welcome to S'Shop!*\n\nHello {first},\nYour business, *{business}*, has been successfully activated on S'Shop.\n\nYou can now access your account using:\n*Login:* {}/login\n*Email:* {email}\n\nPlease use the secure account setup instructions sent to your email to complete your first login.\n\nWelcome aboard!\n*S'Shop Team | SyncScore*",
        state.cfg.public_url
    )
}

/// wa.me needs the number in international format without "+".
fn wa_phone(phone: &str) -> String {
    crate::util::normalize_mobile(phone).unwrap_or_else(|_| phone.chars().filter(|c| c.is_ascii_digit()).collect())
}

/// Welcome email with a single-use set-up link (72 h); earlier set-up links stop working.
async fn send_welcome(state: &AppState, r: &RequestRow, by: Option<Uuid>, retry_of: Option<Uuid>) -> AppResult<Outcome> {
    let (Some(tenant_id), Some(user_id)) = (r.tenant_id, r.admin_user_id) else { return Err(rule("This request has no administrator yet")) };
    let token = {
        let mut conn = state.db.acquire().await?;
        mailer::issue_token(&mut conn, "setup", Some(user_id), None, by, "").await?.0
    };
    let link = mailer::link(state, "set-pin", &token);
    let sign_in = format!("{}/login", state.cfg.public_url);
    let first = r.contact_name.split_whitespace().next().unwrap_or(&r.contact_name).to_string();
    let rows = vec![("Business", r.business_name.clone()), ("Administrator", r.contact_name.clone()), ("Email", r.email.clone()), ("Sign in", sign_in.clone())];
    let mut body = p(&format!("Hello {},", esc(&first)));
    body.push_str(&p(&format!("Your business, <b>{}</b>, has been approved and activated on S'Shop. 🎉", esc(&r.business_name))));
    body.push_str(&receipt("Your S'Shop account", &rows));
    body.push_str(&p("<b>To sign in for the first time</b>, create your own PIN with the secure button below. The link works once and expires in 72 hours."));
    body.push_str(&button("Set up my account", &link));
    body.push_str(&p("If S'Shop gave you a one-time PIN instead, sign in with it at the address above — you will be asked to replace it with your own PIN straight away. One-time PINs expire after 72 hours."));
    body.push_str(&p("<span style=\"color:#78716c;font-size:13px\">Never share your PIN. S'Shop staff will never ask for it.</span>"));
    let mail = Mail {
        kind: "welcome",
        to: vec![r.email.clone()],
        subject: format!("Welcome to S'Shop — {} is ready", r.business_name),
        html: layout("Your S'Shop account is ready — set it up in one step", "Welcome to S'Shop!", &body),
        text: format!(
            "Hello {first},\n\nYour business, {}, has been approved and activated on S'Shop.\n\n{}\n\nTo sign in for the first time, create your own PIN with this secure link (works once, expires in 72 hours):\n{link}\n\nIf S'Shop gave you a one-time PIN instead, sign in with it at {sign_in}; you will be asked to replace it straight away.\n\nNever share your PIN.\n\n{}",
            r.business_name,
            text_rows(&rows),
            support_text()
        ),
        tenant_id: Some(tenant_id),
        access_request_id: Some(r.id),
        user_id: Some(user_id),
        created_by: by,
        retry_of,
    };
    Ok(mailer::send(state, mail).await)
}

fn rejection_mail(r: &RequestRow, by: Option<Uuid>, retry_of: Option<Uuid>) -> Mail {
    let first = r.contact_name.split_whitespace().next().unwrap_or(&r.contact_name).to_string();
    let mut body = p(&format!("Hello {},", esc(&first)));
    body.push_str(&p(&format!(
        "Thank you for your interest in S'Shop. After reviewing your request for <b>{}</b>, we are unable to approve it at this time.",
        esc(&r.business_name)
    )));
    if !r.public_reason.trim().is_empty() {
        body.push_str(&p(&format!("<b>Note from our team:</b> {}", esc(&r.public_reason))));
    }
    body.push_str(&p("If you believe this was a mistake or would like more information, please contact S'Shop Support — we are happy to help."));
    Mail {
        kind: "access_rejected",
        to: vec![r.email.clone()],
        subject: "Your S'Shop access request".into(),
        html: layout("An update on your S'Shop access request", "About your access request", &body),
        text: format!(
            "Hello {first},\n\nThank you for your interest in S'Shop. After reviewing your request for {}, we are unable to approve it at this time.\n{}\nIf you would like more information, please contact S'Shop Support.\n\n{}",
            r.business_name,
            if r.public_reason.trim().is_empty() { String::new() } else { format!("\nNote from our team: {}\n", r.public_reason) },
            support_text()
        ),
        tenant_id: None,
        access_request_id: Some(r.id),
        user_id: None,
        created_by: by,
        retry_of,
    }
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
    let admin_id = crate::bootstrap::create_admin(&mut tx, tenant_id, &contact, &email_addr, &pin).await.map_err(AppError::Other)?;
    set_one_time_pin(&mut tx, admin_id, &pin).await?;
    sqlx::query("UPDATE access_requests SET status = 'approved', tenant_id = $2, admin_user_id = $3, decided_by = $4, decided_at = now() WHERE id = $1")
        .bind(id)
        .bind(tenant_id)
        .bind(admin_id)
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

    // The business exists now; a failed email does not change that (it can be resent).
    let r = load_request(&state, id).await?;
    let email = match send_welcome(&state, &r, Some(ctx.user_id), None).await {
        Ok(o) => o,
        Err(e) => Outcome { id: Uuid::nil(), status: "failed".into(), error: e.to_string() },
    };
    let sign_in = format!("{}/login", state.cfg.public_url);
    Ok(Json(json!({
        "ok": true, "tenant_id": tenant_id, "slug": slug, "business": business, "name": contact, "email": email_addr, "phone": phone,
        "wa_phone": wa_phone(&phone), "temporary_pin": pin, "pin_expires_hours": mailer::TEMP_PIN_HOURS, "sign_in_url": sign_in,
        "message": whatsapp_message(&state, &contact, &business, &email_addr), "email_status": email,
    })))
}

#[derive(Deserialize)]
struct RejectBody {
    /// Internal note (platform only).
    #[serde(default)]
    note: String,
    /// Optional reason included in the email to the applicant.
    #[serde(default)]
    reason: String,
}

async fn reject(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RejectBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let updated: Option<String> = sqlx::query_scalar(
        "UPDATE access_requests SET status = 'rejected', decided_by = $2, decided_at = now(), decision_note = $3, public_reason = $4
         WHERE id = $1 AND status = 'pending' RETURNING business_name",
    )
    .bind(id)
    .bind(ctx.user_id)
    .bind(clip(&b.note, 500))
    .bind(clip(&b.reason, 500))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(business) = updated else { return Err(rule("This request is no longer pending")) };
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("platform", "reject_access", "access_request", id).after(json!({ "business": business, "note": b.note, "reason": b.reason })),
    )
    .await?;
    tx.commit().await?;
    let r = load_request(&state, id).await?;
    let email = mailer::send(&state, rejection_mail(&r, Some(ctx.user_id), None)).await;
    Ok(Json(json!({ "ok": true, "email_status": email })))
}

#[derive(sqlx::FromRow, serde::Serialize)]
struct EmailRow {
    id: Uuid,
    kind: String,
    recipient: String,
    subject: String,
    status: String,
    error: String,
    attempts: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by_name: Option<String>,
}

pub async fn email_history(state: &AppState, request_id: Option<Uuid>, tenant_id: Option<Uuid>) -> AppResult<Vec<Value>> {
    let rows: Vec<EmailRow> = sqlx::query_as(
        "SELECT e.id, e.kind, e.recipient, e.subject, e.status, e.error, e.attempts, e.created_at, e.updated_at, u.name AS created_by_name
         FROM email_log e LEFT JOIN users u ON u.id = e.created_by
         WHERE ($1::uuid IS NOT NULL AND e.access_request_id = $1) OR ($2::uuid IS NOT NULL AND e.tenant_id = $2)
         ORDER BY e.created_at DESC LIMIT 50",
    )
    .bind(request_id)
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|e| {
            let retryable = matches!(e.kind.as_str(), "access_request_received" | "access_request_ack" | "welcome" | "access_rejected");
            let mut v = serde_json::to_value(&e).unwrap_or_default();
            v["retryable"] = json!(retryable);
            v
        })
        .collect())
}

/// Login details for an approved request (never the PIN — it is not stored), activation and email history.
async fn details(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let r = load_request(&state, id).await?;
    let act = activation(&state, r.admin_user_id).await?;
    let (business_status, slug): (Option<String>, Option<String>) = match r.tenant_id {
        Some(t) => sqlx::query_as("SELECT status, slug FROM tenants WHERE id = $1").bind(t).fetch_optional(&state.db).await?.map_or((None, None), |(a, b): (String, String)| (Some(a), Some(b))),
        None => (None, None),
    };
    let history = email_history(&state, Some(id), None).await?;
    Ok(Json(json!({
        "request": r,
        "sign_in_url": format!("{}/login", state.cfg.public_url),
        "business_status": business_status,
        "slug": slug,
        "activation": act.as_ref().map(|a| json!({
            "status": activation_status(a), "name": a.name, "last_login_at": a.last_login_at, "pin_expires_at": a.pin_expires_at,
            "pin_changed_at": a.pin_changed_at,
        })),
        "wa_phone": wa_phone(&r.phone),
        "message": whatsapp_message(&state, &r.contact_name, &r.business_name, &r.email),
        "emails": history,
        "email_configured": state.cfg.email.is_some(),
    })))
}

async fn approved_request(state: &AppState, id: Uuid) -> AppResult<RequestRow> {
    let r = load_request(state, id).await?;
    if r.status != "approved" || r.admin_user_id.is_none() {
        return Err(rule("Only approved requests have login details"));
    }
    Ok(r)
}

/// Resend the welcome email: a fresh set-up link; the previous one stops working.
async fn resend_welcome(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    state.limits.check(&id.to_string(), "resend_welcome", 5, std::time::Duration::from_secs(600))?;
    let r = approved_request(&state, id).await?;
    let out = send_welcome(&state, &r, Some(ctx.user_id), None).await?;
    let mut tx = state.db.begin().await?;
    audit::record(&mut tx, &ctx, Entry::new("platform", "resend_welcome", "access_request", id).after(json!({ "email": r.email, "status": out.status }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "email_status": out })))
}

/// A fresh one-time PIN for the administrator (lost or never received the first). Shown once; the previous PIN and
/// sessions stop working; it expires and must be replaced at first sign-in. The PIN is never logged.
async fn issue_pin(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    state.limits.check(&id.to_string(), "issue_pin", 5, std::time::Duration::from_secs(600))?;
    let r = approved_request(&state, id).await?;
    let user_id = r.admin_user_id.unwrap_or_default();
    let pin = temporary_pin();
    let mut tx = state.db.begin().await?;
    let tenant: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1 FOR UPDATE").bind(user_id).fetch_optional(&mut *tx).await?;
    let tenant = tenant.ok_or(AppError::NotFound("Administrator"))?;
    set_one_time_pin(&mut tx, user_id, &pin).await?;
    let after = json!({ "user": r.contact_name, "email": r.email, "by": ctx.name, "expires_hours": mailer::TEMP_PIN_HOURS });
    super::platform::record_platform(&mut tx, &ctx, tenant, || Entry::new("platform", "reset_pin", "user", user_id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({
        "ok": true, "email": r.email, "name": r.contact_name, "temporary_pin": pin, "expires_at": Utc::now() + Duration::hours(mailer::TEMP_PIN_HOURS),
        "sign_in_url": format!("{}/login", state.cfg.public_url), "wa_phone": wa_phone(&r.phone),
    })))
}

/// Retry an onboarding email. Content is rebuilt from current data (a welcome email gets a fresh link).
async fn retry_email(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    state.limits.check(&id.to_string(), "email_retry", 5, std::time::Duration::from_secs(600))?;
    let (kind, request): (String, Option<Uuid>) = sqlx::query_as("SELECT kind, access_request_id FROM email_log WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Email"))?;
    let r = load_request(&state, request.ok_or_else(|| rule("This email cannot be retried"))?).await?;
    let by = Some(ctx.user_id);
    let out = match kind.as_str() {
        "access_request_received" => {
            let mut m = owner_mail(&state, &r);
            (m.created_by, m.retry_of) = (by, Some(id));
            mailer::send(&state, m).await
        }
        "access_request_ack" => {
            let mut m = ack_mail(&r);
            (m.created_by, m.retry_of) = (by, Some(id));
            mailer::send(&state, m).await
        }
        "access_rejected" if r.status == "rejected" => mailer::send(&state, rejection_mail(&r, by, Some(id))).await,
        "welcome" if r.status == "approved" => send_welcome(&state, &r, by, Some(id)).await?,
        _ => return Err(rule("This email cannot be retried")),
    };
    let mut tx = state.db.begin().await?;
    audit::record(&mut tx, &ctx, Entry::new("platform", "email_retry", "email", id).after(json!({ "kind": kind, "status": out.status }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "email_status": out })))
}

/// Applicant with an approved request whose administrator has not set a PIN yet asks for the set-up email again.
pub(crate) async fn resend_setup_for_applicant(state: &AppState, request_id: Uuid) -> AppResult<Outcome> {
    let r = load_request(state, request_id).await?;
    let awaiting = match (r.status.as_str(), r.admin_user_id) {
        ("approved", Some(u)) => sqlx::query_scalar::<_, bool>("SELECT must_change_pin FROM users WHERE id = $1 AND is_active").bind(u).fetch_optional(&state.db).await?.unwrap_or(false),
        _ => false,
    };
    if !awaiting {
        return Ok(Outcome { id: Uuid::nil(), status: "skipped".into(), error: String::new() });
    }
    send_welcome(state, &r, None, None).await
}
