//! Maker-checker approval engine.
//!
//! A module asks `needs_approval()` before executing a sensitive action. If a
//! workflow is enabled for that action and its conditions match (amount
//! threshold, branch, requester role, expense category), an approval request is
//! stored with the original request as `payload`. The request moves through the
//! workflow's ordered **levels**; when the last level approves, `routes::approvals`
//! executes it in the owning module.
//!
//! Each level (step) has a stable id. A request keeps its own copy of the chain it follows (`approvals.steps`) and
//! every decision records the step it approved, so when a workflow is edited its pending requests are reconciled step
//! by step (`reconcile`, roadmap 49) rather than by position.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Ctx;
use crate::error::AppResult;

pub struct Action {
    pub key: &'static str,
    pub label: &'static str,
    pub uses_amount: bool,
    /// Expense rules can be limited to expense categories.
    pub uses_category: bool,
}

pub const ACTIONS: &[Action] = &[
    Action { key: "product.create", label: "Product creation", uses_amount: false, uses_category: false },
    Action { key: "product.edit", label: "Product edit", uses_amount: false, uses_category: false },
    Action { key: "product.deactivate", label: "Product deactivation", uses_amount: false, uses_category: false },
    Action { key: "stock.add", label: "Stock addition", uses_amount: true, uses_category: false },
    Action { key: "stock.adjust", label: "Stock adjustment / count", uses_amount: false, uses_category: false },
    Action { key: "stock.write_off", label: "Stock write-off", uses_amount: false, uses_category: false },
    Action { key: "stock.transfer", label: "Stock transfer", uses_amount: false, uses_category: false },
    Action { key: "sale.discount", label: "Excessive discount (supervisor PIN at the counter)", uses_amount: true, uses_category: false },
    Action { key: "sale.return", label: "Sale return / refund", uses_amount: true, uses_category: false },
    Action { key: "sale.cancel", label: "Sale cancellation", uses_amount: true, uses_category: false },
    Action { key: "credit.write_off", label: "Credit write-off", uses_amount: true, uses_category: false },
    Action { key: "sale.owner_change", label: "Sale ownership change", uses_amount: true, uses_category: false },
    Action { key: "credit.recall", label: "Credit sale recall", uses_amount: true, uses_category: false },
    Action { key: "expense", label: "Expense", uses_amount: true, uses_category: true },
];

/// One approval level: who may decide at this step.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Level {
    /// Stable identity of the step across workflow edits (assigned when the step is created).
    #[serde(default)]
    pub id: Option<Uuid>,
    /// admin | role | user | branch_manager
    pub approver_type: String,
    #[serde(default)]
    pub approver_role_id: Option<Uuid>,
    #[serde(default)]
    pub approver_user_id: Option<Uuid>,
}

/// When a rule applies. Empty lists mean “any”.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Conditions {
    pub branch_ids: Vec<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub category_ids: Vec<Uuid>,
}

