//! Safaricom Daraja — Lipa na M-Pesa Online (STK Push).
//!
//! Flow: `stk_push` → customer enters PIN on their phone → Daraja calls our
//! callback (routes::webhooks::mpesa_callback) → request marked success/failed.
//! `stk_query` lets the UI recover when a callback is late or lost.

use std::time::{Duration, Instant};

use base64::Engine;
use chrono::Utc;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::config::MpesaConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn config(state: &AppState) -> AppResult<&MpesaConfig> {
    state
        .cfg
        .mpesa
        .as_ref()
        .ok_or_else(|| AppError::Rule("M-Pesa STK Push is not configured. Capture the M-Pesa code manually.".into()))
}

async fn access_token(state: &AppState, cfg: &MpesaConfig) -> AppResult<String> {
    let mut cached = state.mpesa_token.lock().await;
    if let Some((token, expires)) = cached.as_ref() {
        if Instant::now() < *expires {
            return Ok(token.clone());
        }
    }
    let url = format!("{}/oauth/v1/generate?grant_type=client_credentials", cfg.base_url);
    let res = state
        .http
        .get(&url)
        .basic_auth(&cfg.consumer_key, Some(&cfg.consumer_secret))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("M-Pesa unreachable: {e}")))?;
    if !res.status().is_success() {
        return Err(AppError::Upstream("M-Pesa rejected the API credentials".into()));
    }
    let body: Value = res.json().await.map_err(|e| AppError::Upstream(e.to_string()))?;
    let token = body["access_token"].as_str().ok_or_else(|| AppError::Upstream("No M-Pesa token".into()))?.to_string();
    let ttl: u64 = body["expires_in"].as_str().and_then(|s| s.parse().ok()).unwrap_or(3599);
    *cached = Some((token.clone(), Instant::now() + Duration::from_secs(ttl.saturating_sub(60))));
    Ok(token)
}

fn password(cfg: &MpesaConfig) -> (String, String) {
    let ts = Utc::now().with_timezone(&chrono_tz::Africa::Nairobi).format("%Y%m%d%H%M%S").to_string();
    let pwd = base64::engine::general_purpose::STANDARD.encode(format!("{}{}{}", cfg.shortcode, cfg.passkey, ts));
    (pwd, ts)
}

pub struct StkAccepted {
    pub merchant_request_id: String,
    pub checkout_request_id: String,
    pub customer_message: String,
}

pub async fn stk_push(state: &AppState, phone: &str, amount: Decimal, account_ref: &str, desc: &str) -> AppResult<StkAccepted> {
    let cfg = config(state)?;
    let token = access_token(state, cfg).await?;
    let (pwd, ts) = password(cfg);
    // M-Pesa only accepts whole shillings.
    let whole = amount.ceil().to_i64().unwrap_or(0).max(1);
    let callback = format!("{}/api/webhooks/mpesa/{}", state.cfg.public_url, cfg.callback_token);

    let payload = json!({
        "BusinessShortCode": cfg.shortcode,
        "Password": pwd,
        "Timestamp": ts,
        "TransactionType": cfg.transaction_type,
        "Amount": whole,
        "PartyA": phone,
        "PartyB": cfg.party_b,
        "PhoneNumber": phone,
        "CallBackURL": callback,
        "AccountReference": account_ref.chars().take(12).collect::<String>(),
        "TransactionDesc": desc.chars().take(13).collect::<String>(),
    });

    let res = state
        .http
        .post(format!("{}/mpesa/stkpush/v1/processrequest", cfg.base_url))
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("M-Pesa unreachable: {e}")))?;
    let body: Value = res.json().await.unwrap_or(Value::Null);

    if body["ResponseCode"].as_str() != Some("0") {
        let msg = body["errorMessage"]
            .as_str()
            .or(body["ResponseDescription"].as_str())
            .unwrap_or("M-Pesa could not start the payment");
        return Err(AppError::Upstream(msg.to_string()));
    }
    Ok(StkAccepted {
        merchant_request_id: body["MerchantRequestID"].as_str().unwrap_or_default().to_string(),
        checkout_request_id: body["CheckoutRequestID"].as_str().unwrap_or_default().to_string(),
        customer_message: body["CustomerMessage"].as_str().unwrap_or("Check your phone to complete payment").to_string(),
    })
}

