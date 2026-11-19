//! Roadmap 176: selling prices per branch.
//!
//! A product is one record (one code, its barcodes) with a **default** selling price (`products.marked_price`). A branch
//! may have its **own price** (`product_branch_prices`); everywhere a sale, an order or stock is priced at a branch the
//! effective price is used — the branch's own price, else the default (SQL `effective_price`). Completed sales keep the
//! price they were recorded at (`sale_items.marked_price` / `unit_price`).
//!
//! Setting a branch's price needs *Set selling prices* (`products.price`) and that branch among the person's branches;
//! it never touches another branch. A change is refused when the price changed since the person saw it (no silent
//! overwrite), follows the *Product edit* approval when the business has it on, and is audited (before, after, branch,
//! person, time).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::Outcome;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, AppError, AppResult};
use crate::state::AppState;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new().route("/products/{id}/branch-prices", get(list).put(change))
}

/// A price as people read it in messages.
pub fn show(p: Option<Decimal>) -> String {
    match p {
        Some(p) => {
            let p = p.round_dp(2).normalize();
            let s = p.to_string();
            let (int, frac) = s.split_once('.').map_or((s.as_str(), None), |(a, b)| (a, Some(b)));
            let neg = int.starts_with('-');
            let digits: Vec<char> = int.trim_start_matches('-').chars().collect();
            let mut out = String::new();
            for (i, c) in digits.iter().enumerate() {
                if i > 0 && (digits.len() - i) % 3 == 0 {
                    out.push(',');
                }
                out.push(*c);
            }
            format!("{}{out}{}", if neg { "-" } else { "" }, frac.map(|f| format!(".{f:0<2}")).unwrap_or_default())
        }
        None => "no price".into(),
    }
}

/// The branch's own price (if it has one) and the product's default price.
pub async fn at(conn: &mut PgConnection, tenant: Uuid, product: Uuid, branch: Uuid) -> AppResult<(Option<Decimal>, Option<Decimal>)> {
    let row: Option<(Option<Decimal>, Option<Decimal>)> = sqlx::query_as(
        "SELECT (SELECT bp.price FROM product_branch_prices bp WHERE bp.product_id = p.id AND bp.branch_id = $3), p.marked_price
         FROM products p WHERE p.id = $2 AND p.tenant_id = $1",
    )
    .bind(tenant)
    .bind(product)
    .bind(branch)
    .fetch_optional(&mut *conn)
    .await?;
    row.ok_or(AppError::NotFound("Product"))
}

/// The selling price of a product at a branch: its own price, else the default.
pub async fn effective(conn: &mut PgConnection, tenant: Uuid, product: Uuid, branch: Uuid) -> AppResult<Option<Decimal>> {
    let (own, default) = at(conn, tenant, product, branch).await?;
    Ok(own.or(default))
}

/// The product's price at every branch the person may see (Current Branch first): its own price or the default.
pub async fn for_product(conn: &mut PgConnection, ctx: &Ctx, product: Uuid) -> AppResult<Vec<Value>> {
    let rows: Vec<(Uuid, String, Option<Decimal>, Option<Decimal>, bool, Option<chrono::DateTime<chrono::Utc>>, Option<String>)> = sqlx::query_as(
        "SELECT b.id, b.name, bp.price, p.marked_price,
                p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = b.id),
                bp.updated_at, u.name
         FROM branches b JOIN products p ON p.id = $2 AND p.tenant_id = $1
         LEFT JOIN product_branch_prices bp ON bp.product_id = p.id AND bp.branch_id = b.id
         LEFT JOIN users u ON u.id = bp.updated_by
         WHERE b.tenant_id = $1 AND b.is_active AND b.id = ANY($3)
         ORDER BY (b.id = $4) DESC, b.created_at",
    )
    .bind(ctx.tenant_id)
    .bind(product)
    .bind(&ctx.branch_ids)
    .bind(ctx.branch_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, own, default, sells, at, by)| {
            json!({
                "branch_id": id, "branch_name": name, "own_price": own, "price": own.or(default), "uses_default": own.is_none(),
                "is_current": id == ctx.branch_id, "sells_here": sells, "updated_at": at, "updated_by_name": by,
            })
        })
        .collect())
}

