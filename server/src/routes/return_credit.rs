//! Roadmap 179: settling credit sales with returned items, and swapping an item sold on credit.
//!
//! **Return credit** (`return_credits`) is money a customer actually paid for goods that came back, held instead of paid
//! out: from a return on a paid sale (refund method `return_credit`) or a recall that left the customer overpaid. It is
//! applied to a credit sale as a repayment (method `return_credit`), carried into a replacement sale, or paid out — never
//! more than its amount, and every use is locked so two people cannot spend it twice. A return on an unpaid credit sale
//! only reduces what is owed (unchanged engine), so it never creates credit.
//!
//! **Swap**: an item sold on credit comes back (recall) and the customer takes another product as a new linked sale; any
//! money already paid beyond the revised amount owed becomes a return credit that pays the replacement first. The rest is
//! paid now or put on credit. Approvals: a settlement with a new return follows *Sale return / refund*; a swap follows
//! *Credit sale recall*. Nothing changes until final approval.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::sales::{self, ExchangeCredit, LineInput, PaymentInput, ReturnBody, ReturnLine, SaleInput};
use super::{like, Outcome};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::routes::approvals::ApprovalRow;
use crate::settings;
use crate::state::AppState;
use crate::util::{money_str, round2};
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/credit/{id}/return-sources", get(sources))
        .route("/credit/{id}/return-credit/preview", post(preview))
        .route("/credit/{id}/return-credit", post(settle))
        .route("/credit/{id}/swap", post(swap))
        .route("/return-credits/{id}/refund", post(refund_remaining))
}

struct CreditRow {
    sale_id: Uuid,
    branch_id: Uuid,
    status: String,
    balance: Decimal,
    receipt_no: String,
}

async fn credit_row(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, lock: bool) -> AppResult<CreditRow> {
    let lock = if lock { " FOR UPDATE OF cs" } else { "" };
    let row: Option<(Uuid, Uuid, String, Decimal, String)> = sqlx::query_as(&format!(
        "SELECT cs.sale_id, cs.branch_id, cs.status, cs.original_amount - cs.amount_paid - cs.adjustments, s.receipt_no
         FROM credit_sales cs JOIN sales s ON s.id = cs.sale_id WHERE cs.id = $1 AND cs.tenant_id = $2{lock}"
    ))
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (sale_id, branch_id, status, balance, receipt_no) = row.ok_or(AppError::NotFound("Credit sale"))?;
    ctx.ensure_branch(branch_id)?;
    Ok(CreditRow { sale_id, branch_id, status, balance, receipt_no })
}

fn open(c: &CreditRow) -> AppResult<()> {
    if !matches!(c.status.as_str(), "outstanding" | "partially_paid") || c.balance <= Decimal::ZERO {
        return Err(rule(format!("This credit is {}", c.status.replace('_', " "))));
    }
    Ok(())
}

// ───────────────────────────── What can settle it ─────────────────────────────

#[derive(Deserialize)]
struct SourcesQuery {
    q: Option<String>,
}

