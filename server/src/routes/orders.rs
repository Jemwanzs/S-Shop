//! Order management (staff side).
//! New → Confirmed → Preparing → Dispatched → On Delivery → Delivered → Completed,
//! plus Cancelled / Rejected / Returned. Confirmed orders reserve stock
//! (Physical − Reserved = Available); at the configured stage the order becomes
//! a sale and stock is cleared.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Counted, Page, Paged, Period};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
use crate::inventory;
use crate::notify::{self, Note};
use crate::settings::{self, TenantSettings};
use crate::state::AppState;
use crate::util::{money_str, next_doc_no, parse_tz, round2};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orders", get(list).post(create))
        .route("/orders/summary", get(summary))
        .route("/orders/{id}", get(detail))
        .route("/orders/{id}/status", post(change_status))
}

pub const FLOW: [&str; 7] = ["new", "confirmed", "preparing", "dispatched", "on_delivery", "delivered", "completed"];

fn rank(s: &str) -> Option<usize> {
    FLOW.iter().position(|f| *f == s)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub order_no: String,
    pub status: String,
    pub source: String,
    pub branch_id: Uuid,
    pub branch_name: String,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub customer_mobile: String,
    pub delivery_location: String,
    pub notes: String,
    pub total: Decimal,
    pub item_count: i64,
    pub reserved: bool,
    pub sale_id: Option<Uuid>,
    pub receipt_no: Option<String>,
    pub track_token: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT: &str = "SELECT o.id, o.order_no, o.status, o.source, o.branch_id, b.name AS branch_name, o.customer_id,
        TRIM(c.first_name || ' ' || c.other_names) AS customer_name, c.mobile AS customer_mobile, o.delivery_location, o.notes,
        o.total, (SELECT COALESCE(SUM(quantity),0) FROM order_items oi WHERE oi.order_id = o.id)::bigint AS item_count,
        o.reserved, o.sale_id, s.receipt_no, o.track_token, o.created_at, o.updated_at
    FROM orders o JOIN branches b ON b.id = o.branch_id JOIN customers c ON c.id = o.customer_id
    LEFT JOIN sales s ON s.id = o.sale_id";

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    /// a status, "active" (not finished) or "all"
    status: Option<String>,
    branch_id: Option<Uuid>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Paged<OrderRow>>> {
    ctx.require("orders.view")?;
    // Orders scope (roadmap 64): orders the user created or whose sale is credited to them, their branches, or all.
    let vis = ctx.visibility(&mut *state.db.acquire().await?, "orders", q.branch_id, None).await?;
    let branches = vis.branches;
    let (from, to) = q.period.resolve(ctx.today(), "all");
    let select = SELECT.replacen("SELECT", "SELECT COUNT(*) OVER() AS total_count,", 1);
    let rows: Vec<Counted<OrderRow>> = sqlx::query_as(&format!(
        "{select} WHERE o.tenant_id = $1 AND o.branch_id = ANY($2)
           AND (CASE $3 WHEN 'all' THEN true
                        WHEN 'active' THEN o.status NOT IN ('completed','cancelled','rejected','returned')
                        ELSE o.status = $3 END)
           AND ($4::text IS NULL OR o.order_no ILIKE $4 OR c.first_name ILIKE $4 OR c.mobile ILIKE $4)
           AND o.business_date BETWEEN $5 AND $6
           AND ($9::uuid IS NULL OR o.created_by = $9 OR EXISTS (SELECT 1 FROM sales ss WHERE ss.id = o.sale_id AND ss.owner_id = $9))
         ORDER BY (o.status = 'new') DESC, o.created_at DESC LIMIT $7 OFFSET $8"
    ))
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(q.status.as_deref().unwrap_or("active"))
    .bind(like(&q.q))
    .bind(from)
    .bind(to)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .bind(vis.owner)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into()))
}

