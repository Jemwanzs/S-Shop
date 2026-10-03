//! Stock transfers between branches:
//! Draft → Pending Approval → Approved → Dispatched (In Transit) → Received.
//! Stock leaves the source on dispatch and only becomes sellable at the
//! destination once receipt is confirmed (when receipt control is enabled).

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Counted, Page, Paged, Period};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
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
}

const SELECT: &str = "SELECT t.id, t.transfer_no, t.from_branch_id, fb.name AS from_branch_name, t.to_branch_id, tb.name AS to_branch_name,
        t.status, t.transfer_date, t.notes,
        (SELECT COALESCE(SUM(quantity),0) FROM transfer_items ti WHERE ti.transfer_id = t.id)::bigint AS total_units,
        cu.name AS created_by_name, au.name AS approved_by_name, du.name AS dispatched_by_name, ru.name AS received_by_name,
        t.approved_at, t.dispatched_at, t.received_at, t.created_at
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
    let (from, to) = q.period.resolve(ctx.tz, "all");
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
    let items: Vec<(Uuid, Uuid, String, String, i32, Option<String>)> = sqlx::query_as(
        "SELECT ti.id, ti.product_id, p.name, p.code, ti.quantity, si.barcode FROM transfer_items ti
         JOIN products p ON p.id = ti.product_id LEFT JOIN stock_items si ON si.id = ti.stock_item_id
         WHERE ti.transfer_id = $1 ORDER BY p.name",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let can = json!({
        "submit": t.status == "draft" && ctx.can("stock.transfer") && ctx.has_branch(t.from_branch_id),
        "dispatch": t.status == "approved" && ctx.can("stock.transfer") && ctx.has_branch(t.from_branch_id),
        "receive": t.status == "dispatched" && ctx.can("stock.receive_transfer") && ctx.has_branch(t.to_branch_id),
        "cancel": (["draft", "pending_approval", "approved"].contains(&t.status.as_str()) && ctx.can("stock.transfer") && ctx.has_branch(t.from_branch_id)),
    });
    Ok(Json(json!({
        "transfer": t,
        "items": items.into_iter().map(|(id, pid, name, code, qty, barcode)| json!({
            "id": id, "product_id": pid, "product_name": name, "product_code": code, "quantity": qty, "barcode": barcode,
        })).collect::<Vec<_>>(),
        "can": can,
    })))
}

#[derive(Deserialize)]
struct ItemBody {
    product_id: Uuid,
    quantity: i32,
    #[serde(default)]
    barcodes: Vec<String>,
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
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CreateBody>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
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
    audit::record(&mut tx, &ctx, Entry::new("transfers", "create", "transfer", id).branch(from).after(json!({
        "transfer_no": transfer_no, "to_branch_id": b.to_branch_id, "items": b.items.len()
    })))
    .await?;

    let mut approval = None;
    let mut status = "draft";
    if b.submit {
        (status, approval) = submit_inner(&mut tx, &ctx, id, from, &transfer_no).await?;
    }
    tx.commit().await?;
    if let Some(a) = approval {
        super::approvals::notify_approvers(&state, &ctx, "stock.transfer", Some(from), a).await;
    }
    Ok(Json(json!({ "id": id, "transfer_no": transfer_no, "status": status, "approval_id": approval })))
}

async fn submit_inner(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, from: Uuid, no: &str) -> AppResult<(&'static str, Option<Uuid>)> {
    if workflow::needs_approval(conn, ctx.tenant_id, "stock.transfer", None).await? {
        sqlx::query("UPDATE transfers SET status = 'pending_approval' WHERE id = $1").bind(id).execute(&mut *conn).await?;
        let approval = workflow::submit(
            conn,
            ctx,
            workflow::Request {
                action: "stock.transfer",
                entity_type: "transfer",
                entity_id: id,
                branch_id: Some(from),
                summary: format!("Stock transfer {no}"),
                amount: None,
                payload: json!({}),
            },
        )
        .await?;
        Ok(("pending_approval", Some(approval)))
    } else {
        sqlx::query("UPDATE transfers SET status = 'approved', approved_by = $2, approved_at = now() WHERE id = $1")
            .bind(id)
            .bind(ctx.user_id)
            .execute(&mut *conn)
            .await?;
        Ok(("approved", None))
    }
}

async fn submit(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
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
        super::approvals::notify_approvers(&state, &ctx, "stock.transfer", Some(t.from_branch_id), a).await;
    }
    Ok(Json(json!({ "status": status, "approval_id": approval })))
}

async fn transfer_items(conn: &mut PgConnection, id: Uuid) -> AppResult<Vec<(Uuid, Option<Uuid>, i32)>> {
    Ok(sqlx::query_as("SELECT product_id, stock_item_id, quantity FROM transfer_items WHERE transfer_id = $1")
        .bind(id)
        .fetch_all(&mut *conn)
        .await?)
}

