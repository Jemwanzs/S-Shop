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
    Router::new()
        .route("/products/{id}/branch-prices", get(list).put(change))
        // Roadmap 186: Settings → Products → Manage branch prices (Tenant Administrators).
        .route("/branch-prices", get(branch_list).post(branch_change))
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
            // A reset to zero is allowed whatever the maximum discount (nothing is sold at zero — see sales).
            if p > Decimal::ZERO && max_discount.is_some_and(|d| d > p) {
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

// ── Roadmap 186: Manage branch prices ──────────────────────────────────────────────────────────────────────────────
//
// One branch at a time: every product sold there with its stock, default price, the branch's own price and the
// effective price. Status: `default` (no own price — the default applies), `branch` (its own price), `reset` (its own
// price is 0 — shown as 0, never the default, never sold until a price is set), `no_price` (no price at all). Changes —
// one product, the selected ones, a category or the whole branch — are explicit lists with the own price each was seen
// at, applied in one transaction (refused whole if any changed meanwhile), through the *Product edit* approval when the
// business has it on, and audited per product plus one summary.

/// Only the business's own Tenant Administrators, fully signed in, never S'Shop support.
fn may_manage(ctx: &Ctx) -> AppResult<()> {
    ctx.require_full()?;
    if !ctx.is_admin() || ctx.support.is_some() || ctx.acting_from.is_some() {
        return Err(AppError::Forbidden("Only the business's Tenant Administrators manage branch prices".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
struct BranchQuery {
    branch_id: Uuid,
}

async fn branch_list(State(state): State<AppState>, ctx: Ctx, axum::extract::Query(q): axum::extract::Query<BranchQuery>) -> AppResult<Json<Value>> {
    may_manage(&ctx)?;
    let mut conn = state.db.acquire().await?;
    let branch: Option<String> = sqlx::query_scalar("SELECT name FROM branches WHERE id = $1 AND tenant_id = $2 AND is_active")
        .bind(q.branch_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *conn)
        .await?;
    let branch = branch.ok_or_else(|| bad("Unknown or inactive branch"))?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(Uuid, String, String, Option<Uuid>, Option<String>, Option<Uuid>, i32, Option<Decimal>, Option<Decimal>)> = sqlx::query_as(
        "SELECT p.id, p.name, p.code, p.category_id, c.name,
                (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1),
                COALESCE(sl.on_hand - sl.reserved, 0), p.marked_price, bp.price
         FROM products p LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $2
         LEFT JOIN product_branch_prices bp ON bp.product_id = p.id AND bp.branch_id = $2
         WHERE p.tenant_id = $1 AND p.is_active
           AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = $2))
         ORDER BY p.name LIMIT 20000",
    )
    .bind(ctx.tenant_id)
    .bind(q.branch_id)
    .fetch_all(&mut *conn)
    .await?;
    // Waiting for approval at this branch (a product's own change, or a branch-wide change).
    let pending: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM approvals WHERE tenant_id = $1 AND status = 'pending' AND action = 'product.edit'
           AND (payload ? 'branch_prices' OR payload ? 'branch_prices_bulk')",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut waiting: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let approvals: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT entity_id, payload FROM approvals WHERE tenant_id = $1 AND status = 'pending' AND action = 'product.edit' AND payload ? 'branch_prices'",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&mut *conn)
    .await?;
    for (product, payload) in approvals {
        if payload["branch_prices"].as_array().is_some_and(|a| a.iter().any(|c| c["branch_id"].as_str() == Some(&q.branch_id.to_string()))) {
            waiting.insert(product);
        }
    }
    for p in pending {
        let b = &p["branch_prices_bulk"];
        if b["branch_id"].as_str() == Some(&q.branch_id.to_string()) {
            for i in b["items"].as_array().into_iter().flatten() {
                if let Some(id) = i["product_id"].as_str().and_then(|s| Uuid::parse_str(s).ok()) {
                    waiting.insert(id);
                }
            }
        }
    }
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, code, category_id, category, photo, available, default, own)| {
            let status = match own {
                Some(p) if p == Decimal::ZERO => "reset",
                Some(_) => "branch",
                None if default.is_none_or(|d| d == Decimal::ZERO) => "no_price",
                None => "default",
            };
            json!({
                "product_id": id, "name": name, "code": code, "category_id": category_id, "category_name": category, "photo_id": photo,
                "available": available, "default_price": default, "own_price": own, "price": own.or(default), "status": status,
                "pending": waiting.contains(&id),
            })
        })
        .collect();
    Ok(Json(json!({ "branch_id": q.branch_id, "branch_name": branch, "items": items })))
}

