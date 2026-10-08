//! Onboarding and account emails (roadmap 58–61): templates, sending through Resend with a delivery log, and the
//! single-use links they carry.
//!
//! * Every email is recorded in `email_log` (kind, recipient, subject, status, provider id, error) — never its body,
//!   PINs or links. Resend's webhook (`/api/webhooks/resend`) moves `sent` on to `delivered`, `bounced` …
//! * Links (`auth_tokens`) are random 256-bit tokens; only their SHA-256 is stored. They travel in the URL fragment
//!   (`#t=…`), which browsers never send to servers, so they do not end up in access logs.
//! * A failed email never undoes what triggered it (an approval stays approved); it can be retried.

use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::integrations::email;
use crate::state::AppState;

pub const SETUP_HOURS: i64 = 72;
pub const RESET_MINUTES: i64 = 30;
pub const STATUS_MINUTES: i64 = 30;
/// A one-time PIN (approval or replacement) works for this long, once, and must then be replaced.
pub const TEMP_PIN_HOURS: i64 = 72;

// ───────────────────────────── Links ─────────────────────────────

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.trim().as_bytes()))
}

/// A new single-use link token. Earlier open tokens of the same kind for the same person stop working.
pub async fn issue_token(conn: &mut PgConnection, kind: &str, user_id: Option<Uuid>, request_id: Option<Uuid>, by: Option<Uuid>, ip: &str) -> AppResult<(String, DateTime<Utc>)> {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let token = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw);
    let ttl = match kind {
        "setup" => Duration::hours(SETUP_HOURS),
        "reset" => Duration::minutes(RESET_MINUTES),
        _ => Duration::minutes(STATUS_MINUTES),
    };
    let expires = Utc::now() + ttl;
    sqlx::query(
        "UPDATE auth_tokens SET revoked_at = now() WHERE kind = $1 AND used_at IS NULL AND revoked_at IS NULL
           AND (user_id = $2 OR access_request_id = $3)",
    )
    .bind(kind)
    .bind(user_id)
    .bind(request_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO auth_tokens (kind, token_hash, user_id, access_request_id, expires_at, created_by, ip) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(kind)
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(request_id)
        .bind(expires)
        .bind(by)
        .bind(ip)
        .execute(&mut *conn)
        .await?;
    Ok((token, expires))
}

#[derive(sqlx::FromRow)]
pub struct TokenRow {
    pub id: Uuid,
    pub kind: String,
    pub user_id: Option<Uuid>,
    pub access_request_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

/// A link that is still usable (not used, not replaced, not expired). `lock` holds it for the transaction.
pub async fn find_token(conn: &mut PgConnection, token: &str, kinds: &[&str], lock: bool) -> AppResult<TokenRow> {
    let row: Option<TokenRow> = sqlx::query_as(&format!(
        "SELECT id, kind, user_id, access_request_id, expires_at FROM auth_tokens
         WHERE token_hash = $1 AND kind = ANY($2) AND used_at IS NULL AND revoked_at IS NULL AND expires_at > now() {}",
        if lock { "FOR UPDATE" } else { "" }
    ))
    .bind(hash_token(token))
    .bind(kinds.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    .fetch_optional(&mut *conn)
    .await?;
    row.ok_or_else(|| crate::error::refused("Link expired", "This link has expired or was already used. Request a new one from the sign-in page."))
}

pub fn link(state: &AppState, page: &str, token: &str) -> String {
    format!("{}/{page}#t={token}", state.cfg.public_url)
}

// ───────────────────────────── Sending with a log ─────────────────────────────

pub struct Mail {
    pub kind: &'static str,
    pub to: Vec<String>,
    pub subject: String,
    pub html: String,
    pub text: String,
    pub tenant_id: Option<Uuid>,
    pub access_request_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub retry_of: Option<Uuid>,
}

#[derive(Serialize, Clone)]
pub struct Outcome {
    pub id: Uuid,
    /// sent | failed | skipped
    pub status: String,
    pub error: String,
}

/// Sends and records one email. Never fails the caller: the outcome says what happened.
pub async fn send(state: &AppState, m: Mail) -> Outcome {
    let recipient = m.to.join(", ");
    let id: Uuid = match sqlx::query_scalar(
        "INSERT INTO email_log (kind, recipient, subject, tenant_id, access_request_id, user_id, created_by, retry_of)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
    )
    .bind(m.kind)
    .bind(&recipient)
    .bind(&m.subject)
    .bind(m.tenant_id)
    .bind(m.access_request_id)
    .bind(m.user_id)
    .bind(m.created_by)
    .bind(m.retry_of)
    .fetch_one(&state.db)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, kind = m.kind, "email log insert failed");
            return Outcome { id: Uuid::nil(), status: "failed".into(), error: "Could not record the email".into() };
        }
    };
    let (status, provider, error) = match &state.cfg.email {
        None => ("skipped", None, "Email is not configured on the server (RESEND_API_KEY)".to_string()),
        Some(cfg) if m.to.is_empty() => {
            let _ = cfg;
            ("failed", None, "No recipient".to_string())
        }
        Some(cfg) => match email::send_rich(&state.http, cfg, &m.to, &m.subject, Some(&m.html), &m.text).await {
            Ok(pid) => ("sent", (!pid.is_empty()).then_some(pid), String::new()),
            Err(e) => {
                tracing::warn!(error = %e, kind = m.kind, "email failed");
                ("failed", None, e)
            }
        },
    };
    let _ = sqlx::query("UPDATE email_log SET status = $2, provider_id = $3, error = $4, attempts = attempts + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(&provider)
        .bind(&error)
        .execute(&state.db)
        .await;
    Outcome { id, status: status.into(), error }
}

