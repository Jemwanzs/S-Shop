//! Roadmap 191: Edit Sale — corrections to a recorded sale, always approved before they take effect.
//!
//! Sales → (sale) → Corrections → **Edit Sale** loads the sale as it stands; the person (*Edit recorded sales*) corrects
//! any of its business details — date and time, customer, sale owner, branch, products, quantities, barcodes (tracked
//! items), selling prices (so discounts), payment methods, amounts and references, credit due date and notes — and
//! submits the correction with a reason. Nothing changes until the final approval (*Sale edit* workflow; by default the
//! Tenant Administrator; maker-checker and its exceptions as everywhere else).
//!
//! On approval only the **differences** are applied, in one transaction, after checking the sale is still exactly as it
//! was when the correction was made (otherwise *Outdated request*):
//!
//! * stock: only the quantity differences move (`sale` / `sale_reversal` movements referencing the revision); a branch or
//!   date change moves the whole sale (reversed at the old branch and date, deducted at the new) — availability is
//!   checked like a sale;
//! * payments are never edited or deleted: a changed allocation is a pair of linked adjustment rows (−old, +new, the
//!   revision number as reference); verified payments (STK-confirmed M-Pesa, M-Pesa codes, exchanges, return credits,
//!   online payments) cannot be changed — a lower total than they cover needs a return / refund instead;
//! * credit: the amount owed follows the corrected total (never below what was paid), due date, customer (only before
//!   any payment);
//! * loyalty points and customer purchase totals: the difference (or moved to the new customer);
//! * the sale keeps its id, receipt number, recording time and recorder; it becomes revision *n* with a **corrected
//!   receipt** (`RCP-…-R n`) beside the original and earlier receipts; the request keeps the sale before and after, and
//!   the audit trail records every changed field, the reason, requester, approver and approval.
//!
//! Not editable: sales with returns, exchanges or cancellations, imported sales, sales with a request already waiting;
//! credit ↔ paid conversions (cancel and record again); points redeemed on the sale.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, NaiveTime, Timelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as DbJson;
use sqlx::PgConnection;
use uuid::Uuid;

use super::approvals::ApprovalRow;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::inventory::{self, Check, Movement};
use crate::loyalty;
use crate::settings::{self, TenantSettings};
use crate::state::AppState;
use crate::util::{money_str, round2};
use crate::workflow;

pub const ACTION: &str = "sale.edit";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sales/{id}/edit", get(editable).post(submit))
        .route("/sales/{id}/edit/preview", post(preview))
        .route("/sales/{id}/revisions", get(revisions))
}

/// Payment methods that are records of something else (never chosen as a corrected payment).
const SYSTEM_METHODS: [&str; 5] = ["credit", "split", "exchange", "customer_credit", "return_credit"];

// ───────────────────────────── The sale as it stands ─────────────────────────────

#[derive(Clone)]
struct CurLine {
    id: Uuid,
    product_id: Uuid,
    name: String,
    stock_item_id: Option<Uuid>,
    barcode: Option<String>,
    quantity: i32,
    marked_price: Decimal,
    unit_price: Decimal,
    unit_cost: Option<Decimal>,
    scan_source: Option<String>,
}

/// Payments of the sale grouped by what they are (method, reference, verified request) — net of earlier adjustments.
#[derive(Clone, Serialize)]
struct PayGroup {
    key: String,
    method: String,
    reference: String,
    amount: Decimal,
    locked: bool,
}

#[derive(Clone)]
struct Credit {
    paid: Decimal,
    adjustments: Decimal,
    original: Decimal,
    due_date: NaiveDate,
    status: String,
    recall: Option<String>,
}

struct Current {
    receipt_no: String,
    status: String,
    is_legacy: bool,
    imported: bool,
    exchange_linked: bool,
    item_credits: bool,
    branch_id: Uuid,
    customer_id: Option<Uuid>,
    owner_id: Option<Uuid>,
    notes: String,
    date: NaiveDate,
    time: NaiveTime,
    revision: i32,
    gross: Decimal,
    discount: Decimal,
    total: Decimal,
    redeemed_points: i64,
    redeemed_value: Decimal,
    payment_method: String,
    points_earned: i64,
    credit: Option<Credit>,
    lines: Vec<CurLine>,
    payments: Vec<PayGroup>,
    currency: String,
}

fn pay_key(method: &str, reference: &str, request: Option<Uuid>) -> String {
    format!("{method}|{}|{}", reference.trim().to_uppercase(), request.map(|r| r.to_string()).unwrap_or_default())
}

#[derive(sqlx::FromRow)]
struct Head {
    receipt_no: String,
    status: String,
    is_legacy: bool,
    imported: bool,
    branch_id: Uuid,
    customer_id: Option<Uuid>,
    owner_id: Option<Uuid>,
    notes: String,
    business_date: NaiveDate,
    effective_time: Option<NaiveTime>,
    created_at: DateTime<Utc>,
    revision: i32,
    gross_total: Decimal,
    discount_total: Decimal,
    total: Decimal,
    redeemed_points: i64,
    redeemed_value: Decimal,
    payment_method: String,
    points_earned: i64,
    currency: String,
}

