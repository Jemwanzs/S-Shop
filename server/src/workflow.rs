//! Maker-checker approval engine.
//!
//! A module asks `gate()` before executing a sensitive action. If a workflow is
//! enabled for that action (and the amount reaches its threshold) an approval
//! request is stored with the original request as `payload`, and the action is
//! executed later by `routes::approvals` when an eligible approver accepts it.

use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Ctx;
use crate::error::AppResult;

pub struct Action {
    pub key: &'static str,
    pub label: &'static str,
    pub uses_amount: bool,
}

pub const ACTIONS: &[Action] = &[
    Action { key: "product.create", label: "Product creation", uses_amount: false },
    Action { key: "product.edit", label: "Product edit", uses_amount: false },
    Action { key: "product.deactivate", label: "Product deactivation", uses_amount: false },
    Action { key: "stock.add", label: "Stock addition", uses_amount: true },
    Action { key: "stock.adjust", label: "Stock adjustment / count", uses_amount: false },
    Action { key: "stock.write_off", label: "Stock write-off", uses_amount: false },
    Action { key: "stock.transfer", label: "Stock transfer", uses_amount: false },
    Action { key: "sale.discount", label: "Excessive discount (supervisor PIN at the counter)", uses_amount: true },
    Action { key: "sale.return", label: "Sale return / refund", uses_amount: true },
    Action { key: "sale.cancel", label: "Sale cancellation", uses_amount: true },
    Action { key: "credit.write_off", label: "Credit write-off", uses_amount: true },
    Action { key: "expense", label: "Expense", uses_amount: true },
];

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct Rule {
    pub action: String,
    pub enabled: bool,
    pub approver_type: String,
    pub approver_role_id: Option<Uuid>,
    pub approver_user_id: Option<Uuid>,
    pub min_amount: Option<Decimal>,
}

pub async fn rule_for(conn: &mut PgConnection, tenant_id: Uuid, action: &str) -> AppResult<Option<Rule>> {
    Ok(sqlx::query_as(
        "SELECT action, enabled, approver_type, approver_role_id, approver_user_id, min_amount
         FROM workflows WHERE tenant_id = $1 AND action = $2",
    )
    .bind(tenant_id)
    .bind(action)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Whether `action` must go through approval for this amount.
pub async fn needs_approval(conn: &mut PgConnection, tenant_id: Uuid, action: &str, amount: Option<Decimal>) -> AppResult<bool> {
    Ok(match rule_for(conn, tenant_id, action).await? {
        Some(r) if r.enabled => match (r.min_amount, amount) {
            (Some(min), Some(a)) => a >= min,
            _ => true,
        },
        _ => false,
    })
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

/// Can `approver` decide a request for `action` raised in `branch_id` by `requested_by`?
/// Makers can never check their own requests.
pub async fn can_decide(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    action: &str,
    branch_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    approver: Uuid,
) -> AppResult<bool> {
    if requested_by == Some(approver) {
        return Ok(false);
    }
    let (perms, role_id, all_branches): (Vec<String>, Uuid, bool) = sqlx::query_as(
        "SELECT r.permissions, r.id, u.all_branches FROM users u JOIN roles r ON r.id = u.role_id
         WHERE u.id = $1 AND u.tenant_id = $2 AND u.is_active",
    )
    .bind(approver)
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await?;
    let is_admin = perms.iter().any(|p| p == "*");
    if is_admin {
        return Ok(true);
    }
    let rule = rule_for(conn, tenant_id, action).await?;
    let Some(rule) = rule else { return Ok(perms.iter().any(|p| p == "approvals.approve")) };
    Ok(match rule.approver_type.as_str() {
        "user" => rule.approver_user_id == Some(approver),
        "role" => rule.approver_role_id == Some(role_id),
        "branch_manager" => match branch_id {
            Some(b) => {
                let mgr: Option<Uuid> = sqlx::query_scalar("SELECT manager_id FROM branches WHERE id = $1")
                    .bind(b)
                    .fetch_one(&mut *conn)
                    .await?;
                mgr == Some(approver) || (all_branches && perms.iter().any(|p| p == "approvals.approve"))
            }
            None => perms.iter().any(|p| p == "approvals.approve"),
        },
        _ => false, // "admin" — handled above
    })
}

/// Users who may decide a request — used to notify them.
pub async fn approvers(conn: &mut PgConnection, tenant_id: Uuid, action: &str, branch_id: Option<Uuid>, requested_by: Uuid) -> AppResult<Vec<Uuid>> {
    let candidates: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND is_active")
        .bind(tenant_id)
        .fetch_all(&mut *conn)
        .await?;
    let mut out = Vec::new();
    for u in candidates {
        if can_decide(conn, tenant_id, action, branch_id, Some(requested_by), u).await? {
            out.push(u);
        }
    }
    Ok(out)
}
