//! Stock & inventory: levels, receiving, adjustments, stock counts, barcode
//! items, movement history and the stock position report. All changes go
//! through `inventory::apply` (ledger + locked projection).

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Counted, Outcome, Page, Paged, Period};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
use crate::inventory::{self, Check, Movement};
use crate::notify::{self, Note};
use crate::routes::approvals::ApprovalRow;
use crate::settings::{self, BarcodeRequirement, QuantityEntry, TenantSettings, Valuation};
use crate::state::AppState;
use crate::util::local_range;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stock", get(levels))
        .route("/stock/availability/{product_id}", get(availability))
        .route("/stock/receive", post(receive))
        .route("/stock/movements", get(movements))
        .route("/stock/items", get(items))
        .route("/stock/barcode/{code}", get(barcode_history))
        .route("/stock/adjustments", get(list_adjustments).post(create_adjustment))
        .route("/stock/count", post(stock_count))
        .route("/stock/position", get(position))
}

// ───────────────────────────── Levels ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct LevelRow {
    pub product_id: Uuid,
    pub code: String,
    pub name: String,
    pub nickname: String,
    pub category_name: Option<String>,
    pub track_items: bool,
    pub is_active: bool,
    pub marked_price: Decimal,
    pub cost_price: Option<Decimal>,
    pub on_hand: i32,
    pub reserved: i32,
    pub available: i32,
    pub low_threshold: i32,
    /// None when valued at cost and costs are hidden from this user.
    pub value: Option<Decimal>,
    pub primary_photo_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct LevelQuery {
    q: Option<String>,
    category_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    /// in | low | out | all
    status: Option<String>,
    #[serde(flatten)]
    page: Page,
}

fn value_expr(v: Valuation) -> &'static str {
    match v {
        Valuation::Cost => "COALESCE(p.cost_price, p.marked_price)",
        Valuation::Selling => "p.marked_price",
    }
}

async fn levels(State(state): State<AppState>, ctx: Ctx, Query(q): Query<LevelQuery>) -> AppResult<Json<Paged<LevelRow>>> {
    ctx.require("stock.view")?;
    let branch = ctx.branch_or_current(q.branch_id)?;
    let mut conn = state.db.acquire().await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;
    let price = value_expr(s.stock.valuation);
    let mut rows: Vec<Counted<LevelRow>> = sqlx::query_as(&format!(
        "SELECT COUNT(*) OVER() AS total_count, p.id AS product_id, p.code, p.name, p.nickname, c.name AS category_name,
                p.track_items, p.is_active, p.marked_price, p.cost_price,
                COALESCE(sl.on_hand,0) AS on_hand, COALESCE(sl.reserved,0) AS reserved,
                COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) AS available,
                COALESCE(p.low_stock_threshold, $4) AS low_threshold,
                (GREATEST(COALESCE(sl.on_hand,0),0) * {price})::numeric(14,2) AS value,
                (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1) AS primary_photo_id
         FROM products p
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $2
         WHERE p.tenant_id = $1
           AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = $2))
           AND ($3::text IS NULL OR p.name ILIKE $3 OR p.code ILIKE $3 OR p.nickname ILIKE $3 OR p.barcode ILIKE $3)
           AND ($5::uuid IS NULL OR p.category_id = $5)
           AND (CASE $6
                  WHEN 'out' THEN COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) <= 0
                  WHEN 'low' THEN COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) > 0
                              AND COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) <= COALESCE(p.low_stock_threshold, $4)
                  WHEN 'in' THEN COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) > 0
                  ELSE true END)
         ORDER BY p.is_active DESC, p.name LIMIT $7 OFFSET $8"
    ))
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(like(&q.q))
    .bind(s.stock.low_stock_threshold)
    .bind(q.category_id)
    .bind(q.status.as_deref().unwrap_or("all"))
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&mut *conn)
    .await?;
    if super::costs_hidden(&mut conn, &ctx).await? {
        let by_cost = s.stock.valuation == Valuation::Cost;
        for r in rows.iter_mut() {
            r.row.cost_price = None;
            if by_cost {
                r.row.value = None;
            }
        }
    }
    Ok(Json(rows.into()))
}

/// Cross-branch visibility: available stock per branch, Current Branch first.
async fn availability(State(state): State<AppState>, ctx: Ctx, Path(product_id): Path<Uuid>) -> AppResult<Json<Vec<Value>>> {
    ctx.require_any(&["stock.view", "sales.create"])?;
    let rows: Vec<(Uuid, String, i32, i32)> = sqlx::query_as(
        "SELECT b.id, b.name, COALESCE(sl.on_hand,0), COALESCE(sl.reserved,0)
         FROM branches b
         JOIN products p ON p.id = $2 AND p.tenant_id = $1
         LEFT JOIN stock_levels sl ON sl.branch_id = b.id AND sl.product_id = p.id
         WHERE b.tenant_id = $1 AND b.is_active
           AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = b.id))
         ORDER BY (b.id = $3) DESC, b.name",
    )
    .bind(ctx.tenant_id)
    .bind(product_id)
    .bind(ctx.branch_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, on_hand, reserved)| {
                json!({ "branch_id": id, "branch_name": name, "available": (on_hand - reserved).max(0), "on_hand": on_hand,
                        "is_current": id == ctx.branch_id, "accessible": ctx.has_branch(id) })
            })
            .collect(),
    ))
}

