//! Approval inbox: list, approve, reject. Approving executes the deferred action
//! in the owning module, inside the same transaction as the decision.
//!
//! A request follows its own chain of steps (`approvals.steps`, copied from the workflow when it was raised) and each
//! decision records the step it approved. When a workflow is edited, `sync_pending` reconciles every pending request of
//! that action with the new chain (roadmap 49).

use std::collections::HashMap;

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
use crate::error::{AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;
use crate::workflow::{self, Level};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/approvals", get(list))
        .route("/approvals/bulk", post(bulk))
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
    /// Roadmap 151: the requester's reason, and the bulk submission the request belongs to.
    pub note: String,
    pub bulk_ref: Option<Uuid>,
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
        a.comments, a.level, a.decisions, a.steps, a.sync_note, a.synced_at, a.created_at, a.note, a.bulk_ref
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
    /// Roadmap 148: who the current step is assigned to ("Administrator", "Role: Manager", …).
    assigned_to: String,
    /// Why the viewer cannot decide it (pending requests they cannot decide), with the explanation.
    why: Option<workflow::Why>,
    why_message: String,
    /// Nobody can decide the current step: "only_requester" (the only eligible person made the request) or "no_one".
    stuck: Option<&'static str>,
}

/// The people a step designates — "who approves next". Administrators may decide any step, so they are listed only for
/// an administrator step or when nobody else is designated.
async fn responsible(conn: &mut sqlx::PgConnection, step: Option<&Level>, ids: &[Uuid]) -> AppResult<Vec<Uuid>> {
    if ids.is_empty() || step.is_none_or(|s| s.approver_type == "admin") {
        return Ok(ids.to_vec());
    }
    let named: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = ANY($1) AND NOT ('*' = ANY(effective_permissions(r.permissions, u.extra_permissions)))",
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
        let why = if !pending {
            None
        } else if ctx.acting_from.is_some() {
            // Roadmap 148: a platform support session is not a person of this business (never decides its requests).
            Some(workflow::Why::NotInBusiness)
        } else {
            Some(workflow::eligibility_for(&mut conn, ctx.tenant_id, Some(&row.action), row.step(), row.branch_id, row.requested_by, ctx.user_id, &decided).await?)
        };
        let can_decide = why.is_some_and(|w| w.may_decide());
        // Users only see requests they raised or can decide, unless they hold audit visibility.
        if can_decide || row.requested_by == Some(ctx.user_id) || ctx.can("audit.view") || ctx.can("approvals.approve") {
            let (next_approvers, stuck) = if pending {
                let ids = workflow::approvers_for(&mut conn, ctx.tenant_id, Some(&row.action), row.step(), row.branch_id, row.requested_by, &decided).await?;
                // Nobody may decide this step: say so (and why) instead of leaving it waiting silently.
                let stuck = if !ids.is_empty() {
                    None
                } else if let Some(requester) = row.requested_by {
                    let alone = workflow::eligibility_for(&mut conn, ctx.tenant_id, None, row.step(), row.branch_id, None, requester, &decided).await?;
                    Some(if alone == workflow::Why::Eligible { "only_requester" } else { "no_one" })
                } else {
                    Some("no_one")
                };
                let ids = responsible(&mut conn, row.step(), &ids).await?;
                (names(&mut conn, &ids).await?, stuck)
            } else {
                (Vec::new(), None)
            };
            let assigned_to = if pending { workflow::step_label(&mut conn, row.step()).await? } else { String::new() };
            let why = why.filter(|w| !w.may_decide());
            let why_message = why.map(|w| w.message().to_string()).unwrap_or_default();
            let levels = row.total();
            // Roadmap 136: item barcodes inside a request (receipts, transfers, adjustments) leave the server masked; the
            // stored payload keeps the full codes the approved action needs.
            let mut row = row;
            super::barcodes::mask_payload(&mut row.payload);
            items.push(Item { row, can_decide, levels, next_approvers, assigned_to, why, why_message, stuck });
        }
    }
    let total = items.len() as i64;
    Ok(Json(Paged { items, total }))
}

#[derive(Deserialize, Default)]
struct DecisionBody {
    #[serde(default)]
    comments: String,
    /// Roadmap 150: the level the approver was looking at. Several people may decide one level and the first decision
    /// finalises it: a decision for a level that has since moved on is refused (*Already handled*), never applied to
    /// the next level.
    #[serde(default)]
    level: Option<i32>,
}

