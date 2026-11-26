//! Roadmap 188: website publication requests and their approval.
//!
//! Someone who may *Request website publication* but not publish submits the saved draft: the exact configuration is
//! kept with the published version it was based on and a summary generated from the real differences. The request goes
//! through the workflow engine (*Website publication*; by default the Tenant Administrator). Approving the last step —
//! by someone who may publish — publishes exactly the submitted snapshot, provided nothing was published since (an
//! outdated request is refused, to be returned and resubmitted). Later draft changes are never included. *Return for
//! correction* and *Reject* leave everything unpublished. One request waits at a time per business.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::types::Json as DbJson;
use sqlx::PgConnection;
use uuid::Uuid;

use super::approvals::ApprovalRow;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{refused, rule, AppError, AppResult};
use crate::state::AppState;
use crate::website::{self, SiteConfig};
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/website/publication", get(current))
        .route("/website/publication/requests", get(history).post(submit))
        .route("/website/publication/requests/{id}", get(detail))
}

pub const ACTION: &str = "website.publish";

fn may_see_all(ctx: &Ctx) -> bool {
    ctx.can("website.publish") || ctx.can("approvals.approve") || ctx.can("audit.view")
}

/// Product and category names for the summary's entries.
async fn name_refs(conn: &mut PgConnection, tenant: Uuid, summary: &mut [Value]) -> AppResult<()> {
    let mut ids: Vec<Uuid> = Vec::new();
    for s in summary.iter() {
        if matches!(s["part"].as_str(), Some("products" | "categories")) {
            for i in s["items"].as_array().into_iter().flatten() {
                if let Some(id) = i["ref"].as_str().and_then(|x| Uuid::parse_str(x).ok()) {
                    ids.push(id);
                }
            }
        }
    }
    if ids.is_empty() {
        return Ok(());
    }
    let names: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM products WHERE tenant_id = $1 AND id = ANY($2) UNION ALL SELECT id, name FROM categories WHERE tenant_id = $1 AND id = ANY($2)",
    )
    .bind(tenant)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    for s in summary.iter_mut() {
        for i in s["items"].as_array_mut().into_iter().flatten() {
            if let Some(id) = i["ref"].as_str().and_then(|x| Uuid::parse_str(x).ok()) {
                if let Some((_, n)) = names.iter().find(|(k, _)| *k == id) {
                    i["title"] = json!(n);
                }
            }
        }
    }
    Ok(())
}

fn total(summary: &[Value]) -> i64 {
    summary.iter().map(|s| s["count"].as_i64().unwrap_or(0)).sum()
}

/// What the draft would publish now, the waiting request (if any) and what this person may do.
async fn current(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    if !ctx.can("website.view") && !ctx.can("website.request_publish") && !ctx.can("website.publish") {
        return Err(AppError::Forbidden("You do not have access to the website".into()));
    }
    let mut conn = state.db.acquire().await?;
    let row = super::website::load(&mut conn, ctx.tenant_id, false).await?;
    let r = super::website::ensure_active(&row)?;
    let mut summary = website::summarize(r.published.as_ref().map(|p| &p.0), &r.draft.0);
    name_refs(&mut conn, ctx.tenant_id, &mut summary).await?;
    let waiting: Option<(Uuid, Option<String>, DateTime<Utc>, i32)> = sqlx::query_as(
        "SELECT q.id, u.name, q.requested_at, q.base_version FROM website_publication_requests q LEFT JOIN users u ON u.id = q.requested_by
         WHERE q.tenant_id = $1 AND q.status = 'awaiting'",
    )
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let last: Option<(String, String)> = sqlx::query_as(
        "SELECT status, decision_comments FROM website_publication_requests WHERE tenant_id = $1 ORDER BY requested_at DESC LIMIT 1",
    )
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let unpublished = !summary.is_empty();
    // Unpublished changes · Awaiting approval · Returned for correction · Rejected · Published.
    let status = match (&waiting, &last) {
        (Some(_), _) => "awaiting",
        (None, Some((s, _))) if unpublished && (s == "returned" || s == "rejected") => s.as_str(),
        _ if unpublished => "unpublished",
        _ => "published",
    };
    Ok(Json(json!({
        "status": status,
        "summary": summary,
        "total": total(&summary),
        "version": r.version,
        "waiting": waiting.map(|(id, by, at, base)| json!({ "id": id, "requested_by": by, "requested_at": at, "outdated": base != r.version })),
        "last_comments": last.map(|l| l.1).filter(|c| !c.is_empty()),
        "can_publish": ctx.can("website.publish"),
        "can_request": ctx.can("website.request_publish"),
    })))
}

