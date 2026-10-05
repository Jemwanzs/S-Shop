//! Platform administration (platform admins only): every business on this S'Shop installation, opening one
//! to work inside it (audited in both businesses), and the Pablo Niche demo business.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::access::require_platform_admin;
use crate::audit::{self, Entry};
use crate::auth::{issue_acting_token, issue_token, Ctx, STAFF_TOKEN_HOURS};
use crate::error::{rule, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/platform/tenants", get(list))
        .route("/platform/tenants/{id}/open", post(open))
        .route("/platform/demo", get(demo_status).post(demo_start))
}

#[derive(Serialize, sqlx::FromRow)]
struct TenantRow {
    id: Uuid,
    name: String,
    slug: String,
    is_demo: bool,
    created_at: DateTime<Utc>,
    users: i64,
    branches: i64,
    sales: i64,
    last_sale_at: Option<DateTime<Utc>>,
}

async fn list(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let rows: Vec<TenantRow> = sqlx::query_as(
        "SELECT t.id, t.name, t.slug, t.is_demo, t.created_at,
                (SELECT COUNT(*) FROM users u WHERE u.tenant_id = t.id AND u.is_active) AS users,
                (SELECT COUNT(*) FROM branches b WHERE b.tenant_id = t.id AND b.is_active) AS branches,
                (SELECT COUNT(*) FROM sales s WHERE s.tenant_id = t.id) AS sales,
                (SELECT max(created_at) FROM sales s WHERE s.tenant_id = t.id) AS last_sale_at
         FROM tenants t ORDER BY t.is_demo, t.created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "items": rows, "home_tenant_id": ctx.acting_from.unwrap_or(ctx.tenant_id), "current_tenant_id": ctx.tenant_id })))
}

/// Gives the platform admin a session inside another business (full access, re-checked on every request).
/// Opening their own business returns a normal session.
async fn open(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Business"))?;
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    let token = if id == home {
        issue_token(&state.cfg.jwt_secret, ctx.user_id, home, "staff", chrono::Duration::hours(STAFF_TOKEN_HOURS))?
    } else {
        issue_acting_token(&state.cfg.jwt_secret, ctx.user_id, home, id)?
    };
    // Recorded in the business being opened (its own audit trail shows platform access) and at home.
    let mut tx = state.db.begin().await?;
    for tenant in [id, home] {
        let mut c = ctx.clone();
        c.tenant_id = tenant;
        audit::record(&mut tx, &c, Entry::new("platform", "open_business", "tenant", id).after(json!({ "business": name, "by": ctx.name }))).await?;
    }
    tx.commit().await?;
    let profile = super::auth::load_profile(&state, ctx.user_id, id, (id != home).then_some(home)).await?;
    Ok(Json(json!({ "token": token, "profile": profile })))
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct DemoStatus {
    pub running: bool,
    pub step: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub report: Option<crate::demo::DemoReport>,
    pub error: Option<String>,
}

async fn demo_status(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let status = state.demo.lock().map(|s| s.clone()).unwrap_or_default();
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1 AND is_demo").bind(crate::demo::DEMO_SLUG).fetch_optional(&state.db).await?;
    Ok(Json(json!({ "status": status, "tenant_id": exists, "pexels": std::env::var("PEXELS_API_KEY").is_ok() })))
}

#[derive(Deserialize)]
struct DemoBody {
    #[serde(default)]
    reset: bool,
}

/// Builds (or with `reset`, rebuilds) the demo business in the background; poll GET for progress.
async fn demo_start(State(state): State<AppState>, ctx: Ctx, Json(b): Json<DemoBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    {
        let mut s = state.demo.lock().map_err(|_| AppError::Other(anyhow::anyhow!("demo status poisoned")))?;
        if s.running {
            return Err(rule("The demo business is already being built"));
        }
        *s = DemoStatus { running: true, step: "Starting".into(), started_at: Some(Utc::now()), ..Default::default() };
    }
    let mut tx = state.db.begin().await?;
    audit::record(&mut tx, &ctx, Entry::new("platform", if b.reset { "reset_demo" } else { "build_demo" }, "tenant", ctx.tenant_id)).await?;
    tx.commit().await?;
    let st = state.clone();
    tokio::spawn(async move {
        let progress = |step: &str| {
            if let Ok(mut s) = st.demo.lock() {
                s.step = step.to_string();
            }
            tracing::info!(step, "demo seed");
        };
        let result = crate::demo::seed(&st, b.reset, progress).await;
        if let Ok(mut s) = st.demo.lock() {
            s.running = false;
            s.finished_at = Some(Utc::now());
            match result {
                Ok(r) => {
                    s.step = "Done".into();
                    s.report = Some(r);
                }
                Err(e) => {
                    tracing::error!(error = %format!("{e:#}"), "demo seed failed");
                    s.step = "Failed".into();
                    s.error = Some(format!("{e:#}"));
                }
            }
        }
    });
    Ok(Json(json!({ "started": true })))
}
