//! Error alerting for the platform owner (roadmap 147).
//!
//! Failures that need someone to act — server faults (database and internal errors, a crash inside a request),
//! integrations refusing (502 *upstream* answers), failed background jobs, a restart without a new deployment, the
//! database not answering — are reported here. Normal business answers (4xx) and a website shown as *temporarily
//! unavailable* are never reported: only responses that carry an [`Alert`] (added by `AppError` for those faults) are.
//!
//! Reports go through a channel to one background task, so a request never waits and an outage cannot lose them: while
//! the database is unreachable they are kept in memory (up to 500) and stored when it answers again. Each distinct
//! error is one `error_events` row (fingerprint = source, kind, message shape, method and path with ids removed);
//! repeats only count. The first occurrence alerts the platform admins in-app and by email; a repeat alerts again
//! after 6 hours, or when it happens after being marked resolved.

use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::{Path, Query, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::Ctx;
use crate::error::{AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;

/// Added to an error response that should alert (see `AppError::into_response`).
#[derive(Clone, Debug)]
pub struct Alert {
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug)]
struct Report {
    source: &'static str,
    kind: String,
    message: String,
    method: String,
    path: String,
}

static TX: OnceLock<mpsc::UnboundedSender<Report>> = OnceLock::new();
const KEEP_WHILE_DOWN: usize = 500;
const REALERT_AFTER_HOURS: i64 = 6;

/// Report a failure (non-blocking; does nothing before `start`).
pub fn report(source: &'static str, kind: impl Into<String>, message: impl Into<String>, method: &str, path: &str) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Report { source, kind: kind.into(), message: message.into(), method: method.to_string(), path: path.to_string() });
    }
}

/// Starts the alert writer and records this start of the service (restart detection).
pub fn start(state: AppState) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Report>();
    if TX.set(tx).is_err() {
        return;
    }
    let writer = state.clone();
    tokio::spawn(async move {
        let mut pending: Vec<Report> = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                r = rx.recv() => match r {
                    Some(r) => pending.push(r),
                    None => return,
                },
                _ = tick.tick() => {}
            }
            while let Ok(r) = rx.try_recv() {
                pending.push(r);
            }
            // Store in order; stop at the first failure (database down) and try again later.
            let mut stored = 0;
            for r in &pending {
                if let Err(e) = record(&writer, r, false).await {
                    tracing::warn!(error = %e, "error alert not stored yet");
                    break;
                }
                stored += 1;
            }
            pending.drain(..stored);
            if pending.len() > KEEP_WHILE_DOWN {
                let extra = pending.len() - KEEP_WHILE_DOWN;
                pending.drain(..extra);
            }
        }
    });
    // A second start of the same deployment is a restart (crash, out of memory), not a deploy.
    tokio::spawn(async move {
        let Some(deployment) = std::env::var("RAILWAY_DEPLOYMENT_ID").ok().filter(|d| !d.trim().is_empty()) else { return };
        let seen: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM service_starts WHERE deployment_id = $1)")
            .bind(&deployment)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false);
        let _ = sqlx::query("INSERT INTO service_starts (deployment_id) VALUES ($1)").bind(&deployment).execute(&state.db).await;
        if seen {
            report(
                "health",
                "Service restarted",
                "S'Shop restarted without a new deployment (a crash or running out of memory). Check the logs just before this time.",
                "",
                "",
            );
        }
    });
}

/// Every response: report the ones that carry an [`Alert`].
pub async fn capture(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let res = next.run(req).await;
    if let Some(a) = res.extensions().get::<Alert>() {
        report("server", a.kind, a.message.clone(), &method, &path);
    }
    res
}

/// A panic inside a request answers 500 (instead of a dropped connection) and alerts.
pub fn panic_response(err: Box<dyn std::any::Any + Send + 'static>) -> Response {
    let detail = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown panic".into());
    tracing::error!(panic = %detail, "request panicked");
    let mut res = axum::response::IntoResponse::into_response((
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": { "code": "internal", "message": "Something went wrong. Please try again.", "title": null } })),
    ));
    res.extensions_mut().insert(Alert { kind: "Crash inside a request", message: detail });
    res
}

