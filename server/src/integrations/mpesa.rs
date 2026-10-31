//! Safaricom Daraja — Lipa na M-Pesa Online (STK Push).
//!
//! Flow: `stk_push` → customer enters PIN on their phone → Daraja calls our
//! callback (routes::webhooks::mpesa_callback) → request marked success/failed.
//! `stk_query` lets the UI recover when a callback is late or lost. Each business uses its own Till or Paybill
//! and Daraja app (roadmap 91, `mpesa_accounts`).

use std::time::{Duration, Instant};

use base64::Engine;
use chrono::Utc;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Everything one Daraja call needs. Built per request from the business's account (roadmap 91) — or, for the
/// platform owner's own business only, from the `MPESA_*` environment variables.
pub struct Creds {
    pub account_id: Option<Uuid>,
    /// `sandbox` | `production` (logged with refusals; never a secret).
    pub environment: String,
    pub base_url: String,
    pub consumer_key: String,
    pub consumer_secret: String,
    pub shortcode: String,
    pub passkey: String,
    /// "CustomerPayBillOnline" (Paybill) or "CustomerBuyGoodsOnline" (Till)
    pub transaction_type: String,
    /// PartyB: the Till number for Buy Goods, else the shortcode.
    pub party_b: String,
    pub callback_url: String,
    /// Paybill account reference (empty = the caller's reference).
    pub account_reference: String,
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: Uuid,
    kind: String,
    shortcode: String,
    till_number: String,
    account_reference: String,
    environment: String,
    consumer_key: String,
    consumer_secret_enc: String,
    passkey_enc: String,
    callback_token_enc: String,
}

const ACCOUNT_COLS: &str = "id, kind, shortcode, till_number, account_reference, environment, consumer_key, consumer_secret_enc, passkey_enc, callback_token_enc";

/// Daraja's Sandbox accepts STK Push only for its test shortcode, as a Paybill (roadmap 96).
pub const SANDBOX_SHORTCODE: &str = "174379";

fn creds_of(state: &AppState, a: AccountRow) -> AppResult<Creds> {
    // Roadmap 152: read with every key of the ring, so a credential re-encrypted by a newer instance stays readable here.
    let keys = state.cfg.read_keys();
    let token = crate::secrets::open(&keys, &a.callback_token_enc)?;
    let production = a.environment == "production";
    let till = a.kind == "till";
    // Sandbox requests always use Safaricom's test shortcode as a Paybill; the business's own Till / Store / Paybill
    // numbers are used only in production (never substituted into a sandbox request).
    let (shortcode, transaction_type, party_b) = if !production {
        (SANDBOX_SHORTCODE.to_string(), "CustomerPayBillOnline", SANDBOX_SHORTCODE.to_string())
    } else if till {
        (a.shortcode.clone(), "CustomerBuyGoodsOnline", if a.till_number.is_empty() { a.shortcode.clone() } else { a.till_number })
    } else {
        (a.shortcode.clone(), "CustomerPayBillOnline", a.shortcode.clone())
    };
    Ok(Creds {
        account_id: Some(a.id),
        environment: a.environment.clone(),
        base_url: if production { state.cfg.daraja_production_url.clone() } else { state.cfg.daraja_sandbox_url.clone() },
        consumer_secret: crate::secrets::open(&keys, &a.consumer_secret_enc)?,
        passkey: crate::secrets::open(&keys, &a.passkey_enc)?,
        consumer_key: a.consumer_key,
        transaction_type: transaction_type.into(),
        party_b,
        shortcode,
        callback_url: format!("{}/api/webhooks/mpesa/a/{}/{}", state.cfg.public_url, a.id, token),
        account_reference: a.account_reference,
    })
}

/// One account by id (settings: test connection, test prompt, status checks of its requests).
pub async fn account_creds(state: &AppState, tenant: Uuid, id: Uuid) -> AppResult<Creds> {
    let a: AccountRow = sqlx::query_as(&format!("SELECT {ACCOUNT_COLS} FROM mpesa_accounts WHERE id = $1 AND tenant_id = $2"))
        .bind(id)
        .bind(tenant)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("M-Pesa account"))?;
    creds_of(state, a)
}

/// The platform's `MPESA_*` configuration, used only by the platform owner's own business — and by demo businesses,
/// which show S'Shop to prospects — without an account of their own (never by customers' businesses: their customers
/// would pay into the platform's shortcode).
async fn env_creds(state: &AppState, tenant: Uuid) -> AppResult<Option<Creds>> {
    let Some(c) = &state.cfg.mpesa else { return Ok(None) };
    let (own, platform): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM mpesa_accounts WHERE tenant_id = $1), COALESCE((SELECT ownership = 'platform' OR is_demo FROM tenants WHERE id = $1), false)",
    )
    .bind(tenant)
    .fetch_one(&state.db)
    .await?;
    if own || !platform {
        return Ok(None);
    }
    Ok(Some(Creds {
        account_id: None,
        environment: c.environment.clone(),
        base_url: c.base_url.clone(),
        consumer_key: c.consumer_key.clone(),
        consumer_secret: c.consumer_secret.clone(),
        shortcode: c.shortcode.clone(),
        passkey: c.passkey.clone(),
        transaction_type: c.transaction_type.clone(),
        party_b: c.party_b.clone(),
        callback_url: c.callback_url.clone(),
        account_reference: String::new(),
    }))
}