/// Return credits still available, and (when searching) sales whose items could come back to settle this credit — by
/// receipt, customer name or mobile, product name or code, or barcode.
async fn sources(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Query(q): Query<SourcesQuery>) -> AppResult<Json<Value>> {
    ctx.require("credit.collect")?;
    let mut conn = state.db.acquire().await?;
    let c = credit_row(&mut conn, &ctx, id, false).await?;
    let term = q.q.as_deref().map(str::trim).filter(|t| t.chars().count() >= 2).map(str::to_string);
    let pattern = like(&term);
    let customer: Option<Uuid> = sqlx::query_scalar("SELECT customer_id FROM credit_sales WHERE id = $1").bind(id).fetch_one(&mut *conn).await?;
    #[allow(clippy::type_complexity)]
    let credits: Vec<(Uuid, String, String, Option<String>, Decimal, Decimal, DateTime<Utc>, Option<String>, String)> = sqlx::query_as(
        "SELECT rc.id, r.return_no, s.receipt_no, NULLIF(trim(c.first_name || ' ' || c.other_names), ''), rc.amount, rc.used, rc.created_at,
                (SELECT string_agg(COALESCE(si.product_name, p.name), ', ') FROM sale_return_items ri JOIN sale_items si ON si.id = ri.sale_item_id
                   JOIN products p ON p.id = si.product_id WHERE ri.return_id = r.id),
                b.name
         FROM return_credits rc JOIN sale_returns r ON r.id = rc.return_id JOIN sales s ON s.id = rc.source_sale_id
         JOIN branches b ON b.id = rc.branch_id LEFT JOIN customers c ON c.id = rc.customer_id
         WHERE rc.tenant_id = $1 AND rc.used < rc.amount AND rc.branch_id = ANY($2)
           AND ($3::text IS NULL OR s.receipt_no ILIKE $3 OR r.return_no ILIKE $3 OR c.first_name ILIKE $3 OR c.mobile ILIKE $3
                OR EXISTS (SELECT 1 FROM sale_return_items ri JOIN sale_items si ON si.id = ri.sale_item_id JOIN products p ON p.id = si.product_id
                           WHERE ri.return_id = r.id AND (COALESCE(si.product_name, p.name) ILIKE $3 OR p.code ILIKE $3)))
         ORDER BY (rc.customer_id IS NOT DISTINCT FROM $4) DESC, rc.created_at DESC LIMIT 20",
    )
    .bind(ctx.tenant_id)
    .bind(&ctx.branch_ids)
    .bind(&pattern)
    .bind(customer)
    .fetch_all(&mut *conn)
    .await?;
    let mut sales_out = Vec::new();
    if let Some(term) = &term {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(Uuid, String, DateTime<Utc>, Option<String>, String, String, Decimal, Decimal, Decimal)> = sqlx::query_as(
            "SELECT s.id, s.receipt_no, s.created_at, NULLIF(trim(c.first_name || ' ' || c.other_names), ''), b.name, s.payment_method, s.total,
                    s.amount_paid, COALESCE(cs.original_amount - cs.amount_paid - cs.adjustments, 0)
             FROM sales s JOIN branches b ON b.id = s.branch_id LEFT JOIN customers c ON c.id = s.customer_id
             LEFT JOIN LATERAL (SELECT MIN(x.id::text)::uuid AS id, SUM(x.original_amount) AS original_amount, SUM(x.amount_paid) AS amount_paid, SUM(x.adjustments) AS adjustments, CASE WHEN bool_and(x.status = 'paid') THEN 'paid' WHEN bool_and(x.status IN ('paid', 'cancelled', 'recalled', 'written_off')) THEN MIN(x.status) FILTER (WHERE x.status <> 'paid') WHEN bool_or(x.status = 'partially_paid') THEN 'partially_paid' ELSE 'outstanding' END AS status FROM credit_sales x WHERE x.sale_id = s.id HAVING COUNT(*) > 0) cs ON true
             WHERE s.tenant_id = $1 AND s.branch_id = ANY($2) AND s.status IN ('completed', 'partially_returned') AND s.id <> $3
               AND EXISTS (SELECT 1 FROM sale_items si WHERE si.sale_id = s.id AND si.quantity > si.returned_qty)
               AND (s.receipt_no ILIKE $4 OR c.first_name ILIKE $4 OR c.mobile ILIKE $4
                    OR EXISTS (SELECT 1 FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = s.id
                               AND (COALESCE(si.product_name, p.name) ILIKE $4 OR p.code ILIKE $4 OR si.barcode = $5)))
             ORDER BY s.created_at DESC LIMIT 15",
        )
        .bind(ctx.tenant_id)
        .bind(&ctx.branch_ids)
        .bind(c.sale_id)
        .bind(&pattern)
        .bind(term)
        .fetch_all(&mut *conn)
        .await?;
        for (sid, receipt, at, cust, branch, method, total, paid, owed) in rows {
            let lines: Vec<(Uuid, String, String, Option<String>, i32, Decimal, bool)> = sqlx::query_as(
                "SELECT si.id, COALESCE(si.product_name, p.name), COALESCE(si.product_code, p.code),
                        CASE WHEN si.stock_item_id IS NOT NULL THEN mask_item(si.barcode) ELSE si.barcode END,
                        si.quantity - si.returned_qty, si.unit_price, si.stock_item_id IS NOT NULL
                 FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = $1 AND si.quantity > si.returned_qty
                 ORDER BY 2",
            )
            .bind(sid)
            .fetch_all(&mut *conn)
            .await?;
            sales_out.push(json!({
                "sale_id": sid, "receipt_no": receipt, "created_at": at, "customer_name": cust, "branch_name": branch, "payment_method": method,
                "total": total, "amount_paid": paid, "outstanding": owed,
                "lines": lines.into_iter().map(|(lid, name, code, barcode, qty, price, tracked)| json!({
                    "sale_item_id": lid, "product_name": name, "product_code": code, "barcode": barcode, "returnable": qty, "unit_price": price, "tracked": tracked,
                })).collect::<Vec<_>>(),
            }));
        }
    }
    Ok(Json(json!({
        "balance": c.balance,
        "credits": credits.into_iter().map(|(rid, ret, receipt, cust, amount, used, at, products, branch)| json!({
            "id": rid, "return_no": ret, "receipt_no": receipt, "customer_name": cust, "amount": amount, "used": used,
            "remaining": amount - used, "created_at": at, "products": products, "branch_name": branch,
        })).collect::<Vec<_>>(),
        "sales": sales_out,
    })))
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Source {
    sale_id: Uuid,
    items: Vec<ReturnLine>,
    #[serde(default)]
    reason: String,
}

/// What returning these items would give: their value, how much of it is money actually paid (the credit), how much only
/// cancels what is still owed on that sale, and the most that can go to this credit sale. Computed by the return engine
/// itself inside a transaction that is rolled back — exactly what would happen, with nothing changed.
async fn preview(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(src): Json<Source>) -> AppResult<Json<Value>> {
    ctx.require("credit.collect")?;
    ctx.require("sales.return")?;
    let mut tx = state.db.begin().await?;
    let c = credit_row(&mut tx, &ctx, id, false).await?;
    if src.sale_id == c.sale_id {
        return Err(refused("Same sale", "Items from this credit sale come back through Recall or Swap, not as a payment for it."));
    }
    if src.items.is_empty() {
        return Err(bad("Choose the items coming back"));
    }
    let body = ReturnBody { credit_id: None, items: src.items.clone(), reason: "preview".into(), refund_method: "return_credit".into(), restock: true, settle: String::new() };
    let r = sales::execute_return(&mut tx, &ctx, src.sale_id, &body, "return", None).await?;
    tx.rollback().await?;
    let value = dec(&r["refund_amount"]);
    let credit = dec(&r["return_credit"]);
    Ok(Json(json!({
        "return_value": value, "paid_value": credit, "reduces_owed": (value - credit).max(Decimal::ZERO),
        "balance": c.balance, "max_apply": credit.min(c.balance),
    })))
}

fn dec(v: &Value) -> Decimal {
    match v {
        Value::String(s) => s.parse().unwrap_or_default(),
        Value::Number(n) => n.to_string().parse().unwrap_or_default(),
        _ => Decimal::ZERO,
    }
}

// ───────────────────────────── Settling with a returned item ─────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
struct SettleBody {
    /// An existing return credit with value left…
    return_credit_id: Option<Uuid>,
    /// …or items of a paid sale coming back now.
    source: Option<Source>,
    /// How much to apply (default: as much as possible).
    amount: Option<Decimal>,
}

async fn settle(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SettleBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("credit.collect")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "credit").await?;
    if b.amount.is_some_and(|a| a <= Decimal::ZERO) {
        return Err(bad("Enter the amount to apply"));
    }
    let mut tx = state.db.begin().await?;
    let c = credit_row(&mut tx, &ctx, id, true).await?;
    open(&c)?;
    let result = match (&b.return_credit_id, &b.source) {
        (Some(rc), None) => allocate(&mut tx, &ctx, id, *rc, b.amount).await?,
        (None, Some(src)) => {
            ctx.require("sales.return")?;
            if src.reason.trim().chars().count() < 3 {
                return Err(bad("A reason for the return is required"));
            }
            if src.items.is_empty() {
                return Err(bad("Choose the items coming back"));
            }
            if src.sale_id == c.sale_id {
                return Err(refused("Same sale", "Items from this credit sale come back through Recall or Swap, not as a payment for it."));
            }
            let head = sales::sale_head(&mut tx, &ctx, src.sale_id).await?;
            if matches!(head.status.as_str(), "cancelled" | "returned") {
                return Err(rule("That sale has already been fully reversed"));
            }
            let value = sales::return_amount(&mut tx, &head, src.sale_id, &src.items).await?;
            // The return follows the business's return rules: nothing changes until it is approved.
            if workflow::needs_approval(&mut tx, &ctx, "sale.return", workflow::Gate::branch(head.branch_id).amount(value)).await? {
                let approval = workflow::submit(
                    &mut tx,
                    &ctx,
                    workflow::Request {
                        action: "sale.return",
                        entity_type: "sale",
                        entity_id: src.sale_id,
                        branch_id: Some(head.branch_id),
                        summary: format!("Return on {} to settle credit {} — {}", head.receipt_no, c.receipt_no, src.reason.trim()),
                        amount: Some(value),
                        payload: json!({ "settle_credit": { "credit_id": id, "body": &b } }),
                    },
                )
                .await?;
                tx.commit().await?;
                super::approvals::notify_approvers(&state, &ctx, approval).await;
                return Ok(Json(Outcome::pending(approval)));
            }
            settle_with_return(&mut tx, &ctx, id, src, b.amount, None).await?
        }
        _ => return Err(bad("Choose a return credit or the items coming back")),
    };
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "sale", json!({ "credit_id": id }));
    Ok(Json(Outcome::done(result)))
}

