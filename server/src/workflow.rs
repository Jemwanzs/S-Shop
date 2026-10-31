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
    Action { key: "data.import", label: "Data import (onboarding)", uses_amount: false, uses_category: false },
];

/// One approver of a step: the Administrator (full access), a role, a person or the branch manager.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Approver {
    /// admin | role | user | branch_manager
    pub approver_type: String,
    #[serde(default)]
    pub approver_role_id: Option<Uuid>,
    #[serde(default)]
    pub approver_user_id: Option<Uuid>,
}

/// One approval level: who may decide at this step. Roadmap 150: a step may name several approvers (`approvers`) —
/// any one of them decides it, and the first decision finalises the step. Steps saved before keep their single approver
/// in the `approver_*` fields (also filled with the first approver of a list, for older readers).
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approvers: Vec<Approver>,
}

impl Level {
    /// A step with one approver.
    pub fn single(approver_type: &str) -> Self {
        Self { id: Some(Uuid::new_v4()), approver_type: approver_type.into(), approver_role_id: None, approver_user_id: None, approvers: Vec::new() }
    }
    /// Everyone the step names (any one of them may decide it).
    pub fn any_of(&self) -> Vec<Approver> {
        if self.approvers.is_empty() {
            vec![Approver { approver_type: self.approver_type.clone(), approver_role_id: self.approver_role_id, approver_user_id: self.approver_user_id }]
        } else {
            self.approvers.clone()
        }
    }
    /// Keeps the single-approver fields equal to the first approver (older readers, step matching).
    pub fn normalise(&mut self) {
        if let Some(first) = self.approvers.first().cloned() {
            self.approver_type = first.approver_type;
            self.approver_role_id = first.approver_role_id;
            self.approver_user_id = first.approver_user_id;
        }
        if self.approvers.len() == 1 {
            self.approvers.clear();
        }
    }
}

/// Roadmap 150: requests a Tenant Administrator may never approve themselves, whatever the business's policy —
/// money leaving the business or owed to it (refunds, cancellations, credit write-offs and recalls, expenses,
/// counter discounts).
pub const PROTECTED_FROM_SELF_APPROVAL: &[&str] = &["expense", "credit.write_off", "credit.recall", "sale.return", "sale.cancel", "sale.discount"];

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

/// Why a person may or may not decide a request's current step (roadmap 148). The screen shows the reason instead of a
/// silently missing Approve button; the decision endpoints answer with the same reason.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    Eligible,
    /// Roadmap 150: eligible as the request's own maker — a verified Tenant Administrator under the business's
    /// self-approval policy (recorded on the decision and in the audit trail).
    SelfApproval,
    /// Makers never check their own requests.
    OwnRequest,
    /// Nobody decides two levels of the same request.
    DecidedEarlierLevel,
    /// Not an active person of this business (e.g. a platform support session).
    NotInBusiness,
    /// The step is for someone else: another role or person, the branch manager, or an administrator (full access).
    NotDesignated,
    /// A request without a configured step needs the *Approve requests* permission.
    NoPermission,
}

impl Why {
    /// The explanation shown to the person (and returned by approve / reject).
    pub fn message(self) -> &'static str {
        match self {
            Why::Eligible | Why::SelfApproval => "",
            Why::OwnRequest => "You made this request, so someone else must approve it",
            Why::DecidedEarlierLevel => "You already decided an earlier level of this request — the next level needs another person",
            Why::NotInBusiness => "Only people of this business can approve its requests",
            Why::NotDesignated => "This step is assigned to another approver",
            Why::NoPermission => "Your role does not have permission to approve requests",
        }
    }
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
    Ok(eligibility(conn, tenant_id, step, branch_id, requested_by, approver, decided_by).await?.may_decide())
}

impl Why {
    pub fn may_decide(self) -> bool {
        matches!(self, Why::Eligible | Why::SelfApproval)
    }
}

/// [`can_decide_step`] with the reason. Permissions are the person's **effective** permissions (role plus personal
/// grants, minus personal removals) — the same ones every other check uses, so the Approvals screen and the decision
/// agree with what the person may do elsewhere. An administrator step is decided by full access (`*`).
pub async fn eligibility(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    step: Option<&Level>,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    approver: Uuid,
    decided_by: &[Uuid],
) -> AppResult<Why> {
    eligibility_for(conn, tenant_id, None, step, branch_id, requested_by, approver, decided_by).await
}