async fn load_pending(conn: &mut sqlx::PgConnection, ctx: &Ctx, id: Uuid) -> AppResult<ApprovalRow> {
    let row: ApprovalRow = sqlx::query_as(&format!("{SELECT} WHERE a.id = $1 AND a.tenant_id = $2 FOR UPDATE OF a"))
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Approval"))?;
    if row.status != "pending" {
        let by = row.decided_by_name.clone().unwrap_or_else(|| "someone".into());
        let when = row.decided_at.map(|d| d.format("%d %b %Y %H:%M UTC").to_string()).unwrap_or_default();
        let what = match row.status.as_str() {
            "cancelled" => "withdrawn".to_string(),
            s => s.to_string(),
        };
        return Err(crate::error::refused("Already handled", format!("This request was already {what} by {by} {when} — no action is needed")));
    }
    Ok(row)
}

/// Roadmap 150: first action wins — refuse a decision for a level that another approver already decided.
fn ensure_same_level(a: &ApprovalRow, seen: Option<i32>) -> AppResult<()> {
    match seen {
        Some(level) if level != a.level => {
            let last = a.decisions.as_array().and_then(|d| d.iter().rev().find(|x| x["level"].as_i64() == Some(level as i64)));
            let who = last.and_then(|d| d["user_name"].as_str()).unwrap_or("another approver");
            let when = last.and_then(|d| d["at"].as_str()).map(|s| s.chars().take(16).collect::<String>().replace('T', " ")).unwrap_or_default();
            Err(crate::error::refused(
                "Already handled",
                format!("Level {level} was already approved by {who} {when} UTC — the request now waits for level {} of {}", a.level, a.total()),
            ))
        }
        _ => Ok(()),
    }
}

async fn ensure_can_decide(conn: &mut sqlx::PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<workflow::Why> {
    let decided = a.decided_by();
    let why = if ctx.acting_from.is_some() {
        workflow::Why::NotInBusiness
    } else {
        workflow::eligibility_for(conn, ctx.tenant_id, Some(&a.action), a.step(), a.branch_id, a.requested_by, ctx.user_id, &decided).await?
    };
    if why.may_decide() {
        Ok(why)
    } else {
        Err(AppError::Forbidden(why.message().into()))
    }
}

fn decision(ctx: &Ctx, a: &ApprovalRow, verdict: &str, comments: &str, why: workflow::Why, bulk: Option<Uuid>) -> Value {
    json!([{
        "level": a.level, "step_id": a.step().and_then(|s| s.id), "user_id": ctx.user_id, "user_name": ctx.name,
        "decision": verdict, "comments": comments, "at": Utc::now(),
        "self_approved": why == workflow::Why::SelfApproval,
        "bulk_ref": bulk,
    }])
}

/// Roadmap 150: what the audit trail keeps for a decision — requester, approver and role, level, the other approvers
/// of that level, self-approval, the session's and the request's branch, and the outcome.
async fn decision_context(conn: &mut sqlx::PgConnection, ctx: &Ctx, a: &ApprovalRow, why: workflow::Why, others: &[Uuid], outcome: &str) -> AppResult<Value> {
    let role: Option<String> = sqlx::query_scalar("SELECT r.name FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1")
        .bind(ctx.user_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(json!({
        "request": a.summary, "requester": a.requested_by_name, "approver": ctx.name, "approver_role": role,
        "level": a.level, "of": a.total(), "assigned_to": workflow::step_label(conn, a.step()).await?,
        "other_approvers": names(conn, others).await?,
        "self_approved": why == workflow::Why::SelfApproval,
        "session_branch": ctx.branch_id, "request_branch": a.branch_id, "outcome": outcome,
    }))
}

/// Roadmap 150: the other approvers of a level are told it was handled — by whom, when, and what happens next.
async fn notify_handled(state: &AppState, tenant_id: Uuid, a: &ApprovalRow, others: &[Uuid], by: &str, outcome: &str) {
    if others.is_empty() {
        return;
    }
    let label = action_label(&a.action);
    let title = match outcome {
        "rejected" => format!("{label} rejected — no action needed"),
        "approved" => format!("{label} approved — no action needed"),
        _ => format!("{label}: level {} of {} approved — no action needed", a.level, a.total()),
    };
    let next = match outcome {
        "rejected" => "The request is closed.".to_string(),
        "approved" => "The request is fully approved and carried out.".to_string(),
        _ => format!("It now waits for level {} of {}.", a.level + 1, a.total()),
    };
    let verdict = if outcome == "rejected" { "rejected" } else { "approved" };
    let body = format!("{} — {verdict} by {by} at {} UTC. {next} No further action is required from you.", a.summary, Utc::now().format("%d %b %H:%M"));
    notify::to_users(state, tenant_id, others, Note::new("approval_handled", title, body, "/approvals").dedupe(format!("handled:{}:{}", a.id, a.level))).await;
}

fn action_label(action: &str) -> &'static str {
    workflow::ACTIONS.iter().find(|x| x.key == action).map(|x| x.label).unwrap_or("Request")
}

async fn approve(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<DecisionBody>>) -> AppResult<Json<Value>> {
    let body = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(approve_one(&state, &ctx, id, &body.comments, body.level, None).await?.value))
}

