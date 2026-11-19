//! Stock transfers between branches:
//! Draft → Pending Approval → Approved → Dispatched (In Transit) → Received.
//! Stock leaves the source on dispatch and only becomes sellable at the
//! destination once receipt is confirmed (when receipt control is enabled).

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Counted, Page, Paged, Period};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::routes::prices;
use crate::inventory::{self, Check, Movement};
use crate::notify::{self, Note};
use crate::routes::approvals::ApprovalRow;
use crate::settings;
use crate::state::AppState;
use crate::util::{local_range, next_doc_no};
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/transfers", get(list).post(create))
        .route("/transfers/{id}", get(detail))
        .route("/transfers/{id}/submit", post(submit))
        .route("/transfers/{id}/dispatch", post(dispatch))
        .route("/transfers/{id}/receive", post(receive))
        .route("/transfers/{id}/cancel", post(cancel))
        .route("/transfers/prices", get(price_check))
        .route("/transfers/{id}/prices/{product_id}", post(decide_price))
}

#[derive(Serialize, sqlx::FromRow)]
struct TransferRow {
    id: Uuid,
    transfer_no: String,
    from_branch_id: Uuid,
    from_branch_name: String,
    to_branch_id: Uuid,
    to_branch_name: String,
    status: String,
    transfer_date: NaiveDate,
    notes: String,
    total_units: i64,
    created_by_name: Option<String>,
    approved_by_name: Option<String>,
    dispatched_by_name: Option<String>,
    received_by_name: Option<String>,
    approved_at: Option<DateTime<Utc>>,
    dispatched_at: Option<DateTime<Utc>>,
    received_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    /// Units recorded on receipt as not arrived / arrived damaged, and why.
    short_units: i64,
    damaged_units: i64,
    discrepancy_reason: String,
}

const SELECT: &str = "SELECT t.id, t.transfer_no, t.from_branch_id, fb.name AS from_branch_name, t.to_branch_id, tb.name AS to_branch_name,
        t.status, t.transfer_date, t.notes,
        (SELECT COALESCE(SUM(quantity),0) FROM transfer_items ti WHERE ti.transfer_id = t.id)::bigint AS total_units,
        cu.name AS created_by_name, au.name AS approved_by_name, du.name AS dispatched_by_name, ru.name AS received_by_name,
        t.approved_at, t.dispatched_at, t.received_at, t.created_at,
        (SELECT COALESCE(SUM(short_qty),0) FROM transfer_items ti WHERE ti.transfer_id = t.id)::bigint AS short_units,
        (SELECT COALESCE(SUM(damaged_qty),0) FROM transfer_items ti WHERE ti.transfer_id = t.id)::bigint AS damaged_units,
        t.discrepancy_reason
    FROM transfers t
    JOIN branches fb ON fb.id = t.from_branch_id JOIN branches tb ON tb.id = t.to_branch_id
    LEFT JOIN users cu ON cu.id = t.created_by LEFT JOIN users au ON au.id = t.approved_by
    LEFT JOIN users du ON du.id = t.dispatched_by LEFT JOIN users ru ON ru.id = t.received_by";

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    branch_id: Option<Uuid>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Paged<TransferRow>>> {
    ctx.require("stock.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.today(), "all");
    let (start, end) = local_range(from, to, ctx.tz);
    let select = SELECT.replacen("SELECT", "SELECT COUNT(*) OVER() AS total_count,", 1);
    let rows: Vec<Counted<TransferRow>> = sqlx::query_as(&format!(
        "{select} WHERE t.tenant_id = $1 AND (t.from_branch_id = ANY($2) OR t.to_branch_id = ANY($2))
           AND ($3::text IS NULL OR t.status = $3 OR ($3 = 'in_transit' AND t.status = 'dispatched'))
           AND t.created_at >= $4 AND t.created_at < $5
         ORDER BY t.created_at DESC LIMIT $6 OFFSET $7"
    ))
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(&q.status)
    .bind(start)
    .bind(end)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into()))
}

async fn load(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, lock: bool) -> AppResult<TransferRow> {
    let lock = if lock { " FOR UPDATE OF t" } else { "" };
    let t: TransferRow = sqlx::query_as(&format!("{SELECT} WHERE t.id = $1 AND t.tenant_id = $2{lock}"))
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Transfer"))?;
    if !ctx.has_branch(t.from_branch_id) && !ctx.has_branch(t.to_branch_id) {
        return Err(AppError::NotFound("Transfer"));
    }
    Ok(t)
}

