//! M-Pesa: STK Push initiation and status (POS, deposits, credit repayments, orders), each business's own Till /
//! Paybill accounts (roadmap 91) and the payment activity log.
//!
//! A payment is only ever marked paid from Safaricom: the callback (with the M-Pesa receipt) or a status query
//! (`ResultCode 0`). Accepting an STK request means only that the prompt was sent.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::integrations::mpesa;
use crate::state::AppState;
use crate::util::normalize_mobile;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/mpesa/stk", post(push))
        .route("/mpesa/stk/{id}", get(status))
        .route("/mpesa/accounts", get(list_accounts).post(create_account))
        .route("/mpesa/accounts/{id}", put(update_account).delete(delete_account))
        .route("/mpesa/accounts/{id}/test", post(test_connection))
        .route("/mpesa/accounts/{id}/test-prompt", post(test_prompt))
        .route("/mpesa/accounts/{id}/stk", put(set_stk))
        .route("/mpesa/accounts/{id}/check-status", post(check_status))
        .route("/mpesa/platform/check-status", post(platform_check_status))
        .route("/mpesa/activity", get(activity))
        .route("/mpesa/platform", get(platform_status))
        .route("/mpesa/platform/test", post(platform_test))
        .route("/mpesa/platform/test-prompt", post(platform_test_prompt))
}

/// Which M-Pesa requests a sale or credit payment may use: payments taken first, and a waiting sale's own request
/// while that sale is being completed — never a test prompt (roadmap 91).
pub const CONSUMABLE: &str = "(purpose = 'payment' OR (purpose = 'sale' AND EXISTS (SELECT 1 FROM pending_sales ps
    WHERE ps.mpesa_request_id = mpesa_requests.id AND ps.status = 'completed' AND ps.sale_id IS NULL)))";