/// What one decision did — for the response and, in a bulk decision, for one summary notification per person.
pub struct Decided {
    pub value: Value,
    pub summary: String,
    pub requester: Option<Uuid>,
    /// The other approvers of the decided level (told it was handled).
    pub others: Vec<Uuid>,
    /// The request moved on to another level (its approvers are told).
    pub moved: Option<ApprovalRow>,
    pub decided: Vec<Uuid>,
}

/// One approval decision — the same checks for a single click and for every request of a bulk decision: the level
/// the person saw (first decision wins), eligibility (maker-checker, self-approval policy, approvers of the level),
/// the request row locked. `bulk`: the bulk operation it belongs to (recorded on the decision and in the audit trail;
/// notifications are then sent once per person by the caller).
pub async fn approve_one(state: &AppState, ctx: &Ctx, id: Uuid, comments: &str, level: Option<i32>, bulk: Option<Uuid>) -> AppResult<Decided> {
    let quiet = bulk.is_some();
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, ctx, id).await?;
    ensure_same_level(&a, level)?;
    let why = ensure_can_decide(&mut tx, ctx, &a).await?;
    // The other people this level was assigned to (told it was handled; never asked again).
    let others: Vec<Uuid> = workflow::approvers_for(&mut tx, ctx.tenant_id, Some(&a.action), a.step(), a.branch_id, a.requested_by, &a.decided_by())
        .await?
        .into_iter()
        .filter(|u| *u != ctx.user_id)
        .collect();
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
            .bind(decision(ctx, &a, "approved", comments, why, bulk))
            .bind(ctx.tenant_id)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        let mut detail = decision_context(&mut tx, ctx, &a, why, &others, &format!("passed to level {next} of {total}")).await?;
        detail["next"] = json!(next);
        detail["bulk_ref"] = json!(bulk);
        audit::record(
            &mut tx,
            ctx,
            Entry::new("approvals", "approve_level", &a.entity_type, a.entity_id).approval(Some(id)).after(detail).comments(comments),
        )
        .await?;
        tx.commit().await?;
        let mut decided = a.decided_by();
        decided.push(ctx.user_id);
        let mut moved = a.clone();
        moved.level = next;
        if !quiet {
            notify_handled(state, ctx.tenant_id, &a, &others, &ctx.name, "level").await;
            notify_level(state, ctx.tenant_id, &moved, &decided, "").await;
            if let Some(requester) = a.requested_by {
                notify::to_users(
                    state,
                    ctx.tenant_id,
                    &[requester],
                    Note::new("approval_decided", format!("Level {} of {total} approved by {}", a.level, ctx.name), a.summary.clone(), "/approvals"),
                )
                .await;
            }
        }
        state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "pending", "level": next }));
        return Ok(Decided {
            value: json!({ "ok": true, "status": "pending", "level": next, "levels": total }),
            summary: a.summary.clone(),
            requester: a.requested_by,
            others,
            moved: Some(moved),
            decided,
        });
    }

    match a.action.as_str() {
        "product.create" | "product.edit" | "product.deactivate" => super::catalog::on_approved(&mut tx, ctx, &a).await?,
        "stock.add" | "stock.adjust" | "stock.write_off" => super::stock::on_approved(&mut tx, ctx, &a).await?,
        "stock.transfer" => super::transfers::on_approved(&mut tx, ctx, &a).await?,
        "sale.return" | "sale.cancel" | "credit.recall" | "sale.owner_change" => super::sales::on_approved(&mut tx, ctx, &a).await?,
        "credit.write_off" => super::credit::on_approved(&mut tx, ctx, &a).await?,
        "expense" => super::expenses::on_decided(&mut tx, &a, true).await?,
        "data.import" => super::onboarding::on_approved(&mut tx, ctx, &a).await?,
        _ => {}
    }

    sqlx::query("UPDATE approvals SET status='approved', decided_by=$2, decided_at=now(), comments=$3, decisions = decisions || $4 WHERE id=$1 AND tenant_id = $5")
        .bind(id)
        .bind(ctx.user_id)
        .bind(comments)
        .bind(decision(ctx, &a, "approved", comments, why, bulk))
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    let mut detail = decision_context(&mut tx, ctx, &a, why, &others, "approved and carried out").await?;
    detail["bulk_ref"] = json!(bulk);
    audit::record(
        &mut tx,
        ctx,
        Entry::new("approvals", "approve", &a.entity_type, a.entity_id).approval(Some(id)).after(detail).comments(comments),
    )
    .await?;
    tx.commit().await?;

    if !quiet {
        notify_requester(state, ctx, &a, true, comments).await;
        notify_handled(state, ctx.tenant_id, &a, &others, &ctx.name, "approved").await;
    }
    state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "approved" }));
    Ok(Decided { value: json!({ "ok": true, "status": "approved" }), summary: a.summary.clone(), requester: a.requested_by, others, moved: None, decided: Vec::new() })
}