// ───────────────────────────── Receive stock ─────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
pub struct ReceiveBody {
    pub product_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub quantity: i32,
    #[serde(default)]
    pub barcodes: Vec<String>,
    pub cost_price: Option<Decimal>,
    pub marked_price: Option<Decimal>,
    pub max_discount: Option<Decimal>,
    pub supplier_id: Option<Uuid>,
    #[serde(default)]
    pub reference: String,
    pub date_received: Option<NaiveDate>,
    #[serde(default)]
    pub activate: bool,
    /// "received" (default) or "opening" for first-time stock loads.
    pub kind: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ProductInfo {
    name: String,
    barcode: Option<String>,
    track_items: bool,
    is_active: bool,
    marked_price: Decimal,
    cost_price: Option<Decimal>,
}

async fn product_info(conn: &mut PgConnection, tenant_id: Uuid, id: Uuid) -> AppResult<ProductInfo> {
    sqlx::query_as("SELECT name, barcode, track_items, is_active, marked_price, cost_price FROM products WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Product"))
}

/// Validates a receipt against settings; normalises barcodes. Shared by request and approval execution.
async fn check_receipt(conn: &mut PgConnection, ctx: &Ctx, s: &TenantSettings, b: &mut ReceiveBody, p: &ProductInfo) -> AppResult<()> {
    if b.quantity <= 0 {
        return Err(bad("Quantity must be at least 1"));
    }
    if s.stock.quantity_entry == QuantityEntry::Locked && b.quantity != 1 {
        return Err(rule("Quantity entry is locked: capture each item individually"));
    }
    b.barcodes = b.barcodes.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
    if s.stock.barcode_requirement == BarcodeRequirement::Disabled {
        b.barcodes.clear();
    }
    if p.track_items {
        if b.barcodes.len() != b.quantity as usize {
            return Err(rule(format!("Scan one barcode per item: {} scanned for {} item(s)", b.barcodes.len(), b.quantity)));
        }
        let mut sorted = b.barcodes.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != b.barcodes.len() {
            return Err(rule("The same barcode was scanned twice"));
        }
        let taken: Vec<String> = sqlx::query_scalar(
            "SELECT barcode FROM stock_items WHERE tenant_id=$1 AND barcode = ANY($2) AND status IN ('in_stock','reserved','in_transit')
             UNION SELECT barcode FROM products WHERE tenant_id=$1 AND barcode = ANY($2)",
        )
        .bind(ctx.tenant_id)
        .bind(&b.barcodes)
        .fetch_all(&mut *conn)
        .await?;
        if !taken.is_empty() {
            return Err(rule(format!("Barcode already in use: {}", taken.join(", "))));
        }
    } else {
        if b.barcodes.len() > 1 {
            return Err(rule("This product uses one shared barcode — scan a single code"));
        }
        let scanned = b.barcodes.first();
        if s.stock.barcode_requirement == BarcodeRequirement::Required && scanned.is_none() && p.barcode.is_none() {
            return Err(rule("A barcode is required for this product"));
        }
        if let (Some(scanned), Some(existing)) = (scanned, &p.barcode) {
            if scanned != existing {
                return Err(rule(format!("Scanned barcode does not match {} ({existing})", p.name)));
            }
        }
    }
    if let (Some(mp), Some(md)) = (b.marked_price.or(Some(p.marked_price)), b.max_discount) {
        if md > mp {
            return Err(bad("Maximum discount cannot exceed the marked price"));
        }
    }
    if b.activate && !p.is_active && !ctx.can("products.deactivate") {
        return Err(AppError::Forbidden("You cannot activate products".into()));
    }
    match b.kind.as_deref() {
        None | Some("received") | Some("opening") => Ok(()),
        _ => Err(bad("Unknown receipt type")),
    }
}