#[derive(Deserialize)]
struct PushBody {
    phone: String,
    amount: Decimal,
    #[serde(default)]
    reference: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Request {
    pub id: Uuid,
    pub phone: String,
    pub amount: Decimal,
    pub status: String,
    pub result_desc: Option<String>,
    pub mpesa_receipt: Option<String>,
    pub consumed: bool,
    pub created_at: DateTime<Utc>,
}

/// Sends a prompt through the account of the cashier's branch and records the request.
pub async fn send(state: &AppState, ctx: &Ctx, branch: Uuid, phone: &str, amount: Decimal, reference: &str, purpose: &str) -> AppResult<(Uuid, String)> {
    let creds = mpesa::for_branch(state, ctx.tenant_id, branch).await?;
    let accepted = match mpesa::stk_push(state, &creds, phone, amount, reference, "Payment").await {
        Ok(a) => a,
        Err(e) => {
            note_error(state, creds.account_id, &e.to_string()).await;
            return Err(e);
        }
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mpesa_requests (tenant_id, branch_id, merchant_request_id, checkout_request_id, phone, amount, account_ref, user_id, account_id, purpose)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(&accepted.merchant_request_id)
    .bind(&accepted.checkout_request_id)
    .bind(phone)
    .bind(amount.ceil())
    .bind(reference)
    .bind(ctx.user_id)
    .bind(creds.account_id)
    .bind(purpose)
    .fetch_one(&state.db)
    .await?;
    Ok((id, accepted.customer_message))
}

async fn push(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PushBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["sales.create", "credit.collect", "orders.manage"])?;
    if b.amount < Decimal::ONE {
        return Err(bad("Amount must be at least 1"));
    }
    let phone = normalize_mobile(&b.phone)?;
    let account_ref = if b.reference.trim().is_empty() { "SShop".to_string() } else { b.reference.trim().to_string() };
    let (id, message) = send(&state, &ctx, ctx.branch_id, &phone, b.amount, &account_ref, "payment").await?;
    Ok(Json(json!({ "id": id, "status": "pending", "message": message })))
}

/// Status for the POS to poll. A late callback is recovered with a status query.
async fn status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Request>> {
    ctx.require_any(&["sales.create", "credit.collect", "orders.manage"])?;
    reconcile(&state, ctx.tenant_id, id).await?;
    Ok(Json(load(&state, ctx.tenant_id, id).await?))
}

/// The automatic check: a pending request quiet for 20 s is asked about; after 3 minutes it times out.
pub async fn reconcile(state: &AppState, tenant: Uuid, id: Uuid) -> AppResult<()> {
    reconcile_with(state, tenant, id, false).await
}

pub async fn load(state: &AppState, tenant: Uuid, id: Uuid) -> AppResult<Request> {
    sqlx::query_as::<_, Request>(
        "SELECT id, phone, amount, status, result_desc, mpesa_receipt, consumed_by IS NOT NULL AS consumed, created_at
         FROM mpesa_requests WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("M-Pesa request"))
}

/// A pending request older than 20 s (or any, when `force`d by *Check payment status*) is checked with Safaricom;
/// after 3 minutes without an answer it times out.
pub async fn reconcile_with(state: &AppState, tenant: Uuid, id: Uuid, force: bool) -> AppResult<()> {
    let row: Option<(String, DateTime<Utc>, Option<String>, Option<Uuid>)> =
        sqlx::query_as("SELECT status, created_at, checkout_request_id, account_id FROM mpesa_requests WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant)
            .fetch_optional(&state.db)
            .await?;
    let Some((status, created, checkout, account)) = row else { return Err(AppError::NotFound("M-Pesa request")) };
    if status != "pending" || (!force && Utc::now() - created < chrono::Duration::seconds(20)) {
        return Ok(());
    }
    if let Some(checkout) = checkout {
        match mpesa::for_request(state, tenant, account).await {
            Ok(creds) => match mpesa::stk_query(state, &creds, &checkout).await {
                Ok(Some((code, desc))) => {
                    apply_result(state, id, code, &desc, None, None, "query", None).await?;
                    return Ok(());
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "stk query failed"),
            },
            Err(e) => tracing::warn!(error = %e, "stk query: no credentials"),
        }
    }
    if Utc::now() - created > chrono::Duration::minutes(3) {
        let n = sqlx::query("UPDATE mpesa_requests SET status='timeout', result_desc='No response from M-Pesa', updated_at=now() WHERE id=$1 AND status='pending'")
            .bind(id)
            .execute(&state.db)
            .await?
            .rows_affected();
        if n > 0 {
            super::stk_sales::settle(state, id).await;
        }
    }
    Ok(())
}

/// Records Safaricom's answer for one request, exactly once (callback or status query).
/// `ResultCode 0` = paid; 1032 = cancelled by the customer; 1037 = the phone could not be reached; else failed.
/// A paid amount different from the requested one is kept as reported (`paid_amount`) and never trusted for more.
#[allow(clippy::too_many_arguments)]
pub async fn apply_result(
    state: &AppState,
    id: Uuid,
    code: i32,
    desc: &str,
    receipt: Option<&str>,
    paid: Option<Decimal>,
    via: &str,
    raw: Option<&Value>,
) -> AppResult<()> {
    let status = match code {
        0 => "success",
        1032 => "cancelled",
        1037 => "timeout",
        _ => "failed",
    };
    let updated: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
        "UPDATE mpesa_requests SET status = $2, result_code = $3, result_desc = $4, mpesa_receipt = $5,
                paid_amount = CASE WHEN $2 = 'success' THEN COALESCE($6, amount) END, raw_callback = COALESCE($7, raw_callback),
                confirmed_by = $8, updated_at = now()
         WHERE id = $1 AND status IN ('pending', 'timeout') RETURNING tenant_id, purpose, mpesa_receipt",
    )
    .bind(id)
    .bind(status)
    .bind(code)
    .bind(desc)
    .bind(receipt)
    .bind(paid)
    .bind(raw)
    .bind(via)
    .fetch_optional(&state.db)
    .await?;
    match updated {
        Some((tenant, purpose, receipt)) => {
            state.emit(tenant, None, "mpesa", json!({ "id": id, "status": status, "receipt": receipt, "message": desc }));
            if purpose == "sale" {
                super::stk_sales::settle(state, id).await;
            }
        }
        None if code == 0 && receipt.is_some() => {
            // Confirmed earlier by a status query (no receipt then): the late callback adds the M-Pesa receipt number
            // to the request and to the payment that used it.
            let filled = sqlx::query("UPDATE mpesa_requests SET mpesa_receipt = $2, raw_callback = COALESCE($3, raw_callback), updated_at = now() WHERE id = $1 AND status = 'success' AND mpesa_receipt IS NULL")
                .bind(id)
                .bind(receipt)
                .bind(raw)
                .execute(&state.db)
                .await?
                .rows_affected();
            if filled > 0 {
                sqlx::query("UPDATE payments SET reference = $2 WHERE mpesa_request_id = $1 AND reference = ''").bind(id).bind(receipt).execute(&state.db).await?;
            }
        }
        None => tracing::info!(%id, "mpesa result for a request already settled"),
    }
    Ok(())
}

/// Remembers the last integration error on the account (shown in Settings → M-Pesa).
async fn note_error(state: &AppState, account: Option<Uuid>, msg: &str) {
    if let Some(a) = account {
        let _ = sqlx::query("UPDATE mpesa_accounts SET last_error = $2, last_error_at = now() WHERE id = $1").bind(a).bind(msg).execute(&state.db).await;
    }
}

// ── The platform's own Daraja app (roadmap 92, Railway variables) ──────────────────────────────────────────────────

/// What the platform owner sees of the `MPESA_*` configuration: never a secret; the callback secret masked.
async fn platform_status(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    super::access::require_platform_admin(&state, &ctx).await?;
    let applies = mpesa::for_request(&state, ctx.tenant_id, None).await.is_ok();
    Ok(Json(match &state.cfg.mpesa {
        Some(c) => {
            let token = &c.callback_token;
            let tail: String = token.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
            let masked = c.callback_url.replace(token.as_str(), &format!("••••{tail}"));
            json!({
                "configured": true, "environment": c.environment, "shortcode": c.shortcode, "transaction_type": c.transaction_type,
                "party_b": c.party_b, "callback_url": masked,
                "callback_matches_server": c.callback_url.starts_with(state.cfg.public_url.trim_end_matches('/')),
                "public_url": state.cfg.public_url, "used_by_this_business": applies, "problem": Value::Null,
            })
        }
        None => json!({ "configured": false, "problem": state.cfg.mpesa_problem, "public_url": state.cfg.public_url }),
    }))
}

async fn platform_creds(state: &AppState, ctx: &Ctx) -> AppResult<mpesa::Creds> {
    super::access::require_platform_admin(state, ctx).await?;
    ctx.require_full()?;
    mpesa::for_request(state, ctx.tenant_id, None).await.map_err(|_| {
        refused(
            "Platform M-Pesa not in use",
            state.cfg.mpesa_problem.clone().unwrap_or_else(|| "Set the MPESA_* variables on Railway (or this business already has its own M-Pesa account)".into()),
        )
    })
}

/// OAuth: Safaricom issues an access token for the platform's consumer key and secret.
async fn platform_test(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let creds = platform_creds(&state, &ctx).await?;
    Ok(Json(match mpesa::check_credentials(&state, &creds).await {
        Ok(()) => json!({ "ok": true, "message": "Safaricom issued an access token (OAuth works)" }),
        Err(e) => json!({ "ok": false, "message": e.to_string() }),
    }))
}

/// A KSh 1 test prompt through the platform configuration; its result arrives on the callback URL (or the status query).
async fn platform_test_prompt(State(state): State<AppState>, ctx: Ctx, Json(b): Json<TestPrompt>) -> AppResult<Json<Value>> {
    let creds = platform_creds(&state, &ctx).await?;
    let phone = normalize_mobile(&b.phone)?;
    let amount = b.amount()?;
    match mpesa::stk_push(&state, &creds, &phone, amount, "Test", "S'Shop test").await {
        Ok(accepted) => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO mpesa_requests (tenant_id, branch_id, merchant_request_id, checkout_request_id, phone, amount, account_ref, user_id, purpose)
                 VALUES ($1, $2, $3, $4, $5, $7, 'Test', $6, 'test') RETURNING id",
            )
            .bind(ctx.tenant_id)
            .bind(ctx.branch_id)
            .bind(&accepted.merchant_request_id)
            .bind(&accepted.checkout_request_id)
            .bind(&phone)
            .bind(ctx.user_id)
            .bind(amount)
            .fetch_one(&state.db)
            .await?;
            Ok(Json(json!({ "ok": true, "id": id, "message": "Prompt accepted by Safaricom — the result appears in M-Pesa activity when the callback arrives" })))
        }
        Err(e) => Ok(Json(json!({ "ok": false, "message": e.to_string() }))),
    }
}