async fn reject(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, body: Option<Json<DecisionBody>>) -> AppResult<Json<Value>> {
    let body = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(reject_one(&state, &ctx, id, &body.comments, body.level, None).await?.value))
}

/// One rejection — the same checks as [`approve_one`].
pub async fn reject_one(state: &AppState, ctx: &Ctx, id: Uuid, comments: &str, level: Option<i32>, bulk: Option<Uuid>) -> AppResult<Decided> {
    let quiet = bulk.is_some();
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, ctx, id).await?;
    ensure_same_level(&a, level)?;
    let why = ensure_can_decide(&mut tx, ctx, &a).await?;
    let others: Vec<Uuid> = workflow::approvers_for(&mut tx, ctx.tenant_id, Some(&a.action), a.step(), a.branch_id, a.requested_by, &a.decided_by())
        .await?
        .into_iter()
        .filter(|u| *u != ctx.user_id)
        .collect();
    on_closed_without_approval(&mut tx, ctx, &a, "rejected").await?;
    sqlx::query("UPDATE approvals SET status='rejected', decided_by=$2, decided_at=now(), comments=$3, decisions = decisions || $4 WHERE id=$1 AND tenant_id = $5")
        .bind(id)
        .bind(ctx.user_id)
        .bind(comments)
        .bind(decision(ctx, &a, "rejected", comments, why, bulk))
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    let mut detail = decision_context(&mut tx, ctx, &a, why, &others, "rejected — nothing changed").await?;
    detail["bulk_ref"] = json!(bulk);
    audit::record(
        &mut tx,
        ctx,
        Entry::new("approvals", "reject", &a.entity_type, a.entity_id).approval(Some(id)).after(detail).comments(comments),
    )
    .await?;
    tx.commit().await?;
    if !quiet {
        notify_requester(state, ctx, &a, false, comments).await;
        notify_handled(state, ctx.tenant_id, &a, &others, &ctx.name, "rejected").await;
    }
    state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "rejected" }));
    Ok(Decided { value: json!({ "ok": true, "status": "rejected" }), summary: a.summary.clone(), requester: a.requested_by, others, moved: None, decided: Vec::new() })
}

#[derive(Deserialize)]
struct BulkBody {
    ids: Vec<Uuid>,
    /// approve | reject
    decision: String,
    #[serde(default)]
    comment: String,
    /// The level each request showed when it was selected (first decision wins, per request).
    #[serde(default)]
    levels: HashMap<Uuid, i32>,
}

const BULK_MAX: usize = 200;