async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.view")?;
    let mut conn = state.db.acquire().await?;
    let t = load(&mut conn, &ctx, id, false).await?;
    let items: Vec<(Uuid, Uuid, String, String, i32, Option<String>, i32, i32)> = sqlx::query_as(
        "SELECT ti.id, ti.product_id, p.name, p.code, ti.quantity, mask_item(si.barcode), ti.short_qty, ti.damaged_qty FROM transfer_items ti
         JOIN products p ON p.id = ti.product_id LEFT JOIN stock_items si ON si.id = ti.stock_item_id
         WHERE ti.transfer_id = $1 ORDER BY p.name",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let prices = price_lines(&mut conn, &t).await?;
    let can = json!({
        // Roadmap 176: authorise or decline a requested destination price change.
        "decide_price": ctx.can("products.price") && ctx.has_branch(t.to_branch_id) && !["cancelled", "rejected"].contains(&t.status.as_str()),
        "submit": t.status == "draft" && ctx.can("stock.transfer") && ctx.has_branch(t.from_branch_id),
        "dispatch": t.status == "approved" && ctx.can("stock.transfer") && ctx.has_branch(t.from_branch_id),
        "receive": t.status == "dispatched" && ctx.can("stock.receive_transfer") && ctx.has_branch(t.to_branch_id),
        "cancel": (["draft", "pending_approval", "approved"].contains(&t.status.as_str()) && ctx.can("stock.transfer") && ctx.has_branch(t.from_branch_id)),
    });
    Ok(Json(json!({
        "transfer": t,
        "items": items.into_iter().map(|(id, pid, name, code, qty, barcode, short, damaged)| json!({
            "id": id, "product_id": pid, "product_name": name, "product_code": code, "quantity": qty, "barcode": barcode,
            "short_qty": short, "damaged_qty": damaged, "received_qty": qty - short - damaged,
        })).collect::<Vec<_>>(),
        "prices": prices,
        "can": can,
    })))
}

// ───────────────────────────── Destination prices (roadmap 176) ─────────────────────────────

/// Per product on the transfer: its price at the source and at the destination when the transfer was made, the
/// destination's price now, and what the transfer proposes for it.
async fn price_lines(conn: &mut PgConnection, t: &TransferRow) -> AppResult<Vec<Value>> {
    let rows: Vec<(Uuid, String, String, Option<Decimal>, Option<Decimal>, Option<Decimal>, String, Option<Decimal>, String, Option<String>, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        "SELECT tp.product_id, p.name, p.code, tp.source_price, tp.dest_price, effective_price(p.id, $2, p.marked_price),
                tp.action, tp.new_price, tp.status, u.name, tp.decided_at, tp.note
         FROM transfer_prices tp JOIN products p ON p.id = tp.product_id LEFT JOIN users u ON u.id = tp.decided_by
         WHERE tp.transfer_id = $1 ORDER BY p.name",
    )
    .bind(t.id)
    .bind(t.to_branch_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(pid, name, code, source, dest, now, action, new, status, by, at, note)| json!({
            "product_id": pid, "product_name": name, "product_code": code, "source_price": source, "dest_price": dest,
            "dest_price_now": now, "action": action, "new_price": new, "status": status, "decided_by_name": by, "decided_at": at, "note": note,
        }))
        .collect())
}

#[derive(Deserialize)]
struct PriceCheckQuery {
    from_branch_id: Option<Uuid>,
    to_branch_id: Uuid,
    /// Comma-separated product ids.
    products: String,
}

/// Before a transfer is submitted: each product's selling price at the source and at the destination, and whether the
/// person may change the destination's price (otherwise they can only request it).
async fn price_check(State(state): State<AppState>, ctx: Ctx, Query(q): Query<PriceCheckQuery>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    let from = ctx.branch_or_current(q.from_branch_id)?;
    let ids: Vec<Uuid> = q.products.split(',').filter(|s| !s.trim().is_empty()).map(|s| s.trim().parse()).collect::<Result<_, _>>().map_err(|_| bad("Unknown product"))?;
    let rows: Vec<(Uuid, String, Option<Decimal>, Option<Decimal>, bool)> = sqlx::query_as(
        "SELECT p.id, p.name, effective_price(p.id, $2, p.marked_price), effective_price(p.id, $3, p.marked_price),
                EXISTS (SELECT 1 FROM product_branch_prices bp WHERE bp.product_id = p.id AND bp.branch_id = $3)
         FROM products p JOIN branches b ON b.id = $3 AND b.tenant_id = $1 WHERE p.tenant_id = $1 AND p.id = ANY($4) ORDER BY p.name",
    )
    .bind(ctx.tenant_id)
    .bind(from)
    .bind(q.to_branch_id)
    .bind(&ids)
    .fetch_all(&state.db)
    .await?;
    let may_set = ctx.can("products.price") && ctx.has_branch(q.to_branch_id);
    Ok(Json(json!({
        "may_set_price": may_set,
        "items": rows.into_iter().map(|(id, name, source, dest, own)| json!({
            "product_id": id, "product_name": name, "source_price": source, "dest_price": dest, "dest_has_own_price": own,
        })).collect::<Vec<_>>(),
    })))
}

