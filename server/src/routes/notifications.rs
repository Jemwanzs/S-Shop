//! In-app notifications and the live event stream (Server-Sent Events).

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::auth::Ctx;
use crate::error::AppResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list))
        .route("/notifications/read-all", post(read_all))
        .route("/notifications/{id}/read", post(read_one))
        .route("/events", get(stream))
}

#[derive(Serialize, sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    kind: String,
    category: Option<String>,
    branch_id: Option<Uuid>,
    branch_name: Option<String>,
    title: String,
    body: String,
    link: String,
    read_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct ListQuery {
    unread: Option<bool>,
    /// Roadmap 194: all | unread | read.
    status: Option<String>,
    category: Option<String>,
    branch_id: Option<Uuid>,
    limit: Option<i64>,
}

/// Module permissions notifications can depend on (see `notify::classify`).
const MODULE_PERMS: [&str; 5] = ["orders.view", "credit.view", "stock.view", "products.view", "website.view"];

/// Roadmap 194: what this person may see now — categories their role allows, modules they may use, their branches.
/// Applied to every read, count and mark-as-read, so a change of role, permissions or branches takes effect at once.
struct Access {
    categories: Vec<String>,
    perms: Vec<String>,
    branches: Vec<Uuid>,
}

fn access(ctx: &Ctx) -> Access {
    Access {
        categories: crate::notify::CATEGORIES.iter().map(|c| c.0).filter(|c| ctx.can(&format!("notify.{c}"))).map(String::from).collect(),
        perms: MODULE_PERMS.iter().filter(|p| ctx.can(p)).map(|p| p.to_string()).collect(),
        branches: ctx.branch_ids.clone(),
    }
}

/// `$1` user, `$2` tenant, `$3` categories, `$4` module permissions, `$5` branches.
const VISIBLE: &str = "n.user_id = $1 AND n.tenant_id = $2
    AND (n.mandatory OR n.category IS NULL OR n.category = ANY($3))
    AND (n.perm IS NULL OR n.perm = ANY($4))
    AND (n.branch_id IS NULL OR n.branch_id = ANY($5))";

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    let a = access(&ctx);
    let status = q.status.as_deref().unwrap_or(if q.unread.unwrap_or(false) { "unread" } else { "all" }).to_string();
    let rows: Vec<NotificationRow> = sqlx::query_as(&format!(
        "SELECT n.id, n.kind, n.category, n.branch_id, b.name AS branch_name, n.title, n.body, n.link, n.read_at, n.created_at
         FROM notifications n LEFT JOIN branches b ON b.id = n.branch_id
         WHERE {VISIBLE} AND ($6 = 'all' OR ($6 = 'unread') = (n.read_at IS NULL))
           AND ($7::text IS NULL OR n.category = $7) AND ($8::uuid IS NULL OR n.branch_id = $8)
         ORDER BY n.created_at DESC LIMIT $9"
    ))
    .bind(ctx.user_id)
    .bind(ctx.tenant_id)
    .bind(&a.categories)
    .bind(&a.perms)
    .bind(&a.branches)
    .bind(&status)
    .bind(q.category.as_deref().filter(|c| !c.is_empty()))
    .bind(q.branch_id)
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    let unread: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM notifications n WHERE {VISIBLE} AND n.read_at IS NULL"))
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .bind(&a.categories)
        .bind(&a.perms)
        .bind(&a.branches)
        .fetch_one(&state.db)
        .await?;
    let by_category: Vec<(Option<String>, i64)> = sqlx::query_as(&format!(
        "SELECT n.category, COUNT(*) FILTER (WHERE n.read_at IS NULL) FROM notifications n WHERE {VISIBLE} GROUP BY n.category"
    ))
    .bind(ctx.user_id)
    .bind(ctx.tenant_id)
    .bind(&a.categories)
    .bind(&a.perms)
    .bind(&a.branches)
    .fetch_all(&state.db)
    .await?;
    let branches: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM branches WHERE tenant_id = $1 AND id = ANY($2) ORDER BY name")
        .bind(ctx.tenant_id)
        .bind(&a.branches)
        .fetch_all(&state.db)
        .await?;
    // Roadmap 194: the requests this person may decide now (not every pending request in their branches).
    let approvals = super::approvals::decidable_count(&mut *state.db.acquire().await?, &ctx).await?;
    let my_pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM approvals WHERE tenant_id = $1 AND requested_by = $2 AND status = 'pending'")
        .bind(ctx.tenant_id)
        .bind(ctx.user_id)
        .fetch_one(&state.db)
        .await?;
    // Roadmap 69: orders waiting to be confirmed, in the user's orders scope (the same filter as the Orders list) —
    // an operational count, not unread notifications: reading or dismissing alerts never changes it.
    let new_orders: i64 = if ctx.can("orders.view") {
        let vis = ctx.visibility(&mut *state.db.acquire().await?, "orders", None, None).await?;
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders o WHERE o.tenant_id = $1 AND o.branch_id = ANY($2) AND o.status = 'new'
               AND ($3::uuid IS NULL OR o.created_by = $3 OR EXISTS (SELECT 1 FROM sales ss WHERE ss.id = o.sale_id AND ss.owner_id = $3))",
        )
        .bind(ctx.tenant_id)
        .bind(&vis.branches)
        .bind(vis.owner)
        .fetch_one(&state.db)
        .await?
    } else {
        0
    };
    let categories: Vec<Value> = crate::notify::CATEGORIES
        .iter()
        // Only categories that have notifications for this person (all of them visible to them).
        .filter(|c| by_category.iter().any(|b| b.0.as_deref() == Some(c.0)))
        .map(|(k, l)| json!({ "key": k, "label": l, "unread": by_category.iter().find(|b| b.0.as_deref() == Some(*k)).map_or(0, |b| b.1) }))
        .collect();
    Ok(Json(json!({
        "items": rows, "unread": unread, "pending_approvals": approvals, "my_pending_requests": if super::approvals::may_view_own(&ctx) { my_pending } else { 0 },
        "new_orders": new_orders, "categories": categories,
        "branches": branches.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
    })))
}