async fn receive(State(state): State<AppState>, ctx: Ctx, Json(mut b): Json<ReceiveBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("stock.add")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "stock").await?;
    let branch = ctx.branch_or_current(b.branch_id)?;
    b.branch_id = Some(branch);
    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    inventory::ensure_product_in_branch(&mut tx, ctx.tenant_id, b.product_id, branch).await?;
    let p = product_info(&mut tx, ctx.tenant_id, b.product_id).await?;
    if !s.stock.capture_cost {
        b.cost_price = None;
    }
    check_receipt(&mut tx, &ctx, &s, &mut b, &p).await?;

    let unit_value = b.cost_price.or(p.cost_price).unwrap_or(b.marked_price.unwrap_or(p.marked_price));
    let amount = unit_value * Decimal::from(b.quantity);
    if workflow::needs_approval(&mut tx, &ctx, "stock.add", workflow::Gate::branch(branch).amount(amount)).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "stock.add",
                entity_type: "product",
                entity_id: b.product_id,
                branch_id: Some(branch),
                summary: format!("Receive {} × {}", b.quantity, p.name),
                amount: Some(amount),
                payload: serde_json::to_value(&b).unwrap_or_default(),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    let result = execute_receipt(&mut tx, &ctx, &b, None).await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": branch, "product_id": b.product_id }));
    Ok(Json(Outcome::done(result)))
}

async fn execute_receipt(conn: &mut PgConnection, ctx: &Ctx, b: &ReceiveBody, approval_id: Option<Uuid>) -> AppResult<Value> {
    let branch = b.branch_id.unwrap_or(ctx.branch_id);
    let p = product_info(conn, ctx.tenant_id, b.product_id).await?;
    let kind = b.kind.as_deref().unwrap_or("received");
    let marked = b.marked_price.unwrap_or(p.marked_price);

    // Price changes captured with the receipt update the catalogue (audited below).
    sqlx::query(
        "UPDATE products SET marked_price = COALESCE($3, marked_price), max_discount = COALESCE($4, max_discount),
                cost_price = COALESCE($5, cost_price), barcode = COALESCE(barcode, $6),
                is_active = is_active OR $7, updated_at = now()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(b.product_id)
    .bind(ctx.tenant_id)
    .bind(b.marked_price)
    .bind(b.max_discount)
    .bind(b.cost_price)
    .bind(if p.track_items { None } else { b.barcodes.first().cloned() })
    .bind(b.activate)
    .execute(&mut *conn)
    .await?;

    let notes = if b.reference.is_empty() { String::new() } else { format!("Ref: {}", b.reference) };
    let mut level = None;
    if p.track_items {
        for code in &b.barcodes {
            let item_id: Uuid = sqlx::query_scalar(
                "INSERT INTO stock_items (tenant_id, product_id, branch_id, barcode, cost_price) VALUES ($1,$2,$3,$4,$5) RETURNING id",
            )
            .bind(ctx.tenant_id)
            .bind(b.product_id)
            .bind(branch)
            .bind(code)
            .bind(b.cost_price)
            .fetch_one(&mut *conn)
            .await?;
            let mut m = Movement::new(branch, b.product_id, kind, 1).item(Some(item_id)).cost(b.cost_price).price(Some(marked)).notes(&notes);
            m.supplier_id = b.supplier_id;
            m.occurred_on = b.date_received;
            level = Some(inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), true, Check::None, m).await?.1);
        }
    } else {
        let mut m = Movement::new(branch, b.product_id, kind, b.quantity).cost(b.cost_price).price(Some(marked)).notes(&notes);
        m.supplier_id = b.supplier_id;
        m.occurred_on = b.date_received;
        level = Some(inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), true, Check::None, m).await?.1);
    }
    let level = level.expect("at least one movement");

    audit::record(
        conn,
        ctx,
        Entry::new("stock", "receive", "product", b.product_id)
            .branch(branch)
            .before(json!({ "marked_price": p.marked_price, "cost_price": p.cost_price }))
            .after(b)
            .approval(approval_id),
    )
    .await?;
    Ok(json!({ "product_id": b.product_id, "branch_id": branch, "on_hand": level.on_hand, "available": level.available() }))
}

// ───────────────────────────── Movements & items ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct MovementRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    business_date: NaiveDate,
    occurred_on: NaiveDate,
    branch_name: String,
    product_id: Uuid,
    product_name: String,
    kind: String,
    quantity: i32,
    unit_cost: Option<Decimal>,
    unit_price: Option<Decimal>,
    barcode: Option<String>,
    ref_type: Option<String>,
    ref_id: Option<Uuid>,
    notes: String,
    user_name: Option<String>,
}