#[derive(Deserialize)]
struct SubmitBody {
    #[serde(default)]
    comments: String,
    /// The draft as the person saw it (refused if it changed meanwhile: they review what they submit).
    draft_updated_at: Option<DateTime<Utc>>,
}

/// Submits the saved draft for approval (an exact snapshot).
async fn submit(State(state): State<AppState>, ctx: Ctx, Json(b): Json<SubmitBody>) -> AppResult<Json<Value>> {
    if !ctx.can("website.request_publish") && !ctx.can("website.publish") {
        return Err(AppError::Forbidden("You do not have permission to request website publication".into()));
    }
    if ctx.support.is_some() {
        return Err(refused("Not from a support session", "Website publication is requested by the business's own people"));
    }
    let comments: String = b.comments.trim().chars().take(1000).collect();
    let mut tx = state.db.begin().await?;
    let row = super::website::load(&mut tx, ctx.tenant_id, true).await?;
    let r = super::website::ensure_active(&row)?;
    if let (Some(seen), Some(now)) = (b.draft_updated_at, r.draft_updated_at) {
        if (now - seen).num_milliseconds().abs() > 1 {
            return Err(refused("Changed meanwhile", "The draft was saved again after you opened this summary — review the summary again before submitting"));
        }
    }
    website::validate(&r.draft.0)?;
    let blockers = super::website::publish_blockers(&r.draft.0);
    if !blockers.is_empty() {
        return Err(refused("Not ready to publish", blockers.join(" · ")));
    }
    let mut summary = website::summarize(r.published.as_ref().map(|p| &p.0), &r.draft.0);
    if summary.is_empty() {
        return Err(rule("There are no unpublished changes to submit"));
    }
    name_refs(&mut tx, ctx.tenant_id, &mut summary).await?;
    let waiting: Option<Uuid> = sqlx::query_scalar("SELECT id FROM website_publication_requests WHERE tenant_id = $1 AND status = 'awaiting'")
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    if waiting.is_some() {
        return Err(refused("Already waiting", "A website publication request is already waiting for approval — wait for its decision (or withdraw it in Approvals)"));
    }
    let n = total(&summary);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO website_publication_requests (tenant_id, requested_by, comments, snapshot, base_version, summary, total)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(ctx.user_id)
    .bind(&comments)
    .bind(DbJson(&r.draft.0))
    .bind(r.version)
    .bind(json!(summary))
    .bind(n as i32)
    .fetch_one(&mut *tx)
    .await?;
    let sections: Vec<String> = summary.iter().map(|s| format!("{} — {}", s["label"].as_str().unwrap_or(""), s["headline"].as_str().unwrap_or(""))).collect();
    // The workflow's chain for *Website publication*; by default the Tenant Administrator.
    let default_chain = !workflow::rule_for(&mut tx, ctx.tenant_id, ACTION).await?.is_some_and(|r| r.enabled && !r.levels.is_empty());
    let approval = workflow::submit(
        &mut tx,
        &ctx,
        workflow::Request {
            action: ACTION,
            entity_type: "website_publication",
            entity_id: id,
            branch_id: None,
            summary: format!("Website publication — {n} change{}: {}", if n == 1 { "" } else { "s" }, sections.join("; ")),
            amount: None,
            payload: json!({ "request_id": id, "comments": comments }),
        },
    )
    .await?;
    if default_chain {
        sqlx::query("UPDATE approvals SET steps = $2 WHERE id = $1").bind(approval).bind(DbJson(vec![workflow::Level::single("admin")])).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE website_publication_requests SET approval_id = $2 WHERE id = $1").bind(id).bind(approval).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("website", "publication_requested", "website", ctx.tenant_id)
            .after(json!({ "request": id, "changes": n, "sections": sections, "base_version": r.version, "comments": comments }))
            .approval(Some(approval)),
    )
    .await?;
    tx.commit().await?;
    super::approvals::notify_approvers(&state, &ctx, approval).await;
    Ok(Json(json!({ "ok": true, "id": id, "approval_id": approval, "total": n })))
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default, deserialize_with = "super::de::opt_bool")]
    mine: Option<bool>,
}

