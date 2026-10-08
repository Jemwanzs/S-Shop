//! Performance leaderboards (roadmap 15): products and staff ranked by a chosen metric over a period and
//! branch, from the same net sale lines as the dashboard and reports (sales after returns), so figures agree
//! everywhere. Medals follow Settings → Reports (rank, or targets for value/units).

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::dashboard::LINES;
use super::Period;
use crate::auth::Ctx;
use crate::error::{bad, AppError, AppResult};
use crate::settings::{self, MEDALS};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/leaderboards/products", get(products)).route("/leaderboards/staff", get(staff))
}

#[derive(Deserialize)]
struct Query_ {
    #[serde(flatten)]
    period: Period,
    branch_id: Option<Uuid>,
    category_id: Option<Uuid>,
    metric: Option<String>,
    #[serde(default, deserialize_with = "super::de::opt_i64")]
    limit: Option<i64>,
}

const PRODUCT_METRICS: [&str; 6] = ["revenue", "units", "sales", "orders", "profit", "margin"];
const STAFF_METRICS: [&str; 9] = ["revenue", "units", "transactions", "avg_sale", "orders", "customers", "new_customers", "discounts", "credit"];

fn can_view(ctx: &Ctx) -> AppResult<()> {
    ctx.require_any(&["dashboard.view", "reports.view"])
}

/// Medal for position `i`: targets apply to value/units (as on the dashboard); other metrics rank the top three.
fn medal(s: &settings::TenantSettings, t: &settings::MedalTargets, metric: &str, i: usize, revenue: Decimal, units: i64, days: i64) -> Option<&'static str> {
    if matches!(metric, "revenue" | "units") {
        let mut t = t.clone();
        t.basis = if metric == "units" { settings::MedalBasis::Units } else { settings::MedalBasis::Revenue };
        s.reports.medals.award(&t, i, revenue, units, days)
    } else {
        MEDALS.get(i).copied()
    }
}

async fn products(State(state): State<AppState>, ctx: Ctx, Query(q): Query<Query_>) -> AppResult<Json<Value>> {
    can_view(&ctx)?;
    let metric = q.metric.as_deref().unwrap_or("revenue");
    if !PRODUCT_METRICS.contains(&metric) {
        return Err(bad("Unknown metric"));
    }
    let fin = ctx.can("sales.view_financials");
    if matches!(metric, "profit" | "margin") && !fin {
        return Err(AppError::Forbidden("Profit figures need permission".into()));
    }
    // Leaderboard scope (roadmap 64): own sales only, assigned branches or all branches; sales count for their owner.
    let vis = ctx.visibility(&mut *state.db.acquire().await?, "leaderboards", q.branch_id, None).await?;
    let branches = vis.branches;
    let (from, to) = q.period.resolve(ctx.today(), "month");
    let order_by = match metric {
        "units" => "units",
        "sales" => "sales",
        "orders" => "orders",
        "profit" => "profit",
        "margin" => "margin",
        _ => "revenue",
    };
    let rows: Vec<(Uuid, String, String, Option<String>, Decimal, i64, i64, i64, Option<Decimal>, Option<Decimal>)> = sqlx::query_as(&format!(
        "WITH {LINES},
         ord AS (SELECT oi.product_id, COUNT(DISTINCT o.id) AS n FROM order_items oi JOIN orders o ON o.id = oi.order_id
                 WHERE o.tenant_id = $1 AND o.branch_id = ANY($2) AND o.business_date BETWEEN $3 AND $4
                   AND o.status NOT IN ('cancelled','rejected') GROUP BY oi.product_id),
         agg AS (SELECT product_id, SUM(revenue) AS revenue, SUM(qty)::bigint AS units, COUNT(DISTINCT sale_id) AS sales,
                        SUM(profit) AS profit,
                        ROUND(SUM(profit) * 100 / NULLIF(SUM(costed_revenue), 0), 1) AS margin
                 FROM lines GROUP BY product_id)
         SELECT p.id, p.name, p.code, c.name, COALESCE(a.revenue, 0) AS revenue, COALESCE(a.units, 0) AS units,
                COALESCE(a.sales, 0) AS sales, COALESCE(ord.n, 0) AS orders, a.profit AS profit, a.margin AS margin
         FROM products p LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN agg a ON a.product_id = p.id LEFT JOIN ord ON ord.product_id = p.id
         WHERE p.tenant_id = $1 AND (a.product_id IS NOT NULL OR ord.product_id IS NOT NULL)
           AND ($6::uuid IS NULL OR p.category_id = $6)
         ORDER BY {order_by} DESC NULLS LAST, revenue DESC, p.name LIMIT $8"
    ))
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    .bind(None::<Uuid>)
    .bind(q.category_id)
    .bind(vis.owner)
    .bind(q.limit.unwrap_or(25).clamp(1, 100))
    .fetch_all(&state.db)
    .await?;
    let s = settings::load(&mut *state.db.acquire().await?, ctx.tenant_id).await?;
    let days = (to - from).num_days() + 1;
    Ok(Json(json!({
        "from": from, "to": to, "metric": metric, "metrics": PRODUCT_METRICS.iter().filter(|m| fin || !matches!(**m, "profit" | "margin")).collect::<Vec<_>>(),
        "items": rows.into_iter().enumerate().map(|(i, (id, name, code, category, revenue, units, sales, orders, profit, margin))| json!({
            "id": id, "name": name, "code": code, "category": category, "revenue": revenue, "units": units, "sales": sales, "orders": orders,
            "profit": if fin { profit } else { None }, "margin": if fin { margin } else { None },
            "medal": medal(&s, &s.reports.medals.products, metric, i, revenue, units, days),
        })).collect::<Vec<_>>(),
    })))
}

