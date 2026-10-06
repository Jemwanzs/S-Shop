//! Approval inbox: list, approve, reject. Approving executes the deferred action
//! in the owning module, inside the same transaction as the decision.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{Page, Paged};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{rule, AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approvals", get(list))
        .route("/approvals/{id}/approve", post(approve))
        .route("/approvals/{id}/reject", post(reject))
        .route("/approvals/{id}/withdraw", post(withdraw))
}

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct ApprovalRow {
    pub id: Uuid,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub branch_name: Option<String>,
    pub summary: String,
    pub amount: Option<Decimal>,
    pub payload: Value,
    pub status: String,
    pub requested_by: Option<Uuid>,
    pub requested_by_name: Option<String>,
    pub decided_by_name: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub comments: String,
    pub level: i32,
    /// [{level, user_id, user_name, decision, comments, at}]
    pub decisions: Value,
    pub created_at: DateTime<Utc>,
}

impl ApprovalRow {
    /// Users who already decided a level of this request.
    fn decided_by(&self) -> Vec<Uuid> {
        self.decisions
            .as_array()
            .map(|d| d.iter().filter_map(|x| x["user_id"].as_str().and_then(|u| Uuid::parse_str(u).ok())).collect())
            .unwrap_or_default()
    }
}

const SELECT: &str = "SELECT a.id, a.action, a.entity_type, a.entity_id, a.branch_id, b.name AS branch_name, a.summary, a.amount,
        a.payload, a.status, a.requested_by, ru.name AS requested_by_name, du.name AS decided_by_name, a.decided_at,
        a.comments, a.level, a.decisions, a.created_at
    FROM approvals a
    LEFT JOIN branches b ON b.id = a.branch_id
    LEFT JOIN users ru ON ru.id = a.requested_by
    LEFT JOIN users du ON du.id = a.decided_by";

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    #[serde(default, deserialize_with = "super::de::opt_bool")]
    mine: Option<bool>,
    #[serde(flatten)]
    page: Page,
}

#[derive(Serialize)]
struct Item {
    #[serde(flatten)]
    row: ApprovalRow,
    can_decide: bool,
    /// Total levels this request must pass.
    levels: i32,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Paged<Item>>> {
    let status = q.status.unwrap_or_else(|| "pending".into());
    let mine = q.mine.unwrap_or(false);
    let rows: Vec<ApprovalRow> = sqlx::query_as(&format!(
        "{SELECT} WHERE a.tenant_id = $1 AND ($2 = 'all' OR a.status = $2) AND (NOT $3 OR a.requested_by = $4)
           AND (a.branch_id IS NULL OR a.branch_id = ANY($5))
         ORDER BY a.created_at DESC LIMIT $6 OFFSET $7"
    ))
    .bind(ctx.tenant_id)
    .bind(&status)
    .bind(mine)
    .bind(ctx.user_id)
    .bind(&ctx.branch_ids)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;

    let mut conn = state.db.acquire().await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let decided = row.decided_by();
        let can_decide = row.status == "pending"
            && workflow::can_decide(
                &mut conn,
                ctx.tenant_id,
                &row.action,
                row.branch_id,
                row.requested_by,
                workflow::Decider { approver: ctx.user_id, level: row.level, decided_by: &decided },
            )
            .await?;
        // Users only see requests they raised or can decide, unless they hold audit visibility.
        if can_decide || row.requested_by == Some(ctx.user_id) || ctx.can("audit.view") || ctx.can("approvals.approve") {
            let levels = workflow::level_count(&mut conn, ctx.tenant_id, &row.action).await?.max(row.level);
            items.push(Item { row, can_decide, levels });
        }
    }
    let total = items.len() as i64;
    Ok(Json(Paged { items, total }))
}

#[derive(Deserialize, Default)]
struct DecisionBody {
    #[serde(default)]
    comments: String,
}