/// The account that takes STK Push payments at this branch: the business's own active account with STK on (verified),
/// one assigned to the branch first, then one for every branch.
pub async fn for_branch(state: &AppState, tenant: Uuid, branch: Uuid) -> AppResult<Creds> {
    let a: Option<AccountRow> = sqlx::query_as(&format!(
        "SELECT {ACCOUNT_COLS} FROM mpesa_accounts
         WHERE tenant_id = $1 AND is_active AND stk_enabled AND (all_branches OR $2 = ANY(branch_ids))
         ORDER BY all_branches, created_at LIMIT 1"
    ))
    .bind(tenant)
    .bind(branch)
    .fetch_optional(&state.db)
    .await?;
    if let Some(a) = a {
        return creds_of(state, a);
    }
    env_creds(state, tenant)
        .await?
        .ok_or_else(|| AppError::Rule("M-Pesa STK Push is not set up for this branch. Capture the M-Pesa code manually.".into()))
}

/// The credentials a request was sent with (status queries must use the same account).
pub async fn for_request(state: &AppState, tenant: Uuid, account: Option<Uuid>) -> AppResult<Creds> {
    match account {
        Some(id) => account_creds(state, tenant, id).await,
        None => env_creds(state, tenant).await?.ok_or_else(|| AppError::Rule("M-Pesa is not configured".into())),
    }
}

/// Where STK Push works: (any, branches — `None` = every branch). For the profile and the till.
pub async fn stk_branches(state: &AppState, tenant: Uuid) -> AppResult<(bool, Option<Vec<Uuid>>)> {
    let rows: Vec<(bool, Vec<Uuid>)> = sqlx::query_as("SELECT all_branches, branch_ids FROM mpesa_accounts WHERE tenant_id = $1 AND is_active AND stk_enabled")
        .bind(tenant)
        .fetch_all(&state.db)
        .await?;
    if rows.iter().any(|r| r.0) {
        return Ok((true, None));
    }
    if !rows.is_empty() {
        let mut ids: Vec<Uuid> = rows.into_iter().flat_map(|r| r.1).collect();
        ids.sort();
        ids.dedup();
        return Ok((!ids.is_empty(), Some(ids)));
    }
    Ok((env_creds(state, tenant).await?.is_some(), None))
}

async fn access_token(state: &AppState, c: &Creds) -> AppResult<String> {
    let cache_key = format!("{}|{}", c.base_url, c.consumer_key);
    if let Some((token, expires)) = state.mpesa_token.lock().await.get(&cache_key) {
        if Instant::now() < *expires {
            return Ok(token.clone());
        }
    }
    let url = format!("{}/oauth/v1/generate?grant_type=client_credentials", c.base_url);
    let res = state
        .http
        .get(&url)
        .basic_auth(&c.consumer_key, Some(&c.consumer_secret))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("M-Pesa unreachable: {e}")))?;
    if !res.status().is_success() {
        return Err(AppError::Upstream("M-Pesa rejected the consumer key or secret".into()));
    }
    let body: Value = res.json().await.map_err(|e| AppError::Upstream(e.to_string()))?;
    let token = body["access_token"].as_str().ok_or_else(|| AppError::Upstream("No M-Pesa token".into()))?.to_string();
    let ttl: u64 = body["expires_in"].as_str().and_then(|s| s.parse().ok()).or_else(|| body["expires_in"].as_u64()).unwrap_or(3599);
    state.mpesa_token.lock().await.insert(cache_key, (token.clone(), Instant::now() + Duration::from_secs(ttl.saturating_sub(60))));
    Ok(token)
}

/// Checks the consumer key and secret: Safaricom issues an access token.
pub async fn check_credentials(state: &AppState, c: &Creds) -> AppResult<()> {
    state.mpesa_token.lock().await.remove(&format!("{}|{}", c.base_url, c.consumer_key));
    access_token(state, c).await.map(|_| ())
}

fn password(c: &Creds) -> (String, String) {
    let ts = Utc::now().with_timezone(&chrono_tz::Africa::Nairobi).format("%Y%m%d%H%M%S").to_string();
    let pwd = base64::engine::general_purpose::STANDARD.encode(format!("{}{}{}", c.shortcode, c.passkey, ts));
    (pwd, ts)
}

pub struct StkAccepted {
    pub merchant_request_id: String,
    pub checkout_request_id: String,
    pub customer_message: String,
}

