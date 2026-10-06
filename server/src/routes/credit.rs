//! Credit sales: balances, repayments, write-offs, reminders and aging.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Counted, Outcome, Page, Paged};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
use crate::routes::approvals::ApprovalRow;
use crate::state::AppState;
use crate::util::{money_str, normalize_mobile, round2};
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/credit", get(list))
        .route("/credit/aging", get(aging))
        .route("/credit/{id}", get(detail))
        .route("/credit/{id}/payments", post(repay))
        .route("/credit/{id}/write-off", post(write_off))
        .route("/credit/{id}/remind", post(remind))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CreditRow {
    pub id: Uuid,
    pub sale_id: Uuid,
    pub receipt_no: String,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_mobile: String,
    pub branch_name: String,
    pub salesperson: Option<String>,
    pub original_amount: Decimal,
    pub amount_paid: Decimal,
    pub adjustments: Decimal,
    pub balance: Decimal,
    pub due_date: NaiveDate,
    pub days_outstanding: i32,
    /// outstanding | partially_paid | paid | overdue | written_off | cancelled
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// partially_recalled | recalled (goods brought back to stock), else null.
    pub recall_state: Option<String>,
}

pub const CREDIT_SELECT: &str = "SELECT cs.id, cs.sale_id, s.receipt_no, cs.customer_id,
        TRIM(c.first_name || ' ' || c.other_names) AS customer_name, c.mobile AS customer_mobile, b.name AS branch_name,
        u.name AS salesperson, cs.original_amount, cs.amount_paid, cs.adjustments,
        (cs.original_amount - cs.amount_paid - cs.adjustments) AS balance, cs.due_date,
        ($2::date - (cs.created_at AT TIME ZONE $3)::date) AS days_outstanding,
        CASE WHEN cs.status IN ('outstanding','partially_paid') AND cs.due_date < $2 THEN 'overdue' ELSE cs.status END AS status,
        cs.created_at, cs.recall_state
    FROM credit_sales cs JOIN sales s ON s.id = cs.sale_id JOIN customers c ON c.id = cs.customer_id
    JOIN branches b ON b.id = cs.branch_id LEFT JOIN users u ON u.id = cs.user_id";

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    status: Option<String>,
    branch_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    ctx.require("credit.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let today = ctx.today();
    let select = CREDIT_SELECT.replacen("SELECT", "SELECT COUNT(*) OVER() AS total_count,", 1);
    // status filter: open (default) | overdue | paid | written_off | all | outstanding | partially_paid
    let rows: Vec<Counted<CreditRow>> = sqlx::query_as(&format!(
        "{select} WHERE cs.tenant_id = $1 AND cs.branch_id = ANY($4)
           AND (CASE $5
                  WHEN 'all' THEN true
                  WHEN 'open' THEN cs.status IN ('outstanding','partially_paid')
                  WHEN 'overdue' THEN cs.status IN ('outstanding','partially_paid') AND cs.due_date < $2
                  ELSE cs.status = $5 END)
           AND ($6::uuid IS NULL OR cs.customer_id = $6)
           AND ($7::text IS NULL OR c.first_name ILIKE $7 OR c.mobile ILIKE $7 OR s.receipt_no ILIKE $7)
         ORDER BY cs.due_date, cs.created_at LIMIT $8 OFFSET $9"
    ))
    .bind(ctx.tenant_id)
    .bind(today)
    .bind(ctx.tz.name())
    .bind(&branches)
    .bind(q.status.as_deref().unwrap_or("open"))
    .bind(q.customer_id)
    .bind(like(&q.q))
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    let (outstanding, overdue): (Decimal, Decimal) = sqlx::query_as(
        "SELECT COALESCE(SUM(original_amount - amount_paid - adjustments),0),
                COALESCE(SUM(original_amount - amount_paid - adjustments) FILTER (WHERE due_date < $3),0)
         FROM credit_sales WHERE tenant_id = $1 AND branch_id = ANY($2) AND status IN ('outstanding','partially_paid')",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(today)
    .fetch_one(&state.db)
    .await?;
    let page: Paged<CreditRow> = rows.into();
    Ok(Json(json!({ "items": page.items, "total": page.total, "summary": { "outstanding": outstanding, "overdue": overdue } })))
}

async fn load(conn: &mut PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<CreditRow> {
    let row: CreditRow = sqlx::query_as(&format!("{CREDIT_SELECT} WHERE cs.id = $1 AND cs.tenant_id = $4"))
        .bind(id)
        .bind(ctx.today())
        .bind(ctx.tz.name())
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Credit sale"))?;
    Ok(row)
}

async fn branch_of(conn: &mut PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<Uuid> {
    let b: Uuid = sqlx::query_scalar("SELECT branch_id FROM credit_sales WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Credit sale"))?;
    ctx.ensure_branch(b)?;
    Ok(b)
}

async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("credit.view")?;
    let mut conn = state.db.acquire().await?;
    branch_of(&mut conn, &ctx, id).await?;
    let row = load(&mut conn, &ctx, id).await?;
    let payments: Vec<(Uuid, String, Decimal, String, DateTime<Utc>, Option<String>)> = sqlx::query_as(
        "SELECT p.id, p.method, p.amount, p.reference, p.created_at, u.name FROM payments p LEFT JOIN users u ON u.id = p.user_id
         WHERE p.credit_sale_id = $1 ORDER BY p.created_at",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let history: Vec<(String, DateTime<Utc>, Option<String>, String)> = sqlx::query_as(
        "SELECT a.action, a.created_at, u.name, a.comments FROM audit_log a LEFT JOIN users u ON u.id = a.user_id
         WHERE a.entity_type = 'credit_sale' AND a.entity_id = $1 ORDER BY a.created_at",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    // What was sold (for recalls) and every recall so far.
    let items: Vec<(Uuid, String, String, i32, i32, Decimal, Option<String>, bool)> = sqlx::query_as(
        "SELECT si.id, p.name, p.code, si.quantity, si.returned_qty, si.unit_price, si.barcode, si.stock_item_id IS NOT NULL
         FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = $1 ORDER BY p.name, si.barcode",
    )
    .bind(row.sale_id)
    .fetch_all(&mut *conn)
    .await?;
    let recalls: Vec<(String, String, Decimal, Option<Decimal>, Option<Decimal>, Decimal, String, DateTime<Utc>, Option<String>, Value)> = sqlx::query_as(
        "SELECT r.return_no, r.reason, r.refund_amount, r.balance_before, r.balance_after, r.customer_credit, r.refund_method, r.created_at, u.name,
                COALESCE((SELECT jsonb_agg(jsonb_build_object('product', p.name, 'quantity', ri.quantity, 'barcode', si.barcode))
                          FROM sale_return_items ri JOIN sale_items si ON si.id = ri.sale_item_id JOIN products p ON p.id = si.product_id
                          WHERE ri.return_id = r.id), '[]'::jsonb)
         FROM sale_returns r LEFT JOIN users u ON u.id = r.user_id WHERE r.sale_id = $1 AND r.kind = 'recall' ORDER BY r.created_at",
    )
    .bind(row.sale_id)
    .fetch_all(&mut *conn)
    .await?;
    let open = !matches!(row.status.as_str(), "written_off" | "cancelled" | "recalled") && items.iter().any(|i| i.3 > i.4);
    Ok(Json(json!({
        "can_recall": open && ctx.can("credit.recall"),
        "items": items.into_iter().map(|(id, name, code, qty, returned, price, barcode, tracked)| json!({
            "id": id, "product_name": name, "product_code": code, "quantity": qty, "returned_qty": returned,
            "unit_price": price, "barcode": barcode, "tracked": tracked,
        })).collect::<Vec<_>>(),
        "recalls": recalls.into_iter().map(|(no, reason, amount, before, after, credit, method, at, user, items)| json!({
            "return_no": no, "reason": reason, "amount": amount, "balance_before": before, "balance_after": after,
            "customer_credit": credit, "refund_method": method, "created_at": at, "user_name": user, "items": items,
        })).collect::<Vec<_>>(),
        "credit": row,
        "payments": payments.into_iter().map(|(id, method, amount, reference, at, user)| json!({
            "id": id, "method": method, "amount": amount, "reference": reference, "created_at": at, "user_name": user,
        })).collect::<Vec<_>>(),
        "history": history.into_iter().map(|(action, at, user, comments)| json!({
            "action": action, "created_at": at, "user_name": user, "comments": comments,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct RepayBody {
    amount: Decimal,
    method: String,
    #[serde(default)]
    reference: String,
    mpesa_request_id: Option<Uuid>,
}

async fn repay(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RepayBody>) -> AppResult<Json<Value>> {
    ctx.require("credit.collect")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "credit").await?;
    if b.amount <= Decimal::ZERO {
        return Err(bad("Enter the amount received"));
    }
    if b.method == "credit" {
        return Err(bad("Choose how the customer paid"));
    }
    let mut tx = state.db.begin().await?;
    let branch = branch_of(&mut tx, &ctx, id).await?;
    let (status, balance): (String, Decimal) = sqlx::query_as(
        "SELECT status, original_amount - amount_paid - adjustments FROM credit_sales WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if !["outstanding", "partially_paid"].contains(&status.as_str()) {
        return Err(rule(format!("This credit is {}", status.replace('_', " "))));
    }
    if b.amount > balance {
        return Err(rule(format!("The outstanding balance is only {}", money_str(balance))));
    }
    let mut reference = b.reference.trim().to_uppercase();
    if b.method == "mpesa" {
        if let Some(req) = b.mpesa_request_id {
            let (st, amount, receipt, consumed): (String, Decimal, Option<String>, Option<Uuid>) = sqlx::query_as(
                "SELECT status, amount, mpesa_receipt, consumed_by FROM mpesa_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
            )
            .bind(req)
            .bind(ctx.tenant_id)
            .fetch_one(&mut *tx)
            .await?;
            if st != "success" || consumed.is_some() || amount < b.amount {
                return Err(rule("The M-Pesa payment is not confirmed for this amount"));
            }
            sqlx::query("UPDATE mpesa_requests SET consumed_by = $2 WHERE id = $1").bind(req).bind(id).execute(&mut *tx).await?;
            reference = receipt.unwrap_or_default();
        } else if reference.len() < 8 {
            return Err(rule("Enter the M-Pesa confirmation code"));
        }
    }
    sqlx::query(
        "INSERT INTO payments (tenant_id, branch_id, credit_sale_id, method, amount, reference, mpesa_request_id, user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(id)
    .bind(&b.method)
    .bind(round2(b.amount))
    .bind(&reference)
    .bind(b.mpesa_request_id)
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    let new_status: String = sqlx::query_scalar(
        "UPDATE credit_sales SET amount_paid = amount_paid + $2,
             status = CASE WHEN original_amount - amount_paid - adjustments - $2 <= 0 THEN 'paid' ELSE 'partially_paid' END
         WHERE id = $1 AND tenant_id = $3 RETURNING status",
    )
    .bind(id)
    .bind(round2(b.amount))
    .bind(ctx.tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE sales SET amount_paid = amount_paid + $2 WHERE id = (SELECT sale_id FROM credit_sales WHERE id = $1) AND tenant_id = $3")
        .bind(id)
        .bind(round2(b.amount))
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("credit", "repayment", "credit_sale", id)
            .branch(branch)
            .before(json!({ "balance": balance }))
            .after(json!({ "paid": b.amount, "method": b.method, "reference": reference, "balance": balance - b.amount, "status": new_status })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": new_status, "balance": balance - b.amount })))
}

#[derive(Deserialize, Serialize)]
struct WriteOffBody {
    reason: String,
}

async fn write_off(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<WriteOffBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("credit.write_off")?;
    if b.reason.trim().is_empty() {
        return Err(bad("A reason is required"));
    }
    let mut tx = state.db.begin().await?;
    let branch = branch_of(&mut tx, &ctx, id).await?;
    let row = load(&mut tx, &ctx, id).await?;
    if !["outstanding", "partially_paid", "overdue"].contains(&row.status.as_str()) {
        return Err(rule("Only open credit can be written off"));
    }
    if workflow::needs_approval(&mut tx, &ctx, "credit.write_off", workflow::Gate::branch(branch).amount(row.balance)).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "credit.write_off",
                entity_type: "credit_sale",
                entity_id: id,
                branch_id: Some(branch),
                summary: format!("Write off {} owed by {} ({})", money_str(row.balance), row.customer_name, row.receipt_no),
                amount: Some(row.balance),
                payload: json!({ "reason": b.reason.trim() }),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    apply_write_off(&mut tx, &ctx, id, b.reason.trim(), None).await?;
    tx.commit().await?;
    Ok(Json(Outcome::done(json!({ "status": "written_off" }))))
}

async fn apply_write_off(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, reason: &str, approval_id: Option<Uuid>) -> AppResult<()> {
    let balance: Decimal = sqlx::query_scalar(
        "UPDATE credit_sales SET status='written_off', written_off_at=now(), written_off_by=$2
         WHERE id=$1 AND status IN ('outstanding','partially_paid') AND tenant_id = $3 RETURNING original_amount - amount_paid - adjustments",
    )
    .bind(id)
    .bind(ctx.user_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| rule("This credit is no longer open"))?;
    audit::record(
        conn,
        ctx,
        Entry::new("credit", "write_off", "credit_sale", id).after(json!({ "written_off": balance })).approval(approval_id).comments(reason),
    )
    .await
}

pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let reason = a.payload["reason"].as_str().unwrap_or("Approved write-off").to_string();
    apply_write_off(conn, ctx, a.entity_id, &reason, Some(a.id)).await
}

async fn remind(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("credit.view")?;
    let mut conn = state.db.acquire().await?;
    branch_of(&mut conn, &ctx, id).await?;
    let row = load(&mut conn, &ctx, id).await?;
    let business: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&mut *conn).await?;
    let text = format!(
        "Hi {}, this is a friendly reminder from {business}: a balance of KSh {} on receipt {} was due on {}. Thank you! 🙏",
        row.customer_name.split(' ').next().unwrap_or_default(),
        money_str(row.balance),
        row.receipt_no,
        row.due_date.format("%d/%m/%Y")
    );
    let phone = normalize_mobile(&row.customer_mobile)?;
    let sent = if crate::integrations::whatsapp::is_configured(&state) {
        crate::integrations::whatsapp::send_notification(&state, Some(ctx.tenant_id), &phone, &text).await.unwrap_or(false)
    } else {
        false
    };
    Ok(Json(json!({ "sent": sent, "link": crate::integrations::whatsapp::deep_link(&phone, &text), "text": text })))
}

/// Aging buckets of open credit (by days past due).
async fn aging(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    ctx.require("credit.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let rows: Vec<(String, i64, Decimal)> = sqlx::query_as(
        "SELECT bucket, COUNT(*), COALESCE(SUM(balance),0) FROM (
            SELECT original_amount - amount_paid - adjustments AS balance,
                   CASE WHEN due_date >= $3 THEN 'current'
                        WHEN $3 - due_date <= 30 THEN '1-30'
                        WHEN $3 - due_date <= 60 THEN '31-60'
                        WHEN $3 - due_date <= 90 THEN '61-90'
                        ELSE '90+' END AS bucket
            FROM credit_sales WHERE tenant_id = $1 AND branch_id = ANY($2) AND status IN ('outstanding','partially_paid')) t
         GROUP BY bucket",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(ctx.today())
    .fetch_all(&state.db)
    .await?;
    let order = ["current", "1-30", "31-60", "61-90", "90+"];
    let buckets: Vec<Value> = order
        .iter()
        .map(|b| {
            let (count, amount) = rows.iter().find(|r| r.0 == *b).map(|r| (r.1, r.2)).unwrap_or((0, Decimal::ZERO));
            json!({ "bucket": b, "count": count, "amount": amount })
        })
        .collect();
    Ok(Json(json!({ "buckets": buckets })))
}
