//! Staff sessions and automatic logout after inactivity (roadmap 87).
//!
//! Every staff token carries a session id (`Claims.ses`). The session row holds the person's inactivity limit, so the
//! server — not just a browser timer — refuses a session left idle: `Ctx` ends it (`inactivity`, audited) and answers
//! 401 `session_expired`. Activity is only what the person does: the app's heartbeat after real input (taps, keys,
//! scrolling, typing) and requests the app marks as user-initiated (`X-User-Activity: 1`, sent with changes). Background
//! reads, polling, notifications and the live event stream never extend a session.
//!
//! Limits: the platform sets the default (5 min), the range (1–120 min), the warning (30 s) and the longest session
//! (12 h); a business sets its timeout within that range and stricter timeouts for chosen roles.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::routes::quickpin::platform_security;
use crate::state::AppState;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SessionPolicy {
    /// Automatic logout after this many minutes without activity.
    pub idle_minutes: u16,
    /// The expiry warning shows this many seconds before logout.
    pub warning_seconds: u16,
    /// Stricter (shorter) timeouts for sensitive roles.
    pub role_minutes: Vec<RoleMinutes>,
}
impl Default for SessionPolicy {
    fn default() -> Self {
        Self { idle_minutes: 5, warning_seconds: 30, role_minutes: Vec::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoleMinutes {
    pub role_id: Uuid,
    pub minutes: u16,
}

/// The person's effective (inactivity seconds, warning seconds): business timeout, a stricter role timeout if any,
/// always within the platform's range.
pub async fn effective(state: &AppState, user: Uuid, tenant: Uuid) -> AppResult<(i64, i64)> {
    let p = platform_security(&state.db).await?;
    let (policy, role): (Option<serde_json::Value>, Option<Uuid>) = sqlx::query_as(
        "SELECT t.settings->'security'->'session', (SELECT role_id FROM users WHERE id = $1 AND tenant_id = $2) FROM tenants t WHERE t.id = $2",
    )
    .bind(user)
    .bind(tenant)
    .fetch_one(&state.db)
    .await?;
    let policy: SessionPolicy = policy.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_else(|| SessionPolicy { idle_minutes: p.idle_default_minutes, warning_seconds: p.warning_seconds, ..Default::default() });
    let mut minutes = policy.idle_minutes;
    if let Some(r) = role {
        if let Some(rm) = policy.role_minutes.iter().find(|x| x.role_id == r) {
            minutes = minutes.min(rm.minutes);
        }
    }
    let (lo, hi) = (p.idle_min_minutes.max(1), p.idle_max_minutes.max(p.idle_min_minutes.max(1)));
    let idle = (minutes.clamp(lo, hi) as i64) * 60;
    let warning = (policy.warning_seconds as i64).clamp(10, 120).min(idle / 2);
    Ok((idle, warning))
}

pub enum Kind {
    Full,
    Quick,
    /// A platform support session (roadmap 71): the token ends with the support session at the latest.
    Support { home: Uuid, sid: Uuid, until: DateTime<Utc> },
}

/// Starts a session and returns its token.
pub async fn issue(state: &AppState, user: Uuid, tenant: Uuid, kind: Kind) -> AppResult<String> {
    issue_until(state, user, tenant, kind, None).await
}

/// The latest end for a session that continues `ses` without a new PIN (switching business, returning from support):
/// never past the longest session counted from when the person last entered a PIN, nor past `ses`'s own end unless
/// it was a support session (which is shorter by design).
pub async fn carried_end(state: &AppState, ses: Uuid) -> AppResult<Option<DateTime<Utc>>> {
    let p = platform_security(&state.db).await?;
    let row: Option<(DateTime<Utc>, DateTime<Utc>, String)> = sqlx::query_as("SELECT created_at, expires_at, kind FROM user_sessions WHERE id = $1")
        .bind(ses)
        .fetch_optional(&state.db)
        .await?;
    Ok(row.map(|(created, expires, kind)| {
        let longest = created + Duration::hours(p.max_session_hours.clamp(1, 24) as i64);
        if kind == "support" { longest } else { expires.min(longest) }
    }))
}

/// [`issue`], ending no later than `cap` (see [`carried_end`]).
pub async fn issue_until(state: &AppState, user: Uuid, tenant: Uuid, kind: Kind, cap: Option<DateTime<Utc>>) -> AppResult<String> {
    let p = platform_security(&state.db).await?;
    // The inactivity limit follows the business whose data is open (the platform owner's own for support access).
    let (idle, _) = effective(state, user, match &kind { Kind::Support { home, .. } => *home, _ => tenant }).await?;
    let now = Utc::now();
    let (label, until) = match &kind {
        Kind::Full => ("full", now + Duration::hours(p.max_session_hours.clamp(1, 24) as i64)),
        Kind::Quick => ("quick", now + Duration::hours(p.quick_session_hours.clamp(1, 24).min(p.max_session_hours.clamp(1, 24)) as i64)),
        Kind::Support { until, .. } => ("support", *until),
    };
    let until = cap.map_or(until, |c| until.min(c));
    if until <= now {
        return Err(AppError::Unauthorized);
    }
    let ses: Uuid = sqlx::query_scalar("INSERT INTO user_sessions (user_id, tenant_id, kind, idle_seconds, expires_at) VALUES ($1, $2, $3, $4, $5) RETURNING id")
        .bind(user)
        .bind(tenant)
        .bind(label)
        .bind(idle as i32)
        .bind(until)
        .fetch_one(&state.db)
        .await?;
    let claims = crate::auth::Claims {
        sub: user,
        tid: tenant,
        typ: "staff".into(),
        home: match &kind { Kind::Support { home, .. } => Some(*home), _ => None },
        sid: match &kind { Kind::Support { sid, .. } => Some(*sid), _ => None },
        qp: matches!(kind, Kind::Quick),
        ses: Some(ses),
        iat: now.timestamp(),
        exp: until.timestamp(),
    };
    crate::auth::encode_claims(&state.cfg.jwt_secret, &claims)
}

/// Grace for the heartbeat interval: the browser logs out at the limit; the server refuses a little after.
const SLACK: i64 = 30;

/// Checks the session on every request; with `activity`, also records genuine activity. Ends an idle session.
pub async fn check(state: &AppState, ses: Uuid, user: Uuid, activity: bool) -> AppResult<()> {
    let row: Option<(Option<DateTime<Utc>>, Option<String>, DateTime<Utc>, i32, DateTime<Utc>, Uuid)> =
        sqlx::query_as("SELECT ended_at, end_reason, last_active_at, idle_seconds, expires_at, tenant_id FROM user_sessions WHERE id = $1 AND user_id = $2")
            .bind(ses)
            .bind(user)
            .fetch_optional(&state.db)
            .await?;
    let (ended, reason, last, idle, expires, tenant) = row.ok_or(AppError::Unauthorized)?;
    // Only inactivity says so; a sign-out, a replaced session or the longest session just asks to sign in again.
    if ended.is_some() {
        return Err(if reason.as_deref() == Some("inactivity") { AppError::SessionExpired } else { AppError::Unauthorized });
    }
    if expires <= Utc::now() {
        return Err(AppError::Unauthorized);
    }
    if Utc::now() > last + Duration::seconds(idle as i64 + SLACK) {
        let n = sqlx::query("UPDATE user_sessions SET ended_at = now(), end_reason = 'inactivity' WHERE id = $1 AND ended_at IS NULL").bind(ses).execute(&state.db).await?.rows_affected();
        if n > 0 {
            let mut tx = state.db.begin().await?;
            crate::audit::system(&mut tx, tenant, Some(user), crate::audit::Entry::new("auth", "session_expired", "user", user).after(serde_json::json!({ "idle_seconds": idle })), "", "").await?;
            tx.commit().await?;
        }
        return Err(AppError::SessionExpired);
    }
    if activity {
        sqlx::query("UPDATE user_sessions SET last_active_at = now() WHERE id = $1 AND ended_at IS NULL").bind(ses).execute(&state.db).await?;
    }
    Ok(())
}

/// Is the session still usable (for long-lived streams)?
pub async fn alive(db: &sqlx::PgPool, ses: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1 AND ended_at IS NULL AND expires_at > now()
                        AND now() <= last_active_at + make_interval(secs => idle_seconds + $2))",
    )
    .bind(ses)
    .bind(SLACK as f64)
    .fetch_one(db)
    .await
    .unwrap_or(false)
}

/// Ends a session (sign-out, or the browser's inactivity timer) — audited.
pub async fn end(state: &AppState, ses: Uuid, user: Uuid, tenant: Uuid, reason: &str) -> AppResult<()> {
    let n = sqlx::query("UPDATE user_sessions SET ended_at = now(), end_reason = $3 WHERE id = $1 AND user_id = $2 AND ended_at IS NULL")
        .bind(ses)
        .bind(user)
        .bind(reason)
        .execute(&state.db)
        .await?
        .rows_affected();
    // A session handed over to a new one (switching business, a new PIN) is not a sign-out: only those are audited here.
    if n > 0 && reason != "replaced" {
        let mut tx = state.db.begin().await?;
        let action = if reason == "inactivity" { "session_expired" } else { "logout" };
        crate::audit::system(&mut tx, tenant, Some(user), crate::audit::Entry::new("auth", action, "user", user), "", "").await?;
        tx.commit().await?;
    }
    Ok(())
}

/// Refreshes the session's limit from the current policy (a changed policy applies at the next heartbeat).
pub async fn refresh_limit(state: &AppState, ses: Uuid, user: Uuid, tenant: Uuid) -> AppResult<(i64, i64)> {
    let (idle, warning) = effective(state, user, tenant).await?;
    sqlx::query("UPDATE user_sessions SET idle_seconds = $2, last_active_at = now() WHERE id = $1 AND ended_at IS NULL").bind(ses).bind(idle as i32).execute(&state.db).await?;
    Ok((idle, warning))
}