// ── Accounts (roadmap 91) ───────────────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct AccountOut {
    id: Uuid,
    name: String,
    kind: String,
    shortcode: String,
    till_number: String,
    account_reference: String,
    environment: String,
    consumer_key_hint: String,
    has_secret: bool,
    has_passkey: bool,
    is_active: bool,
    stk_enabled: bool,
    verified_at: Option<DateTime<Utc>>,
    last_tested_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    last_error_at: Option<DateTime<Utc>>,
    /// Roadmap 96: the checks, each on its own — credentials (OAuth), STK request accepted (`verified_at`), and the
    /// latest test prompt's outcome (`success` = a test payment confirmed by Safaricom).
    oauth_ok_at: Option<DateTime<Utc>>,
    last_test_status: Option<String>,
    last_test_at: Option<DateTime<Utc>>,
    all_branches: bool,
    branch_ids: Vec<Uuid>,
    updated_at: DateTime<Utc>,
}

/// Secrets never leave the server: the key shows its last four characters, the secret and passkey only whether set.
const ACCOUNT_OUT: &str = "id, name, kind, shortcode, till_number, account_reference, environment,
    CASE WHEN length(consumer_key) > 4 THEN '••••' || right(consumer_key, 4) ELSE '' END AS consumer_key_hint,
    consumer_secret_enc <> '' AS has_secret, passkey_enc <> '' AS has_passkey, is_active, stk_enabled, verified_at,
    last_tested_at, last_error, last_error_at, oauth_ok_at,
    (SELECT r.status FROM mpesa_requests r WHERE r.account_id = mpesa_accounts.id AND r.purpose = 'test' ORDER BY r.created_at DESC LIMIT 1) AS last_test_status,
    (SELECT r.created_at FROM mpesa_requests r WHERE r.account_id = mpesa_accounts.id AND r.purpose = 'test' ORDER BY r.created_at DESC LIMIT 1) AS last_test_at,
    all_branches, branch_ids, updated_at";