async fn load(conn: &mut PgConnection, ctx: &Ctx, s: &TenantSettings, id: Uuid) -> AppResult<Current> {
    let h: Head = sqlx::query_as(
        "SELECT s.receipt_no, s.status, s.is_legacy, s.import_batch_id IS NOT NULL AS imported, s.branch_id, s.customer_id, s.owner_id, s.notes, s.business_date,
                s.effective_time, s.created_at, s.revision, s.gross_total, s.discount_total, s.total, s.redeemed_points, s.redeemed_value,
                s.payment_method, s.points_earned, t.currency
         FROM sales s JOIN tenants t ON t.id = s.tenant_id WHERE s.id = $1 AND s.tenant_id = $2 FOR UPDATE OF s",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("Sale"))?;
    let lines: Vec<(Uuid, Uuid, String, Option<Uuid>, Option<String>, i32, Decimal, Decimal, Option<Decimal>, Option<String>)> = sqlx::query_as(
        "SELECT si.id, si.product_id, p.name, si.stock_item_id, si.barcode, si.quantity, si.marked_price, si.unit_price, si.unit_cost, si.scan_source
         FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = $1 ORDER BY p.name, si.id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let groups: Vec<(String, String, Option<Uuid>, Decimal, bool)> = sqlx::query_as(
        "SELECT method, upper(reference), mpesa_request_id, SUM(amount), bool_or(return_credit_id IS NOT NULL)
         FROM payments WHERE sale_id = $1 AND credit_sale_id IS NULL GROUP BY method, upper(reference), mpesa_request_id
         HAVING SUM(amount) <> 0 ORDER BY MIN(created_at)",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let exchange_linked = groups.iter().any(|g| g.0 == "exchange") || h.notes.starts_with("Exchange for");
    let payments = groups
        .into_iter()
        .map(|(method, reference, request, amount, return_credit)| {
            let locked = request.is_some()
                || (method == "mpesa" && !reference.is_empty())
                || return_credit
                || SYSTEM_METHODS.contains(&method.as_str())
                || !s.sales.payment_methods.iter().any(|m| m.key == method);
            PayGroup { key: pay_key(&method, &reference, request), method, reference, amount, locked }
        })
        .collect();
    let credit: Option<(Decimal, Decimal, Decimal, NaiveDate, String, Option<String>)> = sqlx::query_as(
        "SELECT amount_paid, adjustments, original_amount, due_date, status, recall_state FROM credit_sales WHERE sale_id = $1 AND sale_item_id IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    // Roadmap 196: a credit record per unpaid item.
    let item_credits: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM credit_sales WHERE sale_id = $1 AND sale_item_id IS NOT NULL)")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    let time = h.effective_time.unwrap_or_else(|| h.created_at.with_timezone(&ctx.tz).time().with_second(0).unwrap_or_default().with_nanosecond(0).unwrap_or_default());
    Ok(Current {
        receipt_no: h.receipt_no,
        status: h.status,
        is_legacy: h.is_legacy,
        imported: h.imported,
        exchange_linked,
        item_credits,
        branch_id: h.branch_id,
        customer_id: h.customer_id,
        owner_id: h.owner_id,
        notes: h.notes,
        date: h.business_date,
        time,
        revision: h.revision,
        gross: h.gross_total,
        discount: h.discount_total,
        total: h.total,
        redeemed_points: h.redeemed_points,
        redeemed_value: h.redeemed_value,
        payment_method: h.payment_method,
        points_earned: h.points_earned,
        currency: h.currency,
        credit: credit.map(|(paid, adjustments, original, due_date, status, recall)| Credit { paid, adjustments, original, due_date, status, recall }),
        lines: lines
            .into_iter()
            .map(|(id, product_id, name, stock_item_id, barcode, quantity, marked_price, unit_price, unit_cost, scan_source)| CurLine {
                id, product_id, name, stock_item_id, barcode, quantity, marked_price, unit_price, unit_cost, scan_source,
            })
            .collect(),
        payments,
    })
}

/// What the sale is, for comparing (a correction is applied only to the sale it was made on).
fn fingerprint(c: &Current) -> Value {
    json!({
        "revision": c.revision, "status": c.status, "date": c.date, "time": c.time.format("%H:%M").to_string(), "branch_id": c.branch_id,
        "customer_id": c.customer_id, "owner_id": c.owner_id, "notes": c.notes, "total": c.total,
        "due_date": c.credit.as_ref().map(|k| k.due_date), "credit_paid": c.credit.as_ref().map(|k| k.paid + k.adjustments),
        "lines": c.lines.iter().map(|l| json!({ "id": l.id, "product_id": l.product_id, "quantity": l.quantity, "unit_price": l.unit_price, "item": l.stock_item_id })).collect::<Vec<_>>(),
        "payments": c.payments.iter().map(|p| json!({ "key": p.key, "amount": p.amount })).collect::<Vec<_>>(),
    })
}

/// Why a sale cannot be edited (None = it can).
fn blocked(c: &Current) -> Option<String> {
    if c.is_legacy || c.imported {
        return Some("Imported sales are corrected in the import, not here".into());
    }
    match c.status.as_str() {
        "cancelled" => return Some("A cancelled sale cannot be edited".into()),
        "returned" | "partially_returned" => return Some("This sale has returns — corrections after a return are made with further returns or exchanges".into()),
        _ => {}
    }
    if c.item_credits {
        return Some("Items of this sale are on credit one by one — correct them with recall, return or repayment in Credit Sales".into());
    }
    if c.exchange_linked {
        return Some("This sale comes from an exchange — correct it with a return or exchange".into());
    }
    if let Some(k) = &c.credit {
        if k.recall.is_some() || matches!(k.status.as_str(), "written_off" | "cancelled" | "recalled") {
            return Some("This credit sale was recalled or written off and cannot be edited".into());
        }
    }
    None
}

// ───────────────────────────── The proposal ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EditLine {
    /// The sale line it corrects (None = a product added).
    #[serde(default)]
    pub sale_item_id: Option<Uuid>,
    pub product_id: Uuid,
    pub quantity: i32,
    pub unit_price: Decimal,
    /// Tracked items added: the scanned barcode.
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub scan_source: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EditPayment {
    pub method: String,
    pub amount: Decimal,
    #[serde(default)]
    pub reference: String,
    /// A verified payment kept as it is (its group key).
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub sale_date: NaiveDate,
    /// HH:MM, the business's time zone.
    #[serde(default)]
    pub sale_time: Option<String>,
    pub branch_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub owner_id: Option<Uuid>,
    #[serde(default)]
    pub notes: String,
    pub due_date: Option<NaiveDate>,
    pub lines: Vec<EditLine>,
    #[serde(default)]
    pub payments: Vec<EditPayment>,
    /// Roadmap 192: items removed (or quantities reduced) go back to stock at the selling branch — off when they cannot
    /// be resold (damaged, not physically back); their barcodes then stay as sold.
    #[serde(default = "yes")]
    pub restock: bool,
}

fn yes() -> bool {
    true
}

struct NewLine {
    sale_item_id: Option<Uuid>,
    product_id: Uuid,
    name: String,
    stock_item_id: Option<Uuid>,
    barcode: Option<String>,
    scan_source: Option<String>,
    quantity: i32,
    marked_price: Decimal,
    unit_price: Decimal,
    line_total: Decimal,
    unit_cost: Option<Decimal>,
    points: i64,
    earn: loyalty::EarnLine,
}

struct Plan {
    date: NaiveDate,
    time: NaiveTime,
    branch_id: Uuid,
    customer_id: Option<Uuid>,
    owner_id: Option<Uuid>,
    notes: String,
    due_date: Option<NaiveDate>,
    lines: Vec<NewLine>,
    gross: Decimal,
    net: Decimal,
    total: Decimal,
    points: i64,
    /// Non-credit sales: the corrected allocation by group key → (method, reference, amount).
    payments: Vec<(String, String, String, Decimal)>,
    method: String,
    changes: Vec<Value>,
    implications: Value,
    stock: Vec<(Uuid, Uuid, Option<Uuid>, i32, Option<Decimal>, Decimal, Option<NaiveDate>)>,
}

fn money(cur: &str, d: Decimal) -> String {
    let d = round2(d);
    let body = if d.fract().is_zero() { money_str(d) } else { format!("{}{}", if d.is_sign_negative() { "-" } else { "" }, with_commas(d.abs())) };
    format!("{cur} {body}")
}

fn with_commas(d: Decimal) -> String {
    let whole = money_str(d.trunc());
    let cents = ((d.fract() * Decimal::from(100)).round()).to_string();
    format!("{whole}.{cents:0>2}")
}

fn change(field: &str, label: impl Into<String>, before: impl Into<String>, after: impl Into<String>) -> Value {
    json!({ "field": field, "label": label.into(), "before": before.into(), "after": after.into() })
}

async fn names(conn: &mut PgConnection, table: &str, ids: &[Uuid]) -> AppResult<BTreeMap<Uuid, String>> {
    let sql = match table {
        "users" => "SELECT id, name FROM users WHERE id = ANY($1)",
        "branches" => "SELECT id, name FROM branches WHERE id = ANY($1)",
        _ => "SELECT id, COALESCE(NULLIF(TRIM(first_name || ' ' || COALESCE(other_names, '')), ''), 'Customer') || ' · ' || mobile FROM customers WHERE id = ANY($1)",
    };
    let rows: Vec<(Uuid, Option<String>)> = sqlx::query_as(sql).bind(ids).fetch_all(&mut *conn).await?;
    Ok(rows.into_iter().map(|(i, n)| (i, n.unwrap_or_default())).collect())
}

fn parse_time(t: Option<&str>, fallback: NaiveTime) -> AppResult<NaiveTime> {
    match t.map(str::trim).filter(|x| !x.is_empty()) {
        None => Ok(fallback),
        Some(x) => NaiveTime::parse_from_str(x, "%H:%M").or_else(|_| NaiveTime::parse_from_str(x, "%H:%M:%S")).map(|t| t.with_second(0).unwrap_or(t)).map_err(|_| bad("Enter the time as HH:MM")),
    }
}

#[derive(sqlx::FromRow)]
struct Prod {
    name: String,
    is_active: bool,
    track_items: bool,
    marked_price: Option<Decimal>,
    cost_price: Option<Decimal>,
    loyalty_eligible: bool,
    loyalty_threshold: Option<Decimal>,
    loyalty_points_per: Option<i32>,
}

/// Validates a proposal against the sale as it stands and works out everything it would change.
/// `apply`: at approval time tracked items are locked (claimed) for the sale.
async fn plan(conn: &mut PgConnection, ctx: &Ctx, s: &TenantSettings, cur: &Current, p: &Proposal, apply: bool) -> AppResult<Plan> {
    if let Some(why) = blocked(cur) {
        return Err(refused("Cannot edit this sale", why));
    }
    let cc = cur.currency.as_str();
    // Date and time.
    let today = ctx.today().max(Utc::now().with_timezone(&ctx.tz).date_naive());
    if p.sale_date > today {
        return Err(rule("The sale date cannot be in the future"));
    }
    if p.sale_date < NaiveDate::from_ymd_opt(2000, 1, 1).expect("valid") {
        return Err(rule("The sale date is too far in the past"));
    }
    s.books.check(cur.date, "a correction of this sale")?;
    s.books.check(p.sale_date, "a correction of this sale")?;
    let time = parse_time(p.sale_time.as_deref(), cur.time)?;

    // Branch.
    let branch_ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM branches WHERE id = $1 AND tenant_id = $2 AND is_active)")
        .bind(p.branch_id)
        .bind(ctx.tenant_id)
        .fetch_one(&mut *conn)
        .await?;
    if !branch_ok {
        return Err(AppError::NotFound("Branch"));
    }
    let branch_changed = p.branch_id != cur.branch_id;
    if branch_changed && cur.lines.iter().any(|l| l.stock_item_id.is_some()) {
        return Err(refused("Tracked items", "This sale has items tracked by their own barcode — they belong to the branch that sold them, so the branch cannot be changed. Return them instead."));
    }

    // Owner.
    let owner_id = p.owner_id.or(cur.owner_id);
    if let Some(o) = owner_id {
        if (Some(o) != cur.owner_id || branch_changed) && !super::sales::eligible_owner(conn, ctx.tenant_id, p.branch_id, o).await? {
            return Err(rule("That person cannot be credited with sales at this branch"));
        }
    }

    // Customer.
    if let Some(c) = p.customer_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM customers WHERE id = $1 AND tenant_id = $2)").bind(c).bind(ctx.tenant_id).fetch_one(&mut *conn).await?;
        if !ok {
            return Err(AppError::NotFound("Customer"));
        }
    }
    let customer_changed = p.customer_id != cur.customer_id;
    if customer_changed && cur.redeemed_points > 0 {
        return Err(refused("Points were redeemed", "The customer paid part of this sale with loyalty points, so the customer cannot be changed."));
    }
    if let Some(k) = &cur.credit {
        if p.customer_id.is_none() {
            return Err(rule("A credit sale needs a customer"));
        }
        if customer_changed && (k.paid > Decimal::ZERO || k.adjustments > Decimal::ZERO) {
            return Err(refused("Payments received", "Payments were already received on this credit sale, so the customer cannot be changed."));
        }
    }

    // Lines.
    if p.lines.is_empty() {
        return Err(bad("A sale needs at least one product — to undo the whole sale, cancel it"));
    }
    let mut lines: Vec<NewLine> = Vec::with_capacity(p.lines.len());
    let mut used_lines: Vec<Uuid> = Vec::new();
    let mut seen_items: Vec<Uuid> = Vec::new();
    for l in &p.lines {
        let prod: Prod = sqlx::query_as(
            "SELECT name, is_active, track_items, effective_price(id, $3, marked_price) AS marked_price, cost_price, loyalty_eligible, loyalty_threshold, loyalty_points_per
             FROM products WHERE id = $1 AND tenant_id = $2",
        )
        .bind(l.product_id)
        .bind(ctx.tenant_id)
        .bind(p.branch_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
        let kept = l.sale_item_id.and_then(|id| cur.lines.iter().find(|c| c.id == id && c.product_id == l.product_id));
        if let Some(k) = kept {
            if used_lines.contains(&k.id) {
                return Err(bad("A sale line appears twice"));
            }
            used_lines.push(k.id);
        } else if !prod.is_active {
            return Err(rule(format!("{} is inactive and cannot be added", prod.name)));
        }
        if l.quantity <= 0 {
            return Err(bad(format!("Enter a quantity for {}", prod.name)));
        }
        if l.unit_price < Decimal::ZERO {
            return Err(bad("Selling price cannot be negative"));
        }
        if kept.is_none() || branch_changed {
            inventory::ensure_product_in_branch(conn, ctx.tenant_id, l.product_id, p.branch_id).await?;
        }
        let marked_price = match kept {
            Some(k) => k.marked_price,
            None => match prod.marked_price {
                Some(m) if m == Decimal::ZERO => {
                    return Err(refused("Price needed", format!("{} has no selling price at this branch (it is 0) — set its price first.", prod.name)));
                }
                Some(m) => m,
                None if l.unit_price <= Decimal::ZERO => return Err(refused("Price needed", format!("{} has no selling price yet — enter the price it is sold for.", prod.name))),
                None => l.unit_price,
            },
        };
        let (mut stock_item_id, mut barcode, mut scan_source, mut unit_cost) = (None, None, None, prod.cost_price);
        if let Some(k) = kept {
            stock_item_id = k.stock_item_id;
            barcode = k.barcode.clone();
            scan_source = k.scan_source.clone();
            unit_cost = k.unit_cost;
        }
        let tracked = stock_item_id.is_some() || (kept.is_none() && prod.track_items);
        if tracked {
            if l.quantity != 1 {
                return Err(rule(format!("{} is tracked per item — one line per scanned barcode", prod.name)));
            }
            if stock_item_id.is_none() {
                let code = l.barcode.as_deref().map(str::trim).filter(|c| !c.is_empty()).ok_or_else(|| refused("Scan required", format!("Scan the barcode of the {} being added.", prod.name)))?;
                let (item, cost) = super::sales::claim_item(conn, ctx.tenant_id, l.product_id, &prod.name, p.branch_id, code, apply).await?;
                stock_item_id = Some(item);
                barcode = Some(code.to_string());
                scan_source = match l.scan_source.as_deref() {
                    Some(src @ ("scanner" | "camera" | "manual")) => Some(src.to_string()),
                    _ => None,
                };
                unit_cost = cost.or(prod.cost_price);
            }
            let item = stock_item_id.expect("set above");
            if seen_items.contains(&item) {
                return Err(refused("Already in this sale", "That item is already on this sale."));
            }
            seen_items.push(item);
        }
        let line_total = round2(l.unit_price * Decimal::from(l.quantity));
        lines.push(NewLine {
            sale_item_id: kept.map(|k| k.id),
            product_id: l.product_id,
            name: prod.name.clone(),
            stock_item_id,
            barcode,
            scan_source,
            quantity: l.quantity,
            marked_price,
            unit_price: l.unit_price,
            line_total,
            unit_cost,
            points: 0,
            earn: loyalty::EarnLine { eligible: prod.loyalty_eligible, threshold: prod.loyalty_threshold, points_per: prod.loyalty_points_per, amount: Decimal::ZERO },
        });
    }
    if cur.credit.is_some() && s.sales.credit_single_item {
        let units: i32 = lines.iter().map(|l| l.quantity).sum();
        if units > 1 {
            return Err(refused("One item per credit sale", format!("Credit sales hold one item each, and this correction has {units}.")));
        }
    }

    // Totals and points (as when a sale is recorded).
    let gross: Decimal = round2(lines.iter().map(|l| l.marked_price * Decimal::from(l.quantity)).sum());
    let net: Decimal = lines.iter().map(|l| l.line_total).sum();
    let total = round2(net - cur.redeemed_value);
    if total < Decimal::ZERO {
        return Err(rule("The corrected total is less than the points redeemed on this sale"));
    }
    let mut points = 0;
    if p.customer_id.is_some() && s.loyalty.enabled && net >= s.loyalty.min_spend {
        let share = if cur.redeemed_value > Decimal::ZERO && !s.loyalty.earn_on_redeemed && net > Decimal::ZERO { total / net } else { Decimal::ONE };
        let earn: Vec<loyalty::EarnLine> = lines
            .iter()
            .map(|l| loyalty::EarnLine {
                eligible: l.earn.eligible,
                threshold: l.earn.threshold,
                points_per: l.earn.points_per,
                amount: if s.loyalty.earn_base == "gross" { l.marked_price * Decimal::from(l.quantity) } else { l.line_total } * share,
            })
            .collect();
        let (pts, per_line) = loyalty::sale_points(s, &earn);
        points = pts;
        for (l, n) in lines.iter_mut().zip(per_line) {
            l.points = n;
        }
    }

    // Payments.
    let mut payments: Vec<(String, String, String, Decimal)> = Vec::new();
    let mut method = cur.payment_method.clone();
    let mut due_date = None;
    if let Some(k) = &cur.credit {
        if total < k.paid + k.adjustments {
            return Err(refused(
                "Already paid",
                format!("{} has already been paid or settled on this credit sale — the total cannot be lower. Use a return for the difference.", money(cc, k.paid + k.adjustments)),
            ));
        }
        let due = p.due_date.unwrap_or(k.due_date);
        if due < p.sale_date {
            return Err(rule("The due date cannot be before the sale date"));
        }
        due_date = Some(due);
    } else {
        let mut used_keys: Vec<String> = Vec::new();
        let mut sum = Decimal::ZERO;
        for pay in &p.payments {
            let amount = round2(pay.amount);
            if amount <= Decimal::ZERO {
                continue;
            }
            if let Some(key) = &pay.key {
                let g = cur.payments.iter().find(|g| &g.key == key && g.locked).ok_or_else(|| bad("Unknown verified payment"))?;
                if amount != g.amount {
                    return Err(refused("Verified payment", format!("The {} payment of {} is verified and cannot be changed.", label(s, &g.method), money(cc, g.amount))));
                }
                if used_keys.contains(key) {
                    return Err(bad("A payment appears twice"));
                }
                used_keys.push(key.clone());
                payments.push((key.clone(), g.method.clone(), g.reference.clone(), amount));
                sum += amount;
                continue;
            }
            let m = pay.method.trim();
            let existing = cur.payments.iter().any(|g| g.method == m && !g.locked);
            if SYSTEM_METHODS.contains(&m) || (!s.payment_enabled(m) && !existing) {
                return Err(rule(format!("“{m}” cannot be used for this sale's payment")));
            }
            let mut reference = pay.reference.trim().to_uppercase();
            if m == "mpesa" && !reference.is_empty() {
                if !(8..=12).contains(&reference.len()) || !reference.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return Err(refused("Check the M-Pesa code", "An M-Pesa confirmation code has 8 to 12 letters and digits (e.g. QFT1ABC2DE)."));
                }
                let used: Option<String> = sqlx::query_scalar(
                    "SELECT s.receipt_no FROM payments p JOIN sales s ON s.id = p.sale_id WHERE p.tenant_id = $1 AND p.method = 'mpesa' AND upper(p.reference) = $2 AND p.amount > 0 LIMIT 1",
                )
                .bind(ctx.tenant_id)
                .bind(&reference)
                .fetch_optional(&mut *conn)
                .await?;
                if let Some(r) = used {
                    return Err(refused("M-Pesa code already used", format!("{reference} was already recorded on sale {r}.")));
                }
            } else if m == "mpesa" && !existing && !s.sales.mpesa_manual_confirmation {
                return Err(refused("M-Pesa code needed", "This business takes M-Pesa with Push STK only — enter the M-Pesa confirmation code of the payment received."));
            }
            if m != "mpesa" {
                reference = reference.chars().take(40).collect();
            }
            let key = pay_key(m, &reference, None);
            if used_keys.contains(&key) || payments.iter().any(|x| x.1 == m && !cur.payments.iter().any(|g| g.key == x.0 && g.locked)) {
                return Err(rule("Use each payment method once — add the amounts together"));
            }
            used_keys.push(key.clone());
            payments.push((key, m.to_string(), reference, amount));
            sum += amount;
        }
        for g in cur.payments.iter().filter(|g| g.locked) {
            if !used_keys.contains(&g.key) {
                return Err(refused(
                    "Verified payment",
                    format!("The {} payment of {} is verified and stays on the sale — keep it (a lower total needs a return or refund).", label(s, &g.method), money(cc, g.amount)),
                ));
            }
        }
        let locked: Decimal = cur.payments.iter().filter(|g| g.locked).map(|g| g.amount).sum();
        if locked > total {
            return Err(refused(
                "Verified payments exceed the total",
                format!("Verified payments of {} are more than the corrected total of {} — use Return items / refund for the difference.", money(cc, locked), money(cc, total)),
            ));
        }
        if sum != total {
            return Err(rule(if sum < total {
                format!("{} of the corrected total is not allocated to a payment", money(cc, total - sum))
            } else {
                format!("The payments add up to {} — more than the corrected total of {}", money(cc, sum), money(cc, total))
            }));
        }
        if total == Decimal::ZERO && payments.is_empty() {
            method = cur.payment_method.clone();
        } else {
            let methods: Vec<&String> = payments.iter().map(|x| &x.1).collect();
            method = if methods.iter().all(|m| *m == methods[0]) { methods[0].clone() } else { "split".to_string() };
        }
    }

    // What changes — field by field, for the request, the approver and the audit trail.
    let mut changes: Vec<Value> = Vec::new();
    if p.sale_date != cur.date {
        changes.push(change("sale_date", "Sale date", cur.date.format("%-d %b %Y").to_string(), p.sale_date.format("%-d %b %Y").to_string()));
    }
    if time != cur.time {
        changes.push(change("sale_time", "Sale time", cur.time.format("%H:%M").to_string(), time.format("%H:%M").to_string()));
    }
    let bn = names(conn, "branches", &[cur.branch_id, p.branch_id]).await?;
    if branch_changed {
        changes.push(change("branch", "Branch", bn.get(&cur.branch_id).cloned().unwrap_or_default(), bn.get(&p.branch_id).cloned().unwrap_or_default()));
    }
    if customer_changed {
        let cn = names(conn, "customers", &[cur.customer_id, p.customer_id].into_iter().flatten().collect::<Vec<_>>()).await?;
        let n = |c: Option<Uuid>| c.and_then(|c| cn.get(&c).cloned()).unwrap_or_else(|| "Walk-in (no customer)".into());
        changes.push(change("customer", "Customer", n(cur.customer_id), n(p.customer_id)));
    }
    if owner_id != cur.owner_id {
        let un = names(conn, "users", &[cur.owner_id, owner_id].into_iter().flatten().collect::<Vec<_>>()).await?;
        let n = |c: Option<Uuid>| c.and_then(|c| un.get(&c).cloned()).unwrap_or_else(|| "—".into());
        changes.push(change("owner", "Sale owner", n(cur.owner_id), n(owner_id)));
    }
    let line_text = |name: &str, q: i32, price: Decimal| format!("{name} × {q} @ {}", money(cc, price));
    for c in &cur.lines {
        match lines.iter().find(|l| l.sale_item_id == Some(c.id)) {
            None => changes.push(change("line_removed", format!("Removed — {}", c.name), line_text(&c.name, c.quantity, c.unit_price), "—")),
            Some(n) => {
                if n.quantity != c.quantity {
                    changes.push(change("quantity", format!("Quantity — {}", c.name), c.quantity.to_string(), n.quantity.to_string()));
                }
                if n.unit_price != c.unit_price {
                    changes.push(change("unit_price", format!("Selling price — {}", c.name), money(cc, c.unit_price), money(cc, n.unit_price)));
                }
            }
        }
    }
    for n in lines.iter().filter(|l| l.sale_item_id.is_none()) {
        let code = n.barcode.as_deref().map(|b| if n.stock_item_id.is_some() { super::barcodes::mask(b) } else { b.to_string() });
        changes.push(change("line_added", format!("Added — {}", n.name), "—", format!("{}{}", line_text(&n.name, n.quantity, n.unit_price), code.map(|c| format!(" · {c}")).unwrap_or_default())));
    }
    // Shown as a discount only when sold below the marked price (a price above it is not a negative discount).
    let discount = round2(gross - net);
    let (d_before, d_after) = (cur.discount.max(Decimal::ZERO), discount.max(Decimal::ZERO));
    if d_before != d_after {
        changes.push(change("discount", "Discount", money(cc, d_before), money(cc, d_after)));
    }
    if total != cur.total {
        changes.push(change("total", "Total", money(cc, cur.total), money(cc, total)));
    }
    let pay_text = |m: &str, r: &str, a: Decimal| format!("{} {}{}", label(s, m), money(cc, a), if r.is_empty() { String::new() } else { format!(" ({r})") });
    if cur.credit.is_none() {
        let before: Vec<String> = cur.payments.iter().map(|g| pay_text(&g.method, &g.reference, g.amount)).collect();
        let after: Vec<String> = payments.iter().map(|x| pay_text(&x.1, &x.2, x.3)).collect();
        if before != after {
            changes.push(change("payments", "Payment", if before.is_empty() { "—".into() } else { before.join(" + ") }, if after.is_empty() { "—".into() } else { after.join(" + ") }));
        }
        if method != cur.payment_method {
            changes.push(change("payment_method", "Payment method", label(s, &cur.payment_method), label(s, &method)));
        }
    }
    if let (Some(k), Some(due)) = (&cur.credit, due_date) {
        if due != k.due_date {
            changes.push(change("due_date", "Credit due date", k.due_date.format("%-d %b %Y").to_string(), due.format("%-d %b %Y").to_string()));
        }
    }
    let notes: String = p.notes.trim().chars().take(500).collect();
    if notes != cur.notes.trim() {
        changes.push(change("notes", "Notes", if cur.notes.trim().is_empty() { "—".into() } else { cur.notes.trim().to_string() }, if notes.is_empty() { "—".into() } else { notes.clone() }));
    }
    let content_changed = changes.iter().any(|c| !matches!(c["field"].as_str(), Some("discount" | "total" | "payment_method")));
    if !content_changed {
        return Err(rule("Nothing to change — the sale is exactly as recorded"));
    }
    if points != cur.points_earned || customer_changed {
        changes.push(change("points", "Loyalty points", cur.points_earned.to_string(), points.to_string()));
    }

    // Stock: the differences (or the whole sale moved when its branch or date changes).
    let today = ctx.today();
    let day_of = |d: NaiveDate| if d < today { Some(d) } else { None };
    let whole = branch_changed || p.sale_date != cur.date;
    if whole && !p.restock {
        return Err(rule("A branch or date change returns the sale's items to stock at the original branch — keep “Return removed items to stock” on"));
    }
    // (branch, product, item, quantity (+ back / − out), cost, price, dated)
    let mut stock: Vec<(Uuid, Uuid, Option<Uuid>, i32, Option<Decimal>, Decimal, Option<NaiveDate>)> = Vec::new();
    if whole {
        for c in &cur.lines {
            stock.push((cur.branch_id, c.product_id, c.stock_item_id, c.quantity, c.unit_cost, c.unit_price, day_of(cur.date)));
        }
        for n in &lines {
            stock.push((p.branch_id, n.product_id, n.stock_item_id, -n.quantity, n.unit_cost, n.unit_price, day_of(p.sale_date)));
        }
    } else {
        // Tracked items by item; other products by product.
        for c in cur.lines.iter().filter(|c| c.stock_item_id.is_some()) {
            if !lines.iter().any(|n| n.stock_item_id == c.stock_item_id) {
                stock.push((cur.branch_id, c.product_id, c.stock_item_id, 1, c.unit_cost, c.unit_price, day_of(cur.date)));
            }
        }
        for n in lines.iter().filter(|n| n.stock_item_id.is_some()) {
            if !cur.lines.iter().any(|c| c.stock_item_id == n.stock_item_id) {
                stock.push((p.branch_id, n.product_id, n.stock_item_id, -1, n.unit_cost, n.unit_price, day_of(p.sale_date)));
            }
        }
        let mut by_product: BTreeMap<Uuid, (i32, Option<Decimal>, Decimal)> = BTreeMap::new();
        for c in cur.lines.iter().filter(|c| c.stock_item_id.is_none()) {
            let e = by_product.entry(c.product_id).or_insert((0, c.unit_cost, c.unit_price));
            e.0 += c.quantity;
        }
        for n in lines.iter().filter(|n| n.stock_item_id.is_none()) {
            let e = by_product.entry(n.product_id).or_insert((0, n.unit_cost, n.unit_price));
            e.0 -= n.quantity;
            e.2 = n.unit_price;
        }
        for (product, (q, cost, price)) in by_product {
            if q != 0 {
                stock.push((cur.branch_id, product, None, q, cost, price, day_of(if q > 0 { cur.date } else { p.sale_date })));
            }
        }
    }
    if !p.restock {
        stock.retain(|e| e.3 < 0);
    }
    let product_names: BTreeMap<Uuid, String> = cur.lines.iter().map(|l| (l.product_id, l.name.clone())).chain(lines.iter().map(|l| (l.product_id, l.name.clone()))).collect();
    let mut net_stock: BTreeMap<(Uuid, Uuid), i32> = BTreeMap::new();
    for (b, pr, _, q, ..) in &stock {
        *net_stock.entry((*b, *pr)).or_default() += q;
    }
    // Availability at each branch that gives stock (checked again, under lock, when it is applied).
    if !s.stock.allow_negative {
        for ((b, pr), q) in &net_stock {
            if *q >= 0 {
                continue;
            }
            let avail: i32 = sqlx::query_scalar("SELECT on_hand - reserved FROM stock_levels WHERE branch_id = $1 AND product_id = $2")
                .bind(b)
                .bind(pr)
                .fetch_optional(&mut *conn)
                .await?
                .unwrap_or(0);
            if avail < -q {
                return Err(refused(
                    "Not enough stock",
                    format!("{} has {} available at {} — the correction needs {} more.", product_names.get(pr).cloned().unwrap_or_default(), avail.max(0), bn.get(b).cloned().unwrap_or_default(), -q),
                ));
            }
        }
    }
    let stock_rows: Vec<Value> = net_stock
        .iter()
        .filter(|(_, q)| **q != 0)
        .map(|((b, pr), q)| json!({ "branch": bn.get(b), "product": product_names.get(pr), "change": q }))
        .collect();

    // Payment adjustments (non-credit): −old / +new per allocation.
    let mut adjustments: Vec<Value> = Vec::new();
    if cur.credit.is_none() {
        for g in &cur.payments {
            let next = payments.iter().find(|x| x.0 == g.key).map(|x| x.3).unwrap_or(Decimal::ZERO);
            if next != g.amount {
                adjustments.push(json!({ "method": label(s, &g.method), "amount": next - g.amount, "reference": g.reference }));
            }
        }
        for x in payments.iter().filter(|x| !cur.payments.iter().any(|g| g.key == x.0)) {
            adjustments.push(json!({ "method": label(s, &x.1), "amount": x.3, "reference": x.2 }));
        }
    }
    let credit_imp = cur.credit.as_ref().map(|k| {
        json!({
            "original_before": k.original, "original_after": total,
            "balance_before": k.original - k.paid - k.adjustments, "balance_after": total - k.paid - k.adjustments,
            "due_before": k.due_date, "due_after": due_date,
        })
    });
    let cn = names(conn, "customers", &[cur.customer_id, p.customer_id].into_iter().flatten().collect::<Vec<_>>()).await?;
    let implications = json!({
        "currency": cc,
        "financial": { "total_before": cur.total, "total_after": total, "difference": total - cur.total,
                       "discount_before": d_before, "discount_after": d_after, "payments": adjustments },
        "stock": stock_rows,
        "restock": p.restock,
        "loyalty": { "customer_before": cur.customer_id.and_then(|c| cn.get(&c).cloned()), "points_before": cur.points_earned,
                     "customer_after": p.customer_id.and_then(|c| cn.get(&c).cloned()), "points_after": points },
        "credit": credit_imp,
        "performance": if owner_id != cur.owner_id || total != cur.total || branch_changed || p.sale_date != cur.date {
            Some("Sale owner, branch and daily figures follow the corrected sale (leaderboards, reports and dashboards).")
        } else { None },
    });

    Ok(Plan {
        date: p.sale_date,
        time,
        branch_id: p.branch_id,
        customer_id: p.customer_id,
        owner_id,
        notes,
        due_date,
        lines,
        gross,
        net,
        total,
        points,
        payments,
        method,
        changes,
        implications,
        stock,
    })
}

