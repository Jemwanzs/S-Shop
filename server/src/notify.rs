//! In-app notifications. Rows are stored per recipient and pushed live over
//! SSE. Delivery is best-effort: failures are logged, never surfaced to the
//! action that triggered them. Call these *after* the business transaction commits.

use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

pub struct Note {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub link: String,
    /// Prevents repeats (e.g. one low-stock alert per product per day).
    pub dedupe_key: Option<String>,
}

impl Note {
    pub fn new(kind: &'static str, title: impl Into<String>, body: impl Into<String>, link: impl Into<String>) -> Self {
        Self { kind, title: title.into(), body: body.into(), link: link.into(), dedupe_key: None }
    }
    pub fn dedupe(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }
}

/// Notify every active user holding `perm` who can see `branch_id` (None = any branch).
pub async fn to_permission(state: &AppState, tenant_id: Uuid, branch_id: Option<Uuid>, perm: &str, note: Note) {
    let recipients: Result<Vec<Uuid>, _> = sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id
         WHERE u.tenant_id = $1 AND u.is_active
           AND ('*' = ANY(r.permissions) OR $2 = ANY(r.permissions))
           AND ($3::uuid IS NULL OR u.all_branches OR '*' = ANY(r.permissions)
                OR EXISTS (SELECT 1 FROM user_branches ub WHERE ub.user_id = u.id AND ub.branch_id = $3))",
    )
    .bind(tenant_id)
    .bind(perm)
    .bind(branch_id)
    .fetch_all(&state.db)
    .await;

    match recipients {
        Ok(users) => to_users(state, tenant_id, &users, note).await,
        Err(e) => tracing::warn!(error = %e, "notification recipients lookup failed"),
    }
}

pub async fn to_users(state: &AppState, tenant_id: Uuid, users: &[Uuid], note: Note) {
    for &user_id in users {
        let inserted: Result<Option<(Uuid, chrono::DateTime<chrono::Utc>)>, _> = sqlx::query_as(
            "INSERT INTO notifications (tenant_id, user_id, kind, title, body, link, dedupe_key)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (user_id, dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING
             RETURNING id, created_at",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(note.kind)
        .bind(&note.title)
        .bind(&note.body)
        .bind(&note.link)
        .bind(&note.dedupe_key)
        .fetch_optional(&state.db)
        .await;

        match inserted {
            Ok(Some((id, created_at))) => state.emit(
                tenant_id,
                Some(user_id),
                "notification",
                json!({ "id": id, "kind": note.kind, "title": note.title, "body": note.body,
                        "link": note.link, "created_at": created_at, "read_at": null }),
            ),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "notification insert failed"),
        }
    }
}

/// Fire-and-forget business-initiated WhatsApp message (only when the integration
/// is configured). Uses the approved notification template when one is set, since
/// free-form text is only delivered inside the 24h customer-service window.
pub fn whatsapp(state: &AppState, tenant_id: Uuid, phone: String, body: String) {
    if !crate::integrations::whatsapp::is_configured(state) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::integrations::whatsapp::send_notification(&state, Some(tenant_id), &phone, &body).await {
            tracing::warn!(error = %e, "whatsapp notification failed");
        }
    });
}
