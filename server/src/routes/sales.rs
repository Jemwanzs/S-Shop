//! Sales / POS: product picker, sale completion (one DB transaction), sale
//! history, receipts, returns, cancellations and receipt sharing.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Outcome, Page, Period};
use crate::audit::{self, Entry};
use crate::auth::{verify_pin, Ctx};
use crate::error::{bad, rule, AppError, AppResult};
use crate::inventory::{self, Check, Movement};
use crate::loyalty;
use crate::notify;
use crate::routes::approvals::ApprovalRow;
use crate::settings::{self, QuantityEntry, TenantSettings};
use crate::state::AppState;
use crate::util::{money_str, next_doc_no, round2};
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pos/products", get(pos_products))
        .route("/sales", get(list).post(create))
        .route("/sales/{id}", get(detail))
        .route("/sales/{id}/return", post(return_items))
        .route("/sales/{id}/cancel", post(cancel))
        .route("/sales/{id}/share", post(share))
}

// ───────────────────────────── POS product picker ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct PosProduct {
    id: Uuid,
    code: String,
    name: String,
    nickname: String,
    category_name: Option<String>,
    barcode: Option<String>,
    track_items: bool,
    marked_price: Decimal,
    max_discount: Option<Decimal>,
    loyalty_eligible: bool,
    loyalty_threshold: Option<Decimal>,
    loyalty_points_per: Option<i32>,
    on_hand: i32,
    reserved: i32,
    available: i32,
    primary_photo_id: Option<Uuid>,
    photo_count: i64,
}

#[derive(Deserialize)]
struct PosQuery {
    q: Option<String>,
    branch_id: Option<Uuid>,
    category_id: Option<Uuid>,
    /// include products that are out of stock (shown greyed out)
    all: Option<bool>,
}

async fn pos_products(State(state): State<AppState>, ctx: Ctx, Query(q): Query<PosQuery>) -> AppResult<Json<Vec<PosProduct>>> {
    ctx.require("sales.create")?;
    let branch = ctx.branch_or_current(q.branch_id)?;
    let rows = sqlx::query_as(
        "SELECT p.id, p.code, p.name, p.nickname, c.name AS category_name, p.barcode, p.track_items, p.marked_price, p.max_discount,
                p.loyalty_eligible, p.loyalty_threshold, p.loyalty_points_per,
                COALESCE(sl.on_hand,0) AS on_hand, COALESCE(sl.reserved,0) AS reserved,
                COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) AS available,
                (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1) AS primary_photo_id,
                (SELECT COUNT(*) FROM product_photos ph WHERE ph.product_id = p.id) AS photo_count
         FROM products p
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $2
         WHERE p.tenant_id = $1 AND p.is_active
           AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = $2))
           AND ($3::text IS NULL OR p.name ILIKE $3 OR p.nickname ILIKE $3 OR p.code ILIKE $3 OR p.barcode ILIKE $3)
           AND ($4::uuid IS NULL OR p.category_id = $4)
           AND ($5 OR COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) > 0)
         ORDER BY (COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) > 0) DESC, p.name
         LIMIT 200",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(like(&q.q))
    .bind(q.category_id)
    .bind(q.all.unwrap_or(false))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

// ───────────────────────────── Sale completion ─────────────────────────────