fn can_view(ctx: &Ctx) -> AppResult<()> {
    ctx.require_any(&["settings.integrations", "settings.sales"])
}

/// Changing how a business is paid: its administrators (or `settings.integrations`), fully signed in, never from a
/// support session.
fn can_manage(ctx: &Ctx) -> AppResult<()> {
    ctx.require("settings.integrations")?;
    ctx.require_full()?;
    if ctx.support.is_some() {
        return Err(refused("Not from a support session", "Only the business's own people set up how it is paid"));
    }
    Ok(())
}

async fn list_accounts(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    can_view(&ctx)?;
    let items: Vec<AccountOut> = sqlx::query_as(&format!("SELECT {ACCOUNT_OUT} FROM mpesa_accounts WHERE tenant_id = $1 ORDER BY created_at"))
        .bind(ctx.tenant_id)
        .fetch_all(&state.db)
        .await?;
    let (any, branches) = mpesa::stk_branches(&state, ctx.tenant_id).await?;
    let platform_env: bool = items.is_empty() && any;
    Ok(Json(json!({
        "items": items,
        "callback_base": format!("{}/api/webhooks/mpesa/a/", state.cfg.public_url),
        "stk": { "available": any, "branches": branches, "platform_config": platform_env },
    })))
}

#[derive(Deserialize)]
struct AccountBody {
    name: String,
    kind: String,
    shortcode: String,
    #[serde(default)]
    till_number: String,
    #[serde(default)]
    account_reference: String,
    environment: String,
    #[serde(default)]
    consumer_key: String,
    /// Write-only: empty keeps the saved value.
    #[serde(default)]
    consumer_secret: String,
    #[serde(default)]
    passkey: String,
    #[serde(default = "yes")]
    is_active: bool,
    #[serde(default = "yes")]
    all_branches: bool,
    #[serde(default)]
    branch_ids: Vec<Uuid>,
}
fn yes() -> bool {
    true
}

