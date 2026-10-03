//! Universal search across products, barcodes, customers, orders and receipts,
//! prioritising the Current Branch.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::like;
use crate::auth::Ctx;
use crate::error::AppResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/search", get(search))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(State(state): State<AppState>, ctx: Ctx, Query(q): Query<SearchQuery>) -> AppResult<Json<Value>> {
    let term = q.q.trim().to_string();
    if term.len() < 2 {
        return Ok(Json(json!({ "results": [] })));
    }
    let pattern = like(&Some(term.clone()));
    let mut results: Vec<Value> = Vec::new();

    if ctx.can("products.view") || ctx.can("stock.view") || ctx.can("sales.create") {
        let rows: Vec<(Uuid, String, String, Decimal, i32)> = sqlx::query_as(
            "SELECT p.id, p.name, p.code, p.marked_price, COALESCE(sl.on_hand - sl.reserved, 0)
             FROM products p LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $3
             WHERE p.tenant_id = $1 AND (p.name ILIKE $2 OR p.nickname ILIKE $2 OR p.code ILIKE $2 OR p.barcode = $4
                   OR EXISTS (SELECT 1 FROM stock_items si WHERE si.product_id = p.id AND si.barcode = $4))
             ORDER BY (sl.on_hand - sl.reserved > 0) DESC NULLS LAST, p.is_active DESC, p.name LIMIT 8",
        )
        .bind(ctx.tenant_id)
        .bind(&pattern)
        .bind(ctx.branch_id)
        .bind(&term)
        .fetch_all(&state.db)
        .await?;
        results.extend(rows.into_iter().map(|(id, name, code, price, avail)| {
            json!({ "type": "product", "id": id, "title": name, "subtitle": format!("{code} · {avail} available here"), "amount": price, "link": format!("/products/{id}") })
        }));
    }
    if ctx.can("customers.view") {
        let digits: String = term.chars().filter(|c| c.is_ascii_digit()).collect();
        let digits = (digits.len() >= 3).then(|| format!("%{}%", digits.trim_start_matches('0')));
        let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
            "SELECT id, TRIM(first_name || ' ' || other_names), nickname, mobile FROM customers
             WHERE tenant_id = $1 AND (first_name ILIKE $2 OR other_names ILIKE $2 OR nickname ILIKE $2 OR ($3::text IS NOT NULL AND mobile LIKE $3))
             ORDER BY last_purchase_at DESC NULLS LAST LIMIT 6",
        )
        .bind(ctx.tenant_id)
        .bind(&pattern)
        .bind(digits)
        .fetch_all(&state.db)
        .await?;
        results.extend(rows.into_iter().map(|(id, name, nick, mobile)| {
            let sub = if nick.is_empty() { mobile } else { format!("“{nick}” · {mobile}") };
            json!({ "type": "customer", "id": id, "title": name, "subtitle": sub, "link": format!("/customers/{id}") })
        }));
    }
    if ctx.can("orders.view") {
        let rows: Vec<(Uuid, String, String, Decimal)> = sqlx::query_as(
            "SELECT id, order_no, status, total FROM orders WHERE tenant_id = $1 AND branch_id = ANY($3) AND order_no ILIKE $2
             ORDER BY (branch_id = $4) DESC, created_at DESC LIMIT 5",
        )
        .bind(ctx.tenant_id)
        .bind(&pattern)
        .bind(&ctx.branch_ids)
        .bind(ctx.branch_id)
        .fetch_all(&state.db)
        .await?;
        results.extend(rows.into_iter().map(|(id, no, status, total)| {
            json!({ "type": "order", "id": id, "title": no, "subtitle": status.replace('_', " "), "amount": total, "link": format!("/orders/{id}") })
        }));
    }
    if ctx.can("sales.view") {
        let rows: Vec<(Uuid, String, String, Decimal)> = sqlx::query_as(
            "SELECT id, receipt_no, status, total FROM sales WHERE tenant_id = $1 AND branch_id = ANY($3) AND receipt_no ILIKE $2
             ORDER BY (branch_id = $4) DESC, created_at DESC LIMIT 5",
        )
        .bind(ctx.tenant_id)
        .bind(&pattern)
        .bind(&ctx.branch_ids)
        .bind(ctx.branch_id)
        .fetch_all(&state.db)
        .await?;
        results.extend(rows.into_iter().map(|(id, no, status, total)| {
            json!({ "type": "sale", "id": id, "title": no, "subtitle": status.replace('_', " "), "amount": total, "link": format!("/sales/{id}") })
        }));
    }
    Ok(Json(json!({ "results": results })))
}