/// [`eligibility`] for a stored request's `action`: a verified Tenant Administrator (the business's system role with
/// full access — never a role merely named so) may decide their own request when the business allows it
/// (`security.admin_self_approval`) and the action is not protected. Without an action, makers never decide their own.
#[allow(clippy::too_many_arguments)]
pub async fn eligibility_for(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    action: Option<&str>,
    step: Option<&Level>,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    approver: Uuid,
    decided_by: &[Uuid],
) -> AppResult<Why> {
    if decided_by.contains(&approver) {
        return Ok(Why::DecidedEarlierLevel);
    }
    let row: Option<(Vec<String>, Uuid, bool, bool)> = sqlx::query_as(
        "SELECT effective_permissions(r.permissions, u.extra_permissions), r.id, u.all_branches,
                (r.is_system AND '*' = ANY(r.permissions)) AS tenant_admin
           FROM users u JOIN roles r ON r.id = u.role_id
          WHERE u.id = $1 AND u.tenant_id = $2 AND u.is_active AND r.tenant_id = $2",
    )
    .bind(approver)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let own = requested_by == Some(approver);
    let Some((perms, role_id, all_branches, tenant_admin)) = row else {
        return Ok(if own { Why::OwnRequest } else { Why::NotInBusiness });
    };
    if own {
        let allowed = match action {
            Some(a) if tenant_admin && !PROTECTED_FROM_SELF_APPROVAL.contains(&a) => {
                self_approval_policy(conn, tenant_id).await?
            }
            _ => false,
        };
        if !allowed {
            return Ok(Why::OwnRequest);
        }
    }
    let yes = if own { Why::SelfApproval } else { Why::Eligible };
    if perms.iter().any(|p| p == "*") {
        return Ok(yes);
    }
    let may_approve = perms.iter().any(|p| p == "approvals.approve");
    let Some(level) = step else { return Ok(if may_approve { yes } else { Why::NoPermission }) };
    let mut manager: Option<Option<Uuid>> = None;
    for a in level.any_of() {
        let ok = match a.approver_type.as_str() {
            "user" => a.approver_user_id == Some(approver),
            "role" => a.approver_role_id == Some(role_id),
            "branch_manager" => match branch_id {
                Some(b) => {
                    if manager.is_none() {
                        manager = Some(sqlx::query_scalar("SELECT manager_id FROM branches WHERE id = $1").bind(b).fetch_one(&mut *conn).await?);
                    }
                    manager.flatten() == Some(approver) || (all_branches && may_approve)
                }
                None => may_approve,
            },
            _ => false, // "admin" — full access only (handled above)
        };
        if ok {
            return Ok(yes);
        }
    }
    Ok(if level.any_of().iter().all(|a| a.approver_type == "branch_manager") && branch_id.is_none() { Why::NoPermission } else { Why::NotDesignated })
}

