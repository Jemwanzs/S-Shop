//! Dashboard & analytics. Default period: This Week.
//! Filters: period/from/to, branch, product, category, user.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::Period;
use crate::auth::Ctx;
use crate::error::AppResult;
use crate::settings::{self, Valuation};
use crate::state::AppState;
use crate::util::{local_range, today_in};

pub fn routes() -> Router<AppState> {
    Router::new().route("/dashboard", get(dashboard))
}

#[derive(Deserialize, Default, Clone)]
pub struct Filters {
    #[serde(flatten)]
    pub period: Period,
    pub branch_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

/// Net sale lines (after returns) matching the filters.
/// Binds: $1 tenant, $2 branches, $3 start, $4 end, $5 product, $6 category, $7 user.
pub const LINES: &str = "lines AS (
    SELECT s.id AS sale_id, s.branch_id, s.user_id, s.customer_id, s.created_at, s.payment_method, si.product_id,
           (si.quantity - si.returned_qty) AS qty,
           (si.quantity - si.returned_qty) * si.unit_price AS revenue,
           CASE WHEN si.unit_cost IS NOT NULL THEN (si.quantity - si.returned_qty) * (si.unit_price - si.unit_cost) END AS profit,
           CASE WHEN si.unit_cost IS NOT NULL THEN (si.quantity - si.returned_qty) * si.unit_price END AS costed_revenue,
           (si.quantity - si.returned_qty) * (si.marked_price - si.unit_price) AS discount
    FROM sales s JOIN sale_items si ON si.sale_id = s.id JOIN products p ON p.id = si.product_id
    WHERE s.tenant_id = $1 AND s.branch_id = ANY($2) AND s.created_at >= $3 AND s.created_at < $4 AND s.status <> 'cancelled'
      AND ($5::uuid IS NULL OR si.product_id = $5) AND ($6::uuid IS NULL OR p.category_id = $6) AND ($7::uuid IS NULL OR s.user_id = $7))";

struct Scope {
    branches: Vec<Uuid>,
    from: NaiveDate,
    to: NaiveDate,
}

#[derive(sqlx::FromRow, Default)]
struct Totals {
    revenue: Decimal,
    profit: Decimal,
    costed_revenue: Decimal,
    discount: Decimal,
    units: i64,
    transactions: i64,
    customers: i64,
}