/// The return (stock back, its value kept as a return credit — only what was paid), then the credit applied.
async fn settle_with_return(conn: &mut PgConnection, ctx: &Ctx, credit_id: Uuid, src: &Source, amount: Option<Decimal>, approval_id: Option<Uuid>) -> AppResult<Value> {
    let body = ReturnBody { credit_id: None, items: src.items.clone(), reason: src.reason.trim().to_string(), refund_method: "return_credit".into(), restock: true, settle: String::new() };
    let r = sales::execute_return(conn, ctx, src.sale_id, &body, "return", approval_id).await?;
    let Some(rc) = r["return_credit_id"].as_str().and_then(|v| v.parse::<Uuid>().ok()) else {
        return Err(refused(
            "Nothing was paid for this item",
            "Returning it only cancels what is still owed on its own sale, so it gives no credit for another sale. An item sold on credit comes back through Recall on its own credit sale.",
        ));
    };
    let mut out = allocate(conn, ctx, credit_id, rc, amount).await?;
    out["return_no"] = r["return_no"].clone();
    Ok(out)
}

/// Applies a return credit to a credit sale as a repayment (method `return_credit`), within what is left of the credit
/// and what is still owed. Both rows are locked: the same credit is never applied twice.
async fn allocate(conn: &mut PgConnection, ctx: &Ctx, credit_id: Uuid, rc: Uuid, amount: Option<Decimal>) -> AppResult<Value> {
    let row: Option<(Decimal, Decimal, Uuid, String, String)> = sqlx::query_as(
        "SELECT rc.amount, rc.used, rc.source_sale_id, r.return_no, s.receipt_no FROM return_credits rc
         JOIN sale_returns r ON r.id = rc.return_id JOIN sales s ON s.id = rc.source_sale_id
         WHERE rc.id = $1 AND rc.tenant_id = $2 FOR UPDATE OF rc",
    )
    .bind(rc)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (total, used, source_sale, return_no, source_receipt) = row.ok_or(AppError::NotFound("Return credit"))?;
    let c = credit_row(conn, ctx, credit_id, true).await?;
    open(&c)?;
    if source_sale == c.sale_id {
        return Err(refused("Same sale", "A credit from this sale cannot pay for this sale."));
    }
    let left = total - used;
    if left <= Decimal::ZERO {
        return Err(refused("Already used", format!("The credit from {return_no} has been used in full.")));
    }
    let max = left.min(c.balance);
    let apply = round2(amount.unwrap_or(max));
    if apply <= Decimal::ZERO || apply > max {
        return Err(rule(format!(
            "At most {} can be applied ({} left on {return_no}, {} owed)",
            money_str(max),
            money_str(left),
            money_str(c.balance)
        )));
    }
    sqlx::query(
        "INSERT INTO payments (tenant_id, branch_id, credit_sale_id, method, amount, reference, return_credit_id, user_id)
         VALUES ($1,$2,$3,'return_credit',$4,$5,$6,$7)",
    )
    .bind(ctx.tenant_id)
    .bind(c.branch_id)
    .bind(credit_id)
    .bind(apply)
    .bind(&return_no)
    .bind(rc)
    .bind(ctx.user_id)
    .execute(&mut *conn)
    .await?;
    let status: String = sqlx::query_scalar(
        "UPDATE credit_sales SET amount_paid = amount_paid + $2,
             status = CASE WHEN original_amount - amount_paid - adjustments - $2 <= 0 THEN 'paid' ELSE 'partially_paid' END
         WHERE id = $1 AND tenant_id = $3 RETURNING status",
    )
    .bind(credit_id)
    .bind(apply)
    .bind(ctx.tenant_id)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE sales SET amount_paid = amount_paid + $2 WHERE id = $1 AND tenant_id = $3").bind(c.sale_id).bind(apply).bind(ctx.tenant_id).execute(&mut *conn).await?;
    sqlx::query("UPDATE return_credits SET used = used + $2 WHERE id = $1").bind(rc).bind(apply).execute(&mut *conn).await?;
    let balance = c.balance - apply;
    audit::record(
        conn,
        ctx,
        Entry::new("credit", "repayment", "credit_sale", credit_id)
            .branch(c.branch_id)
            .before(json!({ "balance": c.balance }))
            .after(json!({ "paid": apply, "method": "return_credit", "reference": return_no, "return_credit_id": rc,
                           "from_sale": source_sale, "from_receipt": source_receipt, "balance": balance, "status": status })),
    )
    .await?;
    Ok(json!({ "applied": apply, "balance": balance, "status": status, "return_no": return_no, "return_credit_id": rc, "credit_left": left - apply }))
}

