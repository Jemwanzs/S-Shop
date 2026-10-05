//! Audit trail — every critical action writes one row, inside the same DB
//! transaction as the change it describes.

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::Ctx;
use crate::error::AppResult;

pub struct Entry<'a> {
    pub module: &'a str,
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub approval_id: Option<Uuid>,
    pub comments: &'a str,
}

impl<'a> Entry<'a> {
    pub fn new(module: &'a str, action: &'a str, entity_type: &'a str, entity_id: Uuid) -> Self {
        Self {
            module,
            action,
            entity_type,
            entity_id: Some(entity_id),
            branch_id: None,
            before: None,
            after: None,
            approval_id: None,
            comments: "",
        }
    }
    pub fn branch(mut self, b: Uuid) -> Self {
        self.branch_id = Some(b);
        self
    }
    pub fn before(mut self, v: impl serde::Serialize) -> Self {
        self.before = serde_json::to_value(v).ok();
        self
    }
    pub fn after(mut self, v: impl serde::Serialize) -> Self {
        self.after = serde_json::to_value(v).ok();
        self
    }
    pub fn approval(mut self, id: Option<Uuid>) -> Self {
        self.approval_id = id;
        self
    }
    pub fn comments(mut self, c: &'a str) -> Self {
        self.comments = c;
        self
    }
}

pub async fn record(conn: &mut PgConnection, ctx: &Ctx, e: Entry<'_>) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_log (tenant_id, user_id, module, action, entity_type, entity_id, branch_id,
                                before, after, approval_id, comments, ip, user_agent, location)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(ctx.tenant_id)
    .bind(ctx.user_id)
    .bind(e.module)
    .bind(e.action)
    .bind(e.entity_type)
    .bind(e.entity_id)
    .bind(e.branch_id)
    .bind(e.before)
    .bind(e.after)
    .bind(e.approval_id)
    .bind(e.comments)
    .bind(&ctx.ip)
    .bind(&ctx.user_agent)
    .bind(ctx.location.map(|l| serde_json::json!(l)))
    .execute(&mut *conn)
    .await?;
    Ok(())
}