/// Counts per status for the tab badges.
async fn summary(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("orders.view")?;
    let vis = ctx.visibility(&mut *state.db.acquire().await?, "orders", None, None).await?;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM orders o WHERE tenant_id = $1 AND branch_id = ANY($2)
           AND ($3::uuid IS NULL OR o.created_by = $3 OR EXISTS (SELECT 1 FROM sales ss WHERE ss.id = o.sale_id AND ss.owner_id = $3))
         GROUP BY status",
    )
    .bind(ctx.tenant_id)
    .bind(&vis.branches)
    .bind(vis.owner)
    .fetch_all(&state.db)
    .await?;
    let map: serde_json::Map<String, Value> = rows.into_iter().map(|(s, n)| (s, json!(n))).collect();
    Ok(Json(Value::Object(map)))
}

async fn load(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, lock: bool) -> AppResult<OrderRow> {
    let lock = if lock { " FOR UPDATE OF o" } else { "" };
    let o: OrderRow = sqlx::query_as(&format!("{SELECT} WHERE o.id = $1 AND o.tenant_id = $2{lock}"))
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Order"))?;
    ctx.ensure_branch(o.branch_id)?;
    Ok(o)
}

/// One order for viewing, within the user's orders scope (operations still need the branch — see `load`).
async fn load_visible(conn: &mut PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<OrderRow> {
    let o: OrderRow = sqlx::query_as(&format!("{SELECT} WHERE o.id = $1 AND o.tenant_id = $2"))
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Order"))?;
    let mine: Option<bool> = sqlx::query_scalar(
        "SELECT o.created_by = $2 OR EXISTS (SELECT 1 FROM sales s WHERE s.id = o.sale_id AND s.owner_id = $2) FROM orders o WHERE o.id = $1",
    )
    .bind(id)
    .bind(ctx.user_id)
    .fetch_one(&mut *conn)
    .await?;
    if !ctx.may_view("orders", o.branch_id, mine.unwrap_or(false)) {
        return Err(AppError::Forbidden("This order is outside the orders you can see".into()));
    }
    Ok(o)
}

pub async fn order_items(conn: &mut PgConnection, order_id: Uuid) -> AppResult<Vec<(Uuid, String, i32, Decimal, Decimal, Option<Uuid>)>> {
    Ok(sqlx::query_as(
        "SELECT oi.product_id, p.name, oi.quantity, oi.unit_price, oi.line_total,
                (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1)
         FROM order_items oi JOIN products p ON p.id = oi.product_id WHERE oi.order_id = $1 ORDER BY p.name",
    )
    .bind(order_id)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn order_events(conn: &mut PgConnection, s: &TenantSettings, order_id: Uuid) -> AppResult<Vec<Value>> {
    let rows: Vec<(String, String, DateTime<Utc>, Option<String>)> = sqlx::query_as(
        "SELECT e.status, e.notes, e.created_at, u.name FROM order_events e LEFT JOIN users u ON u.id = e.user_id
         WHERE e.order_id = $1 ORDER BY e.created_at",
    )
    .bind(order_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(status, notes, at, user)| json!({ "label": s.order_label(&status), "status": status, "notes": notes, "created_at": at, "user_name": user }))
        .collect())
}

/// Forward steps the business uses, plus cancel/reject while no sale exists.
fn next_statuses(o: &OrderRow, s: &TenantSettings) -> Vec<&'static str> {
    let mut out = Vec::new();
    if let Some(r) = rank(&o.status) {
        out.extend(FLOW.iter().skip(r + 1).copied().filter(|f| s.order_status_enabled(f)));
        if o.sale_id.is_none() {
            if o.status == "new" {
                out.push("rejected");
            }
            out.push("cancelled");
        }
    }
    out
}

async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("orders.view")?;
    let mut conn = state.db.acquire().await?;
    let o = load_visible(&mut conn, &ctx, id).await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;
    let items = order_items(&mut conn, id).await?;
    let mut lines = Vec::new();
    for (pid, name, qty, price, total, photo) in items {
        let level = sqlx::query_as::<_, (i32, i32)>("SELECT on_hand, reserved FROM stock_levels WHERE branch_id = $1 AND product_id = $2")
            .bind(o.branch_id)
            .bind(pid)
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or((0, 0));
        let tracked: bool = sqlx::query_scalar("SELECT track_items FROM products WHERE id = $1").bind(pid).fetch_one(&mut *conn).await?;
        lines.push(json!({
            "product_id": pid, "product_name": name, "quantity": qty, "unit_price": price, "line_total": total,
            "photo_id": photo, "on_hand": level.0, "available": level.0 - level.1, "track_items": tracked,
        }));
    }
    let events = order_events(&mut conn, &s, id).await?;
    let next = if ctx.can("orders.manage") { next_statuses(&o, &s) } else { vec![] };
    Ok(Json(json!({
        "order": o,
        "items": lines,
        "events": events,
        "next_statuses": next,
        "sale_on_status": s.orders.sale_on_status,
        "track_url": format!("{}/track/{}", state.cfg.public_url, o.track_token),
    })))
}