#[derive(Deserialize, Clone)]
pub struct LineInput {
    pub product_id: Uuid,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub barcode: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
pub struct PaymentInput {
    pub method: String,
    #[serde(default)]
    pub reference: String,
    pub mpesa_request_id: Option<Uuid>,
    #[serde(default)]
    pub phone: String,
}

/// Part-payment taken at the moment of a credit sale; the rest stays on credit.
#[derive(Deserialize, Clone)]
pub struct DepositInput {
    pub amount: Decimal,
    pub method: String,
    #[serde(default)]
    pub reference: String,
    pub mpesa_request_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct CustomerInput {
    pub mobile: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub nickname: String,
}

#[derive(Deserialize)]
pub struct Supervisor {
    pub email: String,
    pub pin: String,
}

#[derive(Deserialize)]
struct CreateBody {
    branch_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    customer: Option<CustomerInput>,
    items: Vec<LineInput>,
    payment: PaymentInput,
    #[serde(default)]
    redeem_points: i64,
    due_date: Option<NaiveDate>,
    deposit: Option<DepositInput>,
    #[serde(default)]
    notes: String,
    supervisor: Option<Supervisor>,
    client_ref: Option<Uuid>,
}

/// Everything needed to record a sale; shared by the counter and order completion.
pub struct SaleInput {
    pub branch_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub lines: Vec<LineInput>,
    pub payment: PaymentInput,
    pub redeem_points: i64,
    pub due_date: Option<NaiveDate>,
    pub deposit: Option<DepositInput>,
    pub notes: String,
    pub approved_by: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub client_ref: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct SaleProduct {
    name: String,
    is_active: bool,
    track_items: bool,
    barcode: Option<String>,
    marked_price: Decimal,
    max_discount: Option<Decimal>,
    cost_price: Option<Decimal>,
    loyalty_eligible: bool,
    loyalty_threshold: Option<Decimal>,
    loyalty_points_per: Option<i32>,
}

struct PreparedLine {
    product_id: Uuid,
    stock_item_id: Option<Uuid>,
    barcode: Option<String>,
    quantity: i32,
    marked_price: Decimal,
    unit_price: Decimal,
    line_total: Decimal,
    unit_cost: Option<Decimal>,
    points: i64,
}

async fn verify_supervisor(conn: &mut PgConnection, ctx: &Ctx, sup: &Supervisor) -> AppResult<Uuid> {
    let row: Option<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, pin_hash, is_active FROM users WHERE tenant_id = $1 AND lower(email) = lower($2)",
    )
    .bind(ctx.tenant_id)
    .bind(sup.email.trim())
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some((id, hash, true)) if verify_pin(&sup.pin, &hash) => {
            if id == ctx.user_id {
                return Err(rule("A different person must approve this discount"));
            }
            Ok(id)
        }
        _ => Err(rule("Supervisor email or PIN is incorrect")),
    }
}

/// Validates every line, locks stock and computes prices/points. No writes except row locks.
async fn prepare_lines(
    conn: &mut PgConnection,
    ctx: &Ctx,
    s: &TenantSettings,
    input: &SaleInput,
    trusted_prices: bool,
) -> AppResult<(Vec<PreparedLine>, bool)> {
    if input.lines.is_empty() {
        return Err(bad("Add at least one item"));
    }
    let mut lines = Vec::with_capacity(input.lines.len());
    let mut excessive_discount = false;
    let mut seen_items: Vec<Uuid> = Vec::new();

    for l in &input.lines {
        let p: SaleProduct = sqlx::query_as(
            "SELECT name, is_active, track_items, barcode, marked_price, max_discount, cost_price, loyalty_eligible,
                    loyalty_threshold, loyalty_points_per FROM products WHERE id = $1 AND tenant_id = $2",
        )
        .bind(l.product_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
        if !p.is_active {
            return Err(rule(format!("{} is inactive and cannot be sold", p.name)));
        }
        inventory::ensure_product_in_branch(conn, ctx.tenant_id, l.product_id, input.branch_id).await?;
        if l.quantity <= 0 {
            return Err(bad(format!("Enter a quantity for {}", p.name)));
        }
        if !trusted_prices && s.sales.quantity_entry == QuantityEntry::Locked && l.quantity != 1 {
            return Err(rule("Quantity is locked to 1 — add each item separately"));
        }
        if l.unit_price < Decimal::ZERO {
            return Err(bad("Selling price cannot be negative"));
        }

        // Single pricing model: the selling price is the truth; discount = marked − selling.
        let unit_discount = p.marked_price - l.unit_price;
        if !trusted_prices && unit_discount > Decimal::ZERO {
            if !ctx.can("sales.discount") {
                return Err(AppError::Forbidden(format!("You cannot sell {} below its marked price", p.name)));
            }
            if p.max_discount.is_some_and(|max| unit_discount > max) {
                excessive_discount = true;
            }
        }

        let barcode = l.barcode.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(String::from);
        let mut stock_item_id = None;
        let mut unit_cost = p.cost_price;
        if p.track_items {
            if l.quantity != 1 {
                return Err(rule(format!("{} is tracked per item — add one line per scanned barcode", p.name)));
            }
            let code = barcode.clone().ok_or_else(|| rule(format!("Scan the barcode of the {} being sold", p.name)))?;
            let item: Option<(Uuid, Option<Decimal>)> = sqlx::query_as(
                "SELECT id, cost_price FROM stock_items WHERE tenant_id = $1 AND product_id = $2 AND branch_id = $3
                   AND barcode = $4 AND status = 'in_stock' FOR UPDATE",
            )
            .bind(ctx.tenant_id)
            .bind(l.product_id)
            .bind(input.branch_id)
            .bind(&code)
            .fetch_optional(&mut *conn)
            .await?;
            let (id, cost) = item.ok_or_else(|| rule(format!("{code} is not an available {} at this branch (already sold or elsewhere)", p.name)))?;
            if seen_items.contains(&id) {
                return Err(rule(format!("{code} is already in the cart")));
            }
            seen_items.push(id);
            stock_item_id = Some(id);
            unit_cost = cost.or(p.cost_price);
        } else if s.sales.require_barcode_clearance && !trusted_prices {
            if let Some(expected) = &p.barcode {
                match &barcode {
                    Some(code) if code == expected => {}
                    Some(_) => return Err(rule(format!("Scanned barcode does not match {}", p.name))),
                    None => return Err(rule(format!("Scan {} to clear it", p.name))),
                }
            }
        }

        let line_total = round2(l.unit_price * Decimal::from(l.quantity));
        let points = loyalty::line_points(s, p.loyalty_eligible, p.loyalty_threshold, p.loyalty_points_per, line_total);
        lines.push(PreparedLine {
            product_id: l.product_id,
            stock_item_id,
            barcode,
            quantity: l.quantity,
            marked_price: p.marked_price,
            unit_price: l.unit_price,
            line_total,
            unit_cost,
            points,
        });
    }
    Ok((lines, excessive_discount))
}

/// Validates an M-Pesa payment of `amount`: a confirmed, unused STK request (then marked used by `consumer`)
/// or, when allowed, a manually entered confirmation code. Returns the reference and the request id.
async fn confirm_mpesa(
    conn: &mut PgConnection,
    ctx: &Ctx,
    s: &TenantSettings,
    request: Option<Uuid>,
    reference: String,
    amount: Decimal,
    consumer: Uuid,
) -> AppResult<(String, Option<Uuid>)> {
    let Some(req_id) = request else {
        if !s.sales.mpesa_manual_confirmation {
            return Err(rule("Use Push STK to collect M-Pesa payments"));
        }
        if reference.len() < 8 || !reference.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(rule("Enter the M-Pesa confirmation code (e.g. QFT1ABC2DE)"));
        }
        return Ok((reference, None));
    };
    let (status, paid, receipt, consumed): (String, Decimal, Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT status, amount, mpesa_receipt, consumed_by FROM mpesa_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(req_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("M-Pesa request"))?;
    if status != "success" {
        return Err(rule("The M-Pesa payment has not been confirmed yet"));
    }
    if consumed.is_some() {
        return Err(rule("This M-Pesa payment was already used"));
    }
    if paid < amount {
        return Err(rule(format!("M-Pesa paid {} but {} is due", money_str(paid), money_str(amount))));
    }
    sqlx::query("UPDATE mpesa_requests SET consumed_by = $2, updated_at = now() WHERE id = $1")
        .bind(req_id)
        .bind(consumer)
        .execute(&mut *conn)
        .await?;
    Ok((receipt.unwrap_or_default(), Some(req_id)))
}

/// Records a complete sale inside the caller's transaction. Returns the sale id.
pub async fn record_sale(conn: &mut PgConnection, ctx: &Ctx, s: &TenantSettings, input: SaleInput, trusted_prices: bool) -> AppResult<Uuid> {
    let (lines, excessive) = prepare_lines(conn, ctx, s, &input, trusted_prices).await?;
    if excessive && input.approved_by.is_none() && !ctx.can("sales.discount_override") {
        return Err(rule("A discount exceeds the allowed maximum — supervisor approval required"));
    }
    let method = input.payment.method.as_str();
    if !s.payment_enabled(method) {
        return Err(rule("This payment method is not enabled"));
    }
    if method == "credit" && (!s.sales.credit_enabled || input.customer_id.is_none()) {
        return Err(rule("Credit sales need a customer"));
    }

    let gross: Decimal = lines.iter().map(|l| l.marked_price * Decimal::from(l.quantity)).sum();
    let net: Decimal = lines.iter().map(|l| l.line_total).sum();
    let sale_id = Uuid::new_v4();

    // Points redemption reduces the amount payable (ledger written once the sale row exists).
    let mut redeemed_value = Decimal::ZERO;
    if input.redeem_points > 0 {
        ctx.require("customers.redeem_points")?;
        if input.customer_id.is_none() {
            return Err(rule("Select the customer redeeming points"));
        }
        redeemed_value = round2(Decimal::from(input.redeem_points) * s.loyalty.point_value);
        if redeemed_value > net {
            return Err(rule("Redeemed points are worth more than the sale"));
        }
    }
    let total = round2(net - redeemed_value);

    // Loyalty points (awarded after the sale rows exist).
    let mut points: i64 = 0;
    if input.customer_id.is_some() && s.loyalty.enabled && net >= s.loyalty.min_spend {
        let raw: i64 = lines.iter().map(|l| l.points).sum();
        points = if net > Decimal::ZERO && redeemed_value > Decimal::ZERO {
            use rust_decimal::prelude::ToPrimitive;
            (Decimal::from(raw) * total / net).floor().to_i64().unwrap_or(0)
        } else {
            raw
        };
    }

    // Payment validation. A credit sale may take a deposit now; the rest stays on credit.
    let mut reference = input.payment.reference.trim().to_uppercase();
    let mut mpesa_request = None;
    let mut deposit = None;
    let amount_paid = match method {
        "credit" => match &input.deposit {
            Some(d) if d.amount > Decimal::ZERO => {
                let amount = round2(d.amount);
                if amount >= total {
                    return Err(rule("A deposit must be less than the total — take a normal payment for the full amount"));
                }
                if d.method == "credit" || !s.payment_enabled(&d.method) {
                    return Err(rule("Choose how the deposit was paid"));
                }
                let mut dref = d.reference.trim().to_uppercase();
                let mut dreq = None;
                if d.method == "mpesa" {
                    (dref, dreq) = confirm_mpesa(conn, ctx, s, d.mpesa_request_id, dref, amount, sale_id).await?;
                }
                deposit = Some((d.method.clone(), amount, dref, dreq));
                amount
            }
            _ => Decimal::ZERO,
        },
        "mpesa" => {
            (reference, mpesa_request) = confirm_mpesa(conn, ctx, s, input.payment.mpesa_request_id, reference, total, sale_id).await?;
            total
        }
        _ => total,
    };

    let receipt_no = next_doc_no(conn, ctx.tenant_id, "RCP", ctx.tz).await?;
    sqlx::query(
        "INSERT INTO sales (id, tenant_id, branch_id, receipt_no, customer_id, user_id, order_id, gross_total, discount_total, total,
                            redeemed_points, redeemed_value, amount_paid, payment_method, points_earned, approved_by, notes, client_ref)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
    )
    .bind(sale_id)
    .bind(ctx.tenant_id)
    .bind(input.branch_id)
    .bind(&receipt_no)
    .bind(input.customer_id)
    .bind(ctx.user_id)
    .bind(input.order_id)
    .bind(round2(gross))
    .bind(round2(gross - net))
    .bind(total)
    .bind(input.redeem_points.max(0))
    .bind(redeemed_value)
    .bind(amount_paid)
    .bind(method)
    .bind(points)
    .bind(input.approved_by)
    .bind(input.notes.trim())
    .bind(input.client_ref)
    .execute(&mut *conn)
    .await?;

    if input.redeem_points > 0 {
        let customer = input.customer_id.expect("checked above");
        loyalty::redeem(conn, s, ctx.tenant_id, customer, input.redeem_points, Some(sale_id), Some(ctx.user_id), &receipt_no).await?;
    }

    let kind = if input.order_id.is_some() { "order_completion" } else { "sale" };
    let raw_points: i64 = lines.iter().map(|l| l.points).sum();
    for l in &lines {
        // Persist the points actually awarded per line so returns reverse exactly.
        let line_points = if raw_points > 0 { l.points * points / raw_points } else { 0 };
        sqlx::query(
            "INSERT INTO sale_items (tenant_id, sale_id, product_id, stock_item_id, barcode, quantity, marked_price, unit_price,
                                     line_total, unit_cost, points)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(ctx.tenant_id)
        .bind(sale_id)
        .bind(l.product_id)
        .bind(l.stock_item_id)
        .bind(&l.barcode)
        .bind(l.quantity)
        .bind(l.marked_price)
        .bind(l.unit_price)
        .bind(l.line_total)
        .bind(l.unit_cost)
        .bind(line_points)
        .execute(&mut *conn)
        .await?;
        if let Some(item) = l.stock_item_id {
            sqlx::query("UPDATE stock_items SET status = 'sold', updated_at = now() WHERE id = $1")
                .bind(item)
                .execute(&mut *conn)
                .await?;
        }
        let m = Movement::new(input.branch_id, l.product_id, kind, -l.quantity)
            .item(l.stock_item_id)
            .cost(l.unit_cost)
            .price(Some(l.unit_price))
            .reference("sale", sale_id)
            .notes(&receipt_no);
        inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), s.stock.allow_negative, Check::Available, m).await?;
    }

    if method == "credit" {
        let customer = input.customer_id.expect("checked above");
        let due = input.due_date.unwrap_or_else(|| ctx.today() + Duration::days(s.sales.credit_default_days));
        let credit_id: Uuid = sqlx::query_scalar(
            "INSERT INTO credit_sales (tenant_id, branch_id, sale_id, customer_id, user_id, original_amount, due_date, amount_paid, status)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8, CASE WHEN $8 > 0 THEN 'partially_paid' ELSE 'outstanding' END) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(input.branch_id)
        .bind(sale_id)
        .bind(customer)
        .bind(ctx.user_id)
        .bind(total)
        .bind(due)
        .bind(amount_paid)
        .fetch_one(&mut *conn)
        .await?;
        if let Some((dmethod, amount, dref, dreq)) = &deposit {
            // The deposit belongs to both the sale (receipt) and the credit (payment history).
            sqlx::query(
                "INSERT INTO payments (tenant_id, branch_id, sale_id, credit_sale_id, method, amount, reference, phone, mpesa_request_id, user_id)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(ctx.tenant_id)
            .bind(input.branch_id)
            .bind(sale_id)
            .bind(credit_id)
            .bind(dmethod)
            .bind(amount)
            .bind(dref)
            .bind(input.payment.phone.trim())
            .bind(dreq)
            .bind(ctx.user_id)
            .execute(&mut *conn)
            .await?;
        }
    } else if total > Decimal::ZERO {
        sqlx::query(
            "INSERT INTO payments (tenant_id, branch_id, sale_id, method, amount, reference, phone, mpesa_request_id, user_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(ctx.tenant_id)
        .bind(input.branch_id)
        .bind(sale_id)
        .bind(method)
        .bind(total)
        .bind(&reference)
        .bind(input.payment.phone.trim())
        .bind(mpesa_request)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    }

    if let Some(customer) = input.customer_id {
        loyalty::record_purchase(conn, s, customer, total, 1).await?;
        loyalty::award_sale(conn, s, ctx.tenant_id, customer, sale_id, points, Some(ctx.user_id), ctx.today()).await?;
    }

    if let Some(approver) = input.approved_by {
        // Counter-side supervisor approval is recorded in the approvals register for the audit trail.
        sqlx::query(
            "INSERT INTO approvals (tenant_id, action, entity_type, entity_id, branch_id, summary, amount, status, requested_by, decided_by, decided_at)
             VALUES ($1,'sale.discount','sale',$2,$3,$4,$5,'approved',$6,$7,now())",
        )
        .bind(ctx.tenant_id)
        .bind(sale_id)
        .bind(input.branch_id)
        .bind(format!("Discount override on {receipt_no}"))
        .bind(round2(gross - net))
        .bind(ctx.user_id)
        .bind(approver)
        .execute(&mut *conn)
        .await?;
    }

    audit::record(
        conn,
        ctx,
        Entry::new("sales", "create", "sale", sale_id).branch(input.branch_id).after(json!({
            "receipt_no": receipt_no, "total": total, "discount": round2(gross - net), "method": method,
            "items": lines.len(), "points": points, "order_id": input.order_id,
        })),
    )
    .await?;
    Ok(sale_id)
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CreateBody>) -> AppResult<Json<Value>> {
    ctx.require("sales.create")?;
    let branch = ctx.branch_or_current(b.branch_id)?;
    if branch != ctx.branch_id {
        ctx.require("sales.change_branch")?;
    }

    // A retried submit returns the original sale instead of selling twice.
    if let Some(cref) = b.client_ref {
        if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM sales WHERE tenant_id = $1 AND client_ref = $2")
            .bind(ctx.tenant_id)
            .bind(cref)
            .fetch_optional(&state.db)
            .await?
        {
            return Ok(Json(sale_detail(&state, &ctx, id).await?));
        }
    }

    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    if s.workspace.outside_hours == settings::OutsideHours::Block && !ctx.can("sales.outside_hours") {
        let hours = settings::branch_hours(&mut tx, ctx.tenant_id, branch, &s).await?;
        if !hours.is_open(Utc::now().with_timezone(&ctx.tz).naive_local()) {
            return Err(rule(format!("The branch is closed (trading hours {}–{}). Sales outside trading hours need permission.", hours.open, hours.close)));
        }
    }

    let customer_id = match (b.customer_id, &b.customer) {
        (Some(id), _) => {
            let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM customers WHERE id = $1 AND tenant_id = $2)")
                .bind(id)
                .bind(ctx.tenant_id)
                .fetch_one(&mut *tx)
                .await?;
            if !ok {
                return Err(AppError::NotFound("Customer"));
            }
            Some(id)
        }
        (None, Some(c)) if !c.mobile.trim().is_empty() => {
            Some(super::customers::upsert_by_mobile(&mut tx, ctx.tenant_id, Some(ctx.user_id), &c.mobile, &c.first_name, &c.nickname).await?.0)
        }
        _ => None,
    };

    let mut approved_by = None;
    if let Some(sup) = &b.supervisor {
        let approver = verify_supervisor(&mut tx, &ctx, sup).await?;
        let allowed = if workflow::needs_approval(&mut tx, &ctx, "sale.discount", workflow::Gate::branch(branch)).await? {
            workflow::can_decide(&mut tx, ctx.tenant_id, "sale.discount", Some(branch), Some(ctx.user_id), workflow::Decider { approver, level: 1, decided_by: &[] }).await?
        } else {
            let perms: Vec<String> = sqlx::query_scalar("SELECT r.permissions FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1")
                .bind(approver)
                .fetch_one(&mut *tx)
                .await?;
            perms.iter().any(|p| p == "*" || p == "sales.discount_override")
        };
        if !allowed {
            return Err(rule("That supervisor cannot approve discounts"));
        }
        approved_by = Some(approver);
    } else if workflow::needs_approval(&mut tx, &ctx, "sale.discount", workflow::Gate::branch(branch)).await? {
        // With the workflow on, even override holders need a second person for excessive discounts.
        let input = SaleInput {
            branch_id: branch,
            customer_id,
            lines: b.items.clone(),
            payment: b.payment.clone(),
            redeem_points: 0,
            due_date: None,
            deposit: None,
            notes: String::new(),
            approved_by: None,
            order_id: None,
            client_ref: None,
        };
        let (_, excessive) = prepare_lines(&mut tx, &ctx, &s, &input, false).await?;
        if excessive {
            return Err(rule("A discount exceeds the allowed maximum — supervisor approval required"));
        }
    }

    let product_ids: Vec<Uuid> = b.items.iter().map(|i| i.product_id).collect();
    let sale_id = record_sale(
        &mut tx,
        &ctx,
        &s,
        SaleInput {
            branch_id: branch,
            customer_id,
            lines: b.items,
            payment: b.payment,
            redeem_points: b.redeem_points,
            due_date: b.due_date,
            deposit: b.deposit,
            notes: b.notes,
            approved_by,
            order_id: None,
            client_ref: b.client_ref,
        },
        false,
    )
    .await?;
    tx.commit().await?;

    after_sale(&state, &ctx, &s, sale_id, branch, &product_ids).await;
    Ok(Json(sale_detail(&state, &ctx, sale_id).await?))
}

/// Post-commit side effects: live updates, stock alerts, WhatsApp receipt.
pub async fn after_sale(state: &AppState, ctx: &Ctx, s: &TenantSettings, sale_id: Uuid, branch: Uuid, product_ids: &[Uuid]) {
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": branch }));
    state.emit(ctx.tenant_id, None, "sale", json!({ "id": sale_id, "branch_id": branch }));
    super::stock::alert_levels(state, ctx.tenant_id, branch, product_ids).await;
    if s.notifications.whatsapp_receipts {
        if let Ok((Some(phone), text)) = receipt_text(state, sale_id).await {
            notify::whatsapp(state, ctx.tenant_id, phone, text);
        }
    }
}

// ───────────────────────────── History & receipts ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct SaleRow {
    id: Uuid,
    receipt_no: String,
    created_at: DateTime<Utc>,
    /// Trading day the sale belongs to (differs from the calendar date after midnight on late hours).
    business_date: NaiveDate,
    branch_name: String,
    customer_id: Option<Uuid>,
    customer_name: Option<String>,
    customer_mobile: Option<String>,
    user_name: Option<String>,
    status: String,
    total: Decimal,
    discount_total: Decimal,
    payment_method: String,
    points_earned: i64,
    item_count: i64,
    order_no: Option<String>,
    is_legacy: bool,
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    branch_id: Option<Uuid>,
    user_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    status: Option<String>,
    payment_method: Option<String>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

#[derive(sqlx::FromRow)]
struct SaleListRow {
    #[sqlx(flatten)]
    row: SaleRow,
    total_count: i64,
    sum_total: Decimal,
    sum_discount: Decimal,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    ctx.require("sales.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.today(), "today");
    let rows: Vec<SaleListRow> = sqlx::query_as(
        "SELECT s.id, s.receipt_no, s.created_at, s.business_date, b.name AS branch_name, s.customer_id,
                NULLIF(TRIM(c.first_name || ' ' || c.other_names), '') AS customer_name, c.mobile AS customer_mobile,
                u.name AS user_name, s.status, s.total, s.discount_total, s.payment_method, s.points_earned,
                (SELECT COALESCE(SUM(quantity),0) FROM sale_items si WHERE si.sale_id = s.id)::bigint AS item_count,
                o.order_no, s.is_legacy,
                COUNT(*) OVER() AS total_count,
                COALESCE(SUM(s.total) FILTER (WHERE s.status <> 'cancelled') OVER(), 0) AS sum_total,
                COALESCE(SUM(s.discount_total) FILTER (WHERE s.status <> 'cancelled') OVER(), 0) AS sum_discount
         FROM sales s JOIN branches b ON b.id = s.branch_id
         LEFT JOIN customers c ON c.id = s.customer_id LEFT JOIN users u ON u.id = s.user_id LEFT JOIN orders o ON o.id = s.order_id
         WHERE s.tenant_id = $1 AND s.branch_id = ANY($2) AND s.business_date BETWEEN $3 AND $4
           AND ($5::uuid IS NULL OR s.user_id = $5) AND ($6::uuid IS NULL OR s.customer_id = $6)
           AND ($7::text IS NULL OR s.status = $7) AND ($8::text IS NULL OR s.payment_method = $8)
           AND ($9::text IS NULL OR s.receipt_no ILIKE $9 OR c.first_name ILIKE $9 OR c.mobile ILIKE $9)
         ORDER BY s.created_at DESC LIMIT $10 OFFSET $11",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    // Without "view other employees" a user only ever sees their own sales.
    .bind(if ctx.sees_others() { q.user_id } else { Some(ctx.user_id) })
    .bind(q.customer_id)
    .bind(&q.status)
    .bind(&q.payment_method)
    .bind(like(&q.q))
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    let (total, sum_total, sum_discount) = rows.first().map(|r| (r.total_count, r.sum_total, r.sum_discount)).unwrap_or_default();
    Ok(Json(json!({
        "from": from, "to": to,
        "items": rows.into_iter().map(|r| r.row).collect::<Vec<_>>(),
        "total": total,
        "summary": { "count": total, "total": sum_total, "discount": sum_discount },
    })))
}

pub async fn sale_detail(state: &AppState, ctx: &Ctx, id: Uuid) -> AppResult<Value> {
    let sale: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'id', s.id, 'receipt_no', s.receipt_no, 'created_at', s.created_at, 'status', s.status,
            'branch_id', s.branch_id, 'branch_name', b.name, 'branch_location', b.location, 'branch_phone', b.phone,
            'user_name', u.name, 'gross_total', s.gross_total, 'discount_total', s.discount_total, 'total', s.total,
            'redeemed_points', s.redeemed_points, 'redeemed_value', s.redeemed_value, 'amount_paid', s.amount_paid,
            'payment_method', s.payment_method, 'points_earned', s.points_earned, 'notes', s.notes, 'is_legacy', s.is_legacy,
            'approved_by_name', au.name, 'cancel_reason', s.cancel_reason, 'cancelled_at', s.cancelled_at,
            'order_id', s.order_id, 'order_no', o.order_no,
            'customer', CASE WHEN c.id IS NULL THEN NULL ELSE jsonb_build_object(
                'id', c.id, 'name', TRIM(c.first_name || ' ' || c.other_names), 'nickname', c.nickname, 'mobile', c.mobile,
                'points_available', c.points_available) END)
         FROM sales s JOIN branches b ON b.id = s.branch_id
         LEFT JOIN users u ON u.id = s.user_id LEFT JOIN users au ON au.id = s.approved_by
         LEFT JOIN customers c ON c.id = s.customer_id LEFT JOIN orders o ON o.id = s.order_id
         WHERE s.id = $1 AND s.tenant_id = $2 AND s.branch_id = ANY($3)",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(&ctx.branch_ids)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Sale"))?;

    let show_cost = ctx.can("sales.view_financials");
    let items: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', si.id, 'product_id', si.product_id, 'product_name', p.name, 'product_code', p.code,
                'barcode', si.barcode, 'quantity', si.quantity, 'returned_qty', si.returned_qty, 'marked_price', si.marked_price,
                'unit_price', si.unit_price, 'discount', (si.marked_price - si.unit_price) * si.quantity,
                'line_total', si.line_total, 'points', si.points, 'unit_cost', CASE WHEN $2 THEN si.unit_cost END)
         FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = $1 ORDER BY p.name",
    )
    .bind(id)
    .bind(show_cost)
    .fetch_all(&state.db)
    .await?;
    let payments: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', p.id, 'method', p.method, 'amount', p.amount, 'reference', p.reference,
                'created_at', p.created_at, 'user_name', u.name)
         FROM payments p LEFT JOIN users u ON u.id = p.user_id WHERE p.sale_id = $1 ORDER BY p.created_at",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let returns: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', r.id, 'return_no', r.return_no, 'kind', r.kind, 'reason', r.reason,
                'refund_amount', r.refund_amount, 'refund_method', r.refund_method, 'restock', r.restock,
                'points_reversed', r.points_reversed, 'created_at', r.created_at, 'user_name', u.name)
         FROM sale_returns r LEFT JOIN users u ON u.id = r.user_id WHERE r.sale_id = $1 ORDER BY r.created_at",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let credit: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', id, 'original_amount', original_amount, 'amount_paid', amount_paid,
                'adjustments', adjustments, 'balance', original_amount - amount_paid - adjustments,
                'due_date', due_date, 'status', status)
         FROM credit_sales WHERE sale_id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let business: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('name', name, 'phone', phone, 'address', address, 'currency', currency,
                'logo_url', CASE WHEN logo IS NOT NULL THEN '/api/public/' || slug || '/logo' END,
                'receipt_footer', COALESCE(settings->'sales'->>'receipt_footer', ''))
         FROM tenants WHERE id = $1",
    )
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let pending: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approvals WHERE entity_type = 'sale' AND entity_id = $1 AND status = 'pending' LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    Ok(json!({
        "sale": sale, "items": items, "payments": payments, "returns": returns, "credit": credit,
        "business": business, "pending_approval_id": pending,
    }))
}