async fn staff(State(state): State<AppState>, ctx: Ctx, Query(q): Query<Query_>) -> AppResult<Json<Value>> {
    can_view(&ctx)?;
    let metric = q.metric.as_deref().unwrap_or("revenue");
    if !STAFF_METRICS.contains(&metric) {
        return Err(bad("Unknown metric"));
    }
    // Own scope: only the user's own row; otherwise everyone in the visible branches, credited by Sale Owner.
    let vis = ctx.visibility(&mut *state.db.acquire().await?, "leaderboards", q.branch_id, None).await?;
    let branches = vis.branches;
    let (from, to) = q.period.resolve(ctx.today(), "month");
    let order_by = match metric {
        "units" => "units",
        "transactions" => "transactions",
        "avg_sale" => "avg_sale",
        "orders" => "orders",
        "customers" => "customers",
        "new_customers" => "new_customers",
        "discounts" => "discounts",
        "credit" => "credit",
        _ => "revenue",
    };
    #[allow(clippy::type_complexity)]
    let rows: Vec<(Uuid, String, String, Decimal, i64, i64, Option<Decimal>, i64, i64, i64, Decimal, Decimal)> = sqlx::query_as(&format!(
        "WITH {LINES},
         per AS (SELECT user_id, SUM(revenue) AS revenue, SUM(qty)::bigint AS units, COUNT(DISTINCT sale_id) AS transactions,
                        COUNT(DISTINCT customer_id) AS customers, SUM(discount) AS discounts,
                        COALESCE(SUM(revenue) FILTER (WHERE payment_method = 'credit'), 0) AS credit
                 FROM lines GROUP BY user_id),
         ord AS (SELECT e.user_id, COUNT(DISTINCT e.order_id) AS n FROM order_events e JOIN orders o ON o.id = e.order_id
                 WHERE o.tenant_id = $1 AND o.branch_id = ANY($2)
                   AND business_date_of(e.created_at, o.tenant_id, o.branch_id) BETWEEN $3 AND $4 GROUP BY e.user_id),
         acq AS (SELECT created_by AS user_id, COUNT(*) AS n FROM customers
                 WHERE tenant_id = $1 AND business_date_of(created_at, tenant_id, NULL) BETWEEN $3 AND $4 GROUP BY created_by)
         SELECT u.id, u.name, r.name, COALESCE(per.revenue, 0) AS revenue, COALESCE(per.units, 0) AS units,
                COALESCE(per.transactions, 0) AS transactions,
                ROUND(per.revenue / NULLIF(per.transactions, 0), 2) AS avg_sale,
                COALESCE(ord.n, 0) AS orders, COALESCE(per.customers, 0) AS customers, COALESCE(acq.n, 0) AS new_customers,
                COALESCE(per.discounts, 0) AS discounts, COALESCE(per.credit, 0) AS credit
         FROM users u JOIN roles r ON r.id = u.role_id
         LEFT JOIN per ON per.user_id = u.id LEFT JOIN ord ON ord.user_id = u.id LEFT JOIN acq ON acq.user_id = u.id
         WHERE u.tenant_id = $1 AND (per.user_id IS NOT NULL OR ord.user_id IS NOT NULL OR acq.user_id IS NOT NULL)
           AND ($7::uuid IS NULL OR u.id = $7)
         ORDER BY {order_by} DESC NULLS LAST, revenue DESC, u.name LIMIT $8"
    ))
    .bind(ctx.tenant_id)
    .bind(&branches)
    .bind(from)
    .bind(to)
    .bind(None::<Uuid>)
    .bind(q.category_id)
    .bind(vis.owner)
    .bind(q.limit.unwrap_or(25).clamp(1, 100))
    .fetch_all(&state.db)
    .await?;
    let s = settings::load(&mut *state.db.acquire().await?, ctx.tenant_id).await?;
    let days = (to - from).num_days() + 1;
    Ok(Json(json!({
        "from": from, "to": to, "metric": metric, "metrics": STAFF_METRICS,
        "items": rows.into_iter().enumerate().map(|(i, (id, name, role, revenue, units, tx, avg, orders, customers, new_c, discounts, credit))| json!({
            "id": id, "name": name, "role": role, "revenue": revenue, "units": units, "transactions": tx, "avg_sale": avg,
            "orders": orders, "customers": customers, "new_customers": new_c, "discounts": discounts, "credit": credit,
            "medal": medal(&s, &s.reports.medals.staff, metric, i, revenue, units, days),
        })).collect::<Vec<_>>(),
    })))
}