/// Publication history: every request (publishers, approvers, auditors) or one's own.
async fn history(State(state): State<AppState>, ctx: Ctx, Query(q): Query<HistoryQuery>) -> AppResult<Json<Value>> {
    if !ctx.can("website.view") && !ctx.can("website.request_publish") && !may_see_all(&ctx) {
        return Err(AppError::Forbidden("You do not have access to the website".into()));
    }
    let only_mine = q.mine.unwrap_or(false) || !may_see_all(&ctx);
    #[allow(clippy::type_complexity)]
    let rows: Vec<(Uuid, DateTime<Utc>, Option<String>, String, Value, i32, String, Option<String>, Option<DateTime<Utc>>, String, Option<i32>, Option<Uuid>)> = sqlx::query_as(
        "SELECT q.id, q.requested_at, ru.name, q.comments, q.summary, q.total, q.status, du.name, q.decided_at, q.decision_comments, q.published_version, q.approval_id
         FROM website_publication_requests q LEFT JOIN users ru ON ru.id = q.requested_by LEFT JOIN users du ON du.id = q.decided_by
         WHERE q.tenant_id = $1 AND (NOT $2 OR q.requested_by = $3) ORDER BY q.requested_at DESC LIMIT 100",
    )
    .bind(ctx.tenant_id)
    .bind(only_mine)
    .bind(ctx.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "items": rows.into_iter().map(|(id, at, by, comments, summary, total, status, decided_by, decided_at, decision, version, approval)| json!({
            "id": id, "requested_at": at, "requested_by": by, "comments": comments,
            "sections": summary.as_array().map(|a| a.iter().map(|s| json!({ "label": s["label"], "headline": s["headline"] })).collect::<Vec<_>>()).unwrap_or_default(),
            "total": total, "status": status, "decided_by": decided_by, "decided_at": decided_at, "decision_comments": decision,
            "published_version": version, "approval_id": approval,
        })).collect::<Vec<_>>(),
        "only_mine": only_mine,
    })))
}

/// One request with its full summary (for the approver's review and the requester).
async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    #[allow(clippy::type_complexity)]
    let row: Option<(Option<Uuid>, Option<String>, DateTime<Utc>, String, Value, i32, String, i32, Option<String>, String)> = sqlx::query_as(
        "SELECT q.requested_by, u.name, q.requested_at, q.comments, q.summary, q.total, q.status, q.base_version, du.name, q.decision_comments
         FROM website_publication_requests q LEFT JOIN users u ON u.id = q.requested_by LEFT JOIN users du ON du.id = q.decided_by
         WHERE q.id = $1 AND q.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?;
    let (by_id, by, at, comments, summary, total, status, base, decided_by, decision) = row.ok_or(AppError::NotFound("Publication request"))?;
    if by_id != Some(ctx.user_id) && !may_see_all(&ctx) {
        return Err(AppError::Forbidden("You can only see your own publication requests".into()));
    }
    let version: i32 = sqlx::query_scalar("SELECT version FROM websites WHERE tenant_id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let slug: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    Ok(Json(json!({
        "id": id, "requested_by": by, "requested_at": at, "comments": comments, "summary": summary, "total": total, "status": status,
        "outdated": status == "awaiting" && base != version, "decided_by": decided_by, "decision_comments": decision,
        "preview_query": format!("publication={id}"), "slug": slug,
    })))
}