#[derive(Deserialize)]
struct MovementQuery {
    branch_id: Option<Uuid>,
    product_id: Option<Uuid>,
    kind: Option<String>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

async fn movements(State(state): State<AppState>, ctx: Ctx, Query(q): Query<MovementQuery>) -> AppResult<Json<Paged<MovementRow>>> {
    ctx.require("stock.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.today(), "month");
    let mut rows: Vec<Counted<MovementRow>> = sqlx::query_as(
        "SELECT COUNT(*) OVER() AS total_count, m.id, m.created_at, m.business_date, m.occurred_on, b.name AS branch_name, m.product_id,
                p.name AS product_name, m.kind, m.quantity, m.unit_cost, m.unit_price, si.barcode, m.ref_type, m.ref_id,
                m.notes, u.name AS user_name
         FROM stock_movements m
         JOIN branches b ON b.id = m.branch_id JOIN products p ON p.id = m.product_id
         LEFT JOIN stock_items si ON si.id = m.stock_item_id LEFT JOIN users u ON u.id = m.user_id
         WHERE m.tenant_id = $1 AND m.branch_id = ANY($2) AND m.business_date BETWEEN $3 AND $4
           AND ($5::uuid IS NULL OR m.product_id = $5) AND ($6::text IS NULL OR m.kind = $6)
         ORDER BY m.created_at DESC LIMIT $7 OFFSET $8",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    .bind(q.product_id)
    .bind(&q.kind)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    if super::costs_hidden(&mut *state.db.acquire().await?, &ctx).await? {
        rows.iter_mut().for_each(|r| r.row.unit_cost = None);
    }
    Ok(Json(rows.into()))
}

#[derive(Serialize, sqlx::FromRow)]
struct ItemRow {
    id: Uuid,
    barcode: String,
    status: String,
    product_id: Uuid,
    product_name: String,
    branch_id: Uuid,
    branch_name: String,
    cost_price: Option<Decimal>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct ItemQuery {
    product_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    status: Option<String>,
    q: Option<String>,
    #[serde(flatten)]
    page: Page,
}

async fn items(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ItemQuery>) -> AppResult<Json<Paged<ItemRow>>> {
    ctx.require_any(&["stock.view", "sales.create"])?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let mut rows: Vec<Counted<ItemRow>> = sqlx::query_as(
        "SELECT COUNT(*) OVER() AS total_count, si.id, si.barcode, si.status, si.product_id, p.name AS product_name,
                si.branch_id, b.name AS branch_name, si.cost_price, si.created_at, si.updated_at
         FROM stock_items si JOIN products p ON p.id = si.product_id JOIN branches b ON b.id = si.branch_id
         WHERE si.tenant_id = $1 AND si.branch_id = ANY($2) AND ($3::uuid IS NULL OR si.product_id = $3)
           AND ($4::text IS NULL OR si.status = $4) AND ($5::text IS NULL OR si.barcode ILIKE $5)
         ORDER BY si.updated_at DESC LIMIT $6 OFFSET $7",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(q.product_id)
    .bind(&q.status)
    .bind(like(&q.q))
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    if super::costs_hidden(&mut *state.db.acquire().await?, &ctx).await? {
        rows.iter_mut().for_each(|r| r.row.cost_price = None);
    }
    Ok(Json(rows.into()))
}

/// Full audit trail of a barcode: every unit that carried it and every movement.
async fn barcode_history(State(state): State<AppState>, ctx: Ctx, Path(code): Path<String>) -> AppResult<Json<Value>> {
    ctx.require("stock.view")?;
    let mut items: Vec<ItemRow> = sqlx::query_as(
        "SELECT si.id, si.barcode, si.status, si.product_id, p.name AS product_name, si.branch_id, b.name AS branch_name,
                si.cost_price, si.created_at, si.updated_at
         FROM stock_items si JOIN products p ON p.id = si.product_id JOIN branches b ON b.id = si.branch_id
         WHERE si.tenant_id = $1 AND si.barcode = $2 ORDER BY si.created_at",
    )
    .bind(ctx.tenant_id)
    .bind(code.trim())
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    let mut history: Vec<MovementRow> = sqlx::query_as(
        "SELECT m.id, m.created_at, m.occurred_on, b.name AS branch_name, m.product_id, p.name AS product_name, m.kind, m.quantity,
                m.unit_cost, m.unit_price, si.barcode, m.ref_type, m.ref_id, m.notes, u.name AS user_name
         FROM stock_movements m JOIN branches b ON b.id = m.branch_id JOIN products p ON p.id = m.product_id
         LEFT JOIN stock_items si ON si.id = m.stock_item_id LEFT JOIN users u ON u.id = m.user_id
         WHERE m.stock_item_id = ANY($1) ORDER BY m.created_at",
    )
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    if super::costs_hidden(&mut *state.db.acquire().await?, &ctx).await? {
        items.iter_mut().for_each(|i| i.cost_price = None);
        history.iter_mut().for_each(|m| m.unit_cost = None);
    }
    let product: Option<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM products WHERE tenant_id = $1 AND barcode = $2")
        .bind(ctx.tenant_id)
        .bind(code.trim())
        .fetch_optional(&state.db)
        .await?;
    Ok(Json(json!({
        "barcode": code.trim(),
        "items": items,
        "history": history,
        "product_barcode_of": product.map(|(id, name)| json!({ "id": id, "name": name })),
    })))
}

// ───────────────────────────── Adjustments & counts ─────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
pub struct AdjustBody {
    pub product_id: Uuid,
    pub branch_id: Option<Uuid>,
    /// count | damage | loss | customer_return | supplier_return | manual | write_off
    pub kind: String,
    /// For `count`: the physically counted quantity.
    pub counted_qty: Option<i32>,
    /// Units affected (positive) for damage/loss/returns/write-off; signed delta for `manual`.
    pub quantity: Option<i32>,
    /// Individually tracked items are identified by barcode.
    pub barcode: Option<String>,
    pub reason: String,
}

fn movement_kind(adj: &str) -> &'static str {
    match adj {
        "count" => "count_variance",
        "damage" => "damage",
        "loss" => "loss",
        "write_off" => "write_off",
        "customer_return" => "customer_return",
        "supplier_return" => "supplier_return",
        _ => "adjustment",
    }
}

async fn create_adjustment(State(state): State<AppState>, ctx: Ctx, Json(b): Json<AdjustBody>) -> AppResult<Json<Outcome<Value>>> {
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "stock").await?;
    let (outcome, branch) = {
        let mut tx = state.db.begin().await?;
        let (outcome, branch) = submit_adjustment(&mut tx, &ctx, b).await?;
        tx.commit().await?;
        if let Some(id) = outcome.approval_id {
            super::approvals::notify_approvers(&state, &ctx, id).await;
        }
        (outcome, branch)
    };
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": branch }));
    Ok(Json(outcome))
}

/// Records an adjustment and applies it (or parks it for approval).
async fn submit_adjustment(conn: &mut PgConnection, ctx: &Ctx, b: AdjustBody) -> AppResult<(Outcome<Value>, Uuid)> {
    let action = if b.kind == "write_off" { "stock.write_off" } else { "stock.adjust" };
    ctx.require(action)?;
    if b.reason.trim().is_empty() {
        return Err(bad("A reason is required for every adjustment"));
    }
    let branch = ctx.branch_or_current(b.branch_id)?;
    let p = product_info(conn, ctx.tenant_id, b.product_id).await?;
    let level = inventory::lock(conn, ctx.tenant_id, branch, b.product_id).await?;

    let qty = b.quantity.unwrap_or(0);
    let delta = match b.kind.as_str() {
        "count" => b.counted_qty.ok_or_else(|| bad("Enter the counted quantity"))?.max(0) - level.on_hand,
        "damage" | "loss" | "write_off" | "supplier_return" if qty > 0 => -qty,
        "customer_return" if qty > 0 => qty,
        "manual" if qty != 0 => qty,
        "damage" | "loss" | "write_off" | "supplier_return" | "customer_return" | "manual" => {
            return Err(bad("Enter the quantity affected"))
        }
        _ => return Err(bad("Unknown adjustment type")),
    };
    if delta == 0 {
        return Err(rule("No change: the quantity matches the current stock"));
    }

    let mut item_id = None;
    if p.track_items {
        if matches!(b.kind.as_str(), "count" | "manual") {
            return Err(rule("Individually tracked items are adjusted by scanning each barcode (damage, loss, write-off or return)"));
        }
        if delta.abs() != 1 {
            return Err(rule("Scan and adjust tracked items one at a time"));
        }
        let code = b.barcode.as_deref().map(str::trim).filter(|c| !c.is_empty()).ok_or_else(|| rule("Scan the item's barcode"))?;
        let expected = if delta < 0 { "in_stock" } else { "sold" };
        let found: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM stock_items WHERE tenant_id=$1 AND product_id=$2 AND barcode=$3 AND status=$4
               AND ($4 = 'sold' OR branch_id = $5) ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(ctx.tenant_id)
        .bind(b.product_id)
        .bind(code)
        .bind(expected)
        .bind(branch)
        .fetch_optional(&mut *conn)
        .await?;
        item_id = Some(found.ok_or_else(|| rule(format!("No {} item with barcode {code} for {}", expected.replace('_', " "), p.name)))?);
    }

    let adj_id: Uuid = sqlx::query_scalar(
        "INSERT INTO stock_adjustments (tenant_id, branch_id, product_id, stock_item_id, kind, previous_qty, delta, new_qty, reason, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(b.product_id)
    .bind(item_id)
    .bind(&b.kind)
    .bind(level.on_hand)
    .bind(delta)
    .bind(level.on_hand + delta)
    .bind(b.reason.trim())
    .bind(ctx.user_id)
    .fetch_one(&mut *conn)
    .await?;

    if workflow::needs_approval(conn, ctx, action, workflow::Gate::branch(branch)).await? {
        let approval = workflow::submit(
            conn,
            ctx,
            workflow::Request {
                action,
                entity_type: "stock_adjustment",
                entity_id: adj_id,
                branch_id: Some(branch),
                summary: format!("{} {:+} × {} — {}", b.kind.replace('_', " "), delta, p.name, b.reason.trim()),
                amount: None,
                payload: json!({}),
            },
        )
        .await?;
        return Ok((Outcome::pending(approval), branch));
    }
    let result = apply_adjustment(conn, ctx, adj_id, None).await?;
    Ok((Outcome::done(result), branch))
}

async fn apply_adjustment(conn: &mut PgConnection, ctx: &Ctx, adj_id: Uuid, approval_id: Option<Uuid>) -> AppResult<Value> {
    let (branch, product, item, kind, delta, new_qty, reason, status): (Uuid, Uuid, Option<Uuid>, String, i32, i32, String, String) =
        sqlx::query_as(
            "SELECT branch_id, product_id, stock_item_id, kind, delta, new_qty, reason, status FROM stock_adjustments
             WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(adj_id)
        .bind(ctx.tenant_id)
        .fetch_one(&mut *conn)
        .await?;
    if status != "pending" {
        return Err(rule("This adjustment was already processed"));
    }
    let level = inventory::lock(conn, ctx.tenant_id, branch, product).await?;
    // A count states the physical truth: re-derive the delta against stock as it is now.
    let delta = if kind == "count" { new_qty - level.on_hand } else { delta };
    let s = settings::load(conn, ctx.tenant_id).await?;

    if delta != 0 {
        if let Some(item_id) = item {
            let new_status = match kind.as_str() {
                "customer_return" => "in_stock",
                "supplier_return" => "returned_to_supplier",
                _ => "written_off",
            };
            sqlx::query("UPDATE stock_items SET status = $2, branch_id = $3, updated_at = now() WHERE id = $1 AND tenant_id = $4")
                .bind(item_id)
                .bind(new_status)
                .bind(branch)
                .bind(ctx.tenant_id)
                .execute(&mut *conn)
                .await?;
        }
        let check = if kind == "count" { Check::None } else { Check::OnHand };
        let m = Movement::new(branch, product, movement_kind(&kind), delta).item(item).reference("stock_adjustment", adj_id).notes(&reason);
        inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), s.stock.allow_negative, check, m).await?;
    }
    sqlx::query(
        "UPDATE stock_adjustments SET status='applied', previous_qty=$2, delta=$3, new_qty=$4, decided_by=$5, decided_at=now() WHERE id=$1 AND tenant_id = $6",
    )
    .bind(adj_id)
    .bind(level.on_hand)
    .bind(delta)
    .bind(level.on_hand + delta)
    .bind(ctx.user_id)
    .bind(ctx.tenant_id)
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        ctx,
        Entry::new("stock", "adjust", "stock_adjustment", adj_id)
            .branch(branch)
            .before(json!({ "quantity": level.on_hand }))
            .after(json!({ "quantity": level.on_hand + delta, "delta": delta, "kind": kind, "product_id": product }))
            .approval(approval_id)
            .comments(&reason),
    )
    .await?;
    Ok(json!({ "id": adj_id, "previous_qty": level.on_hand, "delta": delta, "new_qty": level.on_hand + delta }))
}

#[derive(Deserialize)]
struct CountLine {
    product_id: Uuid,
    counted: i32,
}

#[derive(Deserialize)]
struct CountBody {
    branch_id: Option<Uuid>,
    lines: Vec<CountLine>,
    reason: String,
}

/// Physical stock take: one count adjustment per line with a variance.
async fn stock_count(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CountBody>) -> AppResult<Json<Value>> {
    ctx.require("stock.adjust")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "stock").await?;
    let branch = ctx.branch_or_current(b.branch_id)?;
    if b.lines.is_empty() {
        return Err(bad("Add at least one counted product"));
    }
    let reason = if b.reason.trim().is_empty() { "Stock take".to_string() } else { b.reason.trim().to_string() };
    let mut tx = state.db.begin().await?;
    let (mut applied, mut pending, mut unchanged) = (0, 0, 0);
    let mut variances = Vec::new();
    let mut approvals = Vec::new();
    for line in &b.lines {
        let level = inventory::lock(&mut tx, ctx.tenant_id, branch, line.product_id).await?;
        if line.counted == level.on_hand {
            unchanged += 1;
            continue;
        }
        let body = AdjustBody {
            product_id: line.product_id,
            branch_id: Some(branch),
            kind: "count".into(),
            counted_qty: Some(line.counted),
            quantity: None,
            barcode: None,
            reason: reason.clone(),
        };
        let (outcome, _) = submit_adjustment(&mut tx, &ctx, body).await?;
        variances.push(json!({ "product_id": line.product_id, "system": level.on_hand, "counted": line.counted, "variance": line.counted - level.on_hand }));
        match outcome.approval_id {
            Some(id) => {
                pending += 1;
                approvals.push(id);
            }
            None => applied += 1,
        }
    }
    tx.commit().await?;
    for id in approvals {
        super::approvals::notify_approvers(&state, &ctx, id).await;
    }
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": branch }));
    Ok(Json(json!({ "applied": applied, "pending_approval": pending, "unchanged": unchanged, "variances": variances })))
}

#[derive(Serialize, sqlx::FromRow)]
struct AdjustmentRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    branch_name: String,
    product_name: String,
    barcode: Option<String>,
    kind: String,
    previous_qty: i32,
    delta: i32,
    new_qty: i32,
    reason: String,
    status: String,
    created_by_name: Option<String>,
    decided_by_name: Option<String>,
}