/// An approved return that settles a credit sale.
pub async fn on_settle_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let credit_id: Uuid = a.payload["settle_credit"]["credit_id"].as_str().and_then(|s| s.parse().ok()).ok_or_else(|| bad("Stored settlement is invalid"))?;
    let b: SettleBody = serde_json::from_value(a.payload["settle_credit"]["body"].clone()).map_err(|_| bad("Stored settlement is invalid"))?;
    let src = b.source.ok_or_else(|| bad("Stored settlement is invalid"))?;
    settle_with_return(conn, ctx, credit_id, &src, b.amount, Some(a.id)).await?;
    Ok(())
}

// ───────────────────────────── Paying out what is left ─────────────────────────────

#[derive(Deserialize)]
struct RefundBody {
    method: String,
    #[serde(default)]
    reference: String,
}

/// Pays out what is left of a return credit (it goes back the way the customer chooses, recorded on the original sale).
async fn refund_remaining(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RefundBody>) -> AppResult<Json<Value>> {
    ctx.require("sales.return")?;
    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let method = b.method.trim();
    if matches!(method, "credit" | "exchange" | "return_credit" | "split" | "") || !s.payment_enabled(method) {
        return Err(bad("Choose how the money is paid back"));
    }
    let row: Option<(Decimal, Decimal, Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT rc.amount, rc.used, rc.source_sale_id, rc.branch_id, r.return_no FROM return_credits rc JOIN sale_returns r ON r.id = rc.return_id
         WHERE rc.id = $1 AND rc.tenant_id = $2 FOR UPDATE OF rc",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (amount, used, sale, branch, return_no) = row.ok_or(AppError::NotFound("Return credit"))?;
    ctx.ensure_branch(branch)?;
    let left = amount - used;
    if left <= Decimal::ZERO {
        return Err(refused("Already used", format!("The credit from {return_no} has been used in full.")));
    }
    sqlx::query(
        "INSERT INTO payments (tenant_id, branch_id, sale_id, method, amount, reference, return_credit_id, user_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(sale)
    .bind(method)
    .bind(-left)
    .bind(if b.reference.trim().is_empty() { return_no.clone() } else { b.reference.trim().to_uppercase() })
    .bind(id)
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE return_credits SET used = amount WHERE id = $1").bind(id).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("sales", "return_credit_refund", "sale", sale).branch(branch).after(json!({
        "return_no": return_no, "refunded": left, "method": method,
    })))
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "refunded": left })))
}