/// Own sales are always visible to their seller; other employees' sales need "view other employees".
async fn ensure_sale_visible(state: &AppState, ctx: &Ctx, id: Uuid) -> AppResult<()> {
    if ctx.sees_others() {
        return Ok(());
    }
    let seller: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM sales WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .flatten();
    if seller != Some(ctx.user_id) {
        return Err(AppError::Forbidden("This sale was recorded by another employee".into()));
    }
    Ok(())
}

async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require_any(&["sales.view", "sales.create"])?;
    ensure_sale_visible(&state, &ctx, id).await?;
    Ok(Json(sale_detail(&state, &ctx, id).await?))
}

/// Plain-text receipt for WhatsApp; returns (customer phone, text).
async fn receipt_text(state: &AppState, sale_id: Uuid) -> AppResult<(Option<String>, String)> {
    let (business, receipt_no, at, total, paid, method, points, phone, name, tz): (
        String, String, DateTime<Utc>, Decimal, Decimal, String, i64, Option<String>, Option<String>, String,
    ) = sqlx::query_as(
        "SELECT t.name, s.receipt_no, s.created_at, s.total, s.amount_paid, s.payment_method, s.points_earned, c.mobile, c.first_name, t.timezone
         FROM sales s JOIN tenants t ON t.id = s.tenant_id LEFT JOIN customers c ON c.id = s.customer_id WHERE s.id = $1",
    )
    .bind(sale_id)
    .fetch_one(&state.db)
    .await?;
    let items: Vec<(String, i32, Decimal)> = sqlx::query_as(
        "SELECT p.name, si.quantity, si.line_total FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = $1",
    )
    .bind(sale_id)
    .fetch_all(&state.db)
    .await?;
    let local = at.with_timezone(&crate::util::parse_tz(&tz));
    let mut text = format!("🧾 *{business}*\nReceipt {receipt_no}\n{}\n\n", local.format("%d/%m/%Y %H:%M"));
    if let Some(n) = &name {
        text = format!("Hi {n}! 👋\n\n{text}");
    }
    for (pname, qty, line) in items {
        text.push_str(&format!("• {pname} × {qty} — {}\n", money_str(line)));
    }
    text.push_str(&format!("\n*Total: {}*\n", money_str(total)));
    if method == "credit" {
        if paid > Decimal::ZERO {
            text.push_str(&format!("Deposit paid: {}\n", money_str(paid)));
        }
        text.push_str(&format!("On credit — balance {}\n", money_str(total - paid)));
    } else {
        text.push_str(&format!("Paid via {}\n", method.to_uppercase()));
    }
    if points > 0 {
        text.push_str(&format!("🌼 +{points} loyalty points\n"));
    }
    text.push_str("\nThank you for shopping with us!");
    Ok((phone, text))
}