/// Returns Some((result_code, result_desc)) once Daraja knows the outcome; None while still processing.
pub async fn stk_query(state: &AppState, checkout_request_id: &str) -> AppResult<Option<(i32, String)>> {
    let cfg = config(state)?;
    let token = access_token(state, cfg).await?;
    let (pwd, ts) = password(cfg);
    let res = state
        .http
        .post(format!("{}/mpesa/stkpushquery/v1/query", cfg.base_url))
        .bearer_auth(token)
        .json(&json!({
            "BusinessShortCode": cfg.shortcode,
            "Password": pwd,
            "Timestamp": ts,
            "CheckoutRequestID": checkout_request_id,
        }))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("M-Pesa unreachable: {e}")))?;
    let body: Value = res.json().await.unwrap_or(Value::Null);
    // While processing, Daraja answers with an errorCode such as 500.001.1001.
    if body.get("errorCode").is_some() {
        return Ok(None);
    }
    let code = body["ResultCode"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok())
        .or_else(|| body["ResultCode"].as_i64().map(|v| v as i32));
    Ok(code.map(|c| (c, body["ResultDesc"].as_str().unwrap_or_default().to_string())))
}

/// Parsed `Body.stkCallback` payload.
pub struct Callback {
    pub checkout_request_id: String,
    pub result_code: i32,
    pub result_desc: String,
    pub amount: Option<Decimal>,
    pub receipt: Option<String>,
}

pub fn parse_callback(body: &Value) -> Option<Callback> {
    let cb = &body["Body"]["stkCallback"];
    let checkout_request_id = cb["CheckoutRequestID"].as_str()?.to_string();
    let result_code = cb["ResultCode"].as_i64()? as i32;
    let mut amount = None;
    let mut receipt = None;
    if let Some(items) = cb["CallbackMetadata"]["Item"].as_array() {
        for item in items {
            match item["Name"].as_str() {
                Some("Amount") => amount = item["Value"].as_f64().and_then(|f| Decimal::try_from(f).ok()),
                Some("MpesaReceiptNumber") => receipt = item["Value"].as_str().map(String::from),
                _ => {}
            }
        }
    }
    Some(Callback {
        checkout_request_id,
        result_code,
        result_desc: cb["ResultDesc"].as_str().unwrap_or_default().to_string(),
        amount,
        receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_success_callback() {
        let body = json!({"Body":{"stkCallback":{
            "MerchantRequestID":"29115-34620561-1","CheckoutRequestID":"ws_CO_191220191020363925",
            "ResultCode":0,"ResultDesc":"The service request is processed successfully.",
            "CallbackMetadata":{"Item":[{"Name":"Amount","Value":1.00},{"Name":"MpesaReceiptNumber","Value":"NLJ7RT61SV"},
            {"Name":"TransactionDate","Value":20191219102115i64},{"Name":"PhoneNumber","Value":254708374149i64}]}}}});
        let cb = parse_callback(&body).unwrap();
        assert_eq!(cb.result_code, 0);
        assert_eq!(cb.receipt.as_deref(), Some("NLJ7RT61SV"));
        assert_eq!(cb.amount, Some(Decimal::ONE));
    }

    #[test]
    fn parses_cancelled_callback() {
        let body = json!({"Body":{"stkCallback":{"MerchantRequestID":"x","CheckoutRequestID":"y","ResultCode":1032,"ResultDesc":"Request cancelled by user"}}});
        let cb = parse_callback(&body).unwrap();
        assert_eq!(cb.result_code, 1032);
        assert!(cb.receipt.is_none());
    }
}
