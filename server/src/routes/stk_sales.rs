//! Sales that wait for an M-Pesa STK Push (roadmap 91).
//!
//! The cashier sends the prompt with the cart: the server validates the whole sale exactly as if completing it (then
//! rolls back), holds its stock, and sends the prompt for the amount due. The sale is recorded — once — only when
//! Safaricom confirms the payment (callback or status query), through the same code as any counter sale, with the
//! cashier's context from when it was submitted. A failed, cancelled or timed-out payment releases the stock and is
//! kept for the record. A payment confirmed for a sale that can no longer be completed (cancelled meanwhile, prices
//! changed, a different amount paid) is flagged for attention, never lost: it can be completed or refunded later.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::{Ctx, CtxSnapshot};
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::inventory;
use crate::notify::{self, Note};
use crate::state::AppState;
use crate::util::{money_str, normalize_mobile};

use super::payments;
use super::sales::{self, CreateBody};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sales/stk", post(start))
        .route("/sales/stk/{id}", get(view))
        .route("/sales/stk/{id}/cancel", post(cancel))
        .route("/sales/stk/{id}/complete", post(complete_now))
}

#[derive(Deserialize)]
struct StartBody {
    sale: CreateBody,
    phone: String,
}

async fn start(State(state): State<AppState>, ctx: Ctx, Json(b): Json<StartBody>) -> AppResult<Json<Value>> {
    ctx.require("sales.create")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "sales").await?;
    let sale = b.sale;
    let branch = ctx.branch_or_current(sale.branch_id)?;
    if branch != ctx.branch_id {
        ctx.require("sales.change_branch")?;
    }
    if sale.payment.method != "mpesa" || sale.deposit.is_some() || sale.offline_at.is_some() || sale.due_date.is_some() || !sale.payments.is_empty() || sale.on_credit {
        return Err(bad("STK Push sales are paid in full by M-Pesa"));
    }
    let client_ref = sale.client_ref.ok_or_else(|| bad("Missing cart reference"))?;
    let phone = normalize_mobile(&b.phone)?;

    // Never twice: a completed sale for this cart, or a prompt already waiting, is returned as it is.
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM sales WHERE tenant_id = $1 AND client_ref = $2")
        .bind(ctx.tenant_id)
        .bind(client_ref)
        .fetch_optional(&state.db)
        .await?
    {
        return Ok(Json(json!({ "status": "completed", "sale": sales::sale_detail(&state, &ctx, id).await? })));
    }
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM pending_sales WHERE tenant_id = $1 AND client_ref = $2 AND status = 'awaiting'")
        .bind(ctx.tenant_id)
        .bind(client_ref)
        .fetch_optional(&state.db)
        .await?
    {
        return Ok(Json(view_of(&state, &ctx, id).await?));
    }

    // 1. Validate everything as if completing it now (customer, hours, approvals, prices, discounts, stock), then roll
    //    back: the amount due and the stock to hold come from that dry run.
    let (due, approved, holds, allow_negative) = {
        let mut tx = state.db.begin().await?;
        let mut dry = sale.clone();
        dry.payment.dry_run = true;
        let (sale_id, _, _, s) = sales::create_in_tx(&mut tx, &ctx, dry, None).await?;
        let due: Decimal = sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM payments WHERE sale_id = $1 AND method = 'mpesa'")
            .bind(sale_id)
            .fetch_one(&mut *tx)
            .await?;
        let approved: Option<Uuid> = sqlx::query_scalar("SELECT approved_by FROM sales WHERE id = $1").bind(sale_id).fetch_one(&mut *tx).await?;
        let holds: Vec<(Uuid, i64)> = sqlx::query_as("SELECT product_id, SUM(quantity)::bigint FROM sale_items WHERE sale_id = $1 GROUP BY product_id")
            .bind(sale_id)
            .fetch_all(&mut *tx)
            .await?;
        tx.rollback().await?;
        (due, approved, holds, s.stock.allow_negative)
    };
    if due <= Decimal::ZERO {
        return Err(rule("Nothing is due by M-Pesa — complete the sale normally"));
    }

    // 2. Hold the stock and record the waiting sale (no locks are held during the call to Safaricom).
    let holds_json: Vec<Value> = holds.iter().map(|(p, q)| json!({ "product_id": p, "quantity": q })).collect();
    let mut tx = state.db.begin().await?;
    for (product, qty) in &holds {
        inventory::reserve(&mut tx, ctx.tenant_id, branch, *product, *qty as i32, allow_negative).await?;
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pending_sales (tenant_id, branch_id, user_id, client_ref, amount, phone, body, context, holds, approved_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(ctx.user_id)
    .bind(client_ref)
    .bind(due)
    .bind(&phone)
    .bind(serde_json::to_value(&sale).map_err(anyhow::Error::from)?)
    .bind(serde_json::to_value(ctx.snapshot()).map_err(anyhow::Error::from)?)
    .bind(Value::Array(holds_json))
    .bind(approved)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.constraint() == Some("pending_sales_cart_uq") => rule("A prompt for this sale is already waiting"),
        _ => e.into(),
    })?;
    tx.commit().await?;

    // 3. Send the prompt. If Safaricom does not accept it, nothing waits: the stock is released at once.
    let reference: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    match payments::send(&state, &ctx, branch, &phone, due, &reference, "sale").await {
        Ok((req, _)) => {
            sqlx::query("UPDATE pending_sales SET mpesa_request_id = $2, updated_at = now() WHERE id = $1").bind(id).bind(req).execute(&state.db).await?;
        }
        Err(e) => {
            close(&state, id, "failed", &e.to_string()).await?;
            return Err(e);
        }
    }
    Ok(Json(view_of(&state, &ctx, id).await?))
}

