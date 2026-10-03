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
    title: String,
    body: String,
    link: String,
    read_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct ListQuery {
    unread: Option<bool>,
    limit: Option<i64>,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT id, kind, title, body, link, read_at, created_at FROM notifications
         WHERE user_id = $1 AND (NOT $2 OR read_at IS NULL) ORDER BY created_at DESC LIMIT $3",
    )
    .bind(ctx.user_id)
    .bind(q.unread.unwrap_or(false))
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL")
        .bind(ctx.user_id)
        .fetch_one(&state.db)
        .await?;
    let approvals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals WHERE tenant_id = $1 AND status = 'pending' AND requested_by <> $2
           AND (branch_id IS NULL OR branch_id = ANY($3))",
    )
    .bind(ctx.tenant_id)
    .bind(ctx.user_id)
    .bind(&ctx.branch_ids)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "items": rows, "unread": unread, "pending_approvals": if ctx.can("approvals.approve") { approvals } else { 0 } })))
}

async fn read_one(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE notifications SET read_at = now() WHERE id = $1 AND user_id = $2 AND read_at IS NULL")
        .bind(id)
        .bind(ctx.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn read_all(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    sqlx::query("UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL")
        .bind(ctx.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// One SSE connection per signed-in device. Events carry ids only; clients refetch.
async fn stream(State(state): State<AppState>, ctx: Ctx) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tenant, user) = (ctx.tenant_id, ctx.user_id);
    let events = BroadcastStream::new(state.events.subscribe()).filter_map(move |msg| {
        let ev = msg.ok()?;
        if ev.tenant_id != tenant || ev.user_id.is_some_and(|u| u != user) {
            return None;
        }
        Some(Ok(Event::default().event(ev.topic).data(ev.data.to_string())))
    });
    let hello = tokio_stream::once(Ok(Event::default().event("ready").data("{}")));
    Sse::new(hello.chain(events)).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}