async fn share(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("sales.print")?;
    ensure_sale_visible(&state, &ctx, id).await?;
    sale_detail(&state, &ctx, id).await?; // access check
    let (phone, text) = receipt_text(&state, id).await?;
    let mut sent = false;
    if let Some(p) = &phone {
        if crate::integrations::whatsapp::is_configured(&state) {
            sent = crate::integrations::whatsapp::send_notification(&state, Some(ctx.tenant_id), p, &text).await.unwrap_or(false);
        }
    }
    Ok(Json(json!({
        "sent": sent,
        "text": text,
        "link": phone.map(|p| crate::integrations::whatsapp::deep_link(&p, &text)),
    })))
}

// ───────────────────────────── Returns & cancellations ─────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
pub struct ReturnLine {
    pub sale_item_id: Uuid,
    pub quantity: i32,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ReturnBody {
    pub items: Vec<ReturnLine>,
    pub reason: String,
    #[serde(default)]
    pub refund_method: String,
    #[serde(default = "yes")]
    pub restock: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Serialize, Clone)]
pub struct CancelBody {
    pub reason: String,
    #[serde(default)]
    pub refund_method: String,
}

#[derive(sqlx::FromRow)]
struct SaleHead {
    branch_id: Uuid,
    receipt_no: String,
    status: String,
    customer_id: Option<Uuid>,
    total: Decimal,
    redeemed_value: Decimal,
    payment_method: String,
    order_id: Option<Uuid>,
}