async fn list(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.view", "stock.view", "sales.create"])?;
    let mut conn = state.db.acquire().await?;
    let default: Option<Option<Decimal>> = sqlx::query_scalar("SELECT marked_price FROM products WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?;
    let default = default.ok_or(AppError::NotFound("Product"))?;
    Ok(Json(json!({ "default_price": default, "branches": for_product(&mut conn, &ctx, id).await? })))
}

/// One branch's price change: `price` is its new own price (None: use the default price again); `expected` is its own
/// price as the person saw it (None: it had none) — the change is refused if it no longer is.
#[derive(Deserialize, Serialize, Clone)]
pub struct Change {
    pub branch_id: Uuid,
    pub price: Option<Decimal>,
    #[serde(default)]
    pub expected: Option<Decimal>,
}

#[derive(Deserialize)]
struct ChangeBody {
    changes: Vec<Change>,
}

async fn change(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<ChangeBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("products.price")?;
    if b.changes.is_empty() {
        return Err(bad("Nothing to change"));
    }
    let mut seen: Vec<Uuid> = Vec::new();
    for c in &b.changes {
        if seen.contains(&c.branch_id) {
            return Err(bad("Each branch once"));
        }
        seen.push(c.branch_id);
        if !ctx.has_branch(c.branch_id) {
            return Err(AppError::Forbidden("You can set prices only at your own branches".into()));
        }
    }
    let mut tx = state.db.begin().await?;
    let (name, lines) = check(&mut tx, &ctx, id, &b.changes).await?;
    if workflow::needs_approval(&mut tx, &ctx, "product.edit", workflow::Gate::default()).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "product.edit",
                entity_type: "product",
                entity_id: id,
                branch_id: (b.changes.len() == 1).then(|| b.changes[0].branch_id),
                summary: format!("Selling price of {name} — {}", lines.join("; ")),
                amount: None,
                payload: json!({ "branch_prices": b.changes }),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    apply(&mut tx, &ctx, id, &b.changes, None).await?;
    let branches = for_product(&mut tx, &ctx, id).await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "product_id": id }));
    Ok(Json(Outcome::done(json!({ "branches": branches }))))
}