async fn load_pending(conn: &mut sqlx::PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<ApprovalRow> {
    let row: ApprovalRow = sqlx::query_as(&format!("{SELECT} WHERE a.id = $1 AND a.tenant_id = $2 FOR UPDATE OF a"))
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Approval"))?;
    if row.status != "pending" {
        return Err(rule(format!("This request was already {}", row.status)));
    }
    Ok(row)
}

async fn ensure_can_decide(conn: &mut sqlx::PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    let decided = a.decided_by();
    let ok = workflow::can_decide(
        conn,
        ctx.tenant_id,
        &a.action,
        a.branch_id,
        a.requested_by,
        workflow::Decider { approver: ctx.user_id, level: a.level, decided_by: &decided },
    )
    .await?;
    if ok {
        Ok(())
    } else {
        Err(AppError::Forbidden("You cannot decide this request (or you already decided an earlier level)".into()))
    }
}

fn decision(ctx: &Ctx, level: i32, verdict: &str, comments: &str) -> Value {
    json!([{ "level": level, "user_id": ctx.user_id, "user_name": ctx.name, "decision": verdict, "comments": comments, "at": Utc::now() }])
}

async fn approve(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<DecisionBody>>) -> AppResult<Json<Value>> {
    let comments = body.map(|b| b.0.comments).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, &ctx, id).await?;
    ensure_can_decide(&mut tx, &ctx, &a).await?;
    let total = workflow::level_count(&mut tx, ctx.tenant_id, &a.action).await?;

    // An intermediate level: record the decision and pass the request on.
    if a.level < total {
        sqlx::query("UPDATE approvals SET level = level + 1, decisions = decisions || $2 WHERE id = $1")
            .bind(id)
            .bind(decision(&ctx, a.level, "approved", &comments))
            .execute(&mut *tx)
            .await?;
        audit::record(
            &mut tx,
            &ctx,
            Entry::new("approvals", "approve_level", &a.entity_type, a.entity_id)
                .approval(Some(id))
                .after(json!({ "level": a.level, "of": total }))
                .comments(&comments),
        )
        .await?;
        tx.commit().await?;
        let mut decided = a.decided_by();
        decided.push(ctx.user_id);
        if let Some(requester) = a.requested_by {
            notify_level(&state, ctx.tenant_id, &a, requester, a.level + 1, &decided).await;
            notify::to_users(
                &state,
                ctx.tenant_id,
                &[requester],
                Note::new("approval_decided", format!("Level {} of {total} approved by {}", a.level, ctx.name), a.summary.clone(), "/approvals"),
            )
            .await;
        }
        state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "pending", "level": a.level + 1 }));
        return Ok(Json(json!({ "ok": true, "status": "pending", "level": a.level + 1, "levels": total })));
    }

    match a.action.as_str() {
        "product.create" | "product.edit" | "product.deactivate" => super::catalog::on_approved(&mut tx, &ctx, &a).await?,
        "stock.add" | "stock.adjust" | "stock.write_off" => super::stock::on_approved(&mut tx, &ctx, &a).await?,
        "stock.transfer" => super::transfers::on_approved(&mut tx, &ctx, &a).await?,
        "sale.return" | "sale.cancel" | "credit.recall" => super::sales::on_approved(&mut tx, &ctx, &a).await?,
        "credit.write_off" => super::credit::on_approved(&mut tx, &ctx, &a).await?,
        "expense" => super::expenses::on_decided(&mut tx, &a, true).await?,
        _ => {}
    }

    sqlx::query("UPDATE approvals SET status='approved', decided_by=$2, decided_at=now(), comments=$3, decisions = decisions || $4 WHERE id=$1")
        .bind(id)
        .bind(ctx.user_id)
        .bind(&comments)
        .bind(decision(&ctx, a.level, "approved", &comments))
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("approvals", "approve", &a.entity_type, a.entity_id).approval(Some(id)).comments(&comments),
    )
    .await?;
    tx.commit().await?;

    notify_requester(&state, &ctx, &a, true, &comments).await;
    state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "approved" }));
    Ok(Json(json!({ "ok": true, "status": "approved" })))
}