// ───────────────────────────── Swapping an item sold on credit ─────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
struct SwapBody {
    /// Lines of this credit sale coming back (tracked units scanned again).
    return_items: Vec<ReturnLine>,
    /// What the customer takes instead.
    items: Vec<LineInput>,
    /// How the replacement is paid: on credit (a new credit sale) or now.
    payment: PaymentInput,
    due_date: Option<NaiveDate>,
    reason: String,
    client_ref: Option<Uuid>,
}

async fn swap(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SwapBody>) -> AppResult<Json<Value>> {
    ctx.require("credit.recall")?;
    ctx.require("sales.create")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "returns").await?;
    if b.reason.trim().chars().count() < 3 {
        return Err(bad("A reason is required"));
    }
    if b.return_items.is_empty() {
        return Err(bad("Choose the item coming back"));
    }
    if b.items.is_empty() {
        return Err(bad("Add the product the customer takes instead"));
    }
    if b.payment.method == "split" || b.payment.method == "exchange" {
        return Err(bad("Choose credit, or how the difference is paid"));
    }
    if let Some(cref) = b.client_ref {
        if let Some(sale) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM sales WHERE tenant_id = $1 AND client_ref = $2")
            .bind(ctx.tenant_id)
            .bind(cref)
            .fetch_optional(&state.db)
            .await?
        {
            return Ok(Json(sales::sale_detail(&state, &ctx, sale).await?));
        }
    }
    let mut tx = state.db.begin().await?;
    let c = credit_row(&mut tx, &ctx, id, true).await?;
    if matches!(c.status.as_str(), "written_off" | "cancelled" | "recalled") {
        return Err(refused("Cannot swap", format!("This credit sale is {}.", c.status.replace('_', " "))));
    }
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let units: i64 = b.items.iter().map(|l| i64::from(l.quantity.max(0))).sum();
    if b.payment.method == "credit" && s.sales.credit_single_item && units > 1 {
        return Err(refused("One item per credit sale", "A replacement on credit is one item. Take payment for the others, or swap them one at a time."));
    }
    let head = sales::sale_head(&mut tx, &ctx, c.sale_id).await?;
    check_returned_units(&mut tx, c.sale_id, &b.return_items).await?;
    let value = sales::return_amount(&mut tx, &head, c.sale_id, &b.return_items).await?;
    if workflow::needs_approval(&mut tx, &ctx, "credit.recall", workflow::Gate::branch(head.branch_id).amount(value)).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "credit.recall",
                entity_type: "sale",
                entity_id: c.sale_id,
                branch_id: Some(head.branch_id),
                summary: format!("Swap on credit sale {} — {}", head.receipt_no, b.reason.trim()),
                amount: Some(value),
                payload: json!({ "credit_swap": { "credit_id": id, "body": &b } }),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(json!({ "pending_approval": true, "approval_id": approval })));
    }
    let (sale_id, product_ids) = execute_swap(&mut tx, &ctx, &s, id, b, None).await?;
    tx.commit().await?;
    sales::after_sale(&state, &ctx, &s, sale_id, head.branch_id, &product_ids).await;
    Ok(Json(sales::sale_detail(&state, &ctx, sale_id).await?))
}