#[derive(Deserialize, Serialize, Clone)]
pub struct BulkItem {
    pub product_id: Uuid,
    /// The branch's new own price (0: reset; None: back to the default price).
    pub price: Option<Decimal>,
    /// The branch's own price as the person saw it.
    #[serde(default)]
    pub expected: Option<Decimal>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct BulkChange {
    pub branch_id: Uuid,
    /// reset | restore | set — for the summary.
    pub action: String,
    pub items: Vec<BulkItem>,
}

pub const MAX_BULK: usize = 20000;

fn summary(branch: &str, b: &BulkChange) -> String {
    let n = b.items.len();
    let what = match b.action.as_str() {
        "reset" => "reset to 0".to_string(),
        "restore" => "back to the default price".to_string(),
        _ => "new prices".to_string(),
    };
    format!("Branch prices at {branch} — {n} product{}: {what}", if n == 1 { "" } else { "s" })
}

async fn branch_change(State(state): State<AppState>, ctx: Ctx, Json(b): Json<BulkChange>) -> AppResult<Json<Outcome<Value>>> {
    may_manage(&ctx)?;
    if !matches!(b.action.as_str(), "reset" | "restore" | "set") {
        return Err(bad("Unknown price action"));
    }
    if b.items.is_empty() {
        return Err(bad("Choose at least one product"));
    }
    if b.items.len() > MAX_BULK {
        return Err(bad("Too many products at once"));
    }
    let mut seen = std::collections::HashSet::new();
    for i in &b.items {
        if !seen.insert(i.product_id) {
            return Err(bad("Each product once"));
        }
        if b.action == "reset" && i.price != Some(Decimal::ZERO) || b.action == "restore" && i.price.is_some() {
            return Err(bad("The prices do not match the action"));
        }
    }
    let mut tx = state.db.begin().await?;
    let branch: Option<String> = sqlx::query_scalar("SELECT name FROM branches WHERE id = $1 AND tenant_id = $2 AND is_active")
        .bind(b.branch_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    let branch = branch.ok_or_else(|| bad("Unknown or inactive branch"))?;
    // Checked now (nothing changed meanwhile, valid amounts) whether it waits for approval or not.
    let mut items = b.items.clone();
    items.sort_by_key(|i| i.product_id);
    for i in &items {
        check(&mut tx, &ctx, i.product_id, &[Change { branch_id: b.branch_id, price: i.price, expected: i.expected }]).await?;
    }
    let text = summary(&branch, &b);
    if workflow::needs_approval(&mut tx, &ctx, "product.edit", workflow::Gate::branch(b.branch_id)).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "product.edit",
                entity_type: "branch",
                entity_id: b.branch_id,
                branch_id: Some(b.branch_id),
                summary: text,
                amount: None,
                payload: json!({ "branch_prices_bulk": BulkChange { items, ..b } }),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    let n = apply_bulk(&mut tx, &ctx, &BulkChange { items, ..b.clone() }, None).await?;
    tx.commit().await?;
    state.emit(ctx.tenant_id, None, "stock", json!({ "branch_id": b.branch_id }));
    Ok(Json(Outcome::done(json!({ "changed": n, "summary": text }))))
}

/// Applies a branch-wide change in one transaction (each product checked again: an approved change may arrive after
/// someone else changed a price — then nothing is applied). Audited per product and once for the whole change.
pub async fn apply_bulk(conn: &mut PgConnection, ctx: &Ctx, b: &BulkChange, approval_id: Option<Uuid>) -> AppResult<usize> {
    let mut items = b.items.clone();
    items.sort_by_key(|i| i.product_id);
    let mut changed = 0;
    let why = match b.action.as_str() {
        "reset" => "Branch price reset to 0",
        "restore" => "Branch price back to the default",
        _ => "Branch price set",
    };
    for i in &items {
        check(conn, ctx, i.product_id, &[Change { branch_id: b.branch_id, price: i.price, expected: i.expected }]).await?;
        let (own, _) = at(conn, ctx.tenant_id, i.product_id, b.branch_id).await?;
        if own != i.price {
            set(conn, ctx, i.product_id, b.branch_id, i.price, approval_id, why).await?;
            changed += 1;
        }
    }
    let branch: String = sqlx::query_scalar("SELECT name FROM branches WHERE id = $1").bind(b.branch_id).fetch_one(&mut *conn).await?;
    audit::record(
        conn,
        ctx,
        Entry::new("products", "branch_prices_bulk", "branch", b.branch_id)
            .branch(b.branch_id)
            .after(json!({ "action": b.action, "products": items.len(), "changed": changed, "summary": summary(&branch, b) }))
            .approval(approval_id),
    )
    .await?;
    Ok(changed)
}

/// An approved branch-wide change (a *Product edit* request carrying `branch_prices_bulk`).
pub async fn on_bulk_approved(conn: &mut PgConnection, ctx: &Ctx, payload: &Value, approval_id: Uuid) -> AppResult<()> {
    let b: BulkChange = serde_json::from_value(payload["branch_prices_bulk"].clone()).map_err(|_| bad("Stored price change is invalid"))?;
    apply_bulk(conn, ctx, &b, Some(approval_id)).await.map(|_| ())
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
