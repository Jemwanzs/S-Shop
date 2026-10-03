//! Maker-checker approval engine.
//!
//! A module asks `needs_approval()` before executing a sensitive action. If a
//! workflow is enabled for that action and its conditions match (amount
//! threshold, branch, requester role, expense category), an approval request is
//! stored with the original request as `payload`. The request moves through the
//! workflow's ordered **levels**; when the last level approves, `routes::approvals`
//! executes it in the owning module.

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
    Action { key: "expense", label: "Expense", uses_amount: true, uses_category: true },
];

/// One approval level: who may decide at this step.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Level {
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
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO approvals (tenant_id, action, entity_type, entity_id, branch_id, summary, amount, payload, requested_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
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
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// Number of levels a request for `action` must pass (at least one).
pub async fn level_count(conn: &mut PgConnection, tenant_id: Uuid, action: &str) -> AppResult<i32> {
    Ok(rule_for(conn, tenant_id, action).await?.map(|r| r.levels.len().max(1) as i32).unwrap_or(1))
}

/// Who is asking to decide, and what they have already done on this request.
pub struct Decider<'a> {
    pub approver: Uuid,
    pub level: i32,
    /// Users who decided earlier levels of this request.
    pub decided_by: &'a [Uuid],
}

/// Can `approver` decide `level` of a request for `action` raised in `branch_id` by `requested_by`?
/// Makers never check their own requests, and nobody decides two levels of the same request.
pub async fn can_decide(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    action: &str,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    d: Decider<'_>,
) -> AppResult<bool> {
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
    let rule = rule_for(conn, tenant_id, action).await?;
    let Some(level) = rule.as_ref().and_then(|r| r.levels.get((d.level - 1).max(0) as usize)) else { return Ok(may_approve) };
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

/// Users who may decide `level` of a request — used to notify them.
pub async fn approvers(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    action: &str,
    branch_id: Option<Uuid>,
    requested_by: Uuid,
    level: i32,
    decided_by: &[Uuid],
) -> AppResult<Vec<Uuid>> {
    let candidates: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND is_active")
        .bind(tenant_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut out = Vec::new();
    for u in candidates {
        if can_decide(conn, tenant_id, action, branch_id, Some(requested_by), Decider { approver: u, level, decided_by }).await? {
            out.push(u);
        }
    }
    Ok(out)
}
