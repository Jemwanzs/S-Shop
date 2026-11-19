//! Roadmap 172: deleting inventory captured in error, through the approval chain.
//!
//! - A request names one receipt line (a `received` / `opening` stock movement, or the unit it brought in) and a written
//!   reason. While it waits nothing changes; one request may wait per line.
//! - After the final approval everything is checked again, then the stock is **reversed** by a new `receipt_reversal`
//!   movement in one transaction. The original movement is never deleted: it shows as *Reversed*.
//! - A unit removed this way becomes `voided`, and its barcode is released for reuse unless something still needs it
//!   (an active unit carries it, a waiting request names it, or it is a product's shared barcode).
//! - Stock already sold, transferred, written off or reserved is not deleted here: those have their own corrections.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::approvals::ApprovalRow;
use super::Outcome;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, AppError, AppResult};
use crate::inventory::{self, Check, Movement};
use crate::state::AppState;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stock/movements/{id}/void", get(check_movement).post(request_movement))
        .route("/stock/items/{id}/void", get(check_item).post(request_item))
}

/// The receipt line a request is about, and what deleting it would do now.
struct Line {
    movement: Uuid,
    branch: Uuid,
    branch_name: String,
    product: Uuid,
    product_name: String,
    item: Option<Uuid>,
    barcode: Option<String>,
    /// Units the line still holds (its quantity less what was already reversed).
    remaining: i32,
    on_hand: i32,
    available: i32,
    waiting: bool,
    /// Why it cannot be deleted, if it cannot.
    problem: Option<String>,
}

/// `except`: a request that should not count as waiting (the one being carried out).
async fn line(conn: &mut PgConnection, ctx: &Ctx, movement: Uuid, except: Option<Uuid>) -> AppResult<Line> {
    let row: Option<(Uuid, String, Uuid, String, String, i32, Option<Uuid>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT m.branch_id, b.name, m.product_id, p.name, m.kind, m.quantity, m.stock_item_id, si.barcode, si.status
           FROM stock_movements m JOIN branches b ON b.id = m.branch_id JOIN products p ON p.id = m.product_id
           LEFT JOIN stock_items si ON si.id = m.stock_item_id
          WHERE m.id = $1 AND m.tenant_id = $2",
    )
    .bind(movement)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (branch, branch_name, product, product_name, kind, qty, item, barcode, item_status) = row.ok_or(AppError::NotFound("Stock movement"))?;
    ctx.ensure_branch(branch)?;
    let (reversed, waiting): (i64, bool) = sqlx::query_as(
        "SELECT COALESCE(SUM(quantity) FILTER (WHERE status = 'completed'), 0)::bigint, bool_or(status = 'pending' AND id IS DISTINCT FROM $2) IS TRUE
           FROM stock_voids WHERE movement_id = $1",
    )
    .bind(movement)
    .bind(except)
    .fetch_one(&mut *conn)
    .await?;
    let level = sqlx::query_as::<_, (i32, i32)>("SELECT on_hand, reserved FROM stock_levels WHERE branch_id = $1 AND product_id = $2")
        .bind(branch)
        .bind(product)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or((0, 0));
    let remaining = (qty - reversed as i32).max(0);
    let problem = if !matches!(kind.as_str(), "received" | "opening") || qty <= 0 {
        Some("Only stock received or loaded as opening stock can be deleted. Use a stock adjustment for other corrections.".to_string())
    } else if remaining == 0 {
        Some("This receipt line has already been reversed.".to_string())
    } else if waiting {
        Some("A deletion of this receipt line is already waiting for approval.".to_string())
    } else if item.is_some() && item_status.as_deref() != Some("in_stock") {
        Some(format!(
            "This item is no longer in stock ({}). Sold, transferred or written-off stock is corrected by a return or an adjustment, not deleted.",
            item_status.unwrap_or_default().replace('_', " ")
        ))
    } else if level.0 - level.1 < 1 {
        Some("None of it is available here any more (sold, transferred or reserved). Use a return or an adjustment.".to_string())
    } else {
        None
    };
    Ok(Line {
        movement,
        branch,
        branch_name,
        product,
        product_name,
        item,
        barcode,
        remaining,
        on_hand: level.0,
        available: level.0 - level.1,
        waiting,
        problem,
    })
}

fn describe(l: &Line) -> Value {
    json!({
        "movement_id": l.movement, "product_id": l.product, "product": l.product_name, "branch_id": l.branch, "branch": l.branch_name,
        "barcode": l.barcode.as_deref().map(super::barcodes::mask), "tracked": l.item.is_some(),
        "max_quantity": if l.item.is_some() { 1 } else { l.remaining.min(l.available.max(0)) },
        "on_hand": l.on_hand, "available": l.available, "waiting": l.waiting, "eligible": l.problem.is_none(), "problem": l.problem,
    })
}