/// Roadmap 151: decide several requests at once with one comment. Every request goes through the same single-request
/// decision (its own checks, transaction and row lock); one that is no longer actionable never stops the others. The
/// comment is recorded on every processed request's own decision; all are linked by one bulk reference. Notifications
/// go once per person (requesters, the other approvers of the decided levels, the next levels' approvers).
async fn bulk(State(state): State<AppState>, ctx: Ctx, Json(b): Json<BulkBody>) -> AppResult<Json<Value>> {
    let approve = match b.decision.as_str() {
        "approve" => true,
        "reject" => false,
        _ => return Err(crate::error::bad("Decision: approve or reject")),
    };
    let mut ids = b.ids.clone();
    ids.sort();
    ids.dedup();
    if ids.is_empty() || ids.len() > BULK_MAX {
        return Err(crate::error::bad(format!("Choose 1 to {BULK_MAX} requests")));
    }
    let bulk_ref = Uuid::new_v4();
    let comment = b.comment.trim().to_string();
    let mut items = Vec::new();
    let (mut approved, mut advanced, mut rejected, mut handled, mut refused, mut failed) = (0, 0, 0, 0, 0, 0);
    let mut to_requesters: HashMap<Uuid, Vec<String>> = HashMap::new();
    let mut to_others: HashMap<Uuid, usize> = HashMap::new();
    let mut moved: Vec<(ApprovalRow, Vec<Uuid>)> = Vec::new();
    for id in ids {
        let summary: Option<String> = sqlx::query_scalar("SELECT summary FROM approvals WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&state.db)
            .await?;
        let level = b.levels.get(&id).copied();
        let result = if approve { approve_one(&state, &ctx, id, &comment, level, Some(bulk_ref)).await } else { reject_one(&state, &ctx, id, &comment, level, Some(bulk_ref)).await };
        let (outcome, message, extra) = match result {
            Ok(d) => {
                let status = d.value["status"].as_str().unwrap_or_default().to_string();
                let outcome = match (approve, status.as_str()) {
                    (false, _) => {
                        rejected += 1;
                        "rejected"
                    }
                    (true, "pending") => {
                        advanced += 1;
                        "advanced"
                    }
                    _ => {
                        approved += 1;
                        "approved"
                    }
                };
                if let Some(r) = d.requester {
                    to_requesters.entry(r).or_default().push(format!("{} — {}", d.summary, outcome));
                }
                for o in &d.others {
                    *to_others.entry(*o).or_default() += 1;
                }
                if let Some(m) = d.moved {
                    moved.push((m, d.decided));
                }
                (outcome, String::new(), d.value)
            }
            Err(AppError::Refused { title, message }) if title == "Already handled" => {
                handled += 1;
                ("already_handled", message, Value::Null)
            }
            Err(AppError::Forbidden(m)) => {
                refused += 1;
                ("not_allowed", m, Value::Null)
            }
            Err(AppError::NotFound(_)) => {
                refused += 1;
                ("not_allowed", "Request not found".to_string(), Value::Null)
            }
            Err(e) => {
                failed += 1;
                ("failed", e.to_string(), Value::Null)
            }
        };
        items.push(json!({ "id": id, "summary": summary, "outcome": outcome, "message": message, "level": extra.get("level"), "levels": extra.get("levels") }));
    }
    // The bulk operation itself, in the audit trail (each request also has its own decision entry).
    let mut tx = state.db.begin().await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("approvals", if approve { "bulk_approve" } else { "bulk_reject" }, "approval", bulk_ref)
            .after(json!({
                "bulk_ref": bulk_ref, "selected": items.len(), "approved": approved, "advanced": advanced, "rejected": rejected,
                "already_handled": handled, "not_allowed": refused, "failed": failed, "session_branch": ctx.branch_id,
                "items": items.iter().map(|i| json!({ "id": i["id"], "outcome": i["outcome"] })).collect::<Vec<_>>(),
            }))
            .comments(&comment),
    )
    .await?;
    tx.commit().await?;
    // One notification per person.
    let verb = if approve { "approved" } else { "rejected" };
    for (requester, lines) in &to_requesters {
        let title = if lines.len() == 1 { format!("Your request was {verb} by {}", ctx.name) } else { format!("{} of your requests were decided by {}", lines.len(), ctx.name) };
        let body = format!("{}{}", lines.join("; "), if comment.is_empty() { String::new() } else { format!(" — “{comment}”") });
        notify::to_users(&state, ctx.tenant_id, &[*requester], Note::new("approval_decided", title, body, "/approvals").dedupe(format!("bulk:{bulk_ref}:req"))).await;
    }
    for (user, n) in &to_others {
        notify::to_users(
            &state,
            ctx.tenant_id,
            &[*user],
            Note::new("approval_handled", format!("{n} request(s) {verb} by {} — no action needed", ctx.name), format!("Handled together at {} UTC.", Utc::now().format("%d %b %H:%M")), "/approvals")
                .dedupe(format!("bulk:{bulk_ref}:others")),
        )
        .await;
    }
    for (m, decided) in &moved {
        notify_level(&state, ctx.tenant_id, m, decided, "").await;
    }
    Ok(Json(json!({
        "bulk_ref": bulk_ref,
        "summary": { "selected": items.len(), "approved": approved, "advanced": advanced, "rejected": rejected,
                     "already_handled": handled, "not_allowed": refused, "failed": failed },
        "items": items,
    })))
}