/// Locks the product (price changes to one product happen one at a time) and checks every change: a known active
/// branch, still the price the person saw, a valid amount. Returns the product name and a line per change.
async fn check(conn: &mut PgConnection, ctx: &Ctx, product: Uuid, changes: &[Change]) -> AppResult<(String, Vec<String>)> {
    let (name, default, max_discount): (String, Option<Decimal>, Option<Decimal>) =
        sqlx::query_as("SELECT name, marked_price, max_discount FROM products WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(product)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(AppError::NotFound("Product"))?;
    let mut lines = Vec::new();
    for c in changes {
        let branch: Option<String> = sqlx::query_scalar("SELECT name FROM branches WHERE id = $1 AND tenant_id = $2 AND is_active")
            .bind(c.branch_id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *conn)
            .await?;
        let branch = branch.ok_or_else(|| bad("Unknown or inactive branch"))?;
        let own: Option<Decimal> = sqlx::query_scalar("SELECT price FROM product_branch_prices WHERE product_id = $1 AND branch_id = $2")
            .bind(product)
            .bind(c.branch_id)
            .fetch_optional(&mut *conn)
            .await?;
        if own != c.expected {
            return Err(refused(
                "Price changed meanwhile",
                format!(
                    "The price of {name} at {branch} was changed by someone else — it is now {}. Reload to see it, then try again.",
                    show(own.or(default))
                ),
            ));
        }
        if let Some(p) = c.price {
            if p < Decimal::ZERO {
                return Err(bad("A price cannot be negative"));
            }
            if max_discount.is_some_and(|d| d > p) {
                return Err(bad(format!("The maximum discount ({}) is more than the price at {branch}", show(max_discount))));
            }
        }
        let new = c.price.or(default);
        let tail = if c.price.is_none() { " (default price)" } else { "" };
        lines.push(format!("{branch}: {} → {}{tail}", show(own.or(default)), show(new)));
    }
    Ok((name, lines))
}

/// Applies checked changes (again checked here: an approved change may arrive after someone else changed the price).
pub async fn apply(conn: &mut PgConnection, ctx: &Ctx, product: Uuid, changes: &[Change], approval_id: Option<Uuid>) -> AppResult<()> {
    check(conn, ctx, product, changes).await?;
    for c in changes {
        set(conn, ctx, product, c.branch_id, c.price, approval_id, "").await?;
    }
    Ok(())
}

/// Sets one branch's own price (None: back to the default), audited with the price before and after. The caller has
/// checked who may do it and that nobody changed the price meanwhile (the product row is locked).
pub async fn set(conn: &mut PgConnection, ctx: &Ctx, product: Uuid, branch: Uuid, price: Option<Decimal>, approval_id: Option<Uuid>, why: &str) -> AppResult<()> {
    let (own, default) = at(conn, ctx.tenant_id, product, branch).await?;
    if own == price {
        return Ok(());
    }
    match price {
        Some(p) => {
            sqlx::query(
                "INSERT INTO product_branch_prices (tenant_id, product_id, branch_id, price, updated_by) VALUES ($1,$2,$3,$4,$5)
                 ON CONFLICT (product_id, branch_id) DO UPDATE SET price = EXCLUDED.price, updated_by = EXCLUDED.updated_by, updated_at = now()",
            )
            .bind(ctx.tenant_id)
            .bind(product)
            .bind(branch)
            .bind(p)
            .bind(ctx.user_id)
            .execute(&mut *conn)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM product_branch_prices WHERE product_id = $1 AND branch_id = $2 AND tenant_id = $3")
                .bind(product)
                .bind(branch)
                .bind(ctx.tenant_id)
                .execute(&mut *conn)
                .await?;
        }
    }
    sqlx::query("UPDATE products SET updated_at = now() WHERE id = $1").bind(product).execute(&mut *conn).await?;
    let mut e = Entry::new("products", "branch_price", "product", product)
        .branch(branch)
        .before(json!({ "price": own.or(default), "own_price": own, "default_price": default }))
        .after(json!({ "price": price.or(default), "own_price": price, "default_price": default }))
        .approval(approval_id);
    if !why.is_empty() {
        e = e.comments(why);
    }
    audit::record(conn, ctx, e).await
}

/// An approved branch price change (a *Product edit* request carrying `branch_prices`).
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, product: Uuid, payload: &Value, approval_id: Uuid) -> AppResult<()> {
    let changes: Vec<Change> = serde_json::from_value(payload["branch_prices"].clone()).map_err(|_| bad("Stored price change is invalid"))?;
    apply(conn, ctx, product, &changes, Some(approval_id)).await
}

/// Before the default price changes: it is the price of every branch without its own price, so the person must be able
/// to set prices at each of those branches. Returns the first branch that would change but is not theirs.
pub async fn default_blocked_by(conn: &mut PgConnection, ctx: &Ctx, product: Uuid) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT b.name FROM branches b JOIN products p ON p.id = $2 AND p.tenant_id = $1
         WHERE b.tenant_id = $1 AND b.is_active AND NOT (b.id = ANY($3))
           AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = b.id))
           AND NOT EXISTS (SELECT 1 FROM product_branch_prices bp WHERE bp.product_id = p.id AND bp.branch_id = b.id)
         ORDER BY b.created_at LIMIT 1",
    )
    .bind(ctx.tenant_id)
    .bind(product)
    .bind(&ctx.branch_ids)
    .fetch_optional(&mut *conn)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn prices_read_naturally() {
        assert_eq!(show(Some(Decimal::from_str("100000").unwrap())), "100,000");
        assert_eq!(show(Some(Decimal::from_str("120000.50").unwrap())), "120,000.50");
        assert_eq!(show(Some(Decimal::from_str("999").unwrap())), "999");
        assert_eq!(show(Some(Decimal::from_str("1000.5").unwrap())), "1,000.50");
        assert_eq!(show(None), "no price");
    }
}