/// Shortened, with anything that looks like a secret (long tokens, bearer values) removed.
pub fn clean(message: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut hide_next = false;
    for word in message.split_whitespace() {
        let core: String = word.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '_' | '-' | '=' | '.')).collect();
        if hide_next || (core.len() >= 32 && core.chars().filter(|c| c.is_ascii_digit()).count() >= 4) {
            out.push("[removed]".into());
            hide_next = false;
            continue;
        }
        hide_next = word.eq_ignore_ascii_case("bearer") || word.to_ascii_lowercase().ends_with("key=") || word.to_ascii_lowercase().ends_with("secret=");
        out.push(word.to_string());
    }
    let s = out.join(" ");
    if s.chars().count() > 400 {
        format!("{}…", s.chars().take(400).collect::<String>())
    } else {
        s
    }
}

/// Ids and numbers replaced, so the same error at another record groups together.
fn shape(s: &str) -> String {
    s.split(|c: char| c == '/' || c.is_whitespace())
        .map(|part| {
            if Uuid::parse_str(part.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '-')).is_ok() || part.chars().any(|c| c.is_ascii_digit()) {
                "#"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fingerprint(r: &Report) -> String {
    let mut h = Sha256::new();
    for part in [r.source, &r.kind, &shape(&r.message), &r.method, &shape(&r.path)] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    hex::encode(&h.finalize()[..16])
}

/// Alert again? Always after it was marked resolved; otherwise when no alert went out in the last 6 hours.
fn should_alert(notified: Option<DateTime<Utc>>, resolved: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    resolved.is_some() || notified.is_none_or(|n| now - n > chrono::Duration::hours(REALERT_AFTER_HOURS))
}

/// Stores one occurrence (grouped by fingerprint) and alerts when [`should_alert`] says so, or always with `force`
/// (the test alert).
async fn record(state: &AppState, r: &Report, force: bool) -> anyhow::Result<(Uuid, Option<crate::mailer::Outcome>)> {
    let message = clean(&r.message);
    let fp = fingerprint(r);
    let mut tx = state.db.begin().await?;
    let found: Option<(Uuid, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT id, notified_at, resolved_at FROM error_events WHERE fingerprint = $1 FOR UPDATE")
            .bind(&fp)
            .fetch_optional(&mut *tx)
            .await?;
    let (id, alert) = match found {
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO error_events (fingerprint, source, kind, message, method, path) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
            )
            .bind(&fp)
            .bind(r.source)
            .bind(&r.kind)
            .bind(&message)
            .bind(&r.method)
            .bind(&r.path)
            .fetch_one(&mut *tx)
            .await?;
            (id, true)
        }
        Some((id, notified, resolved)) => {
            sqlx::query("UPDATE error_events SET count = count + 1, last_seen = now(), message = $2, path = $3, resolved_at = NULL, resolved_by = NULL WHERE id = $1")
                .bind(id)
                .bind(&message)
                .bind(&r.path)
                .execute(&mut *tx)
                .await?;
            (id, force || should_alert(notified, resolved, Utc::now()))
        }
    };
    if alert {
        sqlx::query("UPDATE error_events SET notified_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let outcome = if alert { deliver(state, id).await } else { None };
    Ok((id, outcome))
}

/// In-app notification to the platform admins and an email to `PLATFORM_ADMIN_EMAILS` (recorded in the email log).
async fn deliver(state: &AppState, id: Uuid) -> Option<crate::mailer::Outcome> {
    let row: Option<(String, String, String, String, String, i32, DateTime<Utc>)> =
        sqlx::query_as("SELECT source, kind, message, method, path, count, first_seen FROM error_events WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let (source, kind, message, method, path, count, first_seen) = row?;
    let place = if path.is_empty() { String::new() } else { format!("{method} {path}").trim().to_string() };
    let times = if count > 1 { format!(" · {count} times since {}", first_seen.format("%d %b %H:%M UTC")) } else { String::new() };
    let body = format!("{message}{}{times}", if place.is_empty() { String::new() } else { format!(" — {place}") });
    let title = if source == "test" { "Test alert — S'Shop error alerts work".to_string() } else { format!("S'Shop error: {kind}") };
    let note = Note::new("error_alert", title.clone(), body.clone(), "/settings/errors").dedupe(format!("error:{id}:{}", Utc::now().format("%Y%m%d%H%M")));
    let admins: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT id, tenant_id FROM users WHERE is_active AND lower(email) = ANY($1) AND login_user_id IS NULL")
        .bind(&state.cfg.platform_admins)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (user, tenant) in admins {
        notify::to_users(state, tenant, &[user], note.clone()).await;
    }
    if state.cfg.platform_admins.is_empty() {
        return None;
    }
    let link = format!("{}/settings/errors", state.cfg.public_url);
    let rows = [("What", kind.clone()), ("Details", message.clone()), ("Where", if place.is_empty() { "—".into() } else { place.clone() }), ("How often", if count > 1 { format!("{count} times since {}", first_seen.format("%d %b %Y %H:%M UTC")) } else { "First time".into() })];
    let html = crate::mailer::layout(
        &title,
        &title,
        &format!("{}{}{}", crate::mailer::receipt("Error", &rows), crate::mailer::p("Open S'Shop to see every error and mark this one resolved once it is handled."), crate::mailer::button("Open Errors & alerts", &link)),
    );
    let text = format!("{title}\n\n{}\n\nOpen: {link}", crate::mailer::text_rows(&rows));
    Some(
        crate::mailer::send(
            state,
            crate::mailer::Mail {
                kind: "error_alert",
                to: state.cfg.platform_admins.clone(),
                subject: title,
                html,
                text,
                tenant_id: None,
                access_request_id: None,
                user_id: None,
                created_by: None,
                retry_of: None,
            },
        )
        .await,
    )
}

// ── Settings → Platform → Errors & alerts ─────────────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/platform/errors", get(list))
        .route("/platform/errors/test", post(test))
        .route("/platform/errors/{id}/resolve", post(resolve))
}

#[derive(Deserialize)]
struct ListQuery {
    /// open (default) | resolved | all
    status: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    source: String,
    kind: String,
    message: String,
    method: String,
    path: String,
    count: i32,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    notified_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
    resolved_by: Option<String>,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    crate::routes::access::require_platform_admin(&state, &ctx).await?;
    let status = q.status.unwrap_or_else(|| "open".into());
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT e.id, e.source, e.kind, e.message, e.method, e.path, e.count, e.first_seen, e.last_seen, e.notified_at,
                e.resolved_at, u.name AS resolved_by
           FROM error_events e LEFT JOIN users u ON u.id = e.resolved_by
          WHERE ($1 = 'all') OR ($1 = 'open' AND e.resolved_at IS NULL) OR ($1 = 'resolved' AND e.resolved_at IS NOT NULL)
          ORDER BY e.last_seen DESC LIMIT 200",
    )
    .bind(&status)
    .fetch_all(&state.db)
    .await?;
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM error_events WHERE resolved_at IS NULL").fetch_one(&state.db).await?;
    let last_start: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT max(started_at) FROM service_starts").fetch_one(&state.db).await?;
    Ok(Json(json!({
        "items": rows,
        "open": open,
        "email": state.cfg.email.is_some(),
        "recipients": state.cfg.platform_admins.iter().map(|e| crate::mailer::mask_email(e)).collect::<Vec<_>>(),
        "last_start": last_start,
    })))
}

/// Sends a test alert through the real path (event, in-app, email) and says whether the email went out.
async fn test(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    crate::routes::access::require_platform_admin(&state, &ctx).await?;
    // The same path as a real error (grouping, reopening after Resolve), but always delivered.
    let r = Report {
        source: "test",
        kind: "Test alert".into(),
        message: format!("Sent by {} to check that error alerts reach you.", ctx.name),
        method: String::new(),
        path: String::new(),
    };
    let (id, outcome) = record(&state, &r, true).await.map_err(AppError::Other)?;
    Ok(Json(json!({ "id": id, "email": outcome })))
}

async fn resolve(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    crate::routes::access::require_platform_admin(&state, &ctx).await?;
    let done = sqlx::query("UPDATE error_events SET resolved_at = now(), resolved_by = $2 WHERE id = $1 AND resolved_at IS NULL")
        .bind(id)
        .bind(ctx.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if done == 0 {
        return Err(AppError::NotFound("Open error"));
    }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_length_are_removed() {
        let s = clean("Daraja said: Bearer abcDEF123 and token 9f8e7d6c5b4a39281706f5e4d3c2b1a0ffeeddcc rest");
        assert!(!s.contains("abcDEF123") && !s.contains("9f8e7d6c5b4a"), "{s}");
        assert!(s.contains("Daraja said:") && s.contains("rest"));
        assert!(clean(&"x".repeat(1000)).chars().count() <= 401);
        assert_eq!(clean("Railway: You have reached the limit for custom domains"), "Railway: You have reached the limit for custom domains");
    }

    #[tokio::test]
    async fn a_panic_answers_500_and_carries_an_alert() {
        use tower::ServiceExt;
        let app: Router = Router::new()
            .route("/boom", get(|| async { if true { panic!("boom at the till") } }))
            .layer(tower_http::catch_panic::CatchPanicLayer::custom(panic_response));
        let res = app.oneshot(axum::http::Request::get("/boom").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let alert = res.extensions().get::<Alert>().expect("alert attached");
        assert_eq!(alert.message, "boom at the till");
    }

    #[test]
    fn business_answers_never_alert_but_faults_do() {
        use axum::response::IntoResponse;
        for e in [AppError::BadRequest("x".into()), AppError::Rule("x".into()), AppError::Unauthorized, AppError::Forbidden("x".into()), AppError::NotFound("x")] {
            assert!(e.into_response().extensions().get::<Alert>().is_none());
        }
        assert!(AppError::Upstream("Railway: limit".into()).into_response().extensions().get::<Alert>().is_some());
        assert!(AppError::Other(anyhow::anyhow!("x")).into_response().extensions().get::<Alert>().is_some());
        assert!(AppError::Db(sqlx::Error::PoolTimedOut).into_response().extensions().get::<Alert>().is_some());
        assert!(AppError::Db(sqlx::Error::RowNotFound).into_response().extensions().get::<Alert>().is_none());
    }

    #[test]
    fn repeats_alert_again_only_after_six_hours_or_after_resolve() {
        let now = Utc::now();
        assert!(should_alert(None, None, now), "first time");
        assert!(!should_alert(Some(now - chrono::Duration::minutes(5)), None, now), "repeat soon after: counted only");
        assert!(should_alert(Some(now - chrono::Duration::hours(7)), None, now), "still happening after 6 h");
        assert!(should_alert(Some(now - chrono::Duration::minutes(5)), Some(now - chrono::Duration::minutes(1)), now), "back after Resolve");
    }

    #[test]
    fn the_same_error_at_another_record_groups() {
        let a = Report { source: "server", kind: "Database error".into(), message: "row 12 missing".into(), method: "GET".into(), path: "/api/sales/7f9c2a3e-1111-4c4c-8888-123456789abc".into() };
        let b = Report { source: "server", kind: "Database error".into(), message: "row 99 missing".into(), method: "GET".into(), path: "/api/sales/0a0b0c0d-2222-4c4c-9999-abcdefabcdef".into() };
        let c = Report { source: "server", kind: "Database error".into(), message: "row 99 missing".into(), method: "POST".into(), path: "/api/sales".into() };
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&a), fingerprint(&c));
    }
}