/// Tracked units coming back must be the exact units sold on this credit sale (scanned again).
async fn check_returned_units(conn: &mut PgConnection, sale_id: Uuid, lines: &[ReturnLine]) -> AppResult<()> {
    let mut seen: Vec<String> = vec![];
    for l in lines {
        let row: Option<(Option<Uuid>, Option<String>, i32, i32, String)> = sqlx::query_as(
            "SELECT si.stock_item_id, si.barcode, si.quantity, si.returned_qty, COALESCE(si.product_name, p.name) FROM sale_items si
             JOIN products p ON p.id = si.product_id WHERE si.id = $1 AND si.sale_id = $2",
        )
        .bind(l.sale_item_id)
        .bind(sale_id)
        .fetch_optional(&mut *conn)
        .await?;
        let (item, sold_code, qty, returned, name) = row.ok_or_else(|| refused("Not on this sale", "The item coming back is not part of this credit sale."))?;
        if l.quantity <= 0 || l.quantity > qty - returned {
            return Err(refused("Already returned", format!("At most {} of {name} can come back.", (qty - returned).max(0))));
        }
        if item.is_some() {
            let code = l.barcodes.first().map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
                .ok_or_else(|| refused("Scan required", format!("Scan the barcode on the {name} coming back.")))?;
            if Some(&code) != sold_code.as_ref() {
                return Err(refused("Barcode mismatch", format!("{code} is not the {name} sold on this credit sale.")));
            }
            if seen.contains(&code) {
                return Err(refused("Already scanned", format!("{code} is already in this swap.")));
            }
            seen.push(code);
        }
    }
    Ok(())
}