#[derive(Deserialize)]
struct AdjListQuery {
    branch_id: Option<Uuid>,
    status: Option<String>,
    kind: Option<String>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

async fn list_adjustments(State(state): State<AppState>, ctx: Ctx, Query(q): Query<AdjListQuery>) -> AppResult<Json<Paged<AdjustmentRow>>> {
    ctx.require("stock.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.today(), "month");
    let (start, end) = local_range(from, to, ctx.tz);
    let rows: Vec<Counted<AdjustmentRow>> = sqlx::query_as(
        "SELECT COUNT(*) OVER() AS total_count, a.id, a.created_at, b.name AS branch_name, p.name AS product_name, si.barcode,
                a.kind, a.previous_qty, a.delta, a.new_qty, a.reason, a.status, cu.name AS created_by_name, du.name AS decided_by_name
         FROM stock_adjustments a JOIN branches b ON b.id = a.branch_id JOIN products p ON p.id = a.product_id
         LEFT JOIN stock_items si ON si.id = a.stock_item_id
         LEFT JOIN users cu ON cu.id = a.created_by LEFT JOIN users du ON du.id = a.decided_by
         WHERE a.tenant_id = $1 AND a.branch_id = ANY($2) AND a.created_at >= $3 AND a.created_at < $4
           AND ($5::text IS NULL OR a.status = $5) AND ($6::text IS NULL OR a.kind = $6)
         ORDER BY a.created_at DESC LIMIT $7 OFFSET $8",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(start)
    .bind(end)
    .bind(&q.status)
    .bind(&q.kind)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into()))
}

/// Executes approved stock requests.
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    match a.action.as_str() {
        "stock.add" => {
            let mut body: ReceiveBody = serde_json::from_value(a.payload.clone()).map_err(|_| bad("Stored stock receipt is invalid"))?;
            let s = settings::load(conn, ctx.tenant_id).await?;
            let p = product_info(conn, ctx.tenant_id, body.product_id).await?;
            // Re-validate: barcodes may have been used since the request was raised.
            // The requester's activation rights were checked when the request was made.
            let activate = body.activate;
            body.activate = false;
            check_receipt(conn, ctx, &s, &mut body, &p).await?;
            body.activate = activate;
            execute_receipt(conn, ctx, &body, Some(a.id)).await?;
        }
        "stock.adjust" | "stock.write_off" => {
            apply_adjustment(conn, ctx, a.entity_id, Some(a.id)).await?;
        }
        _ => {}
    }
    Ok(())
}

