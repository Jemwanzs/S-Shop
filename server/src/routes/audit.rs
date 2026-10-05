//! Audit trail viewer.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{Counted, Page, Paged, Period};
use crate::auth::Ctx;
use crate::error::AppResult;
use crate::state::AppState;
use crate::util::local_range;

pub fn routes() -> Router<AppState> {
    Router::new().route("/audit", get(list))
}

#[derive(Serialize, sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    user_name: Option<String>,
    module: String,
    action: String,
    entity_type: String,
    entity_id: Option<Uuid>,
    branch_name: Option<String>,
    before: Option<Value>,
    after: Option<Value>,
    approval_status: Option<String>,
    approver_name: Option<String>,
    comments: String,
    ip: String,
    user_agent: String,
    /// Device position at the time ({lat, lng, accuracy_m}), when the browser shared it.
    location: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ListQuery {
    module: Option<String>,
    user_id: Option<Uuid>,
    entity_id: Option<Uuid>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Paged<AuditRow>>> {
    ctx.require("audit.view")?;
    let (from, to) = q.period.resolve(ctx.today(), "week");
    let (start, end) = local_range(from, to, ctx.tz);
    let rows: Vec<Counted<AuditRow>> = sqlx::query_as(
        "SELECT COUNT(*) OVER() AS total_count, a.id, a.created_at, u.name AS user_name, a.module, a.action, a.entity_type, a.entity_id,
                b.name AS branch_name, a.before, a.after, ap.status AS approval_status, du.name AS approver_name,
                a.comments, a.ip, a.user_agent, a.location
         FROM audit_log a LEFT JOIN users u ON u.id = a.user_id LEFT JOIN branches b ON b.id = a.branch_id
         LEFT JOIN approvals ap ON ap.id = a.approval_id LEFT JOIN users du ON du.id = ap.decided_by
         WHERE a.tenant_id = $1 AND a.created_at >= $2 AND a.created_at < $3
           AND ($4::text IS NULL OR a.module = $4) AND ($5::uuid IS NULL OR a.user_id = $5) AND ($6::uuid IS NULL OR a.entity_id = $6)
         ORDER BY a.created_at DESC LIMIT $7 OFFSET $8",
    )
    .bind(ctx.tenant_id)
    .bind(start)
    .bind(end)
    .bind(&q.module)
    .bind(q.user_id)
    .bind(q.entity_id)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into()))
}