/// Records what the transfer proposes for each product's price at the destination. By default nothing changes (*Keep
/// destination price*); a change is explicit, confirmed, and authorised by someone allowed to set prices there — the
/// person making the transfer if they are, otherwise it waits as a request.
async fn record_prices(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, from: Uuid, to: Uuid, items: &[ItemBody], confirmed: bool) -> AppResult<()> {
    let to_name: String = sqlx::query_scalar("SELECT name FROM branches WHERE id = $1").bind(to).fetch_one(&mut *conn).await?;
    let mut done: Vec<Uuid> = Vec::new();
    for item in items {
        if done.contains(&item.product_id) {
            continue;
        }
        done.push(item.product_id);
        let name: String = sqlx::query_scalar("SELECT name FROM products WHERE id = $1 AND tenant_id = $2").bind(item.product_id).bind(ctx.tenant_id).fetch_one(&mut *conn).await?;
        let source = prices::effective(conn, ctx.tenant_id, item.product_id, from).await?;
        let dest = prices::effective(conn, ctx.tenant_id, item.product_id, to).await?;
        let action = item.price_action.as_deref().unwrap_or("keep");
        let new = match action {
            "keep" => None,
            "source" => Some(source.ok_or_else(|| bad(format!("{name} has no selling price at the source branch to use")))?),
            "custom" => {
                let p = item.new_price.ok_or_else(|| bad(format!("Enter the new price of {name} at {to_name}")))?;
                if p < Decimal::ZERO {
                    return Err(bad("A price cannot be negative"));
                }
                Some(p)
            }
            _ => return Err(bad("Price action: keep, source or custom")),
        };
        // The same price as the destination already has is no change.
        let (action, new) = if new.is_none() || new == dest { ("keep", None) } else { (action, new) };
        if action != "keep" && !confirmed {
            return Err(refused("Confirm the price change", format!("Confirm the new price of {name} at {to_name} ({} → {})", prices::show(dest), prices::show(new))));
        }
        let authorised = action != "keep" && ctx.can("products.price") && ctx.has_branch(to);
        let status = if action == "keep" { "unchanged" } else if authorised { "authorized" } else { "requested" };
        sqlx::query(
            "INSERT INTO transfer_prices (transfer_id, product_id, source_price, dest_price, action, new_price, status, decided_by, decided_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(id)
        .bind(item.product_id)
        .bind(source)
        .bind(dest)
        .bind(action)
        .bind(new)
        .bind(status)
        .bind(authorised.then_some(ctx.user_id))
        .bind(authorised.then(Utc::now))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Applies the authorised destination prices of a received transfer. A price that changed at the destination since the
/// transfer was made is not overwritten: the change goes back to *requested* for a fresh decision.
async fn apply_prices(conn: &mut PgConnection, ctx: &Ctx, t: &TransferRow) -> AppResult<()> {
    let rows: Vec<(Uuid, Option<Decimal>, Option<Decimal>, Option<String>)> = sqlx::query_as(
        "SELECT tp.product_id, tp.dest_price, tp.new_price, u.name FROM transfer_prices tp LEFT JOIN users u ON u.id = tp.decided_by
         WHERE tp.transfer_id = $1 AND tp.status = 'authorized' ORDER BY tp.product_id FOR UPDATE OF tp",
    )
    .bind(t.id)
    .fetch_all(&mut *conn)
    .await?;
    for (product, dest, new, by) in rows {
        apply_one(conn, ctx, t, product, dest, new, by.as_deref().unwrap_or("")).await?;
    }
    Ok(())
}

async fn apply_one(conn: &mut PgConnection, ctx: &Ctx, t: &TransferRow, product: Uuid, dest: Option<Decimal>, new: Option<Decimal>, by: &str) -> AppResult<bool> {
    // One price change per product at a time (the same lock as Product → branch prices).
    sqlx::query("SELECT id FROM products WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(product).bind(ctx.tenant_id).execute(&mut *conn).await?;
    let now = prices::effective(conn, ctx.tenant_id, product, t.to_branch_id).await?;
    if now != dest {
        sqlx::query("UPDATE transfer_prices SET status = 'requested', note = $3 WHERE transfer_id = $1 AND product_id = $2")
            .bind(t.id)
            .bind(product)
            .bind(format!("The price at {} changed after the transfer was made (now {}) — decide again", t.to_branch_name, prices::show(now)))
            .execute(&mut *conn)
            .await?;
        return Ok(false);
    }
    let why = format!("Transfer {} from {} (authorised by {by})", t.transfer_no, t.from_branch_name);
    prices::set(conn, ctx, product, t.to_branch_id, new, None, &why).await?;
    sqlx::query("UPDATE transfer_prices SET status = 'applied', note = '' WHERE transfer_id = $1 AND product_id = $2").bind(t.id).bind(product).execute(&mut *conn).await?;
    Ok(true)
}

#[derive(Deserialize)]
struct PriceDecision {
    /// authorize | decline
    decision: String,
}

/// Someone allowed to set prices at the destination authorises or declines a requested price change. Authorised before
/// the stock arrives, it applies on receipt; after, at once.
async fn decide_price(State(state): State<AppState>, ctx: Ctx, Path((id, product)): Path<(Uuid, Uuid)>, Json(b): Json<PriceDecision>) -> AppResult<Json<Value>> {
    ctx.require("products.price")?;
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    if !ctx.has_branch(t.to_branch_id) {
        return Err(AppError::Forbidden(format!("Only someone who can set prices at {} decides its price", t.to_branch_name)));
    }
    if ["cancelled", "rejected"].contains(&t.status.as_str()) {
        return Err(rule("This transfer was not completed — its price change does not apply"));
    }
    let row: Option<(String, Option<Decimal>, Option<Decimal>)> =
        sqlx::query_as("SELECT status, dest_price, new_price FROM transfer_prices WHERE transfer_id = $1 AND product_id = $2 FOR UPDATE")
            .bind(id)
            .bind(product)
            .fetch_optional(&mut *tx)
            .await?;
    let (status, dest, new) = row.ok_or(AppError::NotFound("Price change"))?;
    if status != "requested" {
        return Err(rule("This price change has already been decided"));
    }
    let outcome = match b.decision.as_str() {
        "decline" => {
            sqlx::query("UPDATE transfer_prices SET status = 'declined', decided_by = $3, decided_at = now(), note = '' WHERE transfer_id = $1 AND product_id = $2")
                .bind(id)
                .bind(product)
                .bind(ctx.user_id)
                .execute(&mut *tx)
                .await?;
            "declined"
        }
        "authorize" => {
            // Decided against the price the destination has now (a stale request was sent back with a note).
            let now = prices::effective(&mut tx, ctx.tenant_id, product, t.to_branch_id).await?;
            sqlx::query("UPDATE transfer_prices SET status = 'authorized', dest_price = $3, decided_by = $4, decided_at = now(), note = '' WHERE transfer_id = $1 AND product_id = $2")
                .bind(id)
                .bind(product)
                .bind(now)
                .bind(ctx.user_id)
                .execute(&mut *tx)
                .await?;
            if t.status == "received" {
                apply_one(&mut tx, &ctx, &t, product, now, new, &ctx.name).await?;
                "applied"
            } else {
                "authorized"
            }
        }
        _ => return Err(bad("Decision: authorize or decline")),
    };
    audit::record(&mut tx, &ctx, Entry::new("transfers", "price_decision", "transfer", id).branch(t.to_branch_id).after(json!({
        "product_id": product, "decision": outcome, "dest_price": dest, "new_price": new,
    })))
    .await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": t.to_branch_id }));
    Ok(Json(json!({ "status": outcome })))
}

/// A transfer that will not complete changes no price.
async fn close_prices(conn: &mut PgConnection, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE transfer_prices SET status = 'not_applied' WHERE transfer_id = $1 AND status IN ('requested','authorized')")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// What approvers see: each product, its quantity and the proposed price treatment at the destination.
async fn approval_payload(conn: &mut PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<(String, Value)> {
    let t = load(conn, ctx, id, false).await?;
    let lines: Vec<(Uuid, String, String, i64, Option<Decimal>, Option<Decimal>, Option<String>, Option<Decimal>, Option<String>)> = sqlx::query_as(
        "SELECT ti.product_id, p.name, p.code, SUM(ti.quantity)::bigint, tp.source_price, tp.dest_price, tp.action, tp.new_price, tp.status
         FROM transfer_items ti JOIN products p ON p.id = ti.product_id
         LEFT JOIN transfer_prices tp ON tp.transfer_id = ti.transfer_id AND tp.product_id = ti.product_id
         WHERE ti.transfer_id = $1 GROUP BY ti.product_id, p.name, p.code, tp.source_price, tp.dest_price, tp.action, tp.new_price, tp.status
         ORDER BY p.name",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let first = lines.first().map(|l| format!("{} × {}", l.3, l.1)).unwrap_or_default();
    let more = if lines.len() > 1 { format!(" (+{} more)", lines.len() - 1) } else { String::new() };
    let summary = format!("Transfer {} · {first}{more} · {} → {}", t.transfer_no, t.from_branch_name, t.to_branch_name);
    let payload = json!({
        "transfer_no": t.transfer_no, "from_branch_name": t.from_branch_name, "to_branch_name": t.to_branch_name, "total_units": t.total_units,
        "lines": lines.into_iter().map(|(pid, name, code, qty, source, dest, action, new, status)| json!({
            "product_id": pid, "product_name": name, "product_code": code, "quantity": qty, "source_price": source, "dest_price": dest,
            "price_action": action.unwrap_or_else(|| "keep".into()), "new_price": new, "price_status": status.unwrap_or_else(|| "unchanged".into()),
        })).collect::<Vec<_>>(),
    });
    Ok((summary, payload))
}

#[derive(Deserialize)]
struct ItemBody {
    product_id: Uuid,
    quantity: i32,
    #[serde(default)]
    barcodes: Vec<String>,
    /// Roadmap 176: the destination's price — keep (default) | source (the source branch's price) | custom (`new_price`).
    #[serde(default)]
    price_action: Option<String>,
    #[serde(default)]
    new_price: Option<Decimal>,
}

#[derive(Deserialize)]
struct CreateBody {
    from_branch_id: Option<Uuid>,
    to_branch_id: Uuid,
    transfer_date: Option<NaiveDate>,
    #[serde(default)]
    notes: String,
    items: Vec<ItemBody>,
    #[serde(default)]
    submit: bool,
    /// Roadmap 176: the person confirmed the destination price changes shown to them.
    #[serde(default)]
    price_confirmed: bool,
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CreateBody>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "transfers").await?;
    let from = ctx.branch_or_current(b.from_branch_id)?;
    if from == b.to_branch_id {
        return Err(bad("Choose a different destination branch"));
    }
    if b.items.is_empty() {
        return Err(bad("Add at least one product"));
    }
    let mut tx = state.db.begin().await?;
    let to_ok: Option<bool> = sqlx::query_scalar("SELECT is_active FROM branches WHERE id = $1 AND tenant_id = $2")
        .bind(b.to_branch_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    if to_ok != Some(true) {
        return Err(bad("Unknown or inactive destination branch"));
    }

    let transfer_no = next_doc_no(&mut tx, ctx.tenant_id, "TRF", ctx.tz).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO transfers (tenant_id, transfer_no, from_branch_id, to_branch_id, transfer_date, notes, created_by)
         VALUES ($1,$2,$3,$4,COALESCE($5, CURRENT_DATE),$6,$7) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&transfer_no)
    .bind(from)
    .bind(b.to_branch_id)
    .bind(b.transfer_date)
    .bind(b.notes.trim())
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;

    for item in &b.items {
        let (name, allowed, track): (String, bool, bool) = sqlx::query_as(
            "SELECT name, transfer_allowed AND is_active, track_items FROM products WHERE id = $1 AND tenant_id = $2",
        )
        .bind(item.product_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
        if !allowed {
            return Err(rule(format!("{name} cannot be transferred")));
        }
        inventory::ensure_product_in_branch(&mut tx, ctx.tenant_id, item.product_id, b.to_branch_id).await?;
        if item.quantity <= 0 {
            return Err(bad(format!("Enter a quantity for {name}")));
        }
        let level = inventory::lock(&mut tx, ctx.tenant_id, from, item.product_id).await?;
        if level.available() < item.quantity {
            return Err(rule(format!("Only {} × {name} available to transfer", level.available().max(0))));
        }
        if track {
            let codes: Vec<String> = item.barcodes.iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
            if codes.len() != item.quantity as usize {
                return Err(rule(format!("Scan each {name} being transferred ({} of {})", codes.len(), item.quantity)));
            }
            for code in codes {
                let si: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM stock_items WHERE tenant_id=$1 AND product_id=$2 AND branch_id=$3 AND barcode=$4 AND status='in_stock'",
                )
                .bind(ctx.tenant_id)
                .bind(item.product_id)
                .bind(from)
                .bind(&code)
                .fetch_optional(&mut *tx)
                .await?;
                let si = si.ok_or_else(|| rule(format!("{code} is not an in-stock {name} at this branch")))?;
                let dup: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM transfer_items WHERE transfer_id = $1 AND stock_item_id = $2)")
                    .bind(id)
                    .bind(si)
                    .fetch_one(&mut *tx)
                    .await?;
                if dup {
                    return Err(rule(format!("{code} was scanned twice")));
                }
                sqlx::query("INSERT INTO transfer_items (transfer_id, product_id, stock_item_id, quantity) VALUES ($1,$2,$3,1)")
                    .bind(id)
                    .bind(item.product_id)
                    .bind(si)
                    .execute(&mut *tx)
                    .await?;
            }
        } else {
            sqlx::query("INSERT INTO transfer_items (transfer_id, product_id, quantity) VALUES ($1,$2,$3)")
                .bind(id)
                .bind(item.product_id)
                .bind(item.quantity)
                .execute(&mut *tx)
                .await?;
        }
    }
    record_prices(&mut tx, &ctx, id, from, b.to_branch_id, &b.items, b.price_confirmed).await?;
    audit::record(&mut tx, &ctx, Entry::new("transfers", "create", "transfer", id).branch(from).after(json!({
        "transfer_no": transfer_no, "to_branch_id": b.to_branch_id, "items": b.items.len()
    })))
    .await?;

    let mut approval = None;
    let mut status = "draft";
    if b.submit {
        (status, approval) = submit_inner(&mut tx, &ctx, id, from, &transfer_no).await?;
    }
    let requested: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transfer_prices WHERE transfer_id = $1 AND status = 'requested'")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    if let Some(a) = approval {
        super::approvals::notify_approvers(&state, &ctx, a).await;
    }
    // Roadmap 176: a price change the person could not authorise waits for someone who sets prices at the destination.
    if requested > 0 {
        notify::to_permission(
            &state,
            ctx.tenant_id,
            Some(b.to_branch_id),
            "products.price",
            Note::new(
                "transfer_price_request",
                format!("Price change requested with transfer {transfer_no}"),
                format!("{} asks to change the selling price of {requested} product(s) — authorise or decline it on the transfer", ctx.name),
                format!("/transfers/{id}"),
            ),
        )
        .await;
    }
    Ok(Json(json!({ "id": id, "transfer_no": transfer_no, "status": status, "approval_id": approval })))
}

async fn submit_inner(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, from: Uuid, no: &str) -> AppResult<(&'static str, Option<Uuid>)> {
    if workflow::needs_approval(conn, ctx, "stock.transfer", workflow::Gate::branch(from)).await? {
        sqlx::query("UPDATE transfers SET status = 'pending_approval' WHERE id = $1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).execute(&mut *conn).await?;
        // Roadmap 176: approvers see the products, quantities and proposed prices — not only the transfer number.
        let (summary, payload) = approval_payload(conn, ctx, id).await.unwrap_or_else(|_| (format!("Stock transfer {no}"), json!({})));
        let approval = workflow::submit(
            conn,
            ctx,
            workflow::Request {
                action: "stock.transfer",
                entity_type: "transfer",
                entity_id: id,
                branch_id: Some(from),
                summary,
                amount: None,
                payload,
            },
        )
        .await?;
        Ok(("pending_approval", Some(approval)))
    } else {
        sqlx::query("UPDATE transfers SET status = 'approved', approved_by = $2, approved_at = now() WHERE id = $1 AND tenant_id = $3")
            .bind(id)
            .bind(ctx.user_id)
            .bind(ctx.tenant_id)
            .execute(&mut *conn)
            .await?;
        Ok(("approved", None))
    }
}

async fn submit(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "transfers").await?;
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.from_branch_id)?;
    if t.status != "draft" {
        return Err(rule("Only draft transfers can be submitted"));
    }
    let (status, approval) = submit_inner(&mut tx, &ctx, id, t.from_branch_id, &t.transfer_no).await?;
    audit::record(&mut tx, &ctx, Entry::new("transfers", "submit", "transfer", id).branch(t.from_branch_id)).await?;
    tx.commit().await?;
    if let Some(a) = approval {
        super::approvals::notify_approvers(&state, &ctx, a).await;
    }
    Ok(Json(json!({ "status": status, "approval_id": approval })))
}

/// (line id, product, tracked unit, quantity)
async fn transfer_items(conn: &mut PgConnection, id: Uuid) -> AppResult<Vec<(Uuid, Uuid, Option<Uuid>, i32)>> {
    Ok(sqlx::query_as("SELECT id, product_id, stock_item_id, quantity FROM transfer_items WHERE transfer_id = $1")
        .bind(id)
        .fetch_all(&mut *conn)
        .await?)
}

async fn dispatch(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "transfers").await?;
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.from_branch_id)?;
    if t.status != "approved" {
        return Err(rule("Only approved transfers can be dispatched"));
    }
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let items = transfer_items(&mut tx, id).await?;
    let note = format!("Transfer {} → {}", t.transfer_no, t.to_branch_name);
    for (_, product_id, item_id, qty) in &items {
        if let Some(si) = item_id {
            let n = sqlx::query("UPDATE stock_items SET status='in_transit', updated_at=now() WHERE id=$1 AND status='in_stock' AND branch_id=$2 AND tenant_id = $3")
                .bind(si)
                .bind(t.from_branch_id)
                .bind(ctx.tenant_id)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            if n == 0 {
                return Err(rule("An item on this transfer is no longer in stock at the source branch"));
            }
        }
        let m = Movement::new(t.from_branch_id, *product_id, "transfer_out", -qty).item(*item_id).reference("transfer", id).notes(&note);
        inventory::apply(&mut tx, ctx.tenant_id, Some(ctx.user_id), s.stock.allow_negative, Check::Available, m).await?;
    }
    sqlx::query("UPDATE transfers SET status='dispatched', dispatched_by=$2, dispatched_at=now() WHERE id=$1 AND tenant_id = $3")
        .bind(id)
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("transfers", "dispatch", "transfer", id).branch(t.from_branch_id)).await?;

    let mut status = "dispatched";
    if !s.stock.transfer_receipt_control {
        receive_inner(&mut tx, &ctx, &t, &items, &Receipt::default()).await?;
        apply_prices(&mut tx, &ctx, &t).await?;
        status = "received";
    }
    tx.commit().await?;

    if status == "dispatched" {
        notify::to_permission(
            &state,
            ctx.tenant_id,
            Some(t.to_branch_id),
            "stock.receive_transfer",
            Note::new(
                "transfer_in_transit",
                format!("Transfer {} on its way", t.transfer_no),
                format!("{} unit(s) from {} awaiting receipt", t.total_units, t.from_branch_name),
                format!("/transfers/{id}"),
            ),
        )
        .await;
    }
    let products: Vec<Uuid> = items.iter().map(|i| i.1).collect();
    super::stock::alert_levels(&state, ctx.tenant_id, t.from_branch_id, &products).await;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": t.from_branch_id }));
    Ok(Json(json!({ "status": status })))
}

