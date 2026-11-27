//! In-app notifications. Rows are stored per recipient and pushed live over
//! SSE. Delivery is best-effort: failures are logged, never surfaced to the
//! action that triggered them. Call these *after* the business transaction commits.
//!
//! Roadmap 194: every notification has a **category** (one *Notifications* permission each), the **module permission**
//! its record needs and, when it concerns one branch, that **branch**. A person receives — and later sees, counts and
//! opens — only notifications whose category their role allows, whose module they may use and whose branch they work
//! at. Security alerts about the account and business (support access) are always delivered. Because the rule is
//! applied again whenever notifications are read, a change of role, permissions or branches takes effect at once,
//! without deleting anything.

use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Clone)]
pub struct Note {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub link: String,
    /// Prevents repeats (e.g. one low-stock alert per product per day).
    pub dedupe_key: Option<String>,
    /// Roadmap 194: the branch the notification concerns (None = not branch-specific).
    pub branch_id: Option<Uuid>,
}

/// Roadmap 194: what a notification kind is about.
pub struct Class {
    /// The *Notifications* permission is `notify.{category}`.
    pub category: &'static str,
    /// The module permission needed to see the record it is about.
    pub perm: Option<&'static str>,
    /// Account and business security alerts: always delivered to their recipients.
    pub mandatory: bool,
    /// Requests assigned to (or raised by) a person reach them wherever the request's branch is.
    pub ignores_branch: bool,
}

/// Every notification kind's category, module permission and handling (one place; unknown kinds are administrative).
pub fn classify(kind: &str) -> Class {
    let c = |category, perm, mandatory, ignores_branch| Class { category, perm, mandatory, ignores_branch };
    match kind {
        "new_order" => c("orders", Some("orders.view"), false, false),
        "credit_overdue" => c("credit", Some("credit.view"), false, false),
        "low_stock" | "out_of_stock" => c("stock", Some("stock.view"), false, false),
        k if k.starts_with("transfer_") => c("stock", Some("stock.view"), false, false),
        "product_deactivated" => c("stock", Some("products.view"), false, false),
        "mpesa_attention" => c("sales", None, false, false),
        "approval_pending" | "approval_handled" => c("approvals", None, false, true),
        "approval_decided" | "approval_stuck" => c("own_requests", None, false, true),
        "website_request" | "website_service" | "website_domain" | "article_review" | "article_ai_limit" => c("website", Some("website.view"), false, false),
        "support_request" | "support_started" | "support_decision" | "access_request" | "error_alert" => c("admin", None, true, true),
        _ => c("admin", None, false, false),
    }
}

/// Categories, in the order the notification centre lists them.
pub const CATEGORIES: [(&str, &str); 10] = [
    ("sales", "Sales"), ("credit", "Credit & repayments"), ("stock", "Stock & transfers"), ("customers", "Customers & loyalty"),
    ("expenses", "Expenses"), ("orders", "Orders"), ("approvals", "Approvals assigned to me"), ("own_requests", "My requests"),
    ("website", "Website"), ("admin", "Administration & security"),
];

/// SQL: whether user `u` (role `r`, effective permissions `e.p`) may receive a notification — roadmap 194. Placeholders
/// `{mandatory}`, `{cat}` (`notify.…`), `{perm}`, `{any_branch}`, `{branch}` are replaced by bind positions.
pub const ELIGIBLE: &str = "u.is_active
    AND ({mandatory} OR (('*' = ANY(r.permissions) OR {cat} = ANY(e.p)) AND NOT ({cat} = ANY(r.excluded_permissions))))
    AND ({perm}::text IS NULL OR (('*' = ANY(r.permissions) OR {perm} = ANY(e.p)) AND NOT ({perm} = ANY(r.excluded_permissions))))
    AND ({any_branch} OR {branch}::uuid IS NULL OR '*' = ANY(r.permissions) OR u.all_branches
         OR EXISTS (SELECT 1 FROM user_branches ub WHERE ub.user_id = u.id AND ub.branch_id = {branch}))";

impl Note {
    pub fn new(kind: &'static str, title: impl Into<String>, body: impl Into<String>, link: impl Into<String>) -> Self {
        Self { kind, title: title.into(), body: body.into(), link: link.into(), dedupe_key: None, branch_id: None }
    }
    /// The branch this notification concerns (roadmap 194).
    pub fn branch(mut self, branch: Option<Uuid>) -> Self {
        self.branch_id = branch;
        self
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
           AND ('*' = ANY(r.permissions) OR $2 = ANY(effective_permissions(r.permissions, u.extra_permissions)))
           AND NOT ($2 = ANY(r.excluded_permissions))
           AND ($3::uuid IS NULL OR u.all_branches OR '*' = ANY(r.permissions)
                OR EXISTS (SELECT 1 FROM user_branches ub WHERE ub.user_id = u.id AND ub.branch_id = $3))",
    )
    .bind(tenant_id)
    .bind(perm)
    .bind(branch_id)
    .fetch_all(&state.db)
    .await;

    let note = if note.branch_id.is_none() { note.branch(branch_id) } else { note };
    match recipients {
        Ok(users) => to_users(state, tenant_id, &users, note).await,
        Err(e) => tracing::warn!(error = %e, "notification recipients lookup failed"),
    }
}