async fn totals(conn: &mut PgConnection, ctx: &Ctx, f: &Filters, scope: &Scope, from: NaiveDate, to: NaiveDate) -> AppResult<Totals> {
    let (start, end) = local_range(from, to, ctx.tz);
    Ok(sqlx::query_as(&format!(
        "WITH {LINES}
         SELECT COALESCE(SUM(revenue),0) AS revenue, COALESCE(SUM(profit),0) AS profit,
                COALESCE(SUM(costed_revenue),0) AS costed_revenue, COALESCE(SUM(discount),0) AS discount,
                COALESCE(SUM(qty),0)::bigint AS units, COUNT(DISTINCT sale_id) AS transactions,
                COUNT(DISTINCT customer_id) AS customers
         FROM lines"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_one(&mut *conn)
    .await?)
}

fn pct_change(now: Decimal, before: Decimal) -> Option<Decimal> {
    (before > Decimal::ZERO).then(|| ((now - before) / before * Decimal::ONE_HUNDRED).round_dp(1))
}

async fn dashboard(State(state): State<AppState>, ctx: Ctx, Query(f): Query<Filters>) -> AppResult<Json<Value>> {
    ctx.require("dashboard.view")?;
    let (from, to) = f.period.resolve(ctx.tz, "week");
    let scope = Scope { branches: ctx.branch_scope(f.branch_id)?, from, to };
    let (start, end) = local_range(from, to, ctx.tz);
    let mut conn = state.db.acquire().await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;
    let fin = ctx.can("sales.view_financials");
    let tz = ctx.tz.name();

    let now = totals(&mut conn, &ctx, &f, &scope, from, to).await?;
    let days = (to - from).num_days() + 1;
    let prev = totals(&mut conn, &ctx, &f, &scope, from - chrono::Duration::days(days), from - chrono::Duration::days(1)).await?;

    let expenses: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount),0) FROM expenses WHERE tenant_id = $1 AND branch_id = ANY($2) AND status = 'approved'
           AND expense_date BETWEEN $3 AND $4",
    )
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(scope.from)
    .bind(scope.to)
    .fetch_one(&mut *conn)
    .await?;

    let (orders, orders_value, open_orders): (i64, Decimal, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE created_at >= $3 AND created_at < $4),
                COALESCE(SUM(total) FILTER (WHERE created_at >= $3 AND created_at < $4 AND status NOT IN ('cancelled','rejected')),0),
                COUNT(*) FILTER (WHERE status NOT IN ('completed','delivered','cancelled','rejected','returned'))
         FROM orders WHERE tenant_id = $1 AND branch_id = ANY($2)",
    )
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *conn)
    .await?;

    let price = match s.stock.valuation {
        Valuation::Cost => "COALESCE(p.cost_price, p.marked_price)",
        Valuation::Selling => "p.marked_price",
    };
    let (stock_value, stock_units): (Decimal, i64) = sqlx::query_as(&format!(
        "SELECT COALESCE(SUM(GREATEST(sl.on_hand,0) * {price}),0)::numeric(14,2), COALESCE(SUM(GREATEST(sl.on_hand,0)),0)::bigint
         FROM stock_levels sl JOIN products p ON p.id = sl.product_id
         WHERE sl.tenant_id = $1 AND sl.branch_id = ANY($2) AND ($3::uuid IS NULL OR p.id = $3) AND ($4::uuid IS NULL OR p.category_id = $4)"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(f.product_id)
    .bind(f.category_id)
    .fetch_one(&mut *conn)
    .await?;

    let new_customers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3")
        .bind(ctx.tenant_id)
        .bind(start)
        .bind(end)
        .fetch_one(&mut *conn)
        .await?;
    let credit_outstanding: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(original_amount - amount_paid - adjustments),0) FROM credit_sales
         WHERE tenant_id = $1 AND branch_id = ANY($2) AND status IN ('outstanding','partially_paid')",
    )
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .fetch_one(&mut *conn)
    .await?;
    let (points_issued, points_redeemed): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(points) FILTER (WHERE kind IN ('earn','referral')),0)::bigint,
                COALESCE(-SUM(points) FILTER (WHERE kind = 'redeem'),0)::bigint
         FROM loyalty_ledger WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3",
    )
    .bind(ctx.tenant_id)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *conn)
    .await?;

    // Trend series: daily up to ~2 months, then weekly/monthly buckets.
    let bucket = if days <= 62 { "day" } else if days <= 366 { "week" } else { "month" };
    let series: Vec<(NaiveDate, Decimal, i64, Decimal)> = sqlx::query_as(&format!(
        "WITH {LINES}
         SELECT date_trunc('{bucket}', created_at AT TIME ZONE $8)::date AS d, COALESCE(SUM(revenue),0), COUNT(DISTINCT sale_id),
                COALESCE(SUM(profit),0)
         FROM lines GROUP BY 1 ORDER BY 1"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .bind(tz)
    .fetch_all(&mut *conn)
    .await?;

    let payment_mix: Vec<(String, Decimal, i64)> = sqlx::query_as(&format!(
        "WITH {LINES} SELECT payment_method, SUM(revenue), COUNT(DISTINCT sale_id) FROM lines GROUP BY 1 ORDER BY 2 DESC"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_all(&mut *conn)
    .await?;

    let products: Vec<(Uuid, String, i64, Decimal)> = sqlx::query_as(&format!(
        "WITH {LINES}
         SELECT l.product_id, p.name, SUM(l.qty)::bigint, SUM(l.revenue) FROM lines l JOIN products p ON p.id = l.product_id
         GROUP BY l.product_id, p.name"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_all(&mut *conn)
    .await?;
    let product_json = |v: &[(Uuid, String, i64, Decimal)]| -> Vec<Value> {
        v.iter().map(|(id, name, qty, rev)| json!({ "product_id": id, "name": name, "units": qty, "revenue": rev })).collect()
    };
    let mut by_revenue = products.clone();
    by_revenue.sort_by(|a, b| b.3.cmp(&a.3));
    let mut by_units = products.clone();
    by_units.sort_by(|a, b| b.2.cmp(&a.2));

    // Slow movers: in stock now but sold least (or not at all) in the period.
    let slow: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(&format!(
        "WITH {LINES}, sold AS (SELECT product_id, SUM(qty) AS qty FROM lines GROUP BY product_id)
         SELECT p.id, p.name, COALESCE(sold.qty,0)::bigint, SUM(sl.on_hand)::bigint
         FROM products p JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = ANY($2)
         LEFT JOIN sold ON sold.product_id = p.id
         WHERE p.tenant_id = $1 AND p.is_active AND ($5::uuid IS NULL OR p.id = $5) AND ($6::uuid IS NULL OR p.category_id = $6)
         GROUP BY p.id, p.name, sold.qty HAVING SUM(sl.on_hand) > 0
         ORDER BY COALESCE(sold.qty,0), SUM(sl.on_hand) DESC LIMIT 5"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_all(&mut *conn)
    .await?;

    let low_stock: Vec<(Uuid, String, String, i32, i32)> = sqlx::query_as(
        "SELECT p.id, p.name, b.name, sl.on_hand - sl.reserved, COALESCE(p.low_stock_threshold, $3)
         FROM stock_levels sl JOIN products p ON p.id = sl.product_id JOIN branches b ON b.id = sl.branch_id
         WHERE sl.tenant_id = $1 AND sl.branch_id = ANY($2) AND p.is_active
           AND sl.on_hand - sl.reserved <= COALESCE(p.low_stock_threshold, $3)
         ORDER BY sl.on_hand - sl.reserved, p.name LIMIT 8",
    )
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(s.stock.low_stock_threshold)
    .fetch_all(&mut *conn)
    .await?;

    let top_customers: Vec<(Uuid, String, String, Decimal, i64, i64, String)> = sqlx::query_as(&format!(
        "WITH {LINES}
         SELECT c.id, TRIM(c.first_name || ' ' || c.other_names), c.mobile, SUM(l.revenue), c.own_points, c.referral_points, c.tier
         FROM lines l JOIN customers c ON c.id = l.customer_id
         GROUP BY c.id ORDER BY SUM(l.revenue) DESC LIMIT 5"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_all(&mut *conn)
    .await?;

    let by_branch: Vec<(Uuid, String, Decimal, i64)> = sqlx::query_as(&format!(
        "WITH {LINES}
         SELECT b.id, b.name, COALESCE(SUM(l.revenue),0), COUNT(DISTINCT l.sale_id)
         FROM branches b LEFT JOIN lines l ON l.branch_id = b.id
         WHERE b.id = ANY($2) GROUP BY b.id, b.name ORDER BY 3 DESC"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_all(&mut *conn)
    .await?;

    let by_user: Vec<(Uuid, String, Decimal, i64, i64)> = sqlx::query_as(&format!(
        "WITH {LINES}
         SELECT u.id, u.name, SUM(l.revenue), COUNT(DISTINCT l.sale_id), SUM(l.qty)::bigint
         FROM lines l JOIN users u ON u.id = l.user_id GROUP BY u.id, u.name ORDER BY 3 DESC LIMIT 5"
    ))
    .bind(ctx.tenant_id)
    .bind(&scope.branches)
    .bind(start)
    .bind(end)
    .bind(f.product_id)
    .bind(f.category_id)
    .bind(f.user_id)
    .fetch_all(&mut *conn)
    .await?;

    let medals = ["Gold", "Silver", "Bronze"];
    let avg = if now.transactions > 0 { (now.revenue / Decimal::from(now.transactions)).round_dp(2) } else { Decimal::ZERO };
    let gross_profit = fin.then_some(now.profit);
    Ok(Json(json!({
        "from": scope.from, "to": scope.to, "today": today_in(ctx.tz), "bucket": bucket,
        "kpis": {
            "sales": now.revenue,
            "sales_change_pct": pct_change(now.revenue, prev.revenue),
            "transactions": now.transactions,
            "transactions_change_pct": pct_change(Decimal::from(now.transactions), Decimal::from(prev.transactions)),
            "average_transaction": avg,
            "units_sold": now.units,
            "discounts": now.discount,
            "orders": orders, "orders_value": orders_value, "open_orders": open_orders,
            "gross_profit": gross_profit,
            "profit_coverage_pct": fin.then(|| if now.revenue > Decimal::ZERO { (now.costed_revenue / now.revenue * Decimal::ONE_HUNDRED).round_dp(0) } else { Decimal::ZERO }),
            "expenses": expenses,
            "net_performance": if fin { Some(now.profit - expenses) } else { None },
            "stock_value": if fin || s.stock.valuation == Valuation::Selling { Some(stock_value) } else { None },
            "stock_units": stock_units,
            "customers": now.customers,
            "new_customers": new_customers,
            "credit_outstanding": if ctx.can("credit.view") { Some(credit_outstanding) } else { None },
            "points_issued": points_issued,
            "points_redeemed": points_redeemed,
        },
        "series": series.into_iter().map(|(d, rev, tx, profit)| json!({
            "date": d, "sales": rev, "transactions": tx, "profit": if fin { Some(profit) } else { None },
        })).collect::<Vec<_>>(),
        "payment_mix": payment_mix.into_iter().map(|(m, amount, n)| json!({ "method": m, "amount": amount, "count": n })).collect::<Vec<_>>(),
        "top_products_revenue": product_json(&by_revenue.into_iter().take(5).collect::<Vec<_>>()),
        "top_products_units": product_json(&by_units.into_iter().take(5).collect::<Vec<_>>()),
        "slow_movers": slow.into_iter().map(|(id, name, sold, on_hand)| json!({ "product_id": id, "name": name, "units": sold, "on_hand": on_hand })).collect::<Vec<_>>(),
        "low_stock": low_stock.into_iter().map(|(id, name, branch, avail, threshold)| json!({
            "product_id": id, "name": name, "branch_name": branch, "available": avail, "threshold": threshold,
        })).collect::<Vec<_>>(),
        "top_customers": top_customers.into_iter().enumerate().map(|(i, (id, name, mobile, spend, own, refp, tier))| json!({
            "customer_id": id, "name": name, "mobile": mobile, "spend": spend, "own_points": own, "referral_points": refp,
            "tier": tier, "medal": medals.get(i),
        })).collect::<Vec<_>>(),
        "by_branch": by_branch.into_iter().map(|(id, name, rev, tx)| json!({ "branch_id": id, "name": name, "sales": rev, "transactions": tx })).collect::<Vec<_>>(),
        "by_user": by_user.into_iter().enumerate().map(|(i, (id, name, rev, tx, units))| json!({
            "user_id": id, "name": name, "sales": rev, "transactions": tx, "units": units, "medal": medals.get(i),
        })).collect::<Vec<_>>(),
    })))
}