/// The waiting sale as the till shows it; a request quiet for 20 s is checked with Safaricom first.
async fn view(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("sales.create")?;
    let req: Option<Option<Uuid>> = sqlx::query_scalar("SELECT mpesa_request_id FROM pending_sales WHERE id = $1 AND tenant_id = $2 AND status = 'awaiting'")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&state.db)
        .await?;
    if let Some(Some(req)) = req {
        payments::reconcile(&state, ctx.tenant_id, req).await?;
    }
    Ok(Json(view_of(&state, &ctx, id).await?))
}

async fn view_of(state: &AppState, ctx: &Ctx, id: Uuid) -> AppResult<Value> {
    let row: Option<(String, Decimal, String, Option<Uuid>, Option<String>, Option<Uuid>)> =
        sqlx::query_as("SELECT status, amount, phone, sale_id, error, mpesa_request_id FROM pending_sales WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&state.db)
            .await?;
    let (status, amount, phone, sale_id, error, req) = row.ok_or(AppError::NotFound("STK sale"))?;
    let request = match req {
        Some(r) => Some(payments::load(state, ctx.tenant_id, r).await?),
        None => None,
    };
    let sale = match (status.as_str(), sale_id) {
        ("completed", Some(s)) => Some(sales::sale_detail(state, ctx, s).await?),
        _ => None,
    };
    Ok(json!({ "id": id, "status": status, "amount": amount, "phone": phone, "error": error, "request": request, "sale": sale }))
}

/// The cashier gives up waiting: the stock is released. A payment that still arrives is flagged for attention.
async fn cancel(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("sales.create")?;
    let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pending_sales WHERE id = $1 AND tenant_id = $2 AND status = 'awaiting')")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_one(&state.db)
        .await?;
    if !ok {
        return Err(rule("This sale is no longer waiting for a payment"));
    }
    close(&state, id, "cancelled", "Cancelled at the till").await?;
    Ok(Json(view_of(&state, &ctx, id).await?))
}