/// Roadmap 69: a new order goes to active users who may manage orders at its branch, whose orders scope reaches it
/// (an *own records* scope never sees orders placed by customers) and who have not switched off new-order alerts.
pub async fn to_order_staff(state: &AppState, tenant_id: Uuid, branch_id: Uuid, note: Note) {
    let recipients: Result<Vec<Uuid>, _> = sqlx::query_scalar(
        "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id
         CROSS JOIN LATERAL (SELECT effective_permissions(r.permissions, u.extra_permissions) AS p) e
         WHERE u.tenant_id = $1 AND u.is_active
           AND ('*' = ANY(r.permissions) OR (
                'orders.manage' = ANY(e.p)
                AND NOT ('scope.orders.own' = ANY(e.p))
                AND ('scope.orders.branches' = ANY(e.p) OR 'scope.orders.all' = ANY(e.p) OR 'staff.view_others' = ANY(e.p))))
           AND (u.all_branches OR '*' = ANY(r.permissions) OR 'scope.orders.all' = ANY(e.p)
                OR EXISTS (SELECT 1 FROM user_branches ub WHERE ub.user_id = u.id AND ub.branch_id = $2))
           AND NOT ('orders.manage' = ANY(r.excluded_permissions))
           AND COALESCE((u.preferences->>'notify_new_orders')::boolean, true)",
    )
    .bind(tenant_id)
    .bind(branch_id)
    .fetch_all(&state.db)
    .await;
    let note = note.branch(Some(branch_id));
    match recipients {
        Ok(users) => to_users(state, tenant_id, &users, note).await,
        Err(e) => tracing::warn!(error = %e, "order notification recipients lookup failed"),
    }
}

pub async fn to_users(state: &AppState, tenant_id: Uuid, users: &[Uuid], note: Note) {
    let class = classify(note.kind);
    // Roadmap 194: only people this notification is for — by category, module permission and branch.
    let sql = format!(
        "SELECT u.id FROM users u JOIN roles r ON r.id = u.role_id CROSS JOIN LATERAL (SELECT effective_permissions(r.permissions, u.extra_permissions) AS p) e
         WHERE u.id = ANY($1) AND u.tenant_id = $2 AND {}",
        ELIGIBLE.replace("{mandatory}", "$3").replace("{cat}", "$4").replace("{perm}", "$5").replace("{any_branch}", "$6").replace("{branch}", "$7")
    );
    let eligible: Vec<Uuid> = match sqlx::query_scalar(&sql)
        .bind(users)
        .bind(tenant_id)
        .bind(class.mandatory)
        .bind(format!("notify.{}", class.category))
        .bind(class.perm)
        .bind(class.ignores_branch)
        .bind(note.branch_id)
        .fetch_all(&state.db)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "notification eligibility lookup failed");
            return;
        }
    };
    for &user_id in users.iter().filter(|u| eligible.contains(u)) {
        let inserted: Result<Option<(Uuid, chrono::DateTime<chrono::Utc>)>, _> = sqlx::query_as(
            "INSERT INTO notifications (tenant_id, user_id, kind, title, body, link, dedupe_key, category, perm, branch_id, mandatory)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
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
        .bind(class.category)
        .bind(class.perm)
        .bind(if class.ignores_branch { None } else { note.branch_id })
        .bind(class.mandatory)
        .fetch_optional(&state.db)
        .await;

        match inserted {
            Ok(Some((id, created_at))) => state.emit(
                tenant_id,
                Some(user_id),
                "notification",
                json!({ "id": id, "kind": note.kind, "title": note.title, "body": note.body,
                        "link": note.link, "created_at": created_at, "read_at": null, "category": class.category }),
            ),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "notification insert failed"),
        }
    }
}

/// The platform owner(s) (PLATFORM_ADMIN_EMAILS): in-app in whichever business they sign in to, and by email when email
/// is configured. Best-effort, like every notification.
pub async fn to_platform_admins(state: &AppState, note: Note, email_subject: &str, email_text: &str) {
    let admins: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT id, tenant_id FROM users WHERE is_active AND lower(email) = ANY($1) AND login_user_id IS NULL")
        .bind(&state.cfg.platform_admins)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (user, tenant) in admins {
        to_users(state, tenant, &[user], note.clone()).await;
    }
    if let Some(cfg) = &state.cfg.email {
        if !state.cfg.access_request_notify.is_empty() {
            if let Err(e) = crate::integrations::email::send(&state.http, cfg, &state.cfg.access_request_notify, email_subject, email_text).await {
                tracing::warn!(error = %e, "platform email failed");
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Roadmap 194: every kind the code sends has a deliberate category (a new kind must be added to `classify`).
    #[test]
    fn every_notification_kind_is_classified() {
        let src = [
            include_str!("routes/approvals.rs"), include_str!("routes/access.rs"), include_str!("routes/articles.rs"), include_str!("routes/seo_ai.rs"),
            include_str!("jobs.rs"), include_str!("alerts.rs"), include_str!("routes/stk_sales.rs"), include_str!("routes/orders.rs"),
            include_str!("routes/catalog.rs"), include_str!("routes/tenants.rs"), include_str!("routes/website.rs"), include_str!("routes/stock.rs"),
            include_str!("routes/transfers.rs"),
        ]
        .join("
");
        let mut kinds = Vec::new();
        for part in src.split("Note::new(").skip(1) {
            if let Some(k) = part.trim_start().strip_prefix('"').and_then(|r| r.split('"').next()) {
                kinds.push(k.to_string());
            }
        }
        for k in ["low_stock", "out_of_stock", "transfer_received", "transfer_discrepancy", "approval_stuck"] {
            kinds.push(k.to_string());
        }
        assert!(kinds.len() > 15);
        for k in &kinds {
            let c = classify(k);
            assert!(c.category != "admin" || c.mandatory || matches!(k.as_str(), "website_request"), "kind {k} falls back to Administration");
        }
        assert_eq!(classify("approval_stuck").category, "own_requests");
        assert_eq!(classify("new_order").perm, Some("orders.view"));
    }
}