async fn movement_of_item(conn: &mut PgConnection, ctx: &Ctx, item: Uuid) -> AppResult<Uuid> {
    sqlx::query_scalar(
        "SELECT m.id FROM stock_movements m WHERE m.tenant_id = $1 AND m.stock_item_id = $2 AND m.kind IN ('received','opening')
          ORDER BY m.created_at DESC LIMIT 1",
    )
    .bind(ctx.tenant_id)
    .bind(item)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| refused("Nothing to delete", "This item has no receipt to reverse. Use a stock adjustment instead."))
}

async fn check_movement(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.adjust")?;
    let mut conn = state.db.acquire().await?;
    Ok(Json(describe(&line(&mut conn, &ctx, id, None).await?)))
}

async fn check_item(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.adjust")?;
    let mut conn = state.db.acquire().await?;
    let movement = movement_of_item(&mut conn, &ctx, id).await?;
    Ok(Json(describe(&line(&mut conn, &ctx, movement, None).await?)))
}

#[derive(Deserialize)]
struct RequestBody {
    #[serde(default)]
    reason: String,
    /// Units to delete from a shared-barcode receipt line (default: all it still holds). A unit line is always 1.
    quantity: Option<i32>,
}

async fn request_movement(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RequestBody>) -> AppResult<Json<Outcome<Value>>> {
    request(&state, &ctx, id, b).await
}

async fn request_item(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<RequestBody>) -> AppResult<Json<Outcome<Value>>> {
    let movement = movement_of_item(&mut *state.db.acquire().await?, &ctx, id).await?;
    request(&state, &ctx, movement, b).await
}