/// What the receiving branch found: per transfer line, units that did not arrive and units that arrived damaged.
#[derive(Deserialize, Serialize, Default)]
struct Receipt {
    #[serde(default)]
    lines: Vec<ReceiptLine>,
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize, Serialize, Clone)]
struct ReceiptLine {
    id: Uuid,
    #[serde(default)]
    short: i32,
    #[serde(default)]
    damaged: i32,
}

/// Brings a dispatched transfer into the destination. Every dispatched unit is booked in (transfer_in); short and
/// damaged units are then booked out at the destination as loss / damage with the reason, so the ledger explains
/// every unit and only what arrived in good condition becomes sellable. Tracked units short or damaged are written off.
async fn receive_inner(conn: &mut PgConnection, ctx: &Ctx, t: &TransferRow, items: &[(Uuid, Uuid, Option<Uuid>, i32)], r: &Receipt) -> AppResult<()> {
    let note = format!("Transfer {} from {}", t.transfer_no, t.from_branch_name);
    let reason = r.reason.trim();
    for line in &r.lines {
        let Some((_, _, _, qty)) = items.iter().find(|i| i.0 == line.id) else {
            return Err(bad("A receipt line does not belong to this transfer"));
        };
        if line.short < 0 || line.damaged < 0 || line.short + line.damaged > *qty {
            return Err(bad("Short and damaged units cannot exceed the quantity sent"));
        }
    }
    let discrepancy = r.lines.iter().any(|l| l.short + l.damaged > 0);
    if discrepancy && reason.chars().count() < 3 {
        return Err(bad("Explain what was short or damaged"));
    }
    for (line_id, product_id, item_id, qty) in items {
        let (short, damaged) = r.lines.iter().find(|l| l.id == *line_id).map_or((0, 0), |l| (l.short, l.damaged));
        if let Some(si) = item_id {
            let status = if short + damaged > 0 { "written_off" } else { "in_stock" };
            sqlx::query("UPDATE stock_items SET status=$3, branch_id=$2, updated_at=now() WHERE id=$1 AND status='in_transit' AND tenant_id = $4")
                .bind(si)
                .bind(t.to_branch_id)
                .bind(status)
                .bind(ctx.tenant_id)
                .execute(&mut *conn)
                .await?;
        }
        let m = Movement::new(t.to_branch_id, *product_id, "transfer_in", *qty).item(*item_id).reference("transfer", t.id).notes(&note);
        inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), true, Check::None, m).await?;
        for (kind, n, label) in [("loss", short, "Short on arrival"), ("damage", damaged, "Damaged on arrival")] {
            if n > 0 {
                let why = format!("{label} — {} ({reason})", t.transfer_no);
                let m = Movement::new(t.to_branch_id, *product_id, kind, -n).item(*item_id).reference("transfer", t.id).notes(&why);
                inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), true, Check::None, m).await?;
            }
        }
        if short + damaged > 0 {
            sqlx::query("UPDATE transfer_items SET short_qty = $2, damaged_qty = $3 WHERE id = $1")
                .bind(line_id)
                .bind(short)
                .bind(damaged)
                .execute(&mut *conn)
                .await?;
        }
    }
    sqlx::query("UPDATE transfers SET status='received', received_by=$2, received_at=now(), discrepancy_reason=$3 WHERE id=$1 AND tenant_id = $4")
        .bind(t.id)
        .bind(ctx.user_id)
        .bind(if discrepancy { reason } else { "" })
        .bind(ctx.tenant_id)
        .execute(&mut *conn)
        .await?;
    let mut e = Entry::new("transfers", "receive", "transfer", t.id).branch(t.to_branch_id);
    if discrepancy {
        e = e.after(r).comments(reason);
    }
    audit::record(conn, ctx, e).await
}