async fn sale_head(conn: &mut PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<SaleHead> {
    let head: SaleHead = sqlx::query_as(
        "SELECT branch_id, receipt_no, status, customer_id, total, redeemed_value, payment_method, order_id
         FROM sales WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("Sale"))?;
    ctx.ensure_branch(head.branch_id)?;
    Ok(head)
}

async fn return_items(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<ReturnBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("sales.return")?;
    if b.reason.trim().is_empty() {
        return Err(bad("A reason is required"));
    }
    if b.items.is_empty() {
        return Err(bad("Choose the items being returned"));
    }
    let mut tx = state.db.begin().await?;
    let head = sale_head(&mut tx, &ctx, id).await?;
    if matches!(head.status.as_str(), "cancelled" | "returned") {
        return Err(rule("This sale has already been fully reversed"));
    }
    let amount = return_amount(&mut tx, &head, id, &b.items).await?;
    if workflow::needs_approval(&mut tx, &ctx, "sale.return", workflow::Gate::branch(head.branch_id).amount(amount)).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "sale.return",
                entity_type: "sale",
                entity_id: id,
                branch_id: Some(head.branch_id),
                summary: format!("Return on {} — {}", head.receipt_no, b.reason.trim()),
                amount: Some(amount),
                payload: serde_json::to_value(&b).unwrap_or_default(),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    let r = execute_return(&mut tx, &ctx, id, &b, "return", None).await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": head.branch_id }));
    Ok(Json(Outcome::done(r)))
}

/// Refund value of returned lines, net of any points redemption on the sale.
async fn return_amount(conn: &mut PgConnection, head: &SaleHead, sale_id: Uuid, lines: &[ReturnLine]) -> AppResult<Decimal> {
    let mut gross = Decimal::ZERO;
    for l in lines {
        let (qty, returned, unit_price): (i32, i32, Decimal) =
            sqlx::query_as("SELECT quantity, returned_qty, unit_price FROM sale_items WHERE id = $1 AND sale_id = $2")
                .bind(l.sale_item_id)
                .bind(sale_id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(AppError::NotFound("Sale item"))?;
        if l.quantity <= 0 || l.quantity > qty - returned {
            return Err(rule(format!("You can return at most {} of that item", qty - returned)));
        }
        gross += unit_price * Decimal::from(l.quantity);
    }
    let net = head.total + head.redeemed_value;
    Ok(if net > Decimal::ZERO { round2(gross * head.total / net) } else { Decimal::ZERO })
}

async fn execute_return(conn: &mut PgConnection, ctx: &Ctx, sale_id: Uuid, b: &ReturnBody, kind: &str, approval_id: Option<Uuid>) -> AppResult<Value> {
    let head = sale_head(conn, ctx, sale_id).await?;
    if matches!(head.status.as_str(), "cancelled" | "returned") {
        return Err(rule("This sale has already been fully reversed"));
    }
    let s = settings::load(conn, ctx.tenant_id).await?;
    let refund = return_amount(conn, &head, sale_id, &b.items).await?;
    let return_no = next_doc_no(conn, ctx.tenant_id, "RTN", ctx.tz).await?;
    let return_id: Uuid = sqlx::query_scalar(
        "INSERT INTO sale_returns (tenant_id, sale_id, return_no, kind, reason, refund_amount, refund_method, restock, user_id, approved_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(sale_id)
    .bind(&return_no)
    .bind(kind)
    .bind(b.reason.trim())
    .bind(refund)
    .bind(&b.refund_method)
    .bind(b.restock)
    .bind(ctx.user_id)
    .bind(approval_id.map(|_| ctx.user_id))
    .fetch_one(&mut *conn)
    .await?;

    let mut points_to_reverse = 0;
    for l in &b.items {
        let (product_id, item_id, qty, unit_price, unit_cost, points): (Uuid, Option<Uuid>, i32, Decimal, Option<Decimal>, i64) = sqlx::query_as(
            "UPDATE sale_items SET returned_qty = returned_qty + $3 WHERE id = $1 AND sale_id = $2
             RETURNING product_id, stock_item_id, quantity, unit_price, unit_cost, points",
        )
        .bind(l.sale_item_id)
        .bind(sale_id)
        .bind(l.quantity)
        .fetch_one(&mut *conn)
        .await?;
        points_to_reverse += points * i64::from(l.quantity) / i64::from(qty.max(1));
        sqlx::query("INSERT INTO sale_return_items (return_id, sale_item_id, quantity, amount) VALUES ($1,$2,$3,$4)")
            .bind(return_id)
            .bind(l.sale_item_id)
            .bind(l.quantity)
            .bind(round2(unit_price * Decimal::from(l.quantity)))
            .execute(&mut *conn)
            .await?;
        if b.restock {
            if let Some(si) = item_id {
                sqlx::query("UPDATE stock_items SET status='in_stock', branch_id=$2, updated_at=now() WHERE id=$1")
                    .bind(si)
                    .bind(head.branch_id)
                    .execute(&mut *conn)
                    .await?;
            }
            let mk = if kind == "cancellation" { "sale_reversal" } else { "customer_return" };
            let m = Movement::new(head.branch_id, product_id, mk, l.quantity)
                .item(item_id)
                .cost(unit_cost)
                .price(Some(unit_price))
                .reference("sale_return", return_id)
                .notes(&return_no);
            inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), true, Check::None, m).await?;
        }
    }

    // Money: reduce credit first, refund the rest.
    let mut refunded = refund;
    if head.payment_method == "credit" {
        let (cs_id, balance): (Uuid, Decimal) = sqlx::query_as(
            "SELECT id, original_amount - amount_paid - adjustments FROM credit_sales WHERE sale_id = $1 FOR UPDATE",
        )
        .bind(sale_id)
        .fetch_one(&mut *conn)
        .await?;
        let reduce = refund.min(balance.max(Decimal::ZERO));
        sqlx::query(
            "UPDATE credit_sales SET adjustments = adjustments + $2,
                 status = CASE WHEN original_amount - amount_paid - adjustments - $2 <= 0
                               THEN (CASE WHEN $3 THEN 'cancelled' ELSE 'paid' END) ELSE status END
             WHERE id = $1",
        )
        .bind(cs_id)
        .bind(reduce)
        .bind(kind == "cancellation")
        .execute(&mut *conn)
        .await?;
        refunded = refund - reduce;
    }
    if refunded > Decimal::ZERO {
        let method = if b.refund_method.is_empty() { head.payment_method.replace("credit", "cash") } else { b.refund_method.clone() };
        sqlx::query(
            "INSERT INTO payments (tenant_id, branch_id, sale_id, method, amount, reference, user_id) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(ctx.tenant_id)
        .bind(head.branch_id)
        .bind(sale_id)
        .bind(&method)
        .bind(-refunded)
        .bind(&return_no)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    }

    let reversed = loyalty::reverse_sale(conn, ctx.tenant_id, sale_id, points_to_reverse, Some(ctx.user_id), &return_no).await?;
    sqlx::query("UPDATE sale_returns SET points_reversed = $2 WHERE id = $1").bind(return_id).bind(reversed).execute(&mut *conn).await?;
    if let Some(customer) = head.customer_id {
        let fully = kind == "cancellation";
        loyalty::record_purchase(conn, &s, customer, -refund, if fully { -1 } else { 0 }).await?;
    }

    let remaining: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(quantity - returned_qty),0)::bigint FROM sale_items WHERE sale_id = $1")
        .bind(sale_id)
        .fetch_one(&mut *conn)
        .await?;
    let status = match (kind, remaining) {
        ("cancellation", _) => "cancelled",
        (_, 0) => "returned",
        _ => "partially_returned",
    };
    sqlx::query(
        "UPDATE sales SET status = $2,
             cancelled_at = CASE WHEN $2 = 'cancelled' THEN now() ELSE cancelled_at END,
             cancelled_by = CASE WHEN $2 = 'cancelled' THEN $3 ELSE cancelled_by END,
             cancel_reason = CASE WHEN $2 = 'cancelled' THEN $4 ELSE cancel_reason END
         WHERE id = $1",
    )
    .bind(sale_id)
    .bind(status)
    .bind(ctx.user_id)
    .bind(b.reason.trim())
    .execute(&mut *conn)
    .await?;
    if let (Some(order), "cancelled") = (head.order_id, status) {
        sqlx::query("UPDATE orders SET status = 'returned', updated_at = now() WHERE id = $1").bind(order).execute(&mut *conn).await?;
        sqlx::query("INSERT INTO order_events (order_id, status, user_id, notes) VALUES ($1, 'returned', $2, $3)")
            .bind(order)
            .bind(ctx.user_id)
            .bind(format!("Sale {} cancelled", head.receipt_no))
            .execute(&mut *conn)
            .await?;
    }

    audit::record(
        conn,
        ctx,
        Entry::new("sales", kind, "sale", sale_id)
            .branch(head.branch_id)
            .after(json!({ "return_no": return_no, "refund": refund, "restock": b.restock, "points_reversed": reversed, "status": status }))
            .approval(approval_id)
            .comments(b.reason.trim()),
    )
    .await?;
    Ok(json!({ "return_id": return_id, "return_no": return_no, "refund_amount": refund, "refunded": refunded,
               "points_reversed": reversed, "status": status }))
}

async fn all_lines(conn: &mut PgConnection, sale_id: Uuid) -> AppResult<Vec<ReturnLine>> {
    let rows: Vec<(Uuid, i32)> = sqlx::query_as("SELECT id, quantity - returned_qty FROM sale_items WHERE sale_id = $1 AND quantity > returned_qty")
        .bind(sale_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(|(sale_item_id, quantity)| ReturnLine { sale_item_id, quantity }).collect())
}

async fn cancel(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<CancelBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("sales.cancel")?;
    if b.reason.trim().is_empty() {
        return Err(bad("A reason is required"));
    }
    let mut tx = state.db.begin().await?;
    let head = sale_head(&mut tx, &ctx, id).await?;
    if head.status != "completed" {
        return Err(rule("Only sales without returns can be cancelled — use a return instead"));
    }
    if workflow::needs_approval(&mut tx, &ctx, "sale.cancel", workflow::Gate::branch(head.branch_id).amount(head.total)).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "sale.cancel",
                entity_type: "sale",
                entity_id: id,
                branch_id: Some(head.branch_id),
                summary: format!("Cancel sale {} — {}", head.receipt_no, b.reason.trim()),
                amount: Some(head.total),
                payload: serde_json::to_value(&b).unwrap_or_default(),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    let lines = all_lines(&mut tx, id).await?;
    let body = ReturnBody { items: lines, reason: b.reason.clone(), refund_method: b.refund_method.clone(), restock: true };
    let r = execute_return(&mut tx, &ctx, id, &body, "cancellation", None).await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": head.branch_id }));
    Ok(Json(Outcome::done(r)))
}

pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    match a.action.as_str() {
        "sale.return" => {
            let b: ReturnBody = serde_json::from_value(a.payload.clone()).map_err(|_| bad("Stored return is invalid"))?;
            execute_return(conn, ctx, a.entity_id, &b, "return", Some(a.id)).await?;
        }
        "sale.cancel" => {
            let c: CancelBody = serde_json::from_value(a.payload.clone()).map_err(|_| bad("Stored cancellation is invalid"))?;
            let lines = all_lines(conn, a.entity_id).await?;
            let body = ReturnBody { items: lines, reason: c.reason, refund_method: c.refund_method, restock: true };
            execute_return(conn, ctx, a.entity_id, &body, "cancellation", Some(a.id)).await?;
        }
        _ => {}
    }
    Ok(())
}
