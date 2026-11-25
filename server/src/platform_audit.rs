//! Roadmap 182: the confidential Platform Security Audit Trail (`platform_audit_log`, append-only).
//!
//! Support sessions — requested, approved / declined, started, ended, revoked, expired, signed out — and every change
//! attempted inside one are written here with the real platform administrator, never into the business's own
//! operational audit feed. The business still sees its support sessions (Settings → Security → Support access) and is
//! notified; changes made by support in its data appear in its audit trail marked *S'Shop Support*.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppResult;
use crate::state::AppState;

pub struct Event<'a> {
    pub event: &'a str,
    pub session: Option<Uuid>,
    pub tenant: Option<Uuid>,
    pub user: Option<Uuid>,
    pub method: &'a str,
    pub path: &'a str,
    pub status: Option<i32>,
    pub detail: Value,
}

impl<'a> Event<'a> {
    pub fn new(event: &'a str, session: Uuid) -> Self {
        Event { event, session: Some(session), tenant: None, user: None, method: "", path: "", status: None, detail: serde_json::json!({}) }
    }
    pub fn detail(mut self, d: Value) -> Self {
        self.detail = d;
        self
    }
}

/// Writes one record. The session's business, platform administrator, scope and reason are copied from the session
/// (snapshots: the record stays readable whatever happens to the session, the business or the person later).
pub async fn record(conn: &mut PgConnection, e: Event<'_>, ip: &str, user_agent: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO platform_audit_log (event, session_id, tenant_id, tenant_name, platform_user_id, platform_user, platform_email,
                                        scope, reason, method, path, status, detail, ip, user_agent)
         SELECT $1, $2, COALESCE($3, s.tenant_id), COALESCE(t.name, ''), COALESCE($4, s.user_id), COALESCE(u.name, ''), COALESCE(lower(u.email), ''),
                COALESCE(s.scope, ''), COALESCE(s.reason, ''), $5, $6, $7, $8, $9, $10
         FROM (SELECT 1) one
         LEFT JOIN support_sessions s ON s.id = $2
         LEFT JOIN tenants t ON t.id = COALESCE($3, s.tenant_id)
         LEFT JOIN users u ON u.id = COALESCE($4, s.user_id)",
    )
    .bind(e.event)
    .bind(e.session)
    .bind(e.tenant)
    .bind(e.user)
    .bind(e.method)
    .bind(e.path)
    .bind(e.status)
    .bind(e.detail)
    .bind(ip)
    .bind(user_agent)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Ends the support session carried by a sign-in session that was signed out of or timed out (no orphaned access).
pub async fn close_with_session(state: &AppState, ses: Uuid, note: &str, event: &str) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let sid: Option<Uuid> = sqlx::query_scalar(
        "UPDATE support_sessions s SET status = 'ended', ended_at = now(), end_note = $2
         FROM user_sessions us WHERE us.id = $1 AND s.id = us.support_session_id AND s.status = 'active' RETURNING s.id",
    )
    .bind(ses)
    .bind(note)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(sid) = sid {
        record(&mut tx, Event::new(event, sid), "", "").await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Every request made with a support-session token: a change (or an attempted one) is recorded one by one with its
/// outcome; reads are counted on the session. Runs after the request, so the outcome is the real one.
pub async fn track(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let support = support_claims(&state, &req);
    let method = req.method().clone();
    let path = req.extensions().get::<axum::extract::OriginalUri>().map_or_else(|| req.uri().path().to_string(), |u| u.0.path().to_string());
    let (ip, ua) = crate::auth::client_meta(req.headers());
    let res = next.run(req).await;
    let Some(sid) = support else { return res };
    let status = res.status().as_u16() as i32;
    let read = method == axum::http::Method::GET || method == axum::http::Method::HEAD;
    // The live stream, heartbeats and ending the session itself are not activity in the business.
    if path.ends_with("/events") || path.ends_with("/auth/activity") || path.contains("/platform/support/") {
        return res;
    }
    let result: AppResult<()> = async {
        let mut tx = state.db.begin().await?;
        if read {
            sqlx::query("UPDATE support_sessions SET reads = reads + 1 WHERE id = $1").bind(sid).execute(&mut *tx).await?;
        } else {
            let ok = status < 400;
            sqlx::query(if ok { "UPDATE support_sessions SET changes = changes + 1 WHERE id = $1" } else { "UPDATE support_sessions SET refused = refused + 1 WHERE id = $1" })
                .bind(sid)
                .execute(&mut *tx)
                .await?;
            let e = Event { method: method.as_str(), path: &path, status: Some(status), ..Event::new(if ok { "change" } else { "change_refused" }, sid) };
            record(&mut tx, e, &ip, &ua).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::warn!("platform audit: {e:?}");
    }
    res
}

/// The support session named by a request's token (signature checked; validity is the extractor's job).
fn support_claims(state: &AppState, req: &Request) -> Option<Uuid> {
    let v = req.headers().get("authorization")?.to_str().ok()?;
    let token = v.strip_prefix("Bearer ")?.trim();
    let claims = crate::auth::read_token(&state.cfg.jwt_secret, token, "staff").ok()?;
    claims.home?;
    claims.sid
}
