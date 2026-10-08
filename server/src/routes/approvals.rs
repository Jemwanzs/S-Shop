//! Approval inbox: list, approve, reject. Approving executes the deferred action
//! in the owning module, inside the same transaction as the decision.
//!
//! A request follows its own chain of steps (`approvals.steps`, copied from the workflow when it was raised) and each
//! decision records the step it approved. When a workflow is edited, `sync_pending` reconciles every pending request of
//! that action with the new chain (roadmap 49).

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as DbJson;
use uuid::Uuid;

use super::{Page, Paged};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{rule, AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;
use crate::workflow::{self, Level};

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
    /// Business the request belongs to (guards writes made when it is decided).
    #[serde(skip)]
    pub tenant_id: Uuid,
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
    /// 1-based position of the step the request waits for (pending) or was decided at.
    pub level: i32,
    /// [{level, step_id, user_id, user_name, decision, comments, at}]
    pub decisions: Value,
    /// The chain of steps this request follows.
    pub steps: DbJson<Vec<Level>>,
    /// Set when a workflow change needed an exception (see `workflow::reconcile`).
    pub sync_note: String,
    pub synced_at: Option<DateTime<Utc>>,
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

    /// Steps already approved (by step id).
    fn completed_steps(&self) -> Vec<Uuid> {
        self.decisions
            .as_array()
            .map(|d| {
                d.iter()
                    .filter(|x| x["decision"] == "approved")
                    .filter_map(|x| x["step_id"].as_str().and_then(|u| Uuid::parse_str(u).ok()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The step the request waits for (None: no configured chain — anyone holding approvals.approve).
    fn step(&self) -> Option<&Level> {
        self.steps.0.get((self.level - 1).max(0) as usize)
    }

    /// Steps this request must pass (at least one).
    fn total(&self) -> i32 {
        (self.steps.0.len() as i32).max(1).max(self.level)
    }
}

const SELECT: &str = "SELECT a.id, a.tenant_id, a.action, a.entity_type, a.entity_id, a.branch_id, b.name AS branch_name, a.summary, a.amount,
        a.payload, a.status, a.requested_by, ru.name AS requested_by_name, du.name AS decided_by_name, a.decided_at,
        a.comments, a.level, a.decisions, a.steps, a.sync_note, a.synced_at, a.created_at
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
    /// Who approves next (pending requests).
    next_approvers: Vec<String>,
}

/// The people a step designates — "who approves next". Administrators may decide any step, so they are listed only for
/// an administrator step or when nobody else is designated.
async fn responsible(conn: &mut sqlx::PgConnection, step: Option<&Level>, ids: &[Uuid]) -> AppResult<Vec<Uuid>> {
    if ids.is_empty() || step.is_none_or(|s| s.approver_type == "admin") {
        return Ok(ids.to_vec());
    }
    let named: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = ANY($1) AND NOT ('*' = ANY(r.permissions))",
    )
    .bind(ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(if named.is_empty() { ids.to_vec() } else { ids.iter().filter(|u| named.contains(u)).copied().collect() })
}

async fn names(conn: &mut sqlx::PgConnection, ids: &[Uuid]) -> AppResult<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_scalar("SELECT name FROM users WHERE id = ANY($1) ORDER BY name").bind(ids).fetch_all(&mut *conn).await?)
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
        let pending = row.status == "pending";
        let can_decide =
            pending && workflow::can_decide_step(&mut conn, ctx.tenant_id, row.step(), row.branch_id, row.requested_by, ctx.user_id, &decided).await?;
        // Users only see requests they raised or can decide, unless they hold audit visibility.
        if can_decide || row.requested_by == Some(ctx.user_id) || ctx.can("audit.view") || ctx.can("approvals.approve") {
            let next_approvers = if pending {
                let ids = workflow::approvers_for(&mut conn, ctx.tenant_id, row.step(), row.branch_id, row.requested_by, &decided).await?;
                let ids = responsible(&mut conn, row.step(), &ids).await?;
                names(&mut conn, &ids).await?
            } else {
                Vec::new()
            };
            let levels = row.total();
            items.push(Item { row, can_decide, levels, next_approvers });
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
    let ok = workflow::can_decide_step(conn, ctx.tenant_id, a.step(), a.branch_id, a.requested_by, ctx.user_id, &decided).await?;
    if ok {
        Ok(())
    } else {
        Err(AppError::Forbidden("You cannot decide this request (or you already decided an earlier level)".into()))
    }
}

fn decision(ctx: &Ctx, a: &ApprovalRow, verdict: &str, comments: &str) -> Value {
    json!([{
        "level": a.level, "step_id": a.step().and_then(|s| s.id), "user_id": ctx.user_id, "user_name": ctx.name,
        "decision": verdict, "comments": comments, "at": Utc::now(),
    }])
}

async fn approve(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<DecisionBody>>) -> AppResult<Json<Value>> {
    let comments = body.map(|b| b.0.comments).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, &ctx, id).await?;
    ensure_can_decide(&mut tx, &ctx, &a).await?;
    let total = a.total();
    // The next step still needing a decision (steps approved earlier out of order are not asked again).
    let mut completed = a.completed_steps();
    if let Some(step) = a.step().and_then(|s| s.id) {
        completed.push(step);
    }
    let next = workflow::next_level(&a.steps.0, a.level, &completed);

    // An intermediate level: record the decision and pass the request on.
    if let Some(next) = next {
        sqlx::query("UPDATE approvals SET level = $4, decisions = decisions || $2 WHERE id = $1 AND tenant_id = $3")
            .bind(id)
            .bind(decision(&ctx, &a, "approved", &comments))
            .bind(ctx.tenant_id)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        audit::record(
            &mut tx,
            &ctx,
            Entry::new("approvals", "approve_level", &a.entity_type, a.entity_id)
                .approval(Some(id))
                .after(json!({ "level": a.level, "of": total, "next": next }))
                .comments(&comments),
        )
        .await?;
        tx.commit().await?;
        let mut decided = a.decided_by();
        decided.push(ctx.user_id);
        let mut moved = a.clone();
        moved.level = next;
        notify_level(&state, ctx.tenant_id, &moved, &decided, "").await;
        if let Some(requester) = a.requested_by {
            notify::to_users(
                &state,
                ctx.tenant_id,
                &[requester],
                Note::new("approval_decided", format!("Level {} of {total} approved by {}", a.level, ctx.name), a.summary.clone(), "/approvals"),
            )
            .await;
        }
        state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "pending", "level": next }));
        return Ok(Json(json!({ "ok": true, "status": "pending", "level": next, "levels": total })));
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

    sqlx::query("UPDATE approvals SET status='approved', decided_by=$2, decided_at=now(), comments=$3, decisions = decisions || $4 WHERE id=$1 AND tenant_id = $5")
        .bind(id)
        .bind(ctx.user_id)
        .bind(&comments)
        .bind(decision(&ctx, &a, "approved", &comments))
        .bind(ctx.tenant_id)
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
    sqlx::query("UPDATE approvals SET status='rejected', decided_by=$2, decided_at=now(), comments=$3, decisions = decisions || $4 WHERE id=$1 AND tenant_id = $5")
        .bind(id)
        .bind(ctx.user_id)
        .bind(&comments)
        .bind(decision(&ctx, &a, "rejected", &comments))
        .bind(ctx.tenant_id)
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
    sqlx::query("UPDATE approvals SET status='cancelled', decided_by=$2, decided_at=now() WHERE id=$1 AND tenant_id = $3")
        .bind(id)
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
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
            sqlx::query("UPDATE stock_adjustments SET status='rejected', decided_at=now() WHERE id=$1 AND status='pending' AND tenant_id = $2")
                .bind(a.entity_id)
                .bind(a.tenant_id)
                .execute(&mut *conn)
                .await?;
        }
        "stock.transfer" => {
            sqlx::query("UPDATE transfers SET status='rejected' WHERE id=$1 AND status='pending_approval' AND tenant_id = $2")
                .bind(a.entity_id)
                .bind(a.tenant_id)
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

/// Tell the approvers of the request's current step that it is waiting for them (`only` limits it to some users).
async fn notify_level(state: &AppState, tenant_id: Uuid, a: &ApprovalRow, decided_by: &[Uuid], why: &str) {
    let Ok(mut conn) = state.db.acquire().await else { return };
    match workflow::approvers_for(&mut conn, tenant_id, a.step(), a.branch_id, a.requested_by, decided_by).await {
        Ok(users) => notify_users(state, tenant_id, a, &users, why).await,
        Err(e) => tracing::warn!(error = %e, "approver lookup failed"),
    }
}

async fn notify_users(state: &AppState, tenant_id: Uuid, a: &ApprovalRow, users: &[Uuid], why: &str) {
    if users.is_empty() {
        return;
    }
    let total = a.total();
    let step = if total > 1 { format!(" (level {} of {total})", a.level) } else { String::new() };
    let from = a.requested_by_name.clone().unwrap_or_default();
    let title = if why.is_empty() { format!("Approval needed{step} — from {from}") } else { format!("Approval needed{step} — {why}") };
    notify::to_users(state, tenant_id, users, Note::new("approval_pending", title, a.summary.clone(), "/approvals")).await;
}

/// Tell level-1 approvers a new request is waiting (called by modules right after `workflow::submit`).
pub async fn notify_approvers(state: &AppState, ctx: &Ctx, approval_id: Uuid) {
    let row: Result<Option<ApprovalRow>, _> = sqlx::query_as(&format!("{SELECT} WHERE a.id = $1")).bind(approval_id).fetch_optional(&state.db).await;
    if let Ok(Some(a)) = row {
        notify_level(state, ctx.tenant_id, &a, &[], "").await;
        state.emit(ctx.tenant_id, None, "approval", json!({ "id": approval_id, "status": "pending" }));
    }
}

// ───────────────────────────── Workflow changes (roadmap 49) ─────────────────────────────

/// One pending request moved by a workflow change.
#[derive(Serialize, Clone)]
pub struct Synced {
    pub id: Uuid,
    pub summary: String,
    pub previous_level: i32,
    pub level: i32,
    pub levels: i32,
    pub previous_next_approvers: Vec<String>,
    pub next_approvers: Vec<String>,
    pub note: String,
    #[serde(skip)]
    newly_responsible: Vec<Uuid>,
    #[serde(skip)]
    row: Option<ApprovalRow>,
}

/// Reconciles every pending request of `action` with the workflow's new chain of steps, inside the transaction that
/// saves the workflow: approvals already given are kept and never asked again, each request waits for its first step
/// not yet approved (never back to the start, never approved automatically), responsibility moves to the new
/// approvers, exceptions are flagged on the request and every moved request is audited with its previous and new next
/// approvers. Call `after_sync` once committed to notify and refresh queues.
pub async fn sync_pending(tx: &mut sqlx::PgConnection, ctx: &Ctx, action: &str, new_steps: &[Level]) -> AppResult<Vec<Synced>> {
    let rows: Vec<ApprovalRow> = sqlx::query_as(&format!(
        "{SELECT} WHERE a.tenant_id = $1 AND a.action = $2 AND a.status = 'pending' ORDER BY a.created_at FOR UPDATE OF a"
    ))
    .bind(ctx.tenant_id)
    .bind(action)
    .fetch_all(&mut *tx)
    .await?;
    let mut out = Vec::new();
    for a in rows {
        let decided = a.decided_by();
        let position = workflow::reconcile(new_steps, &a.completed_steps());
        let before_ids = workflow::approvers_for(tx, ctx.tenant_id, a.step(), a.branch_id, a.requested_by, &decided).await?;
        let mut moved = a.clone();
        moved.steps = DbJson(position.steps.clone());
        moved.level = position.level;
        moved.sync_note = position.note.clone();
        let after_ids = workflow::approvers_for(tx, ctx.tenant_id, moved.step(), a.branch_id, a.requested_by, &decided).await?;
        if a.steps.0 == position.steps && a.level == position.level && before_ids == after_ids && a.sync_note == position.note {
            continue; // not affected
        }
        sqlx::query("UPDATE approvals SET steps = $2, level = $3, sync_note = $4, synced_at = now() WHERE id = $1 AND tenant_id = $5")
            .bind(a.id)
            .bind(DbJson(&position.steps))
            .bind(position.level)
            .bind(&position.note)
            .bind(ctx.tenant_id)
            .execute(&mut *tx)
            .await?;
        let shown_before = responsible(tx, a.step(), &before_ids).await?;
        let shown_after = responsible(tx, moved.step(), &after_ids).await?;
        let previous_next_approvers = names(tx, &shown_before).await?;
        let next_approvers = names(tx, &shown_after).await?;
        audit::record(
            tx,
            ctx,
            Entry::new("approvals", "workflow_sync", &a.entity_type, a.entity_id)
                .approval(Some(a.id))
                .before(json!({ "level": a.level, "levels": a.total(), "steps": a.steps.0, "next_approvers": previous_next_approvers }))
                .after(json!({ "level": moved.level, "levels": moved.total(), "steps": position.steps, "next_approvers": next_approvers, "note": position.note })),
        )
        .await?;
        out.push(Synced {
            id: a.id,
            summary: a.summary.clone(),
            previous_level: a.level,
            level: moved.level,
            levels: moved.total(),
            previous_next_approvers,
            next_approvers,
            note: position.note,
            newly_responsible: after_ids.iter().filter(|u| !before_ids.contains(u)).copied().collect(),
            row: Some(moved),
        });
    }
    Ok(out)
}

/// After the workflow change is committed: notify newly responsible approvers and refresh every open inbox.
pub async fn after_sync(state: &AppState, tenant_id: Uuid, synced: &[Synced]) {
    for s in synced {
        if let Some(row) = &s.row {
            notify_users(state, tenant_id, row, &s.newly_responsible, "workflow updated").await;
        }
        state.emit(tenant_id, None, "approval", json!({ "id": s.id, "status": "pending", "level": s.level }));
    }
}