fn digits(v: &str, label: &str, min: usize, max: usize) -> AppResult<String> {
    let v = v.trim();
    if v.len() < min || v.len() > max || !v.chars().all(|c| c.is_ascii_digit()) {
        return Err(bad(format!("{label}: {min}–{max} digits")));
    }
    Ok(v.to_string())
}

async fn validate_account(state: &AppState, ctx: &Ctx, b: &mut AccountBody) -> AppResult<()> {
    b.name = b.name.trim().to_string();
    if b.name.is_empty() || b.name.len() > 60 {
        return Err(bad("Give the account a name (up to 60 characters), e.g. Main Till"));
    }
    if !["till", "paybill"].contains(&b.kind.as_str()) {
        return Err(bad("Choose Buy Goods Till or Paybill"));
    }
    if !["sandbox", "production"].contains(&b.environment.as_str()) {
        return Err(bad("Choose Sandbox (testing) or Production"));
    }
    b.shortcode = digits(&b.shortcode, if b.kind == "till" { "Store / Head Office number" } else { "Paybill number" }, 5, 10)?;
    b.till_number = if b.kind == "till" { digits(&b.till_number, "Till number", 5, 10)? } else { String::new() };
    b.account_reference = b.account_reference.trim().chars().take(12).collect();
    b.consumer_key = b.consumer_key.trim().to_string();
    if b.consumer_key.len() > 200 || b.consumer_secret.trim().len() > 200 || b.passkey.trim().len() > 200 {
        return Err(bad("Credentials are too long — paste them from the Daraja portal"));
    }
    if !b.all_branches {
        if b.branch_ids.is_empty() {
            return Err(bad("Choose the branches this account takes payments for"));
        }
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM branches WHERE tenant_id = $1 AND id = ANY($2)")
            .bind(ctx.tenant_id)
            .bind(&b.branch_ids)
            .fetch_one(&state.db)
            .await?;
        if n as usize != b.branch_ids.len() {
            return Err(bad("Unknown branch"));
        }
    } else {
        b.branch_ids.clear();
    }
    Ok(())
}

