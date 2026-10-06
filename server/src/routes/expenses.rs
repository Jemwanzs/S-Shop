//! Expenses with configurable categories, optional attachments and approval.

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Outcome, Page, Period};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
use crate::routes::approvals::ApprovalRow;
use crate::settings;
use crate::state::AppState;
use crate::util::round2;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/expenses", get(list).post(create))
        .route("/expenses/{id}/attachment", get(attachment))
        .route("/expenses/{id}/void", post(void))
        .route("/expense-categories", get(list_categories).post(create_category))
        .route("/expense-categories/{id}", put(update_category))
}

#[derive(Serialize, sqlx::FromRow)]
struct ExpenseRow {
    id: Uuid,
    expense_date: NaiveDate,
    branch_id: Uuid,
    branch_name: String,
    category_id: Uuid,
    category_name: String,
    amount: Decimal,
    description: String,
    payee: String,
    payment_method: String,
    has_attachment: bool,
    status: String,
    user_name: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ExpenseListRow {
    #[sqlx(flatten)]
    row: ExpenseRow,
    total_count: i64,
    sum_amount: Decimal,
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    branch_id: Option<Uuid>,
    category_id: Option<Uuid>,
    status: Option<String>,
    #[serde(flatten)]
    period: Period,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    ctx.require("expenses.view")?;
    let branches = ctx.branch_scope(q.branch_id)?;
    let (from, to) = q.period.resolve(ctx.today(), "month");
    let rows: Vec<ExpenseListRow> = sqlx::query_as(
        "SELECT e.id, e.expense_date, e.branch_id, b.name AS branch_name, e.category_id, c.name AS category_name, e.amount,
                e.description, e.payee, e.payment_method, e.attachment IS NOT NULL AS has_attachment, e.status, u.name AS user_name,
                e.created_at, COUNT(*) OVER() AS total_count,
                COALESCE(SUM(e.amount) FILTER (WHERE e.status = 'approved') OVER(), 0) AS sum_amount
         FROM expenses e JOIN branches b ON b.id = e.branch_id JOIN expense_categories c ON c.id = e.category_id
         LEFT JOIN users u ON u.id = e.user_id
         WHERE e.tenant_id = $1 AND e.branch_id = ANY($2) AND e.expense_date BETWEEN $3 AND $4
           AND ($5::uuid IS NULL OR e.category_id = $5) AND ($6::text IS NULL OR e.status = $6)
           AND ($7::text IS NULL OR e.description ILIKE $7 OR e.payee ILIKE $7)
         ORDER BY e.expense_date DESC, e.created_at DESC LIMIT $8 OFFSET $9",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    .bind(q.category_id)
    .bind(&q.status)
    .bind(like(&q.q))
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    let by_category: Vec<(String, Decimal)> = sqlx::query_as(
        "SELECT c.name, SUM(e.amount) FROM expenses e JOIN expense_categories c ON c.id = e.category_id
         WHERE e.tenant_id = $1 AND e.branch_id = ANY($2) AND e.expense_date BETWEEN $3 AND $4 AND e.status = 'approved'
         GROUP BY c.name ORDER BY 2 DESC",
    )
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;
    let (total, sum) = rows.first().map(|r| (r.total_count, r.sum_amount)).unwrap_or_default();
    Ok(Json(json!({
        "from": from, "to": to,
        "items": rows.into_iter().map(|r| r.row).collect::<Vec<_>>(),
        "total": total,
        "summary": { "approved_total": sum, "by_category": by_category.into_iter().map(|(n, a)| json!({ "category": n, "amount": a })).collect::<Vec<_>>() },
    })))
}

#[derive(Deserialize)]
struct CreateBody {
    branch_id: Option<Uuid>,
    category_id: Uuid,
    amount: Decimal,
    expense_date: Option<NaiveDate>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    payee: String,
    #[serde(default)]
    payment_method: String,
    /// data URL or base64 file (image or PDF, ≤ 5 MB)
    attachment: Option<String>,
    attachment_mime: Option<String>,
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CreateBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("expenses.create")?;
    crate::geo::require_on_site(&mut *state.db.acquire().await?, &ctx, "expenses").await?;
    let branch = ctx.branch_or_current(b.branch_id)?;
    if b.amount <= Decimal::ZERO {
        return Err(bad("Enter the amount"));
    }
    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    if s.expenses.require_description && b.description.trim().is_empty() {
        return Err(bad("Describe the expense"));
    }
    let (attachment, mime) = match b.attachment.as_deref().filter(|a| !a.is_empty()) {
        Some(raw) => {
            let (mime, data) = match raw.strip_prefix("data:").and_then(|r| r.split_once(";base64,")) {
                Some((m, d)) => (m.to_string(), d),
                None => (b.attachment_mime.clone().unwrap_or_else(|| "application/octet-stream".into()), raw),
            };
            if !(mime.starts_with("image/") || mime == "application/pdf") {
                return Err(bad("Attach an image or PDF"));
            }
            let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| bad("The attachment could not be read"))?;
            if bytes.len() > 5 * 1024 * 1024 {
                return Err(bad("Attachment must be under 5 MB"));
            }
            (Some(bytes), Some(mime))
        }
        None if s.expenses.require_attachment => return Err(bad("Attach the receipt")),
        None => (None, None),
    };
    let (category, active): (String, bool) = sqlx::query_as("SELECT name, is_active FROM expense_categories WHERE id = $1 AND tenant_id = $2")
        .bind(b.category_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| bad("Choose a category"))?;
    if !active {
        return Err(bad("That category is no longer in use"));
    }
    let gated = workflow::needs_approval(&mut tx, &ctx, "expense", workflow::Gate::branch(branch).amount(b.amount).category(b.category_id)).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO expenses (tenant_id, branch_id, category_id, amount, expense_date, description, payee, payment_method,
                               attachment, attachment_mime, status, user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(b.category_id)
    .bind(round2(b.amount))
    .bind(b.expense_date.unwrap_or_else(|| ctx.today()))
    .bind(b.description.trim())
    .bind(b.payee.trim())
    .bind(if b.payment_method.is_empty() { "cash" } else { b.payment_method.as_str() })
    .bind(attachment)
    .bind(mime)
    .bind(if gated { "pending" } else { "approved" })
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("expenses", "create", "expense", id).branch(branch).after(json!({
            "category": category, "amount": b.amount, "description": b.description, "payee": b.payee,
        })),
    )
    .await?;
    if gated {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "expense",
                entity_type: "expense",
                entity_id: id,
                branch_id: Some(branch),
                summary: format!("{category} — {}", b.description.trim()),
                amount: Some(b.amount),
                payload: json!({}),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    tx.commit().await?;
    Ok(Json(Outcome::done(json!({ "id": id }))))
}

pub async fn on_decided(conn: &mut PgConnection, a: &ApprovalRow, approved: bool) -> AppResult<()> {
    sqlx::query("UPDATE expenses SET status = $2 WHERE id = $1 AND status = 'pending' AND tenant_id = $3")
        .bind(a.entity_id)
        .bind(if approved { "approved" } else { "rejected" })
        .bind(a.tenant_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn attachment(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<impl IntoResponse> {
    ctx.require("expenses.view")?;
    let (data, mime, branch): (Option<Vec<u8>>, Option<String>, Uuid) =
        sqlx::query_as("SELECT attachment, attachment_mime, branch_id FROM expenses WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Expense"))?;
    ctx.ensure_branch(branch)?;
    let data = data.ok_or(AppError::NotFound("Attachment"))?;
    Ok(([(header::CONTENT_TYPE, mime.unwrap_or_else(|| "application/octet-stream".into())), (header::CACHE_CONTROL, "private, max-age=3600".into())], data))
}

#[derive(Deserialize)]
struct VoidBody {
    reason: String,
}

/// Expenses are voided (kept for audit), never deleted.
async fn void(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<VoidBody>) -> AppResult<Json<Value>> {
    ctx.require("expenses.create")?;
    if b.reason.trim().is_empty() {
        return Err(bad("A reason is required"));
    }
    let mut tx = state.db.begin().await?;
    let (branch, status, owner): (Uuid, String, Option<Uuid>) =
        sqlx::query_as("SELECT branch_id, status, user_id FROM expenses WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("Expense"))?;
    ctx.ensure_branch(branch)?;
    if owner != Some(ctx.user_id) && !ctx.can("approvals.approve") {
        return Err(AppError::Forbidden("Only the person who recorded it or an approver can void an expense".into()));
    }
    if status == "void" {
        return Err(rule("Already voided"));
    }
    sqlx::query("UPDATE expenses SET status = 'void' WHERE id = $1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE approvals SET status = 'cancelled', decided_by = $2, decided_at = now() WHERE entity_id = $1 AND status = 'pending' AND tenant_id = $3")
        .bind(id)
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("expenses", "void", "expense", id).branch(branch).before(json!({ "status": status })).comments(b.reason.trim()))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, sqlx::FromRow)]
struct Category {
    id: Uuid,
    name: String,
    is_active: bool,
}

async fn list_categories(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Category>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, name, is_active FROM expense_categories WHERE tenant_id = $1 ORDER BY name")
            .bind(ctx.tenant_id)
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize)]
struct CategoryBody {
    name: String,
    is_active: Option<bool>,
}

async fn create_category(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CategoryBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.expenses")?;
    if b.name.trim().is_empty() {
        return Err(bad("Category name is required"));
    }
    let id: Uuid = sqlx::query_scalar("INSERT INTO expense_categories (tenant_id, name) VALUES ($1,$2) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_category(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<CategoryBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.expenses")?;
    sqlx::query("UPDATE expense_categories SET name = $3, is_active = COALESCE($4, is_active) WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.is_active)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