/// Completes a sale flagged for attention whose payment was confirmed (after fixing what stopped it).
async fn complete_now(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("sales.create")?;
    let st: Option<String> = sqlx::query_scalar("SELECT status FROM pending_sales WHERE id = $1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).fetch_optional(&state.db).await?;
    if st.as_deref() != Some("attention") {
        return Err(rule("Only a paid sale flagged for attention can be completed here"));
    }
    if let Err(e) = complete(&state, id, true).await {
        return Err(refused("Could not complete the sale", e.to_string()));
    }
    Ok(Json(view_of(&state, &ctx, id).await?))
}

/// Ends a waiting sale without recording it and releases its stock.
async fn close(state: &AppState, id: Uuid, status: &str, why: &str) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, Uuid, Value, bool)> =
        sqlx::query_as("SELECT tenant_id, branch_id, holds, held FROM pending_sales WHERE id = $1 AND status IN ('awaiting', 'attention') FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((tenant, branch, holds, held)) = row else { return Ok(()) };
    if held {
        release_holds(&mut tx, tenant, branch, &holds).await?;
    }
    sqlx::query("UPDATE pending_sales SET status = $2, error = $3, held = false, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(why)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.emit(tenant, None, "stk_sale", json!({ "id": id, "status": status }));
    Ok(())
}

async fn release_holds(conn: &mut sqlx::PgConnection, tenant: Uuid, branch: Uuid, holds: &Value) -> AppResult<()> {
    for h in holds.as_array().into_iter().flatten() {
        if let (Some(p), Some(q)) = (h["product_id"].as_str().and_then(|s| Uuid::parse_str(s).ok()), h["quantity"].as_i64()) {
            inventory::release(conn, tenant, branch, p, q as i32).await?;
        }
    }
    Ok(())
}

/// Safaricom answered for a request: finish its waiting sale (if it has one). Never fails the caller.
pub async fn settle(state: &AppState, request: Uuid) {
    let row: Option<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT ps.id, ps.status, r.status, r.result_desc FROM pending_sales ps JOIN mpesa_requests r ON r.id = ps.mpesa_request_id WHERE r.id = $1",
    )
    .bind(request)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some((id, pending, paid, desc)) = row else { return };
    let result = match (paid.as_str(), pending.as_str()) {
        ("success", "awaiting" | "timeout") => complete(state, id, false).await,
        // Paid after the cashier gave up (or after it failed): money received, no sale — someone decides.
        ("success", "cancelled" | "failed") => flag(state, id, "Paid after the sale was cancelled at the till — complete it or refund the customer").await,
        ("failed" | "cancelled" | "timeout", "awaiting") => close(state, id, &paid, desc.as_deref().unwrap_or("Payment not completed")).await,
        _ => Ok(()),
    };
    if let Err(e) = result {
        tracing::error!(error = %e, %id, "settling an STK sale failed");
    }
}

/// Records the sale exactly once. On any problem the payment is kept and the sale flagged for attention.
async fn complete(state: &AppState, id: Uuid, manual: bool) -> AppResult<()> {
    match try_complete(state, id, manual).await {
        Ok(()) => Ok(()),
        Err(e) => {
            flag(state, id, &e.to_string()).await?;
            Err(e)
        }
    }
}