#[derive(sqlx::FromRow)]
struct RuleRow {
    action: String,
    enabled: bool,
    levels: Json<Vec<Level>>,
    min_amount: Option<Decimal>,
    conditions: Json<Conditions>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Rule {
    pub action: String,
    pub enabled: bool,
    pub levels: Vec<Level>,
    pub min_amount: Option<Decimal>,
    pub conditions: Conditions,
}

impl From<RuleRow> for Rule {
    fn from(r: RuleRow) -> Self {
        Self { action: r.action, enabled: r.enabled, levels: r.levels.0, min_amount: r.min_amount, conditions: r.conditions.0 }
    }
}

pub async fn rules(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<Vec<Rule>> {
    let rows: Vec<RuleRow> = sqlx::query_as("SELECT action, enabled, levels, min_amount, conditions FROM workflows WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Rule::from).collect())
}

pub async fn rule_for(conn: &mut PgConnection, tenant_id: Uuid, action: &str) -> AppResult<Option<Rule>> {
    let row: Option<RuleRow> =
        sqlx::query_as("SELECT action, enabled, levels, min_amount, conditions FROM workflows WHERE tenant_id = $1 AND action = $2")
            .bind(tenant_id)
            .bind(action)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.map(Rule::from))
}

/// Facts about the request that conditions are evaluated against.
#[derive(Default, Clone, Copy)]
pub struct Gate {
    pub amount: Option<Decimal>,
    pub branch_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
}

impl Gate {
    pub fn branch(branch_id: Uuid) -> Self {
        Self { branch_id: Some(branch_id), ..Self::default() }
    }
    pub fn amount(mut self, amount: Decimal) -> Self {
        self.amount = Some(amount);
        self
    }
    pub fn category(mut self, category_id: Uuid) -> Self {
        self.category_id = Some(category_id);
        self
    }
}

/// Whether `action` must go through approval for this request.
/// A condition only narrows the rule when the request carries that fact.
pub async fn needs_approval(conn: &mut PgConnection, ctx: &Ctx, action: &str, gate: Gate) -> AppResult<bool> {
    let Some(rule) = rule_for(conn, ctx.tenant_id, action).await? else { return Ok(false) };
    if !rule.enabled {
        return Ok(false);
    }
    if let (Some(min), Some(amount)) = (rule.min_amount, gate.amount) {
        if amount < min {
            return Ok(false);
        }
    }
    let c = &rule.conditions;
    if let Some(b) = gate.branch_id {
        if !c.branch_ids.is_empty() && !c.branch_ids.contains(&b) {
            return Ok(false);
        }
    }
    if let Some(cat) = gate.category_id {
        if !c.category_ids.is_empty() && !c.category_ids.contains(&cat) {
            return Ok(false);
        }
    }
    if !c.role_ids.is_empty() {
        let role: Uuid = sqlx::query_scalar("SELECT role_id FROM users WHERE id = $1").bind(ctx.user_id).fetch_one(&mut *conn).await?;
        if !c.role_ids.contains(&role) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub struct Request<'a> {
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub summary: String,
    pub amount: Option<Decimal>,
    pub payload: Value,
}

pub async fn submit(conn: &mut PgConnection, ctx: &Ctx, r: Request<'_>) -> AppResult<Uuid> {
    // The request follows the workflow's chain as it is now; later edits reach it through `reconcile`.
    let steps = rule_for(conn, ctx.tenant_id, r.action).await?.map(|rule| rule.levels).unwrap_or_default();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO approvals (tenant_id, action, entity_type, entity_id, branch_id, summary, amount, payload, requested_by, steps)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(r.action)
    .bind(r.entity_type)
    .bind(r.entity_id)
    .bind(r.branch_id)
    .bind(&r.summary)
    .bind(r.amount)
    .bind(&r.payload)
    .bind(ctx.user_id)
    .bind(Json(&steps))
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Who is asking to decide, and what they have already done on this request.
pub struct Decider<'a> {
    pub approver: Uuid,
    pub level: i32,
    /// Users who decided earlier levels of this request.
    pub decided_by: &'a [Uuid],
}

/// Can `approver` decide `level` of a request for `action`, using the workflow as it is now? (The counter discount
/// supervisor check, which has no stored request; stored requests use `can_decide_step` with their own chain.)
pub async fn can_decide(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    action: &str,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    d: Decider<'_>,
) -> AppResult<bool> {
    let rule = rule_for(conn, tenant_id, action).await?;
    let step = rule.as_ref().and_then(|r| r.levels.get((d.level - 1).max(0) as usize)).cloned();
    can_decide_step(conn, tenant_id, step.as_ref(), branch_id, requested_by, d.approver, d.decided_by).await
}

/// Can `approver` decide `step` of a request raised in `branch_id` by `requested_by`? No step = a request without a
/// configured chain: anyone holding `approvals.approve`. Makers never check their own requests, and nobody decides two
/// levels of the same request.
pub async fn can_decide_step(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    step: Option<&Level>,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    approver: Uuid,
    decided_by: &[Uuid],
) -> AppResult<bool> {
    let d = Decider { approver, level: 0, decided_by };
    if requested_by == Some(d.approver) || d.decided_by.contains(&d.approver) {
        return Ok(false);
    }
    let row: Option<(Vec<String>, Uuid, bool)> = sqlx::query_as(
        "SELECT r.permissions, r.id, u.all_branches FROM users u JOIN roles r ON r.id = u.role_id
         WHERE u.id = $1 AND u.tenant_id = $2 AND u.is_active",
    )
    .bind(d.approver)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((perms, role_id, all_branches)) = row else { return Ok(false) };
    if perms.iter().any(|p| p == "*") {
        return Ok(true);
    }
    let may_approve = perms.iter().any(|p| p == "approvals.approve");
    let Some(level) = step else { return Ok(may_approve) };
    Ok(match level.approver_type.as_str() {
        "user" => level.approver_user_id == Some(d.approver),
        "role" => level.approver_role_id == Some(role_id),
        "branch_manager" => match branch_id {
            Some(b) => {
                let mgr: Option<Uuid> = sqlx::query_scalar("SELECT manager_id FROM branches WHERE id = $1").bind(b).fetch_one(&mut *conn).await?;
                mgr == Some(d.approver) || (all_branches && may_approve)
            }
            None => may_approve,
        },
        _ => false, // "admin" — handled above
    })
}

/// Users who may decide `step` of a request — the "who approves next" list, used for queues and notifications.
pub async fn approvers_for(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    step: Option<&Level>,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    decided_by: &[Uuid],
) -> AppResult<Vec<Uuid>> {
    let candidates: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND is_active ORDER BY name")
        .bind(tenant_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut out = Vec::new();
    for u in candidates {
        if can_decide_step(conn, tenant_id, step, branch_id, requested_by, u, decided_by).await? {
            out.push(u);
        }
    }
    Ok(out)
}

/// Steps sent without an id (API clients, older screens) keep the identity of an existing step with the same definition
/// (approver type, role, user), in order — so saving the same workflow again never turns approved steps into new ones.
pub fn inherit_ids(levels: &mut [Level], previous: &[Level]) {
    let mut taken: Vec<Uuid> = levels.iter().filter_map(|l| l.id).collect();
    for l in levels.iter_mut().filter(|l| l.id.is_none()) {
        if let Some(old) = previous.iter().find(|o| {
            o.id.is_some_and(|id| !taken.contains(&id))
                && o.approver_type == l.approver_type
                && o.approver_role_id == l.approver_role_id
                && o.approver_user_id == l.approver_user_id
        }) {
            l.id = old.id;
            taken.extend(old.id);
        }
    }
}

/// Gives every step an id (new steps get a new one; a repeated id is treated as a new step).
pub fn with_ids(levels: &mut [Level]) {
    let mut seen = Vec::new();
    for l in levels.iter_mut() {
        match l.id {
            Some(id) if !seen.contains(&id) => seen.push(id),
            _ => {
                let id = Uuid::new_v4();
                l.id = Some(id);
                seen.push(id);
            }
        }
    }
}

/// Where a pending request stands under a (changed) chain of steps.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    /// The chain the request follows from now on.
    pub steps: Vec<Level>,
    /// 1-based index of the step it waits for.
    pub level: i32,
    /// Why the result is exceptional, if it is ("" otherwise).
    pub note: String,
}

pub const NOTE_OUT_OF_ORDER: &str =
    "The workflow now has a step before an approval already given: that approval is kept and not asked again, and the request waits for the new step";
pub const NOTE_ALL_DONE: &str =
    "Every step of the changed workflow was already approved: an administrator confirms the final decision";

/// Reconciles a pending request with `steps` (roadmap 49). Steps already approved (`completed`, by step id) stay
/// approved and are never asked again; the request waits for the first step not yet approved — never back to the start,
/// and a step is never approved automatically. Exceptions are reported in `note`: a not-yet-approved step placed before
/// an approved one, and a chain whose steps are all approved already (then an administrator gives the final decision
/// as an extra step, instead of the action running unattended).
pub fn reconcile(steps: &[Level], completed: &[Uuid]) -> Position {
    let done = |l: &Level| l.id.is_some_and(|id| completed.contains(&id));
    match steps.iter().position(|l| !done(l)) {
        Some(i) => {
            let out_of_order = steps[i + 1..].iter().any(done);
            Position { steps: steps.to_vec(), level: i as i32 + 1, note: if out_of_order { NOTE_OUT_OF_ORDER.into() } else { String::new() } }
        }
        None => {
            let mut chain = steps.to_vec();
            chain.push(Level { id: Some(Uuid::new_v4()), approver_type: "admin".into(), approver_role_id: None, approver_user_id: None });
            Position { level: chain.len() as i32, steps: chain, note: NOTE_ALL_DONE.into() }
        }
    }
}

/// After step `level` (1-based) of `steps` is approved: the next step that still needs a decision (steps approved
/// earlier out of order are skipped, never asked twice), or None when the request is fully approved.
pub fn next_level(steps: &[Level], level: i32, completed: &[Uuid]) -> Option<i32> {
    steps
        .iter()
        .enumerate()
        .skip(level.max(0) as usize)
        .find(|(_, l)| !l.id.is_some_and(|id| completed.contains(&id)))
        .map(|(i, _)| i as i32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(n: u128, kind: &str) -> Level {
        Level { id: Some(Uuid::from_u128(n)), approver_type: kind.into(), approver_role_id: None, approver_user_id: None }
    }

    #[test]
    fn insert_after_current_step() {
        // Initiator → Manager → Finance, Manager approved; Operations inserted before Finance.
        let (mgr, ops, fin) = (step(1, "role"), step(2, "role"), step(3, "role"));
        let p = reconcile(&[mgr.clone(), ops.clone(), fin.clone()], &[Uuid::from_u128(1)]);
        assert_eq!((p.level, p.note.as_str()), (2, ""));
        assert_eq!(p.steps[1], ops);
    }

    #[test]
    fn nothing_completed_stays_at_the_start() {
        let p = reconcile(&[step(1, "admin"), step(2, "admin")], &[]);
        assert_eq!((p.level, p.note.as_str()), (1, ""));
    }

    #[test]
    fn insert_before_completed_step_is_flagged_not_rewritten() {
        // Manager approved; a new Audit step placed before Manager.
        let p = reconcile(&[step(9, "role"), step(1, "role"), step(3, "role")], &[Uuid::from_u128(1)]);
        assert_eq!(p.level, 1);
        assert_eq!(p.note, NOTE_OUT_OF_ORDER);
        // After Audit approves, Manager (already approved) is skipped, Finance is next.
        assert_eq!(next_level(&p.steps, 1, &[Uuid::from_u128(1), Uuid::from_u128(9)]), Some(3));
    }

    #[test]
    fn removed_future_steps_and_all_done() {
        // Manager approved; Finance removed: every remaining step is done → an administrator decides, never automatic.
        let p = reconcile(&[step(1, "role")], &[Uuid::from_u128(1)]);
        assert_eq!((p.level, p.steps.len(), p.note.as_str()), (2, 2, NOTE_ALL_DONE));
        assert_eq!(p.steps[1].approver_type, "admin");
    }

    #[test]
    fn same_steps_without_ids_keep_their_identity() {
        let old = vec![step(1, "role"), step(2, "admin")];
        let mut new = vec![
            Level { id: None, approver_type: "admin".into(), approver_role_id: None, approver_user_id: None },
            Level { id: None, approver_type: "role".into(), approver_role_id: None, approver_user_id: None },
            Level { id: None, approver_type: "admin".into(), approver_role_id: None, approver_user_id: None },
        ];
        inherit_ids(&mut new, &old);
        with_ids(&mut new);
        assert_eq!(new[0].id, Some(Uuid::from_u128(2)));
        assert_eq!(new[1].id, Some(Uuid::from_u128(1)));
        assert!(new[2].id.is_some() && new[2].id != Some(Uuid::from_u128(2)));
    }

    #[test]
    fn ids_assigned_once() {
        let mut l = vec![step(1, "admin"), Level { id: None, approver_type: "admin".into(), approver_role_id: None, approver_user_id: None }, step(1, "role")];
        with_ids(&mut l);
        assert_eq!(l[0].id, Some(Uuid::from_u128(1)));
        assert!(l[1].id.is_some() && l[2].id.is_some() && l[2].id != Some(Uuid::from_u128(1)));
        assert_eq!(next_level(&l, 3, &[]), None);
    }
}