#[derive(Deserialize)]
pub struct OrderLine {
    pub product_id: Uuid,
    pub quantity: i32,
}

/// Creates an order with current marked prices. Shared by staff and the portal.
pub async fn create_order(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    branch_id: Uuid,
    customer_id: Uuid,
    lines: &[OrderLine],
    delivery_location: &str,
    notes: &str,
    source: &str,
    user_id: Option<Uuid>,
) -> AppResult<(Uuid, String, Uuid, Decimal)> {
    if lines.is_empty() {
        return Err(bad("Your cart is empty"));
    }
    let tz: String = sqlx::query_scalar("SELECT timezone FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(&mut *conn).await?;
    let order_no = next_doc_no(conn, tenant_id, "ORD", parse_tz(&tz)).await?;
    let id: Uuid = Uuid::new_v4();
    let mut total = Decimal::ZERO;
    let mut prepared = Vec::new();
    for l in lines {
        if l.quantity <= 0 || l.quantity > 999 {
            return Err(bad("Choose a valid quantity"));
        }
        let (name, price, active, for_orders): (String, Decimal, bool, bool) = sqlx::query_as(
            "SELECT name, marked_price, is_active, available_for_orders FROM products WHERE id = $1 AND tenant_id = $2",
        )
        .bind(l.product_id)
        .bind(tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
        if !active || (source == "portal" && !for_orders) {
            return Err(rule(format!("{name} is not available for ordering")));
        }
        inventory::ensure_product_in_branch(conn, tenant_id, l.product_id, branch_id).await?;
        let level = inventory::lock(conn, tenant_id, branch_id, l.product_id).await?;
        if level.available() < l.quantity {
            return Err(rule(format!("Only {} × {name} in stock", level.available().max(0))));
        }
        let line_total = round2(price * Decimal::from(l.quantity));
        total += line_total;
        prepared.push((l.product_id, l.quantity, price, line_total));
    }
    let track_token: Uuid = sqlx::query_scalar(
        "INSERT INTO orders (id, tenant_id, branch_id, order_no, customer_id, source, delivery_location, notes, total, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING track_token",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(branch_id)
    .bind(&order_no)
    .bind(customer_id)
    .bind(source)
    .bind(delivery_location.trim())
    .bind(notes.trim())
    .bind(total)
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    for (pid, qty, price, line_total) in prepared {
        sqlx::query("INSERT INTO order_items (order_id, product_id, quantity, unit_price, line_total) VALUES ($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(pid)
            .bind(qty)
            .bind(price)
            .bind(line_total)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("INSERT INTO order_events (order_id, status, user_id, notes) VALUES ($1, 'new', $2, $3)")
        .bind(id)
        .bind(user_id)
        .bind(if source == "portal" { "Placed via ordering link" } else { "Created by staff" })
        .execute(&mut *conn)
        .await?;
    Ok((id, order_no, track_token, total))
}

/// Tell order staff about a new order.
pub async fn announce_new(state: &AppState, tenant_id: Uuid, branch_id: Uuid, order_id: Uuid, order_no: &str, customer: &str, total: Decimal) {
    // Roadmap 69: who, where, how much — one notification per order and person, however often it is announced.
    let (branch, items, source): (String, i64, String) = sqlx::query_as(
        "SELECT b.name, COALESCE((SELECT SUM(quantity) FROM order_items WHERE order_id = o.id), 0)::bigint, o.source
         FROM orders o JOIN branches b ON b.id = o.branch_id WHERE o.id = $1",
    )
    .bind(order_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| (String::new(), 0, String::new()));
    let title = if source == "internal" { format!("New order {order_no}") } else { format!("New customer order {order_no}") };
    let mut body = format!("Customer: {customer}");
    if !branch.is_empty() {
        body.push_str(&format!(" · {branch}"));
    }
    body.push_str(&format!(" · {items} item{} · KSh {}", if items == 1 { "" } else { "s" }, money_str(total)));
    notify::to_order_staff(state, tenant_id, branch_id, Note::new("new_order", title, body, format!("/orders/{order_id}")).dedupe(format!("order:{order_id}"))).await;
    state.emit(tenant_id, None, "order", json!({ "id": order_id, "status": "new" }));
}

#[derive(Deserialize)]
struct CreateBody {
    branch_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    customer: Option<super::sales::CustomerInput>,
    items: Vec<OrderLine>,
    #[serde(default)]
    delivery_location: String,
    #[serde(default)]
    notes: String,
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CreateBody>) -> AppResult<Json<Value>> {
    ctx.require("orders.manage")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "orders").await?;
    let branch = ctx.branch_or_current(b.branch_id)?;
    let mut tx = state.db.begin().await?;
    let customer_id = match (b.customer_id, &b.customer) {
        (Some(id), _) => id,
        (None, Some(c)) => super::customers::upsert_by_mobile(&mut tx, ctx.tenant_id, Some(ctx.user_id), &c.mobile, &c.first_name, &c.nickname).await?.0,
        _ => return Err(bad("Choose the customer")),
    };
    let (id, order_no, _, total) =
        create_order(&mut tx, ctx.tenant_id, branch, customer_id, &b.items, &b.delivery_location, &b.notes, "internal", Some(ctx.user_id)).await?;
    audit::record(&mut tx, &ctx, Entry::new("orders", "create", "order", id).branch(branch).after(json!({ "order_no": order_no, "total": total })))
        .await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "order", json!({ "id": id, "status": "new" }));
    Ok(Json(json!({ "id": id, "order_no": order_no })))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
    #[serde(default)]
    notes: String,
    /// Required when the order becomes a sale.
    payment: Option<super::sales::PaymentInput>,
    /// Individually tracked products: the barcode of every unit handed over, scanned at fulfilment.
    #[serde(default)]
    barcodes: Vec<ItemBarcodes>,
}

#[derive(Deserialize)]
struct ItemBarcodes {
    product_id: Uuid,
    barcodes: Vec<String>,
}

async fn change_status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<StatusBody>) -> AppResult<Json<Value>> {
    ctx.require("orders.manage")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "orders").await?;
    let mut tx = state.db.begin().await?;
    let o = load(&mut tx, &ctx, id, true).await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let target = b.status.as_str();
    if !next_statuses(&o, &s).contains(&target) {
        return Err(rule(format!("An order that is {} cannot move to {}", s.order_label(&o.status), s.order_label(target))));
    }
    let items = order_items(&mut tx, id).await?;
    let mut reserved = o.reserved;
    let mut sale_id = o.sale_id;

    match target {
        "cancelled" | "rejected" => {
            if reserved {
                for (pid, _, qty, ..) in &items {
                    inventory::release(&mut tx, ctx.tenant_id, o.branch_id, *pid, *qty).await?;
                }
                reserved = false;
            }
        }
        _ => {
            let target_rank = rank(target).unwrap_or(0);
            // Confirming (or jumping past confirmation) reserves stock.
            if s.orders.reserve_stock && !reserved && sale_id.is_none() && target_rank >= 1 {
                for (pid, _, qty, ..) in &items {
                    inventory::reserve(&mut tx, ctx.tenant_id, o.branch_id, *pid, *qty, s.stock.allow_negative).await?;
                }
                reserved = true;
            }
            // Reaching the configured stage turns the order into a sale.
            let sale_rank = rank(&s.orders.sale_on_status).unwrap_or(5);
            if sale_id.is_none() && target_rank >= sale_rank {
                let payment = b.payment.clone().ok_or_else(|| rule("Record how the customer paid to complete this order"))?;
                if reserved {
                    for (pid, _, qty, ..) in &items {
                        inventory::release(&mut tx, ctx.tenant_id, o.branch_id, *pid, *qty).await?;
                    }
                    reserved = false;
                }
                // Tracked products become one line per scanned unit; record_sale verifies each barcode is
                // in stock at the order's branch and belongs to the product.
                let mut lines = Vec::new();
                for (pid, _, qty, price, ..) in &items {
                    let (name, tracked): (String, bool) = sqlx::query_as("SELECT name, track_items FROM products WHERE id = $1")
                        .bind(pid)
                        .fetch_one(&mut *tx)
                        .await?;
                    if !tracked {
                        lines.push(super::sales::LineInput { product_id: *pid, quantity: *qty, unit_price: *price, barcode: None });
                        continue;
                    }
                    let mut codes: Vec<String> = b
                        .barcodes
                        .iter()
                        .filter(|x| x.product_id == *pid)
                        .flat_map(|x| x.barcodes.iter().map(|c| c.trim().to_string()))
                        .filter(|c| !c.is_empty())
                        .collect();
                    codes.sort();
                    codes.dedup();
                    if codes.len() != *qty as usize {
                        return Err(rule(format!("Scan {qty} barcode(s) for {name} — {} scanned", codes.len())));
                    }
                    for code in codes {
                        lines.push(super::sales::LineInput { product_id: *pid, quantity: 1, unit_price: *price, barcode: Some(code) });
                    }
                }
                // Orders are fulfilled at the order's branch on behalf of the user progressing it.
                let mut sale_ctx = ctx.clone();
                sale_ctx.branch_id = o.branch_id;
                let new_sale = super::sales::record_sale(
                    &mut tx,
                    &sale_ctx,
                    &s,
                    super::sales::SaleInput {
                        branch_id: o.branch_id,
                        customer_id: Some(o.customer_id),
                        lines,
                        payment,
                        redeem_points: 0,
                        due_date: None,
                        deposit: None,
                        notes: format!("Order {}", o.order_no),
                        approved_by: None,
                        order_id: Some(id),
                        client_ref: None,
                        exchange: None,
                        owner_id: None,
                    },
                    true,
                )
                .await?;
                // The order's sale gets its receipt in the same transaction (roadmap 65).
                crate::receipts::issue_original(&mut tx, ctx.tenant_id, new_sale, Some(ctx.user_id)).await?;
                sale_id = Some(new_sale);
            }
        }
    }

    sqlx::query("UPDATE orders SET status=$2, reserved=$3, sale_id=$4, updated_at=now() WHERE id=$1 AND tenant_id = $5")
        .bind(id)
        .bind(target)
        .bind(reserved)
        .bind(sale_id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO order_events (order_id, status, user_id, notes) VALUES ($1,$2,$3,$4)")
        .bind(id)
        .bind(target)
        .bind(ctx.user_id)
        .bind(b.notes.trim())
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("orders", "status", "order", id)
            .branch(o.branch_id)
            .before(json!({ "status": o.status }))
            .after(json!({ "status": target, "sale_id": sale_id }))
            .comments(b.notes.trim()),
    )
    .await?;
    tx.commit().await?;

    state.emit(ctx.tenant_id, None, "order", json!({ "id": id, "status": target }));
    if let (Some(new_sale), None) = (sale_id, o.sale_id) {
        let products: Vec<Uuid> = items.iter().map(|i| i.0).collect();
        super::sales::after_sale(&state, &ctx, &s, new_sale, o.branch_id, &products).await;
    }
    if s.orders.notify_customer_whatsapp {
        let business: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
        let text = format!(
            "Hi {}! Your {business} order {} is now *{}*. Track it here: {}/track/{}",
            o.customer_name.split(' ').next().unwrap_or_default(),
            o.order_no,
            s.order_label(target),
            state.cfg.public_url,
            o.track_token
        );
        notify::whatsapp(&state, ctx.tenant_id, o.customer_mobile.clone(), text);
    }
    Ok(Json(json!({ "status": target, "sale_id": sale_id })))
}
