//! Transactional email via the Resend HTTP API (https://resend.com). HTTP rather than SMTP because
//! many hosts, Railway included, restrict outbound SMTP.

use serde_json::{json, Value};

use crate::config::EmailConfig;

/// Sends a plain-text email. Returns an error message on failure; callers log it and carry on.
pub async fn send(http: &reqwest::Client, cfg: &EmailConfig, to: &[String], subject: &str, text: &str) -> Result<(), String> {
    send_rich(http, cfg, to, subject, None, text).await.map(|_| ())
}

/// Sends an email with an HTML body and a plain-text alternative; returns Resend's message id (for delivery tracking).
pub async fn send_rich(http: &reqwest::Client, cfg: &EmailConfig, to: &[String], subject: &str, html: Option<&str>, text: &str) -> Result<String, String> {
    let mut body = json!({ "from": cfg.from, "to": to, "subject": subject, "text": text });
    if let Some(h) = html {
        body["html"] = json!(h);
    }
    if let Some(r) = &cfg.reply_to {
        body["reply_to"] = json!(r);
    }
    let res = http
        .post(format!("{}/emails", cfg.base_url))
        .bearer_auth(&cfg.api_key)
        .timeout(std::time::Duration::from_secs(15))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("email request failed: {e}"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if status.is_success() {
        let id = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["id"].as_str().map(str::to_string)).unwrap_or_default();
        Ok(id)
    } else {
        // Resend explains rejections in `message` (e.g. an unverified sender domain).
        let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["message"].as_str().map(str::to_string)).unwrap_or(text);
        Err(format!("Resend {status}: {}", msg.chars().take(300).collect::<String>()))
    }
}
