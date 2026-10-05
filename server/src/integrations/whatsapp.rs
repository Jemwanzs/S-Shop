//! WhatsApp Cloud API (Meta Graph API).
//!
//! Outbound: text messages (inside the 24h customer-service window) and
//! approved template messages (for business-initiated notifications).
//! Inbound: webhook verification + message/status callbacks (see routes::webhooks).

use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::state::AppState;

pub fn is_configured(state: &AppState) -> bool {
    state.cfg.whatsapp.is_some()
}

/// Send a free-form text message. Returns Ok(false) when WhatsApp is not configured.
pub async fn send_text(state: &AppState, tenant_id: Option<Uuid>, phone: &str, body: &str) -> anyhow::Result<bool> {
    let payload = json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": phone,
        "type": "text",
        "text": { "preview_url": true, "body": body },
    });
    send(state, tenant_id, phone, body, payload).await
}

/// Business-initiated message: uses the approved notification template when
/// configured (free-form text only reaches customers inside the 24h window).
pub async fn send_notification(state: &AppState, tenant_id: Option<Uuid>, phone: &str, body: &str) -> anyhow::Result<bool> {
    let Some(cfg) = &state.cfg.whatsapp else { return Ok(false) };
    match &cfg.notification_template {
        Some(template) => {
            // Template parameters may not contain newlines or tabs.
            let param = body.split(['\n', '\t']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
            send_template(state, tenant_id, phone, template, &cfg.template_language, &[param]).await
        }
        None => send_text(state, tenant_id, phone, body).await,
    }
}

/// Send an approved template (needed outside the 24h window).
async fn send_template(
    state: &AppState,
    tenant_id: Option<Uuid>,
    phone: &str,
    template: &str,
    language: &str,
    params: &[String],
) -> anyhow::Result<bool> {
    let parameters: Vec<Value> = params.iter().map(|p| json!({ "type": "text", "text": p })).collect();
    let payload = json!({
        "messaging_product": "whatsapp",
        "to": phone,
        "type": "template",
        "template": {
            "name": template,
            "language": { "code": language },
            "components": [{ "type": "body", "parameters": parameters }],
        },
    });
    send(state, tenant_id, phone, &format!("[template:{template}] {}", params.join(" | ")), payload).await
}

async fn send(state: &AppState, tenant_id: Option<Uuid>, phone: &str, log_body: &str, payload: Value) -> anyhow::Result<bool> {
    // Demo businesses never message anyone: their customer numbers are placeholders.
    if let Some(t) = tenant_id {
        let demo: bool = sqlx::query_scalar("SELECT is_demo FROM tenants WHERE id = $1").bind(t).fetch_optional(&state.db).await?.unwrap_or(false);
        if demo {
            return Ok(false);
        }
    }
    let Some(cfg) = &state.cfg.whatsapp else { return Ok(false) };
    let url = format!("https://graph.facebook.com/{}/{}/messages", cfg.api_version, cfg.phone_number_id);

    let res = state.http.post(&url).bearer_auth(&cfg.token).json(&payload).send().await;
    let (wa_id, status, error) = match res {
        Ok(r) => {
            let ok = r.status().is_success();
            let body: Value = r.json().await.unwrap_or(Value::Null);
            if ok {
                (body["messages"][0]["id"].as_str().map(String::from), "sent".to_string(), None)
            } else {
                let msg = body["error"]["message"].as_str().unwrap_or("WhatsApp API error").to_string();
                (None, "failed".to_string(), Some(msg))
            }
        }
        Err(e) => (None, "failed".to_string(), Some(e.to_string())),
    };

    sqlx::query(
        "INSERT INTO whatsapp_messages (tenant_id, direction, phone, body, wa_message_id, status, error)
         VALUES ($1, 'out', $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(phone)
    .bind(log_body)
    .bind(&wa_id)
    .bind(&status)
    .bind(&error)
    .execute(&state.db)
    .await?;

    if let Some(e) = error {
        tracing::warn!(phone, error = %e, "whatsapp send failed");
        anyhow::bail!(e);
    }
    Ok(true)
}

/// Verify Meta's `X-Hub-Signature-256` header (HMAC-SHA256 of the raw body with the app secret).
pub fn verify_signature(app_secret: &str, signature_header: Option<&str>, body: &[u8]) -> bool {
    let Some(sig) = signature_header.and_then(|s| s.strip_prefix("sha256=")) else { return false };
    let Ok(expected) = hex::decode(sig) else { return false };
    let mut mac = Hmac::<Sha256>::new_from_slice(app_secret.as_bytes()).expect("hmac key");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// `https://wa.me/<phone>?text=…` deep link (works without API credentials).
pub fn deep_link(phone: &str, text: &str) -> String {
    let encoded: String = text
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://wa.me/{phone}?text={encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_check() {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(b"{}");
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_signature("secret", Some(&sig), b"{}"));
        assert!(!verify_signature("other", Some(&sig), b"{}"));
        assert!(!verify_signature("secret", None, b"{}"));
    }

    #[test]
    fn deep_link_encodes() {
        assert_eq!(deep_link("254700000000", "Hi there!"), "https://wa.me/254700000000?text=Hi%20there%21");
    }
}