/// The last step approved: publish exactly the snapshot, by someone who may publish, if nothing was published since.
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    if !ctx.can("website.publish") {
        return Err(AppError::Forbidden("Approving this request publishes the website — only someone who may publish it can give the final approval".into()));
    }
    let id = a.entity_id;
    let req: Option<(DbJson<SiteConfig>, i32, String, Value)> =
        sqlx::query_as("SELECT snapshot, base_version, status, summary FROM website_publication_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (snapshot, base, status, summary) = req.ok_or(AppError::NotFound("Publication request"))?;
    if status != "awaiting" {
        return Err(rule(format!("This publication request is already {status}")));
    }
    let row = super::website::load(&mut *conn, ctx.tenant_id, true).await?;
    let r = super::website::ensure_active(&row)?;
    if r.version != base {
        return Err(refused(
            "Outdated request",
            format!("The website was published again (now version {}) after this request was made on version {base} — return it for correction so it can be checked and resubmitted", r.version),
        ));
    }
    website::validate(&snapshot.0)?;
    // Media archived since the request would leave holes.
    let ids = website::media_ids(&snapshot.0);
    if !ids.is_empty() {
        let ok: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM website_media WHERE tenant_id = $1 AND id = ANY($2) AND NOT archived")
            .bind(ctx.tenant_id)
            .bind(&ids)
            .fetch_one(&mut *conn)
            .await?;
        if ok as usize != ids.len() {
            return Err(refused("Cannot publish", "An image in this request was archived since — return it for correction"));
        }
    }
    let version = r.version + 1;
    let requester: Option<String> = sqlx::query_scalar("SELECT u.name FROM website_publication_requests q JOIN users u ON u.id = q.requested_by WHERE q.id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    // Only the snapshot goes live; the draft keeps whatever was changed after the request (still unpublished).
    sqlx::query("UPDATE websites SET published = $2, version = $3, published_at = now(), published_by = $4 WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .bind(&snapshot)
        .bind(version)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    let note = format!("Publication request by {}", requester.unwrap_or_default());
    sqlx::query("INSERT INTO website_versions (tenant_id, version, config, published_by, note) VALUES ($1, $2, $3, $4, $5)")
        .bind(ctx.tenant_id)
        .bind(version)
        .bind(&snapshot)
        .bind(ctx.user_id)
        .bind(&note)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE website_publication_requests SET status = 'published', decided_by = $2, decided_at = now(), published_version = $3 WHERE id = $1")
        .bind(id)
        .bind(ctx.user_id)
        .bind(version)
        .execute(&mut *conn)
        .await?;
    let parts: Vec<Value> = summary.as_array().map(|a| a.iter().map(|s| s["label"].clone()).collect()).unwrap_or_default();
    audit::record(
        conn,
        ctx,
        Entry::new("website", "publish", "website", ctx.tenant_id).after(json!({ "version": version, "parts": parts, "request": id, "note": note })).approval(Some(a.id)),
    )
    .await
}

/// Returned for correction, rejected or withdrawn: nothing is published; the draft stays for correction.
pub async fn on_closed(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow, outcome: &str, comments: &str) -> AppResult<()> {
    let status = match outcome {
        "returned" => "returned",
        "cancelled" => "withdrawn",
        _ => "rejected",
    };
    sqlx::query(
        "UPDATE website_publication_requests SET status = $3, decided_by = $4, decided_at = now(), decision_comments = $5
         WHERE id = $1 AND tenant_id = $2 AND status = 'awaiting'",
    )
    .bind(a.entity_id)
    .bind(ctx.tenant_id)
    .bind(status)
    .bind(ctx.user_id)
    .bind(comments)
    .execute(&mut *conn)
    .await?;
    let action = format!("publication_{status}");
    audit::record(conn, ctx, Entry::new("website", &action, "website", ctx.tenant_id).after(json!({ "request": a.entity_id, "comments": comments })).approval(Some(a.id))).await
}

/// Roadmap 188: the staff preview of one request's exact snapshot (`/s/{slug}?preview=1&publication={id}`).
pub async fn snapshot(state: &AppState, tenant: Uuid, id: Uuid) -> AppResult<Option<SiteConfig>> {
    Ok(sqlx::query_scalar::<_, DbJson<SiteConfig>>("SELECT snapshot FROM website_publication_requests WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant)
        .fetch_optional(&state.db)
        .await?
        .map(|c| c.0))
}