async fn receive(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Bytes) -> AppResult<Json<Value>> {
    ctx.require("stock.receive_transfer")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "transfers").await?;
    // No body = everything arrived as sent.
    let receipt: Receipt = if body.iter().all(u8::is_ascii_whitespace) {
        Receipt::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| bad("Invalid receipt"))?
    };
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.to_branch_id)?;
    if t.status != "dispatched" {
        return Err(rule("Only dispatched transfers can be received"));
    }
    let items = transfer_items(&mut tx, id).await?;
    receive_inner(&mut tx, &ctx, &t, &items, &receipt).await?;
    apply_prices(&mut tx, &ctx, &t).await?;
    tx.commit().await?;
    let (short, damaged): (i32, i32) = receipt.lines.iter().fold((0, 0), |a, l| (a.0 + l.short, a.1 + l.damaged));
    let people: Vec<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>("SELECT unnest(ARRAY[created_by, dispatched_by]) FROM transfers WHERE id = $1")
        .bind(id)
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .flatten()
        .fold(vec![], |mut v, u| {
            if !v.contains(&u) {
                v.push(u);
            }
            v
        });
    if !people.is_empty() {
        let (kind, title, body) = if short + damaged > 0 {
            (
                "transfer_discrepancy",
                format!("Transfer {} received with discrepancies", t.transfer_no),
                format!("{} at {}: {short} short, {damaged} damaged — {}", ctx.name, t.to_branch_name, receipt.reason.trim()),
            )
        } else {
            ("transfer_received", format!("Transfer {} received", t.transfer_no), format!("Confirmed by {} at {}", ctx.name, t.to_branch_name))
        };
        notify::to_users(&state, ctx.tenant_id, &people, Note::new(kind, title, body, format!("/transfers/{id}"))).await;
    }
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": t.to_branch_id }));
    Ok(Json(json!({ "status": "received", "short": short, "damaged": damaged })))
}

#[derive(Deserialize, Default)]
struct CancelBody {
    #[serde(default)]
    reason: String,
}

async fn cancel(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<CancelBody>>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "transfers").await?;
    let reason = body.map(|b| b.0.reason).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.from_branch_id)?;
    if !["draft", "pending_approval", "approved"].contains(&t.status.as_str()) {
        return Err(rule("A dispatched transfer cannot be cancelled — receive it, then transfer it back"));
    }
    sqlx::query("UPDATE transfers SET status='cancelled' WHERE id=$1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).execute(&mut *tx).await?;
    close_prices(&mut tx, id).await?;
    sqlx::query("UPDATE approvals SET status='cancelled', decided_by=$2, decided_at=now() WHERE entity_id=$1 AND status='pending' AND tenant_id = $3")
        .bind(id)
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("transfers", "cancel", "transfer", id).branch(t.from_branch_id).comments(&reason)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "cancelled" })))
}

pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let n = sqlx::query("UPDATE transfers SET status='approved', approved_by=$2, approved_at=now() WHERE id=$1 AND status='pending_approval' AND tenant_id = $3")
        .bind(a.entity_id)
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(rule("This transfer is no longer awaiting approval"));
    }
    Ok(())
}