async fn create_account(State(state): State<AppState>, ctx: Ctx, Json(mut b): Json<AccountBody>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    validate_account(&state, &ctx, &mut b).await?;
    if b.consumer_key.is_empty() || b.consumer_secret.trim().is_empty() || b.passkey.trim().is_empty() {
        return Err(bad("Enter the consumer key, consumer secret and passkey from the Daraja portal"));
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM mpesa_accounts WHERE tenant_id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    if n >= 20 {
        return Err(rule("Up to 20 M-Pesa accounts per business"));
    }
    let key = &state.cfg.data_key.0;
    let token = crate::secrets::random_token();
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mpesa_accounts (tenant_id, name, kind, shortcode, till_number, account_reference, environment, consumer_key,
                consumer_secret_enc, passkey_enc, callback_token_enc, callback_token_digest, is_active, all_branches, branch_ids, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&b.name)
    .bind(&b.kind)
    .bind(&b.shortcode)
    .bind(&b.till_number)
    .bind(&b.account_reference)
    .bind(&b.environment)
    .bind(&b.consumer_key)
    .bind(crate::secrets::encrypt(key, b.consumer_secret.trim())?)
    .bind(crate::secrets::encrypt(key, b.passkey.trim())?)
    .bind(crate::secrets::encrypt(key, &token)?)
    .bind(crate::secrets::digest(&token))
    .bind(b.is_active)
    .bind(b.all_branches)
    .bind(&b.branch_ids)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "mpesa_account_created", "mpesa_account", id).after(public_view(&b))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

/// What the audit trail keeps of an account: never a secret.
fn public_view(b: &AccountBody) -> Value {
    json!({ "name": b.name, "kind": b.kind, "shortcode": b.shortcode, "till_number": b.till_number, "environment": b.environment,
            "is_active": b.is_active, "all_branches": b.all_branches, "branch_ids": b.branch_ids })
}

async fn update_account(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(mut b): Json<AccountBody>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    validate_account(&state, &ctx, &mut b).await?;
    let key = &state.cfg.data_key.0;
    let mut tx = state.db.begin().await?;
    let before: Option<(String, String, String, String, String)> =
        sqlx::query_as("SELECT kind, shortcode, till_number, environment, consumer_key FROM mpesa_accounts WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?;
    let before = before.ok_or(AppError::NotFound("M-Pesa account"))?;
    let consumer_key = if b.consumer_key.is_empty() { before.4.clone() } else { b.consumer_key.clone() };
    // New credentials or a different till must be verified again before STK Push is used.
    let reverify = before != (b.kind.clone(), b.shortcode.clone(), b.till_number.clone(), b.environment.clone(), consumer_key.clone())
        || !b.consumer_secret.trim().is_empty()
        || !b.passkey.trim().is_empty();
    let secret = if b.consumer_secret.trim().is_empty() { None } else { Some(crate::secrets::encrypt(key, b.consumer_secret.trim())?) };
    let passkey = if b.passkey.trim().is_empty() { None } else { Some(crate::secrets::encrypt(key, b.passkey.trim())?) };
    sqlx::query(
        "UPDATE mpesa_accounts SET name = $3, kind = $4, shortcode = $5, till_number = $6, account_reference = $7, environment = $8,
                consumer_key = $9, consumer_secret_enc = COALESCE($10, consumer_secret_enc), passkey_enc = COALESCE($11, passkey_enc),
                is_active = $12, all_branches = $13, branch_ids = $14,
                stk_enabled = CASE WHEN $15 THEN false ELSE stk_enabled END, verified_at = CASE WHEN $15 THEN NULL ELSE verified_at END,
                oauth_ok_at = CASE WHEN $15 THEN NULL ELSE oauth_ok_at END,
                updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(&b.name)
    .bind(&b.kind)
    .bind(&b.shortcode)
    .bind(&b.till_number)
    .bind(&b.account_reference)
    .bind(&b.environment)
    .bind(&consumer_key)
    .bind(secret)
    .bind(passkey)
    .bind(b.is_active)
    .bind(b.all_branches)
    .bind(&b.branch_ids)
    .bind(reverify)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("settings", "mpesa_account_updated", "mpesa_account", id).after(json!({ "account": public_view(&b), "credentials_changed": reverify })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "needs_verification": reverify })))
}

/// Removes an account that never took a payment; one with history is switched off instead (kept for the records).
async fn delete_account(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    let mut tx = state.db.begin().await?;
    let used: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM mpesa_requests WHERE account_id = $1 AND purpose <> 'test')")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let n = if used {
        sqlx::query("UPDATE mpesa_accounts SET is_active = false, stk_enabled = false, updated_at = now() WHERE id = $1 AND tenant_id = $2")
    } else {
        sqlx::query("DELETE FROM mpesa_accounts WHERE id = $1 AND tenant_id = $2")
    }
    .bind(id)
    .bind(ctx.tenant_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("M-Pesa account"));
    }
    audit::record(&mut tx, &ctx, Entry::new("settings", if used { "mpesa_account_deactivated" } else { "mpesa_account_deleted" }, "mpesa_account", id)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "deactivated": used })))
}

