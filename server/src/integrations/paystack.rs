//! Paystack (https://paystack.com/docs/api) for platform billing: initialise a transaction for one invoice,
//! verify it, and check webhook signatures. Only the server talks to Paystack; the secret key never leaves it.

use hmac::{Hmac, Mac};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::Sha512;

use crate::config::PaystackConfig;

/// Amount in the currency's subunit (KES cents), as Paystack expects.
pub fn subunits(amount: Decimal) -> i64 {
    (amount * Decimal::from(100)).round().to_i64().unwrap_or(0)
}

pub struct Checkout {
    pub authorization_url: String,
    pub access_code: String,
}

pub async fn initialize(
    http: &reqwest::Client,
    cfg: &PaystackConfig,
    email: &str,
    amount: Decimal,
    currency: &str,
    reference: &str,
    callback_url: &str,
    metadata: Value,
) -> Result<Checkout, String> {
    let res = http
        .post(format!("{}/transaction/initialize", cfg.base_url))
        .bearer_auth(&cfg.secret_key)
        .json(&json!({
            "email": email,
            "amount": subunits(amount),
            "currency": currency,
            "reference": reference,
            "callback_url": callback_url,
            "metadata": metadata,
        }))
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("Paystack could not be reached: {e}"))?;
    let status = res.status();
    let body: Value = res.json().await.map_err(|e| format!("Paystack response unreadable: {e}"))?;
    if !status.is_success() || body["status"] != json!(true) {
        return Err(format!("Paystack refused the payment: {}", body["message"].as_str().unwrap_or("unknown error")));
    }
    let data = &body["data"];
    Ok(Checkout {
        authorization_url: data["authorization_url"].as_str().unwrap_or_default().to_string(),
        access_code: data["access_code"].as_str().unwrap_or_default().to_string(),
    })
}

/// What Paystack says about a transaction.
#[derive(Debug)]
pub struct Verified {
    /// "success" | "failed" | "abandoned" | "pending" | "ongoing" | "reversed" …
    pub status: String,
    pub amount: i64,
    pub currency: String,
    pub channel: String,
    pub paid_at: Option<chrono::DateTime<chrono::Utc>>,
    pub raw: Value,
}

pub async fn verify(http: &reqwest::Client, cfg: &PaystackConfig, reference: &str) -> Result<Verified, String> {
    let res = http
        .get(format!("{}/transaction/verify/{}", cfg.base_url, urlencode(reference)))
        .bearer_auth(&cfg.secret_key)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("Paystack could not be reached: {e}"))?;
    let status = res.status();
    let body: Value = res.json().await.map_err(|e| format!("Paystack response unreadable: {e}"))?;
    if !status.is_success() || body["status"] != json!(true) {
        return Err(format!("Paystack could not verify the payment: {}", body["message"].as_str().unwrap_or("unknown error")));
    }
    let d = &body["data"];
    Ok(Verified {
        status: d["status"].as_str().unwrap_or_default().to_string(),
        amount: d["amount"].as_i64().unwrap_or(0),
        currency: d["currency"].as_str().unwrap_or_default().to_string(),
        channel: d["channel"].as_str().unwrap_or_default().to_string(),
        paid_at: d["paid_at"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&chrono::Utc)),
        // Kept for reconciliation; card authorisation details are dropped.
        raw: json!({ "id": d["id"], "status": d["status"], "reference": d["reference"], "amount": d["amount"], "currency": d["currency"],
                     "channel": d["channel"], "paid_at": d["paid_at"], "gateway_response": d["gateway_response"] }),
    })
}

/// `x-paystack-signature` is the hex HMAC-SHA512 of the raw body with the secret key.
pub fn signature_ok(cfg: &PaystackConfig, body: &[u8], signature: &str) -> bool {
    let Ok(expected) = hex::decode(signature.trim()) else { return false };
    let Ok(mut mac) = Hmac::<Sha512>::new_from_slice(cfg.secret_key.as_bytes()) else { return false };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn urlencode(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subunits_round() {
        assert_eq!(subunits(Decimal::new(150050, 2)), 150050);
        assert_eq!(subunits(Decimal::from(2500)), 250000);
    }

    #[test]
    fn signature() {
        let cfg = PaystackConfig { secret_key: "sk_test_x".into(), base_url: String::new() };
        let body = br#"{"event":"charge.success"}"#;
        let mut mac = Hmac::<Sha512>::new_from_slice(b"sk_test_x").unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());
        assert!(signature_ok(&cfg, body, &sig));
        assert!(!signature_ok(&cfg, b"tampered", &sig));
        assert!(!signature_ok(&cfg, body, "zz"));
    }
}