// ───────────────────────────── Stock position ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct PositionRow {
    pub product_id: Uuid,
    pub code: String,
    pub name: String,
    pub category_name: Option<String>,
    pub opening: i64,
    pub added: i64,
    pub transfers_in: i64,
    pub transfers_out: i64,
    pub sold: i64,
    pub returns: i64,
    pub adjustments: i64,
    pub damaged_written_off: i64,
    pub closing: i64,
    pub reserved: i64,
    pub available_now: i64,
    pub low_threshold: i32,
    pub value: Decimal,
}

#[derive(Deserialize, Default)]
pub struct PositionQuery {
    pub branch_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    /// low | out
    pub status: Option<String>,
    #[serde(flatten)]
    pub period: Period,
}

pub async fn position_rows(conn: &mut PgConnection, ctx: &Ctx, q: &PositionQuery) -> AppResult<(NaiveDate, NaiveDate, Vec<PositionRow>)> {
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.today(), "month");
    let s = settings::load(conn, ctx.tenant_id).await?;
    let price = value_expr(s.stock.valuation);
    let rows = sqlx::query_as(&format!(
        "WITH m AS (
            SELECT product_id,
              SUM(quantity) FILTER (WHERE business_date < $3) AS opening,
              SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind IN ('received','opening')) AS added,
              SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind = 'transfer_in') AS t_in,
              -SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind = 'transfer_out') AS t_out,
              -SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind IN ('sale','order_completion')) AS sold,
              SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind IN ('customer_return','sale_reversal')) AS returns,
              SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind IN ('adjustment','count_variance','supplier_return')) AS adj,
              -SUM(quantity) FILTER (WHERE business_date BETWEEN $3 AND $4 AND kind IN ('damage','loss','write_off')) AS dmg,
              SUM(quantity) FILTER (WHERE business_date <= $4) AS closing
            FROM stock_movements WHERE tenant_id = $1 AND branch_id = ANY($2) GROUP BY product_id),
         lv AS (SELECT product_id, SUM(reserved) AS reserved, SUM(on_hand - reserved) AS available
                FROM stock_levels WHERE branch_id = ANY($2) GROUP BY product_id)
         SELECT p.id AS product_id, p.code, p.name, c.name AS category_name,
                COALESCE(m.opening,0)::bigint AS opening, COALESCE(m.added,0)::bigint AS added,
                COALESCE(m.t_in,0)::bigint AS transfers_in, COALESCE(m.t_out,0)::bigint AS transfers_out,
                COALESCE(m.sold,0)::bigint AS sold, COALESCE(m.returns,0)::bigint AS returns,
                COALESCE(m.adj,0)::bigint AS adjustments, COALESCE(m.dmg,0)::bigint AS damaged_written_off,
                COALESCE(m.closing,0)::bigint AS closing, COALESCE(lv.reserved,0)::bigint AS reserved,
                COALESCE(lv.available,0)::bigint AS available_now,
                COALESCE(p.low_stock_threshold, $5) AS low_threshold,
                (GREATEST(COALESCE(m.closing,0),0) * {price})::numeric(14,2) AS value
         FROM products p
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN m ON m.product_id = p.id LEFT JOIN lv ON lv.product_id = p.id
         WHERE p.tenant_id = $1 AND ($6::uuid IS NULL OR p.category_id = $6) AND ($7::uuid IS NULL OR p.id = $7)
           AND (m.product_id IS NOT NULL OR p.is_active)
           AND (CASE $8 WHEN 'out' THEN COALESCE(lv.available,0) <= 0
                        WHEN 'low' THEN COALESCE(lv.available,0) > 0 AND COALESCE(lv.available,0) <= COALESCE(p.low_stock_threshold, $5)
                        ELSE true END)
         ORDER BY p.name"
    ))
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    .bind(s.stock.low_stock_threshold)
    .bind(q.category_id)
    .bind(q.product_id)
    .bind(q.status.as_deref().unwrap_or("all"))
    .fetch_all(&mut *conn)
    .await?;
    Ok((from, to, rows))
}