/// A deletion request: checked, recorded, and sent for approval (or applied at once where the business has switched the
/// *Inventory deletion* approval off).
async fn request(state: &AppState, ctx: &Ctx, movement: Uuid, b: RequestBody) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("stock.adjust")?;
    let reason = b.reason.trim().to_string();
    // Roadmap 175: any reason that says something ("Error", "Duplicate") is enough; blank or spaces only is refused.
    if reason.is_empty() {
        return Err(bad("Give the reason for deleting this stock"));
    }
    let mut tx = state.db.begin().await?;
    // The line is locked so two requests cannot be made at the same moment.
    sqlx::query("SELECT id FROM stock_movements WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(movement).bind(ctx.tenant_id).execute(&mut *tx).await?;
    let l = line(&mut tx, ctx, movement, None).await?;
    if let Some(p) = &l.problem {
        return Err(refused("Cannot delete this stock", p.clone()));
    }
    let max = if l.item.is_some() { 1 } else { l.remaining.min(l.available) };
    let qty = b.quantity.unwrap_or(max);
    if qty < 1 || qty > max {
        return Err(bad(format!("Quantity: 1 to {max}")));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO stock_voids (tenant_id, branch_id, product_id, movement_id, stock_item_id, quantity, reason, requested_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(l.branch)
    .bind(l.product)
    .bind(movement)
    .bind(l.item)
    .bind(qty)
    .bind(&reason)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        ctx,
        Entry::new("stock", "void_requested", "stock_void", id)
            .branch(l.branch)
            .after(json!({ "movement_id": movement, "product_id": l.product, "quantity": qty, "barcode": l.barcode.as_deref().map(super::barcodes::mask) }))
            .comments(&reason),
    )
    .await?;
    if workflow::needs_approval(&mut tx, ctx, "stock.void", workflow::Gate::branch(l.branch)).await? {
        let approval = workflow::submit(
            &mut tx,
            ctx,
            workflow::Request {
                action: "stock.void",
                entity_type: "stock_void",
                entity_id: id,
                branch_id: Some(l.branch),
                summary: format!(
                    "Delete inventory: {qty} × {}{} at {} — {reason}",
                    l.product_name,
                    l.barcode.as_deref().map(|c| format!(" ({})", super::barcodes::mask(c))).unwrap_or_default(),
                    l.branch_name
                ),
                amount: None,
                payload: json!({ "movement_id": movement, "quantity": qty, "on_hand": l.on_hand, "after": l.on_hand - qty }),
            },
        )
        .await?;
        sqlx::query("UPDATE stock_voids SET approval_id = $2 WHERE id = $1").bind(id).bind(approval).execute(&mut *tx).await?;
        tx.commit().await?;
        super::approvals::notify_approvers(state, ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    let done = apply(&mut tx, ctx, id, None).await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": l.branch }));
    Ok(Json(Outcome::done(done)))
}

/// Carry out an approved (or not approval-gated) deletion. Everything is checked again: the stock may have moved since.
async fn apply(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, approval: Option<Uuid>) -> AppResult<Value> {
    let v: Option<(Uuid, Uuid, Uuid, Option<Uuid>, i32, String, String)> = sqlx::query_as(
        "SELECT movement_id, branch_id, product_id, stock_item_id, quantity, reason, status FROM stock_voids WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (movement, branch, product, item, qty, reason, status) = v.ok_or(AppError::NotFound("Deletion request"))?;
    if status != "pending" {
        return Err(refused("Already processed", "This deletion was already carried out or closed."));
    }
    // The request itself no longer counts as "waiting" for the recheck.
    let l = line(conn, ctx, movement, Some(id)).await?;
    if let Some(p) = &l.problem {
        return Err(refused("Cannot delete this stock any more", format!("{p} Reject the request.")));
    }
    if qty > l.remaining || qty > l.available || (item.is_some() && qty != 1) {
        return Err(refused("Cannot delete this stock any more", "Less of it is available now than the request says. Reject it and ask again."));
    }
    // Deleting stock never takes a balance below zero, whatever the business allows for sales.
    let m = Movement::new(branch, product, "receipt_reversal", -qty).item(item).reference("stock_void", id).notes(&reason);
    let (reversal, after) = inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), false, Check::Available, m).await?;
    let mut released = false;
    if let Some(item) = item {
        sqlx::query("UPDATE stock_items SET status = 'voided', updated_at = now() WHERE id = $1 AND tenant_id = $2").bind(item).bind(ctx.tenant_id).execute(&mut *conn).await?;
        released = release_barcode(conn, ctx.tenant_id, l.barcode.as_deref().unwrap_or_default()).await?;
    }
    sqlx::query("UPDATE stock_voids SET status = 'completed', reversal_id = $2, barcode_released = $3, decided_by = $4, decided_at = now() WHERE id = $1")
        .bind(id)
        .bind(reversal)
        .bind(released)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    audit::record(
        &mut *conn,
        ctx,
        Entry::new("stock", "inventory_voided", "stock_void", id)
            .branch(branch)
            .before(json!({ "on_hand": after.on_hand + qty }))
            .after(json!({ "on_hand": after.on_hand, "quantity": qty, "movement_id": movement, "reversal_id": reversal, "barcode_released": released }))
            .approval(approval)
            .comments(&reason),
    )
    .await?;
    Ok(json!({ "id": id, "quantity": qty, "on_hand": after.on_hand, "barcode_released": released }))
}

/// A deleted unit's barcode becomes free again — unless an active unit carries it, a waiting request names it, or it is a
/// product's shared barcode. Codes S'Shop issued go back to *unused* for this business and product; codes the business
/// entered (a manufacturer's barcode) are forgotten entirely.
async fn release_barcode(conn: &mut PgConnection, tenant: Uuid, code: &str) -> AppResult<bool> {
    if code.is_empty() {
        return Ok(false);
    }
    let needed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM stock_items WHERE tenant_id = $1 AND barcode = $2 AND status IN ('in_stock','reserved','in_transit'))
             OR EXISTS (SELECT 1 FROM products WHERE tenant_id = $1 AND barcode = $2)
             OR EXISTS (SELECT 1 FROM approvals WHERE tenant_id = $1 AND status = 'pending' AND payload::text LIKE '%' || $2 || '%')",
    )
    .bind(tenant)
    .bind(code)
    .fetch_one(&mut *conn)
    .await?;
    if needed {
        return Ok(false);
    }
    sqlx::query("DELETE FROM barcode_registry WHERE code = $1 AND tenant_id = $2 AND source = 'entered'").bind(code).bind(tenant).execute(&mut *conn).await?;
    sqlx::query("UPDATE barcode_registry SET status = 'allocated', stock_item_id = NULL, assigned_at = NULL WHERE code = $1 AND tenant_id = $2 AND source = 'generated'")
        .bind(code)
        .bind(tenant)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// Final approval.
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    apply(conn, ctx, a.entity_id, Some(a.id)).await.map(|_| ())
}

/// Rejected or withdrawn: inventory unchanged; the request is closed.
pub async fn on_closed(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow, outcome: &str) -> AppResult<()> {
    let status = if outcome == "withdrawn" { "withdrawn" } else { "rejected" };
    sqlx::query("UPDATE stock_voids SET status = $3, decided_by = $4, decided_at = now() WHERE id = $1 AND tenant_id = $2 AND status = 'pending'")
        .bind(a.entity_id)
        .bind(a.tenant_id)
        .bind(status)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