pub async fn stk_push(state: &AppState, c: &Creds, phone: &str, amount: Decimal, account_ref: &str, desc: &str) -> AppResult<StkAccepted> {
    let token = access_token(state, c).await?;
    let (pwd, ts) = password(c);
    // M-Pesa only accepts whole shillings.
    let whole = amount.ceil().to_i64().unwrap_or(0).max(1);
    let paybill = c.transaction_type == "CustomerPayBillOnline";
    let reference = if paybill && !c.account_reference.trim().is_empty() { c.account_reference.trim() } else { account_ref };

    let payload = json!({
        "BusinessShortCode": c.shortcode,
        "Password": pwd,
        "Timestamp": ts,
        "TransactionType": c.transaction_type,
        "Amount": whole,
        "PartyA": phone,
        "PartyB": c.party_b,
        "PhoneNumber": phone,
        "CallBackURL": c.callback_url,
        "AccountReference": reference.chars().take(12).collect::<String>(),
        "TransactionDesc": desc.chars().take(13).collect::<String>(),
    });

    let res = state
        .http
        .post(format!("{}/mpesa/stkpush/v1/processrequest", c.base_url))
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
        // Sanitised: what was asked and what Safaricom said — never credentials, the password or the access token.
        tracing::warn!(
            environment = %c.environment, transaction_type = %c.transaction_type, shortcode = %c.shortcode, party_b = %c.party_b,
            amount = whole, error_code = %body["errorCode"].as_str().unwrap_or(""), error = %msg, "daraja refused an STK request"
        );
        return Err(AppError::Upstream(explain(msg, c)));
    }
    Ok(StkAccepted {
        merchant_request_id: body["MerchantRequestID"].as_str().unwrap_or_default().to_string(),
        checkout_request_id: body["CheckoutRequestID"].as_str().unwrap_or_default().to_string(),
        customer_message: body["CustomerMessage"].as_str().unwrap_or("Check your phone to complete payment").to_string(),
    })
}

/// Safaricom's refusal with what to do about it (the original wording is kept first).
pub fn explain(msg: &str, c: &Creds) -> String {
    let m = msg.to_lowercase();
    let hint = if m.contains("transactiontype") {
        if c.environment == "sandbox" {
            format!("Sandbox only accepts Safaricom's test shortcode {SANDBOX_SHORTCODE} as a Paybill (CustomerPayBillOnline).")
        } else {
            "The transaction type does not match this shortcode: a Buy Goods Till needs its Store / Head Office number as the shortcode and the Till number as the till, and Safaricom must have enabled M-Pesa Express (STK Push) for it; a Paybill uses CustomerPayBillOnline.".to_string()
        }
    } else if m.contains("password") || m.contains("passkey") {
        "The passkey does not match the shortcode — use the Lipa na M-Pesa Online passkey issued for this shortcode.".to_string()
    } else if m.contains("access token") || m.contains("authentication") {
        "The consumer key or secret is not accepted — check them against the Daraja app.".to_string()
    } else if m.contains("businessshortcode") || m.contains("partyb") {
        "Safaricom does not accept this shortcode or till for STK Push in this environment.".to_string()
    } else if m.contains("phonenumber") || m.contains("partya") {
        "Check the phone number (07…, 01… or 254…).".to_string()
    } else if m.contains("callback") {
        "Safaricom refused the callback URL — it must be public HTTPS.".to_string()
    } else {
        return msg.to_string();
    };
    format!("{msg} — {hint}")
}

/// Returns Some((result_code, result_desc)) once Daraja knows the outcome; None while still processing.
pub async fn stk_query(state: &AppState, c: &Creds, checkout_request_id: &str) -> AppResult<Option<(i32, String)>> {
    let token = access_token(state, c).await?;
    let (pwd, ts) = password(c);
    let res = state
        .http
        .post(format!("{}/mpesa/stkpushquery/v1/query", c.base_url))
        .bearer_auth(token)
        .json(&json!({
            "BusinessShortCode": c.shortcode,
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

    fn creds(env: &str) -> Creds {
        Creds {
            account_id: None, environment: env.into(), base_url: String::new(), consumer_key: String::new(), consumer_secret: String::new(),
            shortcode: "7928587".into(), passkey: String::new(), transaction_type: "CustomerBuyGoodsOnline".into(), party_b: "4489262".into(),
            callback_url: String::new(), account_reference: String::new(),
        }
    }

    #[test]
    fn refusals_are_explained() {
        let s = explain("Bad Request - Invalid TransactionType", &creds("sandbox"));
        assert!(s.starts_with("Bad Request - Invalid TransactionType") && s.contains("174379"), "{s}");
        assert!(explain("Bad Request - Invalid TransactionType", &creds("production")).contains("Store / Head Office"));
        assert!(explain("Bad Request - Invalid Password", &creds("sandbox")).contains("passkey"));
        assert_eq!(explain("Something new", &creds("sandbox")), "Something new");
    }

    #[test]
    fn parses_cancelled_callback() {
        let body = json!({"Body":{"stkCallback":{"MerchantRequestID":"x","CheckoutRequestID":"y","ResultCode":1032,"ResultDesc":"Request cancelled by user"}}});
        let cb = parse_callback(&body).unwrap();
        assert_eq!(cb.result_code, 1032);
        assert!(cb.receipt.is_none());
    }
}
