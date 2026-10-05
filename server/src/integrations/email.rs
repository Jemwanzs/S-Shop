//! Transactional email via the Resend HTTP API (https://resend.com). HTTP rather than SMTP because
//! many hosts, Railway included, restrict outbound SMTP.

use serde_json::json;

use crate::config::EmailConfig;

/// Sends a plain-text email. Returns an error message on failure; callers log it and carry on.
pub async fn send(http: &reqwest::Client, cfg: &EmailConfig, to: &[String], subject: &str, text: &str) -> Result<(), String> {
    let res = http
        .post("https://api.resend.com/emails")
        .bearer_auth(&cfg.api_key)
        .json(&json!({ "from": cfg.from, "to": to, "subject": subject, "text": text }))
        .send()
        .await
        .map_err(|e| format!("email request failed: {e}"))?;
    if res.status().is_success() {
        Ok(())
    } else {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        Err(format!("email rejected ({status}): {}", body.chars().take(300).collect::<String>()))
    }
}