fn label(s: &TenantSettings, method: &str) -> String {
    match method {
        "split" => "Split payment".into(),
        "credit" => "Credit".into(),
        "exchange" => "Exchange credit".into(),
        "customer_credit" => "Customer credit".into(),
        "return_credit" => "Return credit".into(),
        _ => s.sales.payment_methods.iter().find(|m| m.key == method).map(|m| m.label.clone()).unwrap_or_else(|| method.to_string()),
    }
}

// ───────────────────────────── Endpoints ─────────────────────────────

/// The sale as it can be edited: its details, lines, payments (verified ones locked) and whether it can be edited.
async fn editable(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("sales.edit")?;
    super::sales::ensure_visible(&state, &ctx, id).await?;
    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let cur = load(&mut tx, &ctx, &s, id).await?;
    let waiting: Option<(Uuid, Option<Uuid>)> = sqlx::query_as("SELECT id, approval_id FROM sale_edit_requests WHERE sale_id = $1 AND status = 'awaiting'")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let other_pending: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM approvals WHERE entity_type = 'sale' AND entity_id = $1 AND status = 'pending')")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let customer: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', id, 'name', TRIM(first_name || ' ' || COALESCE(other_names, '')), 'mobile', mobile) FROM customers WHERE id = $1",
    )
    .bind(cur.customer_id)
    .fetch_optional(&mut *tx)
    .await?;
    let owner: Option<String> = sqlx::query_scalar("SELECT name FROM users WHERE id = $1").bind(cur.owner_id).fetch_optional(&mut *tx).await?;
    let tracked_masks: Vec<Value> = cur
        .lines
        .iter()
        .map(|l| {
            json!({
                "sale_item_id": l.id, "product_id": l.product_id, "product_name": l.name, "quantity": l.quantity,
                "unit_price": l.unit_price, "marked_price": l.marked_price, "tracked": l.stock_item_id.is_some(),
                "barcode": l.barcode.as_deref().map(|b| if l.stock_item_id.is_some() { super::barcodes::mask(b) } else { b.to_string() }),
            })
        })
        .collect();
    tx.rollback().await?;
    let reason = blocked(&cur).or_else(|| waiting.map(|_| "A correction of this sale is already waiting for approval".to_string())).or_else(|| {
        other_pending.then(|| "A return, exchange, cancellation or ownership change for this sale is awaiting approval".to_string())
    });
    Ok(Json(json!({
        "id": id, "receipt_no": cur.receipt_no, "revision": cur.revision, "editable": reason.is_none(), "blocked_reason": reason,
        "pending_request": waiting.map(|w| json!({ "id": w.0, "approval_id": w.1 })),
        "sale_date": cur.date, "sale_time": cur.time.format("%H:%M").to_string(), "branch_id": cur.branch_id,
        "customer": customer, "owner_id": cur.owner_id, "owner_name": owner, "notes": cur.notes,
        "gross_total": cur.gross, "discount_total": cur.discount, "total": cur.total, "redeemed_points": cur.redeemed_points, "redeemed_value": cur.redeemed_value,
        "payment_method": cur.payment_method, "points_earned": cur.points_earned,
        "credit": cur.credit.as_ref().map(|k| json!({ "due_date": k.due_date, "paid": k.paid + k.adjustments, "original": k.original })),
        "lines": tracked_masks,
        "payments": cur.payments.iter().map(|g| json!({ "key": g.key, "method": g.method, "label": label(&s, &g.method), "reference": g.reference, "amount": g.amount, "locked": g.locked })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct SubmitBody {
    proposal: Proposal,
    #[serde(default)]
    reason: String,
    /// The sale's revision as the person saw it.
    revision: i32,
}

async fn prepare(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, b: &SubmitBody) -> AppResult<(TenantSettings, Current, Plan)> {
    let s = settings::load(&mut *conn, ctx.tenant_id).await?;
    let cur = load(conn, ctx, &s, id).await?;
    ctx.ensure_branch(cur.branch_id)?;
    if b.proposal.branch_id != cur.branch_id {
        ctx.ensure_branch(b.proposal.branch_id)?;
    }
    if cur.revision != b.revision {
        return Err(refused("Changed meanwhile", "This sale was corrected after you opened it — reopen it to see the current version"));
    }
    let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sale_edit_requests WHERE sale_id = $1 AND status = 'awaiting')").bind(id).fetch_one(&mut *conn).await?;
    if waiting {
        return Err(refused("Already waiting", "A correction of this sale is already waiting for approval — wait for its decision (or withdraw it in Approvals)"));
    }
    let pending: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM approvals WHERE entity_type = 'sale' AND entity_id = $1 AND status = 'pending')")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    if pending {
        return Err(refused("Awaiting approval", "A return, exchange, cancellation or ownership change for this sale is awaiting approval — wait for its decision first"));
    }
    let plan = plan(conn, ctx, &s, &cur, &b.proposal, false).await?;
    Ok((s, cur, plan))
}

/// The summary of what a correction changes (nothing is saved).
async fn preview(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SubmitBody>) -> AppResult<Json<Value>> {
    ctx.require("sales.edit")?;
    super::sales::ensure_visible(&state, &ctx, id).await?;
    let mut tx = state.db.begin().await?;
    let (_, cur, plan) = prepare(&mut tx, &ctx, id, &b).await?;
    tx.rollback().await?;
    Ok(Json(json!({ "receipt_no": cur.receipt_no, "changes": plan.changes, "implications": plan.implications, "total": plan.total })))
}

/// Submits a correction for approval. The sale stays exactly as it is until the final approval.
async fn submit(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SubmitBody>) -> AppResult<Json<Value>> {
    ctx.require("sales.edit")?;
    let reason: String = b.reason.trim().chars().take(500).collect();
    if reason.is_empty() {
        return Err(bad("Give the reason for the correction"));
    }
    super::sales::ensure_visible(&state, &ctx, id).await?;
    let mut tx = state.db.begin().await?;
    let (_, cur, plan) = prepare(&mut tx, &ctx, id, &b).await?;
    // The original receipt is fixed before anything can change.
    crate::receipts::issue_original(&mut tx, ctx.tenant_id, id, Some(ctx.user_id)).await?;
    let request: Uuid = sqlx::query_scalar(
        "INSERT INTO sale_edit_requests (tenant_id, sale_id, branch_id, reason, before, proposed, changes, implications, requested_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(cur.branch_id)
    .bind(&reason)
    .bind(fingerprint(&cur))
    .bind(DbJson(&b.proposal))
    .bind(json!(plan.changes))
    .bind(&plan.implications)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    let custom = workflow::rule_for(&mut tx, ctx.tenant_id, ACTION).await?.is_some_and(|r| r.enabled && !r.levels.is_empty());
    let labels: Vec<&str> = plan.changes.iter().filter_map(|c| c["label"].as_str()).take(4).collect();
    let approval = workflow::submit(
        &mut tx,
        &ctx,
        workflow::Request {
            action: ACTION,
            entity_type: "sale",
            entity_id: id,
            branch_id: Some(cur.branch_id),
            summary: format!("Edit sale {} — {}{} — {reason}", cur.receipt_no, labels.join(", "), if plan.changes.len() > labels.len() { "…" } else { "" }),
            amount: Some(plan.total),
            payload: json!({ "request_id": request, "receipt_no": cur.receipt_no, "reason": reason, "changes": plan.changes, "implications": plan.implications }),
        },
    )
    .await?;
    // Always approved: the configured chain, else the Tenant Administrator.
    if !custom {
        sqlx::query("UPDATE approvals SET steps = $2 WHERE id = $1").bind(approval).bind(DbJson(vec![workflow::Level::single("admin")])).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE sale_edit_requests SET approval_id = $2 WHERE id = $1").bind(request).bind(approval).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("sales", "edit_requested", "sale", id)
            .branch(cur.branch_id)
            .approval(Some(approval))
            .after(json!({ "request": request, "changes": plan.changes }))
            .comments(&reason),
    )
    .await?;
    tx.commit().await?;
    super::approvals::notify_approvers(&state, &ctx, approval).await;
    Ok(Json(json!({ "pending_approval": true, "approval_id": approval, "request_id": request })))
}

/// Every correction of a sale (requested, applied, returned, rejected, withdrawn), newest first.
async fn revisions(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require_any(&["sales.view", "sales.create"])?;
    super::sales::ensure_visible(&state, &ctx, id).await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', r.id, 'status', r.status, 'reason', r.reason, 'changes', r.changes, 'implications', r.implications,
                'revision_no', r.revision_no, 'number', r.number, 'requested_by', ru.name, 'requested_at', r.requested_at,
                'decided_by', du.name, 'decided_at', r.decided_at, 'decision_comments', r.decision_comments, 'approval_id', r.approval_id)
         FROM sale_edit_requests r LEFT JOIN users ru ON ru.id = r.requested_by LEFT JOIN users du ON du.id = r.decided_by
         WHERE r.sale_id = $1 AND r.tenant_id = $2 ORDER BY r.requested_at DESC",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "items": rows })))
}

// ───────────────────────────── Decisions ─────────────────────────────

/// Returned, rejected or withdrawn: the sale stays exactly as it was.
pub async fn on_closed(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow, outcome: &str, comments: &str) -> AppResult<()> {
    let status = match outcome {
        "returned" => "returned",
        "withdrawn" | "cancelled" => "withdrawn",
        _ => "rejected",
    };
    let request: Option<Uuid> = a.payload["request_id"].as_str().and_then(|s| s.parse().ok());
    sqlx::query(
        "UPDATE sale_edit_requests SET status = $3, decided_by = $4, decided_at = now(), decision_comments = $5
         WHERE id = $1 AND tenant_id = $2 AND status = 'awaiting'",
    )
    .bind(request)
    .bind(ctx.tenant_id)
    .bind(status)
    .bind(ctx.user_id)
    .bind(comments)
    .execute(&mut *conn)
    .await?;
    audit::record(conn, ctx, Entry::new("sales", &format!("edit_{status}"), "sale", a.entity_id).approval(Some(a.id)).after(json!({ "request": request })).comments(comments)).await
}

/// Final approval: the correction is applied — only the differences — to the sale exactly as it was reviewed.
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let request: Uuid = a.payload["request_id"].as_str().and_then(|s| s.parse().ok()).ok_or_else(|| bad("Stored sale correction is invalid"))?;
    let (sale_id, status, before, proposal, reason, requested_by): (Uuid, String, Value, DbJson<Proposal>, String, Option<Uuid>) = sqlx::query_as(
        "SELECT sale_id, status, before, proposed, reason, requested_by FROM sale_edit_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(request)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("Sale correction"))?;
    if status != "awaiting" {
        return Err(rule(format!("This correction is already {status}")));
    }
    let s = settings::load(&mut *conn, ctx.tenant_id).await?;
    let cur = load(conn, ctx, &s, sale_id).await?;
    if fingerprint(&cur) != before {
        return Err(refused("Outdated request", "The sale changed after this correction was requested — return it so it can be redone on the current sale"));
    }
    let p = proposal.0;
    let plan = plan(conn, ctx, &s, &cur, &p, true).await?;
    crate::receipts::issue_original(conn, ctx.tenant_id, sale_id, requested_by).await?;
    let revision = cur.revision + 1;
    let number = format!("{}-R{revision}", cur.receipt_no);

    // Roadmap 192: a barcode is reinstated only if it is still sold on this sale and no other item carries it now
    // (never reinstated twice, never a duplicate active barcode).
    for (_, _, item, qty, ..) in &plan.stock {
        if let (Some(i), true) = (item, *qty > 0) {
            let (status, code): (String, String) = sqlx::query_as("SELECT status, barcode FROM stock_items WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
                .bind(i)
                .bind(ctx.tenant_id)
                .fetch_one(&mut *conn)
                .await?;
            if status != "sold" {
                return Err(refused("Item already back", format!("Item {} is no longer recorded as sold ({status}) — it was already returned or reinstated. Return this correction.", super::barcodes::mask(&code))));
            }
            let clash: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM stock_items WHERE tenant_id = $1 AND barcode = $2 AND id <> $3 AND status IN ('in_stock','reserved','in_transit'))")
                .bind(ctx.tenant_id)
                .bind(&code)
                .bind(i)
                .fetch_one(&mut *conn)
                .await?;
            if clash {
                return Err(refused("Barcode in use", format!("Another item now carries barcode {} — it cannot be reinstated. Return this correction.", super::barcodes::mask(&code))));
            }
        }
    }
    // Stock: only the differences (or the whole sale moved).
    for (branch, product, item, qty, cost, price, day) in &plan.stock {
        let kind = if *qty > 0 { "sale_reversal" } else { "sale" };
        if let Some(i) = item {
            if *qty > 0 {
                sqlx::query("UPDATE stock_items SET status = 'in_stock', branch_id = $2, updated_at = now() WHERE id = $1 AND tenant_id = $3")
                    .bind(i)
                    .bind(branch)
                    .bind(ctx.tenant_id)
                    .execute(&mut *conn)
                    .await?;
            } else {
                sqlx::query("UPDATE stock_items SET status = 'sold', updated_at = now() WHERE id = $1 AND tenant_id = $2").bind(i).bind(ctx.tenant_id).execute(&mut *conn).await?;
            }
        }
        let m = Movement::new(*branch, *product, kind, *qty).item(*item).cost(*cost).price(Some(*price)).reference("sale_revision", sale_id).notes(&number).dated(*day);
        let check = if *qty < 0 { Check::Available } else { Check::None };
        inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), *qty > 0 || s.stock.allow_negative, check, m).await?;
    }
    if !s.stock.allow_negative {
        let mut checked: Vec<Uuid> = Vec::new();
        for (branch, product, _, qty, _, _, day) in &plan.stock {
            if let (true, Some(d)) = (*qty < 0, day) {
                if !checked.contains(product) {
                    checked.push(*product);
                    super::sales::check_stock_history(conn, ctx, *branch, *product, *d).await?;
                }
            }
        }
    }

    // Lines: removed, corrected, added (the sale as it was stays in the request and the original receipt).
    for c in &cur.lines {
        if !plan.lines.iter().any(|n| n.sale_item_id == Some(c.id)) {
            sqlx::query("DELETE FROM sale_items WHERE id = $1 AND sale_id = $2").bind(c.id).bind(sale_id).execute(&mut *conn).await?;
        }
    }
    for n in &plan.lines {
        match n.sale_item_id {
            Some(sid) => {
                sqlx::query("UPDATE sale_items SET quantity = $3, unit_price = $4, line_total = $5, points = $6, sale_date = NULL WHERE id = $1 AND sale_id = $2")
                    .bind(sid)
                    .bind(sale_id)
                    .bind(n.quantity)
                    .bind(n.unit_price)
                    .bind(n.line_total)
                    .bind(n.points)
                    .execute(&mut *conn)
                    .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO sale_items (tenant_id, sale_id, product_id, stock_item_id, barcode, quantity, marked_price, unit_price, line_total, unit_cost, points, sale_date, scan_source)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
                )
                .bind(ctx.tenant_id)
                .bind(sale_id)
                .bind(n.product_id)
                .bind(n.stock_item_id)
                .bind(&n.barcode)
                .bind(n.quantity)
                .bind(n.marked_price)
                .bind(n.unit_price)
                .bind(n.line_total)
                .bind(n.unit_cost)
                .bind(n.points)
                .bind(Option::<NaiveDate>::None)
                .bind(&n.scan_source)
                .execute(&mut *conn)
                .await?;
            }
        }
    }

    // Loyalty points and customer totals: the difference, or moved to the new customer.
    let today = ctx.today();
    let mut unrecovered = 0i64;
    if cur.customer_id == plan.customer_id {
        if let Some(c) = plan.customer_id {
            let delta = plan.points - cur.points_earned;
            unrecovered += adjust_points(conn, &s, ctx, sale_id, c, delta, &number, today).await?;
            if plan.total != cur.total {
                loyalty::record_purchase(conn, &s, c, plan.total - cur.total, 0).await?;
            }
        }
    } else {
        if let Some(old) = cur.customer_id {
            unrecovered += adjust_points(conn, &s, ctx, sale_id, old, -cur.points_earned, &number, today).await?;
            loyalty::record_purchase(conn, &s, old, -cur.total, -1).await?;
        }
        if let Some(new) = plan.customer_id {
            adjust_points(conn, &s, ctx, sale_id, new, plan.points, &number, today).await?;
            loyalty::record_purchase(conn, &s, new, plan.total, 1).await?;
        }
    }

    // Money: allocations move with the sale's branch and date; changed amounts are linked adjustments (never edits).
    let effective = if plan.date != cur.date { Some(plan.date) } else { None };
    if plan.branch_id != cur.branch_id {
        sqlx::query("UPDATE payments SET branch_id = $3 WHERE sale_id = $1 AND tenant_id = $2").bind(sale_id).bind(ctx.tenant_id).bind(plan.branch_id).execute(&mut *conn).await?;
    }
    if let Some(d) = effective {
        sqlx::query("UPDATE payments SET effective_date = $3 WHERE sale_id = $1 AND tenant_id = $2 AND mpesa_request_id IS NULL AND credit_sale_id IS NULL")
            .bind(sale_id)
            .bind(ctx.tenant_id)
            .bind(d)
            .execute(&mut *conn)
            .await?;
    }
    let pay_day = Some(plan.date);
    if cur.credit.is_none() {
        let mut rows: Vec<(String, Decimal, String)> = Vec::new();
        for g in &cur.payments {
            let next = plan.payments.iter().find(|x| x.0 == g.key).map(|x| x.3).unwrap_or(Decimal::ZERO);
            if next != g.amount {
                rows.push((g.method.clone(), next - g.amount, g.reference.clone()));
            }
        }
        for x in plan.payments.iter().filter(|x| !cur.payments.iter().any(|g| g.key == x.0)) {
            rows.push((x.1.clone(), x.3, x.2.clone()));
        }
        for (method, amount, reference) in rows {
            // Linked to the revision (`sale_edit_id`); the reference stays the allocation's own, so it nets with it.
            sqlx::query(
                "INSERT INTO payments (tenant_id, branch_id, sale_id, method, amount, reference, user_id, effective_date, sale_edit_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(ctx.tenant_id)
            .bind(plan.branch_id)
            .bind(sale_id)
            .bind(&method)
            .bind(amount)
            .bind(&reference)
            .bind(ctx.user_id)
            .bind(pay_day)
            .bind(request)
            .execute(&mut *conn)
            .await?;
        }
    } else {
        sqlx::query(
            "UPDATE credit_sales SET original_amount = $3, customer_id = $4, due_date = $5, branch_id = $6,
                 effective_date = COALESCE($7, effective_date),
                 status = CASE WHEN $3 - amount_paid - adjustments <= 0 THEN 'paid' WHEN amount_paid > 0 THEN 'partially_paid' ELSE 'outstanding' END
             WHERE sale_id = $1 AND tenant_id = $2",
        )
        .bind(sale_id)
        .bind(ctx.tenant_id)
        .bind(plan.total)
        .bind(plan.customer_id)
        .bind(plan.due_date)
        .bind(plan.branch_id)
        .bind(effective)
        .execute(&mut *conn)
        .await?;
    }

    // The sale: corrected in place; id, receipt number, recording time and recorder never change.
    let time = if plan.time != cur.time || plan.date != cur.date { Some(plan.time) } else { None };
    sqlx::query(
        "UPDATE sales SET branch_id = $3, customer_id = $4, owner_id = $5, notes = $6, effective_date = CASE WHEN $7 THEN $8 ELSE effective_date END,
             effective_time = COALESCE($9, effective_time), gross_total = $10, discount_total = $11, total = $12,
             amount_paid = CASE WHEN $13 THEN amount_paid ELSE $12 END, payment_method = $14, points_earned = $15, revision = $16
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(sale_id)
    .bind(ctx.tenant_id)
    .bind(plan.branch_id)
    .bind(plan.customer_id)
    .bind(plan.owner_id)
    .bind(&plan.notes)
    .bind(plan.date != cur.date)
    .bind(plan.date)
    .bind(time)
    .bind(plan.gross)
    .bind(round2(plan.gross - plan.net))
    .bind(plan.total)
    .bind(cur.credit.is_some())
    .bind(&plan.method)
    .bind(plan.points)
    .bind(revision)
    .execute(&mut *conn)
    .await?;

    let after_state = load(conn, ctx, &s, sale_id).await?;
    let after = fingerprint(&after_state);
    sqlx::query(
        "UPDATE sale_edit_requests SET status = 'applied', after = $2, revision_no = $3, number = $4, decided_by = $5, decided_at = now(),
             implications = implications || jsonb_build_object('points_unrecovered', $6::bigint)
         WHERE id = $1",
    )
    .bind(request)
    .bind(&after)
    .bind(revision)
    .bind(&number)
    .bind(ctx.user_id)
    .bind(unrecovered)
    .execute(&mut *conn)
    .await?;
    let changes: Value = a.payload["changes"].clone();
    crate::receipts::issue_revision(conn, ctx.tenant_id, sale_id, request, &number, json!({
        "number": number, "revision": revision, "original_receipt": cur.receipt_no, "reason": reason, "changes": changes,
        "approved_by": ctx.name, "approval_id": a.id,
    }), Some(ctx.user_id))
    .await?;
    audit::record(
        conn,
        ctx,
        Entry::new("sales", "edit", "sale", sale_id)
            .branch(plan.branch_id)
            .approval(Some(a.id))
            .before(before)
            .after(json!({ "sale": after, "changes": changes, "revision": revision, "number": number, "request": request, "requested_by": requested_by }))
            .comments(&reason),
    )
    .await
}

/// Changes a customer's points for this sale by `delta` (negative: taken back as far as they are still available,
/// with the matching share of any referral bonus). Returns the points that could not be taken back.
#[allow(clippy::too_many_arguments)]
async fn adjust_points(conn: &mut PgConnection, s: &TenantSettings, ctx: &Ctx, sale_id: Uuid, customer: Uuid, delta: i64, note: &str, today: NaiveDate) -> AppResult<i64> {
    if delta > 0 {
        loyalty::award_sale(conn, s, ctx.tenant_id, customer, sale_id, delta, Some(ctx.user_id), today).await?;
        return Ok(0);
    }
    if delta == 0 {
        return Ok(0);
    }
    let want = -delta;
    let earned: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(points), 0)::bigint FROM loyalty_ledger WHERE sale_id = $1 AND customer_id = $2 AND kind = 'earn'")
        .bind(sale_id)
        .bind(customer)
        .fetch_one(&mut *conn)
        .await?;
    let available: i64 = sqlx::query_scalar("SELECT points_available FROM customers WHERE id = $1 FOR UPDATE").bind(customer).fetch_one(&mut *conn).await?;
    let take = want.min(available.max(0)).min(earned.max(0));
    if take > 0 {
        // Recorded as negative earnings on the sale, so later returns reverse only what is left.
        sqlx::query("INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, notes, user_id) VALUES ($1,$2,'earn',$3,$4,$5,$6)")
            .bind(ctx.tenant_id)
            .bind(customer)
            .bind(-take)
            .bind(sale_id)
            .bind(format!("Sale correction {note}"))
            .bind(ctx.user_id)
            .execute(&mut *conn)
            .await?;
        sqlx::query("UPDATE customers SET own_points = GREATEST(own_points - $2, 0), points_available = points_available - $2 WHERE id = $1")
            .bind(customer)
            .bind(take)
            .execute(&mut *conn)
            .await?;
        // The referrer's bonus on these points, in proportion.
        let referrals: Vec<(Uuid, i64, Option<Uuid>)> = sqlx::query_as(
            "SELECT customer_id, SUM(points)::bigint, MAX(referral_id::text)::uuid FROM loyalty_ledger WHERE sale_id = $1 AND kind = 'referral'
               AND referral_id IN (SELECT id FROM referrals WHERE referred_id = $2) GROUP BY customer_id",
        )
        .bind(sale_id)
        .bind(customer)
        .fetch_all(&mut *conn)
        .await?;
        for (referrer, pts, referral) in referrals {
            let share = if earned > 0 { pts * take / earned } else { 0 };
            let avail: i64 = sqlx::query_scalar("SELECT points_available FROM customers WHERE id = $1 FOR UPDATE").bind(referrer).fetch_one(&mut *conn).await?;
            let t = share.min(avail.max(0));
            if t <= 0 {
                continue;
            }
            sqlx::query("INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, referral_id, notes, user_id) VALUES ($1,$2,'referral',$3,$4,$5,$6,$7)")
                .bind(ctx.tenant_id)
                .bind(referrer)
                .bind(-t)
                .bind(sale_id)
                .bind(referral)
                .bind(format!("Sale correction {note}"))
                .bind(ctx.user_id)
                .execute(&mut *conn)
                .await?;
            sqlx::query("UPDATE customers SET referral_points = GREATEST(referral_points - $2, 0), points_available = points_available - $2 WHERE id = $1")
                .bind(referrer)
                .bind(t)
                .execute(&mut *conn)
                .await?;
            if let Some(r) = referral {
                sqlx::query("UPDATE referrals SET bonus_points_earned = GREATEST(bonus_points_earned - $2, 0) WHERE id = $1").bind(r).bind(t).execute(&mut *conn).await?;
            }
        }
    }
    Ok(want - take)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_keeps_cents_only_when_there_are_some() {
        assert_eq!(money("KES", Decimal::from(5000)), "KES 5,000");
        assert_eq!(money("KES", Decimal::new(550050, 2)), "KES 5,500.50");
        assert_eq!(pay_key("mpesa", " qft1abc2de ", None), "mpesa|QFT1ABC2DE|");
    }
}