async fn reject(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<DecisionBody>>) -> AppResult<Json<Value>> {
    let comments = body.map(|b| b.0.comments).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, &ctx, id).await?;
    ensure_can_decide(&mut tx, &ctx, &a).await?;
    on_closed_without_approval(&mut tx, &a).await?;
    sqlx::query("UPDATE approvals SET status='rejected', decided_by=$2, decided_at=now(), comments=$3, decisions = decisions || $4 WHERE id=$1")
        .bind(id)
        .bind(ctx.user_id)
        .bind(&comments)
        .bind(decision(&ctx, a.level, "rejected", &comments))
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("approvals", "reject", &a.entity_type, a.entity_id).approval(Some(id)).comments(&comments),
    )
    .await?;
    tx.commit().await?;
    notify_requester(&state, &ctx, &a, false, &comments).await;
    state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "rejected" }));
    Ok(Json(json!({ "ok": true, "status": "rejected" })))
}

/// The requester may withdraw their own pending request.
async fn withdraw(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, &ctx, id).await?;
    if a.requested_by != Some(ctx.user_id) {
        return Err(AppError::Forbidden("Only the requester can withdraw a request".into()));
    }
    on_closed_without_approval(&mut tx, &a).await?;
    sqlx::query("UPDATE approvals SET status='cancelled', decided_by=$2, decided_at=now() WHERE id=$1")
        .bind(id)
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("approvals", "withdraw", &a.entity_type, a.entity_id).approval(Some(id))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Restore entities that were parked in a "pending" state.
async fn on_closed_without_approval(conn: &mut sqlx::PgConnection, a: &ApprovalRow) -> AppResult<()> {
    match a.action.as_str() {
        "stock.adjust" | "stock.write_off" => {
            sqlx::query("UPDATE stock_adjustments SET status='rejected', decided_at=now() WHERE id=$1 AND status='pending'")
                .bind(a.entity_id)
                .execute(&mut *conn)
                .await?;
        }
        "stock.transfer" => {
            sqlx::query("UPDATE transfers SET status='rejected' WHERE id=$1 AND status='pending_approval'")
                .bind(a.entity_id)
                .execute(&mut *conn)
                .await?;
        }
        "expense" => super::expenses::on_decided(conn, a, false).await?,
        _ => {}
    }
    Ok(())
}

async fn notify_requester(state: &AppState, ctx: &Ctx, a: &ApprovalRow, approved: bool, comments: &str) {
    if let Some(requester) = a.requested_by {
        let verdict = if approved { "approved" } else { "rejected" };
        let body = if comments.is_empty() { a.summary.clone() } else { format!("{} — “{comments}”", a.summary) };
        notify::to_users(
            state,
            ctx.tenant_id,
            &[requester],
            Note::new("approval_decided", format!("Request {verdict} by {}", ctx.name), body, "/approvals"),
        )
        .await;
    }
}

/// Tell the approvers of `level` that a request is waiting for them.
async fn notify_level(state: &AppState, tenant_id: Uuid, a: &ApprovalRow, requested_by: Uuid, level: i32, decided_by: &[Uuid]) {
    let Ok(mut conn) = state.db.acquire().await else { return };
    let total = workflow::level_count(&mut conn, tenant_id, &a.action).await.unwrap_or(1);
    match workflow::approvers(&mut conn, tenant_id, &a.action, a.branch_id, requested_by, level, decided_by).await {
        Ok(users) => {
            let step = if total > 1 { format!(" (level {level} of {total})") } else { String::new() };
            let from = a.requested_by_name.clone().unwrap_or_default();
            notify::to_users(
                state,
                tenant_id,
                &users,
                Note::new("approval_pending", format!("Approval needed{step} — from {from}"), a.summary.clone(), "/approvals"),
            )
            .await;
        }
        Err(e) => tracing::warn!(error = %e, "approver lookup failed"),
    }
}

/// Tell level-1 approvers a new request is waiting (called by modules right after `workflow::submit`).
pub async fn notify_approvers(state: &AppState, ctx: &Ctx, approval_id: Uuid) {
    let row: Result<Option<ApprovalRow>, _> = sqlx::query_as(&format!("{SELECT} WHERE a.id = $1")).bind(approval_id).fetch_optional(&state.db).await;
    if let Ok(Some(a)) = row {
        notify_level(state, ctx.tenant_id, &a, ctx.user_id, 1, &[]).await;
        state.emit(ctx.tenant_id, None, "approval", json!({ "id": approval_id, "status": "pending" }));
    }
}