/// The business's Tenant Administrator self-approval policy, read from its one setting (never by parsing the whole
/// settings document). Not set → the documented default (on); set to anything but a boolean → off (fails closed).
pub async fn self_approval_policy(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<bool> {
    Ok(sqlx::query_scalar::<_, Option<bool>>(SELF_APPROVAL_SQL).bind(tenant_id).fetch_optional(&mut *conn).await?.flatten().unwrap_or(false))
}

pub const SELF_APPROVAL_SQL: &str = "SELECT CASE
        WHEN jsonb_typeof(settings->'security'->'admin_self_approval') = 'boolean' THEN (settings->'security'->>'admin_self_approval')::boolean
        WHEN settings->'security' ? 'admin_self_approval' THEN false
        ELSE true END
      FROM tenants WHERE id = $1";

/// Who a step is assigned to, in words: "Administrator", the role's or person's name, "Branch manager".
pub async fn step_label(conn: &mut PgConnection, step: Option<&Level>) -> AppResult<String> {
    let Some(s) = step else { return Ok("Anyone who may approve requests".into()) };
    let mut parts = Vec::new();
    for a in s.any_of() {
        parts.push(approver_label(conn, &a).await?);
    }
    Ok(parts.join(" · "))
}

/// One approver in words.
pub async fn approver_label(conn: &mut PgConnection, s: &Approver) -> AppResult<String> {
    Ok(match s.approver_type.as_str() {
        "role" => sqlx::query_scalar::<_, String>("SELECT name FROM roles WHERE id = $1").bind(s.approver_role_id).fetch_optional(&mut *conn).await?
            .map_or_else(|| "A role that no longer exists".into(), |n| format!("Role: {n}")),
        "user" => sqlx::query_scalar::<_, String>("SELECT name FROM users WHERE id = $1").bind(s.approver_user_id).fetch_optional(&mut *conn).await?
            .map_or_else(|| "A person who no longer exists".into(), |n| format!("Person: {n}")),
        "branch_manager" => "Branch manager".into(),
        _ => "Administrator".into(),
    })
}

/// Users who may decide `step` of a request — the "who approves next" list, used for queues and notifications.
pub async fn approvers_for(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    action: Option<&str>,
    step: Option<&Level>,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    decided_by: &[Uuid],
) -> AppResult<Vec<Uuid>> {
    // Roadmap 152: only people who could possibly qualify are checked — full access, the step's roles and people, the
    // branch manager (or all-branch approvers), anyone holding *Approve requests* for a step without a named approver.
    // Each candidate still goes through the full eligibility check, so the answer is the same as checking everyone.
    let list = step.map(|s| s.any_of()).unwrap_or_default();
    let roles: Vec<Uuid> = list.iter().filter_map(|a| a.approver_role_id.filter(|_| a.approver_type == "role")).collect();
    let users: Vec<Uuid> = list.iter().filter_map(|a| a.approver_user_id.filter(|_| a.approver_type == "user")).collect();
    let manager = list.iter().any(|a| a.approver_type == "branch_manager");
    let open = step.is_none() || (manager && branch_id.is_none());
    let candidates: Vec<Uuid> = sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id
          WHERE u.tenant_id = $1 AND u.is_active AND (
                '*' = ANY(effective_permissions(r.permissions, u.extra_permissions))
             OR u.role_id = ANY($2) OR u.id = ANY($3)
             OR ($4 AND (u.id = (SELECT manager_id FROM branches WHERE id = $5)
                         OR (u.all_branches AND 'approvals.approve' = ANY(effective_permissions(r.permissions, u.extra_permissions)))))
             OR ($6 AND 'approvals.approve' = ANY(effective_permissions(r.permissions, u.extra_permissions))))
          ORDER BY u.name",
    )
    .bind(tenant_id)
    .bind(&roles)
    .bind(&users)
    .bind(manager && branch_id.is_some())
    .bind(branch_id)
    .bind(open)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::new();
    for u in candidates {
        if eligibility_for(conn, tenant_id, action, step, branch_id, requested_by, u, decided_by).await?.may_decide() {
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
                && o.any_of() == l.any_of()
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
            chain.push(Level::single("admin"));
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

    #[test]
    fn a_step_names_one_or_several_approvers() {
        let mut l = step(1, "admin");
        assert_eq!(l.any_of().len(), 1, "older steps keep their single approver");
        let role = Uuid::from_u128(77);
        l.approvers = vec![
            Approver { approver_type: "role".into(), approver_role_id: Some(role), approver_user_id: None },
            Approver { approver_type: "branch_manager".into(), approver_role_id: None, approver_user_id: None },
        ];
        l.normalise();
        assert_eq!(l.any_of().len(), 2);
        assert_eq!((l.approver_type.as_str(), l.approver_role_id), ("role", Some(role)), "first approver mirrored for older readers");
        let mut one = step(2, "admin");
        one.approvers = vec![Approver { approver_type: "user".into(), approver_role_id: None, approver_user_id: Some(role) }];
        one.normalise();
        assert!(one.approvers.is_empty() && one.approver_type == "user", "a list of one is stored the old way");
    }

    #[test]
    fn saving_the_same_multi_approver_step_keeps_its_identity() {
        let mut old = step(5, "admin");
        old.approvers = vec![
            Approver { approver_type: "admin".into(), approver_role_id: None, approver_user_id: None },
            Approver { approver_type: "branch_manager".into(), approver_role_id: None, approver_user_id: None },
        ];
        let mut again = old.clone();
        again.id = None;
        let mut levels = vec![again];
        inherit_ids(&mut levels, &[old.clone()]);
        assert_eq!(levels[0].id, old.id);
        let mut changed = old.clone();
        changed.id = None;
        changed.approvers.pop();
        let mut levels = vec![changed];
        inherit_ids(&mut levels, &[old]);
        assert_eq!(levels[0].id, None, "a different set of approvers is a new step");
    }

    #[test]
    fn money_actions_are_never_self_approved() {
        for a in ["expense", "credit.write_off", "sale.return", "sale.cancel", "credit.recall", "sale.discount"] {
            assert!(PROTECTED_FROM_SELF_APPROVAL.contains(&a));
        }
        for a in ["product.deactivate", "stock.adjust", "stock.add"] {
            assert!(!PROTECTED_FROM_SELF_APPROVAL.contains(&a));
        }
    }

    fn step(n: u128, kind: &str) -> Level {
        Level { id: Some(Uuid::from_u128(n)), approver_type: kind.into(), approver_role_id: None, approver_user_id: None, approvers: Vec::new() }
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
            Level { id: None, approver_type: "admin".into(), approver_role_id: None, approver_user_id: None, approvers: Vec::new() },
            Level { id: None, approver_type: "role".into(), approver_role_id: None, approver_user_id: None, approvers: Vec::new() },
            Level { id: None, approver_type: "admin".into(), approver_role_id: None, approver_user_id: None, approvers: Vec::new() },
        ];
        inherit_ids(&mut new, &old);
        with_ids(&mut new);
        assert_eq!(new[0].id, Some(Uuid::from_u128(2)));
        assert_eq!(new[1].id, Some(Uuid::from_u128(1)));
        assert!(new[2].id.is_some() && new[2].id != Some(Uuid::from_u128(2)));
    }

    #[test]
    fn ids_assigned_once() {
        let mut l = vec![step(1, "admin"), Level { id: None, approver_type: "admin".into(), approver_role_id: None, approver_user_id: None, approvers: Vec::new() }, step(1, "role")];
        with_ids(&mut l);
        assert_eq!(l[0].id, Some(Uuid::from_u128(1)));
        assert!(l[1].id.is_some() && l[2].id.is_some() && l[2].id != Some(Uuid::from_u128(1)));
        assert_eq!(next_level(&l, 3, &[]), None);
    }
}