/// Resend webhook event → email status. Unknown ids are ignored.
pub async fn apply_event(state: &AppState, provider_id: &str, event: &str) -> AppResult<()> {
    let status = match event {
        "email.sent" => "sent",
        "email.delivered" => "delivered",
        "email.delivery_delayed" => "delayed",
        "email.bounced" => "bounced",
        "email.complained" => "complained",
        "email.failed" => "failed",
        _ => return Ok(()),
    };
    // A late "sent" never overwrites a later state.
    sqlx::query(
        "UPDATE email_log SET status = $2, updated_at = now() WHERE provider_id = $1
           AND NOT ($2 = 'sent' AND status IN ('delivered', 'bounced', 'complained'))",
    )
    .bind(provider_id)
    .bind(status)
    .execute(&state.db)
    .await
    .map_err(AppError::from)?;
    Ok(())
}

// ───────────────────────────── Templates ─────────────────────────────

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub struct Support {
    pub phones: [&'static str; 2],
}

pub const SUPPORT: Support = Support { phones: crate::routes::access::SUPPORT_PHONES };

/// Mobile-friendly, client-safe HTML (tables + inline styles), S'Shop brand colours, light background.
pub fn layout(preheader: &str, title: &str, body: &str) -> String {
    let phones = SUPPORT.phones.iter().map(|p| format!("<a href=\"tel:{}\" style=\"color:#9a3412;text-decoration:none;white-space:nowrap\">{}</a>", p.replace(' ', ""), p.replace(' ', "&nbsp;"))).collect::<Vec<_>>().join(" &nbsp;·&nbsp; ");
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light"><title>{title}</title></head>
<body style="margin:0;padding:0;background:#faf5ee;font-family:'Segoe UI',Roboto,Helvetica,Arial,sans-serif;color:#1c1917">
<span style="display:none;max-height:0;overflow:hidden;opacity:0">{pre}</span>
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#faf5ee"><tr><td align="center" style="padding:24px 12px">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:560px">
<tr><td style="padding:0 4px 16px"><span style="font-size:22px;font-weight:800;color:#ea580c;letter-spacing:-.02em">S'Shop</span><span style="font-size:12px;color:#78716c">&nbsp; by SyncScore</span></td></tr>
<tr><td style="background:#ffffff;border-radius:16px;padding:28px 24px;border:1px solid #efe6db">
<h1 style="margin:0 0 14px;font-size:21px;line-height:1.3;color:#1c1917">{title}</h1>
{body}
</td></tr>
<tr><td style="padding:18px 6px;font-size:12px;line-height:1.6;color:#78716c;text-align:center">Need help? Call or WhatsApp S'Shop Support: {phones}<br>S'Shop Team · SyncScore</td></tr>
</table></td></tr></table></body></html>"#,
        title = esc(title),
        pre = esc(preheader),
    )
}

pub fn p(text: &str) -> String {
    format!("<p style=\"margin:0 0 12px;font-size:15px;line-height:1.6\">{text}</p>")
}

pub fn button(label: &str, href: &str) -> String {
    format!(
        "<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:18px 0\"><tr><td style=\"border-radius:999px;background:#ea580c\">\
         <a href=\"{}\" style=\"display:inline-block;padding:13px 26px;font-size:15px;font-weight:700;color:#ffffff;text-decoration:none;border-radius:999px\">{}</a></td></tr></table>",
        esc(href),
        esc(label)
    )
}

/// Receipt-style details block (dashed rules, label / value rows) — like an ETR receipt.
pub fn receipt(title: &str, rows: &[(&str, String)]) -> String {
    let body: String = rows
        .iter()
        .map(|(k, v)| {
            format!(
                "<tr><td style=\"padding:5px 0;font-size:13px;color:#78716c;white-space:nowrap;vertical-align:top\">{}</td>\
                 <td style=\"padding:5px 0 5px 12px;font-size:13px;font-family:'Courier New',monospace;text-align:right;word-break:break-all\">{}</td></tr>",
                esc(k),
                esc(v)
            )
        })
        .collect();
    format!(
        "<div style=\"margin:16px 0;padding:14px 16px;background:#fffdf9;border:1px dashed #d6c7b4;border-radius:10px\">\
         <div style=\"font-size:11px;letter-spacing:.12em;text-transform:uppercase;color:#a8a29e;text-align:center;padding-bottom:8px;border-bottom:1px dashed #d6c7b4\">{}</div>\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin-top:6px\">{body}</table></div>",
        esc(title)
    )
}

pub fn text_rows(rows: &[(&str, String)]) -> String {
    rows.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n")
}

pub fn support_text() -> String {
    format!("Need help? Call or WhatsApp S'Shop Support: {} / {}\nS'Shop Team · SyncScore", SUPPORT.phones[0], SUPPORT.phones[1])
}

/// Masks an email for screens shown before sign-in: j•••y@gmail.com
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((user, domain)) if user.chars().count() > 2 => {
            let first = user.chars().next().unwrap_or('•');
            let last = user.chars().last().unwrap_or('•');
            format!("{first}•••{last}@{domain}")
        }
        Some((_, domain)) => format!("•••@{domain}"),
        None => "•••".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_hash_and_masking() {
        assert_eq!(hash_token("abc").len(), 64);
        assert_eq!(hash_token("abc "), hash_token("abc"));
        assert_eq!(mask_email("james@example.com"), "j•••s@example.com");
        assert_eq!(mask_email("jo@x.io"), "•••@x.io");
        let html = layout("pre", "Hello <b>", &p("x"));
        assert!(html.contains("Hello &lt;b&gt;") && html.contains("0798993404"));
    }
}