async fn try_complete(state: &AppState, id: Uuid, manual: bool) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    #[allow(clippy::type_complexity)]
    let row: Option<(String, Uuid, Uuid, Value, bool, Value, Value, Option<Uuid>, Decimal, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT status, tenant_id, branch_id, holds, held, body, context, approved_by, amount, mpesa_request_id, phone
         FROM pending_sales WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, tenant, branch, holds, held, body, context, approved, amount, req, phone)) = row else { return Ok(()) };
    let allowed = if manual { status == "attention" } else { status == "awaiting" || status == "timeout" };
    if !allowed {
        return Ok(()); // already completed (or closed) by the other path
    }
    let req = req.ok_or_else(|| rule("No M-Pesa request"))?;
    let (paid_status, paid, consumed): (String, Option<Decimal>, Option<Uuid>) =
        sqlx::query_as("SELECT status, paid_amount, consumed_by FROM mpesa_requests WHERE id = $1 FOR UPDATE").bind(req).fetch_one(&mut *tx).await?;
    if paid_status != "success" || consumed.is_some() {
        return Err(rule("The M-Pesa payment is not confirmed (or was already used)"));
    }
    let paid = paid.unwrap_or(amount);
    if paid < amount {
        return Err(rule(format!("M-Pesa paid {} but {} was due", money_str(paid), money_str(amount))));
    }
    // Claim it first (the request becomes usable by this sale only), then give back the held stock to the sale itself.
    sqlx::query("UPDATE pending_sales SET status = 'completed', sale_id = NULL, held = false, error = NULL, updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if held {
        release_holds(&mut tx, tenant, branch, &holds).await?;
    }
    let snapshot: CtxSnapshot = serde_json::from_value(context).map_err(anyhow::Error::from)?;
    let ctx = Ctx::from_snapshot(snapshot);
    let mut sale: CreateBody = serde_json::from_value(body).map_err(anyhow::Error::from)?;
    sale.payment = sales::PaymentInput { method: "mpesa".into(), reference: String::new(), mpesa_request_id: Some(req), phone, dry_run: false };
    sale.supervisor = None;
    let (sale_id, branch, product_ids, s) = sales::create_in_tx(&mut tx, &ctx, sale, approved).await?;
    crate::receipts::issue_original(&mut tx, ctx.tenant_id, sale_id, Some(ctx.user_id)).await?;
    sqlx::query("UPDATE pending_sales SET sale_id = $2, updated_at = now() WHERE id = $1").bind(id).bind(sale_id).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("sales", "stk_sale_completed", "sale", sale_id)
            .branch(branch)
            .after(json!({ "pending_sale": id, "mpesa_request": req, "amount": amount, "completed": if manual { "manually" } else { "on confirmation" } })),
    )
    .await?;
    tx.commit().await?;
    sales::after_sale(state, &ctx, &s, sale_id, branch, &product_ids).await;
    state.emit(tenant, None, "stk_sale", json!({ "id": id, "status": "completed", "sale_id": sale_id }));
    Ok(())
}

/// Payment confirmed but the sale could not be recorded: kept for a person to complete or refund.
async fn flag(state: &AppState, id: Uuid, why: &str) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, Uuid, Value, bool, Option<Uuid>, Decimal)> = sqlx::query_as(
        "SELECT tenant_id, branch_id, holds, held, user_id, amount FROM pending_sales WHERE id = $1 AND status <> 'completed' FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((tenant, branch, holds, held, cashier, amount)) = row else { return Ok(()) };
    if held {
        release_holds(&mut tx, tenant, branch, &holds).await?;
    }
    sqlx::query("UPDATE pending_sales SET status = 'attention', error = $2, held = false, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(why)
        .execute(&mut *tx)
        .await?;
    audit::system(&mut tx, tenant, cashier, Entry::new("sales", "stk_sale_attention", "pending_sale", id).branch(branch).after(json!({ "reason": why, "amount": amount })), "", "").await?;
    tx.commit().await?;
    let note = Note::new("mpesa_attention", "M-Pesa payment needs attention".to_string(), format!("KSh {} received — {why}", money_str(amount)), "/settings/integrations".to_string());
    notify::to_permission(state, tenant, Some(branch), "settings.integrations", note.clone()).await;
    if let Some(c) = cashier {
        notify::to_users(state, tenant, &[c], note).await;
    }
    state.emit(tenant, None, "stk_sale", json!({ "id": id, "status": "attention" }));
    Ok(())
}

/// Every minute: waiting sales whose prompt went quiet are checked with Safaricom (and time out after 3 minutes);
/// a waiting sale whose prompt was never sent (the server stopped in between) is closed.
pub async fn sweep(state: &AppState) -> AppResult<()> {
    let waiting: Vec<(Uuid, Uuid, Option<Uuid>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, tenant_id, mpesa_request_id, created_at FROM pending_sales WHERE status = 'awaiting' AND created_at < now() - interval '25 seconds'",
    )
    .fetch_all(&state.db)
    .await?;
    for (id, tenant, req, created) in waiting {
        match req {
            Some(r) => {
                if let Err(e) = payments::reconcile(state, tenant, r).await {
                    tracing::warn!(error = %e, %id, "stk sale check");
                }
            }
            None if chrono::Utc::now() - created > chrono::Duration::minutes(2) => close(state, id, "failed", "The prompt was not sent").await?,
            None => {}
        }
    }
    Ok(())
}
