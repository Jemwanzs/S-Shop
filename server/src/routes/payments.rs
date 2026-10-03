//! M-Pesa STK Push initiation and status polling (POS, credit repayments).

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::Ctx;
use crate::error::{bad, AppError, AppResult};
use crate::integrations::mpesa;
use crate::state::AppState;
use crate::util::normalize_mobile;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/mpesa/stk", post(push))
        .route("/mpesa/stk/{id}", get(status))
}

#[derive(Deserialize)]
struct PushBody {
    phone: String,
    amount: Decimal,
    #[serde(default)]
    reference: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct Request {
    id: Uuid,
    phone: String,
    amount: Decimal,
    status: String,
    result_desc: Option<String>,
    mpesa_receipt: Option<String>,
    consumed: bool,
    created_at: DateTime<Utc>,
}

async fn push(State(state): State<AppState>, ctx: Ctx, Json(b): Json<PushBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["sales.create", "credit.collect", "orders.manage"])?;
    if b.amount < Decimal::ONE {
        return Err(bad("Amount must be at least 1"));
    }
    let phone = normalize_mobile(&b.phone)?;
    let account_ref = if b.reference.trim().is_empty() { "SShop".to_string() } else { b.reference.trim().to_string() };
    let accepted = mpesa::stk_push(&state, &phone, b.amount, &account_ref, "Payment").await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO mpesa_requests (tenant_id, branch_id, merchant_request_id, checkout_request_id, phone, amount, account_ref, user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(ctx.branch_id)
    .bind(&accepted.merchant_request_id)
    .bind(&accepted.checkout_request_id)
    .bind(&phone)
    .bind(b.amount.ceil())
    .bind(&account_ref)
    .bind(ctx.user_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "id": id, "status": "pending", "message": accepted.customer_message })))
}

/// Status for the POS to poll. If the callback is late, ask Daraja directly.
async fn status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Request>> {
    ctx.require_any(&["sales.create", "credit.collect", "orders.manage"])?;
    let load = || {
        sqlx::query_as::<_, Request>(
            "SELECT id, phone, amount, status, result_desc, mpesa_receipt, consumed_by IS NOT NULL AS consumed, created_at
             FROM mpesa_requests WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(ctx.tenant_id)
    };
    let req = load().fetch_optional(&state.db).await?.ok_or(AppError::NotFound("M-Pesa request"))?;

    if req.status == "pending" && Utc::now() - req.created_at > chrono::Duration::seconds(20) {
        let checkout: Option<String> = sqlx::query_scalar("SELECT checkout_request_id FROM mpesa_requests WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
        if let Some(checkout) = checkout {
            match mpesa::stk_query(&state, &checkout).await {
                // Success is only recorded from the callback, which carries the receipt number.
                Ok(Some((code, desc))) if code != 0 => {
                    let status = if code == 1032 { "cancelled" } else { "failed" };
                    sqlx::query("UPDATE mpesa_requests SET status=$2, result_code=$3, result_desc=$4, updated_at=now() WHERE id=$1 AND status='pending'")
                        .bind(id)
                        .bind(status)
                        .bind(code)
                        .bind(&desc)
                        .execute(&state.db)
                        .await?;
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "stk query failed"),
            }
        }
        if Utc::now() - req.created_at > chrono::Duration::minutes(3) {
            sqlx::query("UPDATE mpesa_requests SET status='timeout', result_desc='No response from M-Pesa', updated_at=now() WHERE id=$1 AND status='pending'")
                .bind(id)
                .execute(&state.db)
                .await?;
        }
        return Ok(Json(load().fetch_one(&state.db).await?));
    }
    Ok(Json(req))
}