/// The swap: the item comes back (recall — the amount owed is reduced; money paid beyond the revised amount becomes a
/// return credit), the replacement is sold as a new sale linked to the old one, paid first by that credit, the rest now
/// or on credit. One transaction.
async fn execute_swap(conn: &mut PgConnection, ctx: &Ctx, s: &settings::TenantSettings, credit_id: Uuid, b: SwapBody, approval_id: Option<Uuid>) -> AppResult<(Uuid, Vec<Uuid>)> {
    let c = credit_row(conn, ctx, credit_id, true).await?;
    let head = sales::sale_head(conn, ctx, c.sale_id).await?;
    check_returned_units(conn, c.sale_id, &b.return_items).await?;
    // 1. The recall: stock back, the amount owed reduced; what had been paid beyond it is kept as a return credit.
    let rb = ReturnBody { credit_id: None, items: b.return_items.clone(), reason: b.reason.trim().to_string(), refund_method: String::new(), restock: true, settle: "credit".into() };
    let r = sales::execute_return(conn, ctx, c.sale_id, &rb, "recall", approval_id).await?;
    let return_no = r["return_no"].as_str().unwrap_or_default().to_string();
    let rc: Option<Uuid> = r["return_credit_id"].as_str().and_then(|v| v.parse().ok());
    let paid_credit = dec(&r["return_credit"]);
    // 2. The replacement, at the original sale's branch, for the same customer and salesperson.
    let on_credit = b.payment.method == "credit";
    let estimate: Decimal = b.items.iter().map(|l| l.unit_price * Decimal::from(l.quantity)).sum();
    let carry = if on_credit { Decimal::ZERO } else { paid_credit.min(estimate) };
    let product_ids: Vec<Uuid> = b.items.iter().map(|i| i.product_id).collect();
    let mut at_branch = ctx.clone();
    at_branch.branch_id = head.branch_id;
    let sale_id = sales::record_sale(
        conn,
        &at_branch,
        s,
        SaleInput {
            branch_id: head.branch_id,
            customer_id: head.customer_id,
            lines: b.items,
            payment: b.payment,
            redeem_points: 0,
            due_date: b.due_date,
            deposit: None,
            notes: format!("Replacement for {} ({return_no})", head.receipt_no),
            approved_by: None,
            order_id: None,
            client_ref: b.client_ref,
            exchange: (carry > Decimal::ZERO).then(|| ExchangeCredit { amount: carry, reference: return_no.clone() }),
            owner_id: head.owner_id,
            redemption_approved: false,
            split: Vec::new(),
            on_credit: false,
            sale_date: None,
        },
        false,
    )
    .await?;
    sqlx::query("UPDATE sales SET replaces_sale_id = $2 WHERE id = $1 AND tenant_id = $3").bind(sale_id).bind(c.sale_id).bind(ctx.tenant_id).execute(&mut *conn).await?;
    // 3. The paid value pays the replacement first.
    let mut applied = Decimal::ZERO;
    if let Some(rc) = rc {
        if carry > Decimal::ZERO {
            sqlx::query("UPDATE return_credits SET used = used + $2 WHERE id = $1").bind(rc).bind(carry).execute(&mut *conn).await?;
            sqlx::query("UPDATE payments SET return_credit_id = $2 WHERE sale_id = $1 AND method = 'exchange' AND tenant_id = $3")
                .bind(sale_id)
                .bind(rc)
                .bind(ctx.tenant_id)
                .execute(&mut *conn)
                .await?;
            applied = carry;
        } else if on_credit {
            let new_credit: Uuid = sqlx::query_scalar("SELECT id FROM credit_sales WHERE sale_id = $1").bind(sale_id).fetch_one(&mut *conn).await?;
            let v = allocate(conn, ctx, new_credit, rc, None).await?;
            applied = dec(&v["applied"]);
        }
    }
    let new_receipt: String = sqlx::query_scalar("SELECT receipt_no FROM sales WHERE id = $1").bind(sale_id).fetch_one(&mut *conn).await?;
    audit::record(
        conn,
        ctx,
        Entry::new("credit", "swap", "credit_sale", credit_id)
            .branch(head.branch_id)
            .after(json!({ "original_receipt": head.receipt_no, "return_no": return_no, "replacement_sale": sale_id, "replacement_receipt": new_receipt,
                           "returned_value": dec(&r["refund_amount"]), "paid_value_credited": paid_credit, "applied_to_replacement": applied,
                           "replacement_on_credit": on_credit }))
            .approval(approval_id)
            .comments(b.reason.trim()),
    )
    .await?;
    crate::receipts::issue_original(conn, ctx.tenant_id, sale_id, Some(ctx.user_id)).await?;
    Ok((sale_id, product_ids))
}

/// An approved swap.
pub async fn on_swap_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let credit_id: Uuid = a.payload["credit_swap"]["credit_id"].as_str().and_then(|s| s.parse().ok()).ok_or_else(|| bad("Stored swap is invalid"))?;
    let b: SwapBody = serde_json::from_value(a.payload["credit_swap"]["body"].clone()).map_err(|_| bad("Stored swap is invalid"))?;
    let s = settings::load(conn, ctx.tenant_id).await?;
    execute_swap(conn, ctx, &s, credit_id, b, Some(a.id)).await?;
    Ok(())
}