/// Checks the consumer key and secret with Safaricom.
async fn test_connection(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    let creds = mpesa::account_creds(&state, ctx.tenant_id, id).await?;
    let result = mpesa::check_credentials(&state, &creds).await;
    let error = result.as_ref().err().map(|e| e.to_string());
    sqlx::query(
        "UPDATE mpesa_accounts SET last_tested_at = now(), last_error = $2, last_error_at = CASE WHEN $2 IS NULL THEN last_error_at ELSE now() END,
                oauth_ok_at = CASE WHEN $2 IS NULL THEN now() ELSE NULL END WHERE id = $1",
    )
    .bind(id)
    .bind(&error)
    .execute(&state.db)
    .await?;
    // OAuth proves the credentials only — STK Push is proven by an accepted test prompt.
    Ok(Json(json!({ "ok": error.is_none(), "message": error.unwrap_or_else(|| "OAuth works: Safaricom issued an access token. Send a test prompt to check STK Push.".into()) })))
}

#[derive(Deserialize)]
struct TestPrompt {
    phone: String,
    /// A controlled real payment in production: KSh 1–100 (default 1).
    #[serde(default)]
    amount: Option<i64>,
}

impl TestPrompt {
    fn amount(&self) -> AppResult<Decimal> {
        match self.amount.unwrap_or(1) {
            a @ 1..=100 => Ok(Decimal::from(a)),
            _ => Err(bad("A test payment is KSh 1–100")),
        }
    }
}

/// The latest test prompt of an account (`None` = the platform configuration) in this business, checked with
/// Safaricom now: *Check payment status*.
async fn check_latest_test(state: &AppState, tenant: Uuid, account: Option<Uuid>) -> AppResult<Json<Value>> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM mpesa_requests WHERE tenant_id = $1 AND purpose = 'test' AND account_id IS NOT DISTINCT FROM $2 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(account)
    .fetch_optional(&state.db)
    .await?;
    let id = id.ok_or_else(|| rule("No test payment yet — send a test STK Push first"))?;
    reconcile_with(state, tenant, id, true).await?;
    let r = load(state, tenant, id).await?;
    let message = match r.status.as_str() {
        "success" => format!("Payment confirmed by Safaricom{}", r.mpesa_receipt.as_deref().map(|x| format!(" — receipt {x}")).unwrap_or_default()),
        "pending" => "Still waiting — the customer has not completed (or Safaricom has not answered) yet".to_string(),
        _ => r.result_desc.clone().unwrap_or_else(|| "Payment not completed".into()),
    };
    Ok(Json(json!({ "ok": r.status == "success", "status": r.status, "message": message, "request": r })))
}

async fn check_status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    let mine: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM mpesa_accounts WHERE id = $1 AND tenant_id = $2)")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_one(&state.db)
        .await?;
    if !mine {
        return Err(AppError::NotFound("M-Pesa account"));
    }
    check_latest_test(&state, ctx.tenant_id, Some(id)).await
}

async fn platform_check_status(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    platform_creds(&state, &ctx).await?;
    check_latest_test(&state, ctx.tenant_id, None).await
}

/// Sends a KSh 1 prompt to a phone. Safaricom accepting it proves the shortcode, passkey and Till / Paybill
/// together: the account is verified and STK Push can be switched on. Nobody has to pay it.
async fn test_prompt(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<TestPrompt>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    let phone = normalize_mobile(&b.phone)?;
    let amount = b.amount()?;
    let creds = mpesa::account_creds(&state, ctx.tenant_id, id).await?;
    match mpesa::stk_push(&state, &creds, &phone, amount, "Test", "S'Shop test").await {
        Ok(accepted) => {
            let mut tx = state.db.begin().await?;
            sqlx::query(
                "INSERT INTO mpesa_requests (tenant_id, branch_id, merchant_request_id, checkout_request_id, phone, amount, account_ref, user_id, account_id, purpose)
                 VALUES ($1, $2, $3, $4, $5, $8, 'Test', $6, $7, 'test')",
            )
            .bind(ctx.tenant_id)
            .bind(ctx.branch_id)
            .bind(&accepted.merchant_request_id)
            .bind(&accepted.checkout_request_id)
            .bind(&phone)
            .bind(ctx.user_id)
            .bind(id)
            .bind(amount)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE mpesa_accounts SET verified_at = now(), last_tested_at = now(), last_error = NULL, oauth_ok_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
            audit::record(&mut tx, &ctx, Entry::new("settings", "mpesa_account_verified", "mpesa_account", id)).await?;
            tx.commit().await?;
            Ok(Json(json!({ "ok": true, "message": format!("STK request accepted by Safaricom — the account is verified. Paying the KSh {amount} prompt confirms a payment end to end (or cancel it on the phone); then Check payment status.") })))
        }
        Err(e) => {
            sqlx::query("UPDATE mpesa_accounts SET last_tested_at = now(), last_error = $2, last_error_at = now() WHERE id = $1")
                .bind(id)
                .bind(e.to_string())
                .execute(&state.db)
                .await?;
            Ok(Json(json!({ "ok": false, "message": e.to_string() })))
        }
    }
}