/// The requester may withdraw their own pending request.
async fn withdraw(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    let a = load_pending(&mut tx, &ctx, id).await?;
    if a.requested_by != Some(ctx.user_id) {
        return Err(AppError::Forbidden("Only the requester can withdraw a request".into()));
    }
    on_closed_without_approval(&mut tx, &ctx, &a, "withdrawn").await?;
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
async fn on_closed_without_approval(conn: &mut sqlx::PgConnection, ctx: &Ctx, a: &ApprovalRow, outcome: &str) -> AppResult<()> {
    match a.action.as_str() {
        // The original owner stays; the request is closed with who decided and when.
        "sale.owner_change" => {
            sqlx::query("UPDATE sale_owner_changes SET status = $3, decided_by = $4, decided_at = now() WHERE approval_id = $1 AND tenant_id = $2 AND status = 'pending'")
                .bind(a.id)
                .bind(a.tenant_id)
                .bind(outcome)
                .bind(ctx.user_id)
                .execute(&mut *conn)
                .await?;
        }
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
        "data.import" => super::onboarding::on_rejected(conn, ctx, a, outcome).await?,
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
    match workflow::approvers_for(&mut conn, tenant_id, Some(&a.action), a.step(), a.branch_id, a.requested_by, decided_by).await {
        Ok(users) if users.is_empty() => {
            // Roadmap 148: nobody may decide this step (e.g. the only administrator made the request). Say so at once
            // to the requester and to whoever manages workflows — never leave it waiting silently.
            let label = workflow::step_label(&mut conn, a.step()).await.unwrap_or_default();
            let note = Note::new(
                "approval_stuck",
                "Nobody can approve this request",
                format!("{} — the step is for: {label}. Change the approver in Settings → Workflow engine (the request follows the change) or withdraw the request.", a.summary),
                "/approvals",
            )
            .dedupe(format!("approval_stuck:{}:{}", a.id, a.level));
            let mut people: Vec<Uuid> = sqlx::query_scalar(
                "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id
                  WHERE u.tenant_id = $1 AND u.is_active
                    AND effective_permissions(r.permissions, u.extra_permissions) && ARRAY['*', 'settings.workflows']",
            )
            .bind(tenant_id)
            .fetch_all(&mut *conn)
            .await
            .unwrap_or_default();
            people.extend(a.requested_by);
            people.sort();
            people.dedup();
            notify::to_users(state, tenant_id, &people, note).await;
        }
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

/// Roadmap 151: requests created together (a bulk deactivation) — their approvers are told once ("15 requests waiting"),
/// not once per request. If nobody can approve them, the usual "nobody can approve" notice goes out instead.
pub async fn notify_approvers_bulk(state: &AppState, ctx: &Ctx, ids: &[Uuid], label: &str) {
    let Some(first) = ids.first() else { return };
    let row: Result<Option<ApprovalRow>, _> = sqlx::query_as(&format!("{SELECT} WHERE a.id = $1")).bind(first).fetch_optional(&state.db).await;
    let Ok(Some(a)) = row else { return };
    let users = match state.db.acquire().await {
        Ok(mut conn) => workflow::approvers_for(&mut conn, ctx.tenant_id, Some(&a.action), a.step(), a.branch_id, a.requested_by, &[]).await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    if users.is_empty() || ids.len() == 1 {
        notify_level(state, ctx.tenant_id, &a, &[], "").await;
    } else {
        notify::to_users(
            state,
            ctx.tenant_id,
            &users,
            Note::new("approval_pending", format!("{} requests waiting for approval — {label}", ids.len()), format!("Submitted together by {}", ctx.name), "/approvals")
                .dedupe(format!("bulk-submitted:{first}")),
        )
        .await;
    }
    for id in ids {
        state.emit(ctx.tenant_id, None, "approval", json!({ "id": id, "status": "pending" }));
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
        let before_ids = workflow::approvers_for(tx, ctx.tenant_id, Some(&a.action), a.step(), a.branch_id, a.requested_by, &decided).await?;
        let mut moved = a.clone();
        moved.steps = DbJson(position.steps.clone());
        moved.level = position.level;
        moved.sync_note = position.note.clone();
        let after_ids = workflow::approvers_for(tx, ctx.tenant_id, Some(&a.action), moved.step(), a.branch_id, a.requested_by, &decided).await?;
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