async fn position(State(state): State<AppState>, ctx: Ctx, Query(q): Query<PositionQuery>) -> AppResult<Json<Value>> {
    ctx.require("stock.view")?;
    let mut conn = state.db.acquire().await?;
    let (from, to, rows) = position_rows(&mut conn, &ctx, &q).await?;
    let total_value: Decimal = rows.iter().map(|r| r.value).sum();
    let hide_value = super::costs_hidden(&mut conn, &ctx).await? && settings::load(&mut conn, ctx.tenant_id).await?.stock.valuation == Valuation::Cost;
    let mut rows = serde_json::to_value(&rows).map_err(|e| crate::error::AppError::Other(e.into()))?;
    if hide_value {
        rows.as_array_mut().into_iter().flatten().for_each(|r| r["value"] = Value::Null);
    }
    Ok(Json(json!({ "from": from, "to": to, "rows": rows, "total_value": if hide_value { None } else { Some(total_value) } })))
}

/// Raise low/out-of-stock alerts for products touched by a transaction (deduped per day).
pub async fn alert_levels(state: &AppState, tenant_id: Uuid, branch_id: Uuid, product_ids: &[Uuid]) {
    let rows: Result<Vec<(Uuid, String, i32, i32, String)>, _> = sqlx::query_as(
        "SELECT p.id, p.name, sl.on_hand - sl.reserved,
                COALESCE(p.low_stock_threshold, COALESCE((t.settings->'stock'->>'low_stock_threshold')::int, 3)), b.name
         FROM stock_levels sl JOIN products p ON p.id = sl.product_id JOIN tenants t ON t.id = p.tenant_id
         JOIN branches b ON b.id = sl.branch_id
         WHERE sl.branch_id = $1 AND sl.product_id = ANY($2) AND p.tenant_id = $3 AND p.is_active",
    )
    .bind(branch_id)
    .bind(product_ids)
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await;
    let Ok(rows) = rows else { return };
    let today = chrono::Utc::now().format("%Y-%m-%d");
    for (pid, name, available, threshold, branch) in rows {
        let (kind, title) = if available <= 0 {
            ("out_of_stock", format!("Out of stock: {name}"))
        } else if available <= threshold {
            ("low_stock", format!("Low stock: {name}"))
        } else {
            continue;
        };
        notify::to_permission(
            state,
            tenant_id,
            Some(branch_id),
            "stock.add",
            Note::new(kind, title, format!("{} available at {branch}", available.max(0)), format!("/stock?product={pid}"))
                .dedupe(format!("{kind}:{branch_id}:{pid}:{today}")),
        )
        .await;
    }
}