#[derive(Deserialize)]
struct StkBody {
    enabled: bool,
}

/// STK Push on or off for an account — on only once verified (and active).
async fn set_stk(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<StkBody>) -> AppResult<Json<Value>> {
    can_manage(&ctx)?;
    let row: Option<(Option<DateTime<Utc>>, bool)> = sqlx::query_as("SELECT verified_at, is_active FROM mpesa_accounts WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&state.db)
        .await?;
    let (verified, active) = row.ok_or(AppError::NotFound("M-Pesa account"))?;
    if b.enabled && (verified.is_none() || !active) {
        return Err(refused(
            "Verify the account first",
            "Send a test prompt and let Safaricom accept it with these credentials before switching STK Push on",
        ));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE mpesa_accounts SET stk_enabled = $2, updated_at = now() WHERE id = $1").bind(id).bind(b.enabled).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", if b.enabled { "mpesa_stk_on" } else { "mpesa_stk_off" }, "mpesa_account", id)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ActivityQuery {
    status: Option<String>,
    limit: Option<i64>,
}

/// Recent M-Pesa requests (payments, waiting sales, tests) with their outcome, and sales needing attention.
async fn activity(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ActivityQuery>) -> AppResult<Json<Value>> {
    can_view(&ctx)?;
    let items: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object('id', r.id, 'created_at', r.created_at, 'status', r.status, 'purpose', r.purpose,
                'amount', r.amount, 'paid_amount', r.paid_amount, 'receipt', r.mpesa_receipt, 'result', r.result_desc,
                'phone', CASE WHEN length(r.phone) > 6 THEN left(r.phone, 6) || '•••' || right(r.phone, 3) ELSE r.phone END,
                'confirmed_by', r.confirmed_by, 'account', a.name, 'branch', b.name, 'cashier', u.name,
                'sale_id', COALESCE(ps.sale_id, CASE WHEN s.id IS NOT NULL THEN s.id END), 'receipt_no', s.receipt_no,
                'pending_id', ps.id, 'pending_status', ps.status, 'pending_error', ps.error)
         FROM mpesa_requests r
         LEFT JOIN mpesa_accounts a ON a.id = r.account_id LEFT JOIN branches b ON b.id = r.branch_id
         LEFT JOIN users u ON u.id = r.user_id LEFT JOIN pending_sales ps ON ps.mpesa_request_id = r.id
         LEFT JOIN sales s ON s.id = COALESCE(ps.sale_id, r.consumed_by)
         WHERE r.tenant_id = $1 AND ($2::text IS NULL OR r.status = $2 OR ($2 = 'attention' AND ps.status = 'attention'))
         ORDER BY r.created_at DESC LIMIT $3",
    )
    .bind(ctx.tenant_id)
    .bind(q.status.as_deref().filter(|s| !s.is_empty()))
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    let attention: i64 = sqlx::query_scalar("SELECT count(*) FROM pending_sales WHERE tenant_id = $1 AND status = 'attention'")
        .bind(ctx.tenant_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "items": items, "attention": attention })))
}