/// Opening a notification: marked read and its link returned only if the person may still see it (roadmap 194).
async fn read_one(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let a = access(&ctx);
    let link: Option<String> = sqlx::query_scalar(&format!(
        "UPDATE notifications n SET read_at = COALESCE(n.read_at, now()) WHERE n.id = $6 AND {VISIBLE} RETURNING n.link"
    ))
    .bind(ctx.user_id)
    .bind(ctx.tenant_id)
    .bind(&a.categories)
    .bind(&a.perms)
    .bind(&a.branches)
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    match link {
        Some(link) => Ok(Json(json!({ "ok": true, "link": link }))),
        None => Err(crate::error::AppError::NotFound("Notification")),
    }
}

async fn read_all(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let a = access(&ctx);
    sqlx::query(&format!("UPDATE notifications n SET read_at = now() WHERE {VISIBLE} AND n.read_at IS NULL"))
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .bind(&a.categories)
        .bind(&a.perms)
        .bind(&a.branches)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// One SSE connection per signed-in device. Events carry ids only; clients refetch.
async fn stream(State(state): State<AppState>, ctx: Ctx) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tenant, user) = (ctx.tenant_id, ctx.user_id);
    // A support session's stream closes within 15 s of the session ending, expiring or being revoked (roadmap 71).
    let live = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    // …and every stream closes when its sign-in session ends or goes idle (roadmap 87).
    {
        let support = ctx.support.as_ref().map(|s| s.id);
        let ses = ctx.session_id;
        let (db, flag) = (state.db.clone(), live.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                let support_ok = match support {
                    Some(s) => sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM support_sessions WHERE id = $1 AND status = 'active' AND expires_at > now())")
                        .bind(s)
                        .fetch_one(&db)
                        .await
                        .unwrap_or(false),
                    None => true,
                };
                let ok = support_ok && crate::sessions::alive(&db, ses).await;
                if !ok || std::sync::Arc::strong_count(&flag) == 1 {
                    flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
            }
        });
    }
    let open = live.clone();
    let events = BroadcastStream::new(state.events.subscribe()).take_while(move |_| open.load(std::sync::atomic::Ordering::Relaxed)).filter_map(move |msg| {
        let ev = msg.ok()?;
        if ev.tenant_id != tenant || ev.user_id.is_some_and(|u| u != user) {
            return None;
        }
        Some(Ok(Event::default().event(ev.topic).data(ev.data.to_string())))
    });
    let hello = tokio_stream::once(Ok(Event::default().event("ready").data("{}")));
    Sse::new(hello.chain(events)).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}