async fn dispatch(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.from_branch_id)?;
    if t.status != "approved" {
        return Err(rule("Only approved transfers can be dispatched"));
    }
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let items = transfer_items(&mut tx, id).await?;
    let note = format!("Transfer {} → {}", t.transfer_no, t.to_branch_name);
    for (product_id, item_id, qty) in &items {
        if let Some(si) = item_id {
            let n = sqlx::query("UPDATE stock_items SET status='in_transit', updated_at=now() WHERE id=$1 AND status='in_stock' AND branch_id=$2")
                .bind(si)
                .bind(t.from_branch_id)
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
    sqlx::query("UPDATE transfers SET status='dispatched', dispatched_by=$2, dispatched_at=now() WHERE id=$1")
        .bind(id)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("transfers", "dispatch", "transfer", id).branch(t.from_branch_id)).await?;

    let mut status = "dispatched";
    if !s.stock.transfer_receipt_control {
        receive_inner(&mut tx, &ctx, &t, &items).await?;
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
    let products: Vec<Uuid> = items.iter().map(|i| i.0).collect();
    super::stock::alert_levels(&state, ctx.tenant_id, t.from_branch_id, &products).await;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": t.from_branch_id }));
    Ok(Json(json!({ "status": status })))
}

async fn receive_inner(conn: &mut PgConnection, ctx: &Ctx, t: &TransferRow, items: &[(Uuid, Option<Uuid>, i32)]) -> AppResult<()> {
    let note = format!("Transfer {} from {}", t.transfer_no, t.from_branch_name);
    for (product_id, item_id, qty) in items {
        if let Some(si) = item_id {
            sqlx::query("UPDATE stock_items SET status='in_stock', branch_id=$2, updated_at=now() WHERE id=$1 AND status='in_transit'")
                .bind(si)
                .bind(t.to_branch_id)
                .execute(&mut *conn)
                .await?;
        }
        let m = Movement::new(t.to_branch_id, *product_id, "transfer_in", *qty).item(*item_id).reference("transfer", t.id).notes(&note);
        inventory::apply(conn, ctx.tenant_id, Some(ctx.user_id), true, Check::None, m).await?;
    }
    sqlx::query("UPDATE transfers SET status='received', received_by=$2, received_at=now() WHERE id=$1")
        .bind(t.id)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    audit::record(conn, ctx, Entry::new("transfers", "receive", "transfer", t.id).branch(t.to_branch_id)).await
}

async fn receive(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("stock.receive_transfer")?;
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.to_branch_id)?;
    if t.status != "dispatched" {
        return Err(rule("Only dispatched transfers can be received"));
    }
    let items = transfer_items(&mut tx, id).await?;
    receive_inner(&mut tx, &ctx, &t, &items).await?;
    tx.commit().await?;
    if let Some(creator) = sqlx::query_scalar::<_, Option<Uuid>>("SELECT created_by FROM transfers WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await?
    {
        notify::to_users(
            &state,
            ctx.tenant_id,
            &[creator],
            Note::new("transfer_received", format!("Transfer {} received", t.transfer_no), format!("Confirmed by {} at {}", ctx.name, t.to_branch_name), format!("/transfers/{id}")),
        )
        .await;
    }
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": t.to_branch_id }));
    Ok(Json(json!({ "status": "received" })))
}

#[derive(Deserialize, Default)]
struct CancelBody {
    #[serde(default)]
    reason: String,
}

async fn cancel(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<CancelBody>>) -> AppResult<Json<Value>> {
    ctx.require("stock.transfer")?;
    let reason = body.map(|b| b.0.reason).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    let t = load(&mut tx, &ctx, id, true).await?;
    ctx.ensure_branch(t.from_branch_id)?;
    if !["draft", "pending_approval", "approved"].contains(&t.status.as_str()) {
        return Err(rule("A dispatched transfer cannot be cancelled — receive it, then transfer it back"));
    }
    sqlx::query("UPDATE transfers SET status='cancelled' WHERE id=$1").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE approvals SET status='cancelled', decided_by=$2, decided_at=now() WHERE entity_id=$1 AND status='pending'")
        .bind(id)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("transfers", "cancel", "transfer", id).branch(t.from_branch_id).comments(&reason)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "cancelled" })))
}

pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let n = sqlx::query("UPDATE transfers SET status='approved', approved_by=$2, approved_at=now() WHERE id=$1 AND status='pending_approval'")
        .bind(a.entity_id)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(rule("This transfer is no longer awaiting approval"));
    }
    Ok(())
}
