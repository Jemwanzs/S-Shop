//! Products (master catalogue), categories, suppliers, product photos and barcode lookup.

use axum::body::Bytes;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Counted, Outcome, Page, Paged};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
use crate::routes::approvals::ApprovalRow;
use crate::settings;
use crate::state::AppState;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/products", get(list).post(create))
        .route("/products/lookup", get(lookup))
        .route("/products/{id}", get(detail).put(update))
        .route("/products/{id}/status", post(set_status))
        .route("/products/{id}/photos", post(upload_photo))
        .route("/products/{id}/photos/{photo_id}", axum::routing::delete(delete_photo))
        .route("/products/{id}/photos/{photo_id}/primary", post(set_primary))
        .route("/photos/{id}", get(photo))
        .route("/categories", get(list_categories).post(create_category))
        .route("/categories/{id}", put(update_category))
        .route("/suppliers", get(list_suppliers).post(create_supplier))
        .route("/suppliers/{id}", put(update_supplier))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ProductRow {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub nickname: String,
    pub description: String,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub supplier_id: Option<Uuid>,
    pub supplier_name: Option<String>,
    pub marked_price: Decimal,
    pub max_discount: Option<Decimal>,
    pub cost_price: Option<Decimal>,
    pub barcode: Option<String>,
    pub track_items: bool,
    pub is_active: bool,
    pub available_for_orders: bool,
    pub transfer_allowed: bool,
    pub loyalty_eligible: bool,
    pub loyalty_threshold: Option<Decimal>,
    pub loyalty_points_per: Option<i32>,
    pub low_stock_threshold: Option<i32>,
    pub all_branches: bool,
    pub custom_fields: Value,
    pub on_hand: i32,
    pub reserved: i32,
    pub available: i32,
    pub primary_photo_id: Option<Uuid>,
    pub photo_count: i64,
    pub updated_at: DateTime<Utc>,
}

/// Shared SELECT for product rows with stock for branch `$2`.
pub const PRODUCT_SELECT: &str = "
    SELECT p.id, p.code, p.name, p.nickname, p.description, p.category_id, c.name AS category_name,
           p.supplier_id, s.name AS supplier_name, p.marked_price, p.max_discount, p.cost_price, p.barcode,
           p.track_items, p.is_active, p.available_for_orders, p.transfer_allowed, p.loyalty_eligible,
           p.loyalty_threshold, p.loyalty_points_per, p.low_stock_threshold, p.all_branches, p.custom_fields,
           COALESCE(sl.on_hand, 0) AS on_hand, COALESCE(sl.reserved, 0) AS reserved,
           COALESCE(sl.on_hand, 0) - COALESCE(sl.reserved, 0) AS available,
           (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1) AS primary_photo_id,
           (SELECT COUNT(*) FROM product_photos ph WHERE ph.product_id = p.id) AS photo_count,
           p.updated_at
    FROM products p
    LEFT JOIN categories c ON c.id = p.category_id
    LEFT JOIN suppliers s ON s.id = p.supplier_id
    LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $2";

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    category_id: Option<Uuid>,
    status: Option<String>,
    branch_id: Option<Uuid>,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Paged<ProductRow>>> {
    ctx.require_any(&["products.view", "stock.view", "sales.create"])?;
    let branch = ctx.branch_or_current(q.branch_id)?;
    let select = PRODUCT_SELECT.replacen("SELECT", "SELECT COUNT(*) OVER() AS total_count,", 1);
    let rows: Vec<Counted<ProductRow>> = sqlx::query_as(&format!(
        "{select} WHERE p.tenant_id = $1
           AND ($3::text IS NULL OR p.name ILIKE $3 OR p.nickname ILIKE $3 OR p.code ILIKE $3 OR p.barcode ILIKE $3)
           AND ($4::uuid IS NULL OR p.category_id = $4)
           AND ($5::text IS NULL OR $5 = 'all' OR ($5 = 'active' AND p.is_active) OR ($5 = 'inactive' AND NOT p.is_active))
         ORDER BY p.is_active DESC, p.name LIMIT $6 OFFSET $7"
    ))
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(like(&q.q))
    .bind(q.category_id)
    .bind(&q.status)
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into()))
}

async fn detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.view", "stock.view", "sales.create"])?;
    let product: ProductRow = sqlx::query_as(&format!("{PRODUCT_SELECT} WHERE p.tenant_id = $1 AND p.id = $3"))
        .bind(ctx.tenant_id)
        .bind(ctx.branch_id)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
    let photos: Vec<(Uuid, bool)> =
        sqlx::query_as("SELECT id, is_primary FROM product_photos WHERE product_id = $1 ORDER BY is_primary DESC, sort_order")
            .bind(id)
            .fetch_all(&state.db)
            .await?;
    let branch_ids: Vec<Uuid> = sqlx::query_scalar("SELECT branch_id FROM product_branches WHERE product_id = $1")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let stock: Vec<(Uuid, String, i32, i32)> = sqlx::query_as(
        "SELECT b.id, b.name, COALESCE(sl.on_hand, 0), COALESCE(sl.reserved, 0)
         FROM branches b LEFT JOIN stock_levels sl ON sl.branch_id = b.id AND sl.product_id = $2
         WHERE b.tenant_id = $1 AND b.is_active AND b.id = ANY($3) ORDER BY b.created_at",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(&ctx.branch_ids)
    .fetch_all(&state.db)
    .await?;
    let pending: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approvals WHERE entity_type = 'product' AND entity_id = $1 AND status = 'pending' LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(json!({
        "product": product,
        "photos": photos.into_iter().map(|(id, p)| json!({ "id": id, "is_primary": p, "url": format!("/api/photos/{id}") })).collect::<Vec<_>>(),
        "branch_ids": branch_ids,
        "stock_by_branch": stock.into_iter().map(|(bid, name, on_hand, reserved)| json!({
            "branch_id": bid, "branch_name": name, "on_hand": on_hand, "reserved": reserved, "available": on_hand - reserved,
            "is_current": bid == ctx.branch_id,
        })).collect::<Vec<_>>(),
        "pending_approval_id": pending,
    })))
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ProductBody {
    pub code: Option<String>,
    pub name: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub description: String,
    pub category_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub marked_price: Decimal,
    pub max_discount: Option<Decimal>,
    pub cost_price: Option<Decimal>,
    pub barcode: Option<String>,
    #[serde(default)]
    pub track_items: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
    #[serde(default = "yes")]
    pub available_for_orders: bool,
    #[serde(default = "yes")]
    pub transfer_allowed: bool,
    #[serde(default = "yes")]
    pub loyalty_eligible: bool,
    pub loyalty_threshold: Option<Decimal>,
    pub loyalty_points_per: Option<i32>,
    pub low_stock_threshold: Option<i32>,
    #[serde(default = "yes")]
    pub all_branches: bool,
    #[serde(default)]
    pub branch_ids: Vec<Uuid>,
    #[serde(default)]
    pub custom_fields: serde_json::Map<String, Value>,
}

fn yes() -> bool {
    true
}

async fn validate(conn: &mut PgConnection, ctx: &Ctx, b: &mut ProductBody, existing: Option<Uuid>) -> AppResult<()> {
    b.name = b.name.trim().to_string();
    if b.name.is_empty() {
        return Err(bad("Product name is required"));
    }
    b.custom_fields = super::fields::clean(conn, ctx.tenant_id, super::fields::Kind::Product, &b.custom_fields).await?;
    if b.marked_price < Decimal::ZERO {
        return Err(bad("Price cannot be negative"));
    }
    if let Some(d) = b.max_discount {
        if d > b.marked_price {
            return Err(bad("Maximum discount cannot exceed the marked price"));
        }
    }
    b.barcode = b.barcode.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(code) = &b.barcode {
        let clash: Option<String> = sqlx::query_scalar(
            "SELECT name FROM products WHERE tenant_id = $1 AND barcode = $2 AND ($3::uuid IS NULL OR id <> $3)
             UNION ALL
             SELECT p.name FROM stock_items si JOIN products p ON p.id = si.product_id
             WHERE si.tenant_id = $1 AND si.barcode = $2 AND si.status IN ('in_stock','reserved','in_transit')
               AND ($3::uuid IS NULL OR si.product_id <> $3)
             LIMIT 1",
        )
        .bind(ctx.tenant_id)
        .bind(code)
        .bind(existing)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(name) = clash {
            return Err(rule(format!("Barcode {code} already belongs to {name}")));
        }
    }
    if !b.all_branches {
        if b.branch_ids.is_empty() {
            return Err(bad("Choose at least one branch, or make the product available in all branches"));
        }
        let valid: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM branches WHERE tenant_id = $1 AND id = ANY($2)")
            .bind(ctx.tenant_id)
            .bind(&b.branch_ids)
            .fetch_one(&mut *conn)
            .await?;
        if valid != b.branch_ids.len() as i64 {
            return Err(bad("Unknown branch selected"));
        }
    }
    let s = settings::load(conn, ctx.tenant_id).await?;
    if s.stock.barcode_requirement == settings::BarcodeRequirement::Disabled {
        b.track_items = false;
    }
    let code = b.code.as_ref().map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty());
    b.code = match code {
        Some(c) => Some(c),
        None if existing.is_none() => {
            let n: i64 = sqlx::query_scalar(
                "INSERT INTO doc_counters (tenant_id, kind, year, value) VALUES ($1, 'product_code', 0, 1)
                 ON CONFLICT (tenant_id, kind, year) DO UPDATE SET value = doc_counters.value + 1 RETURNING value",
            )
            .bind(ctx.tenant_id)
            .fetch_one(&mut *conn)
            .await?;
            Some(format!("{}{n:04}", s.product.auto_code_prefix))
        }
        None => None,
    };
    Ok(())
}

async fn write_branches(conn: &mut PgConnection, product_id: Uuid, b: &ProductBody) -> AppResult<()> {
    sqlx::query("DELETE FROM product_branches WHERE product_id = $1").bind(product_id).execute(&mut *conn).await?;
    if !b.all_branches {
        for br in &b.branch_ids {
            sqlx::query("INSERT INTO product_branches (product_id, branch_id) VALUES ($1,$2)")
                .bind(product_id)
                .bind(br)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(mut b): Json<ProductBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("products.create")?;
    let mut tx = state.db.begin().await?;
    validate(&mut tx, &ctx, &mut b, None).await?;
    let gated = workflow::needs_approval(&mut tx, ctx.tenant_id, "product.create", None).await?;
    let desired_active = b.is_active;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO products (tenant_id, code, name, nickname, description, category_id, supplier_id, marked_price, max_discount,
             cost_price, barcode, track_items, is_active, available_for_orders, transfer_allowed, loyalty_eligible,
             loyalty_threshold, loyalty_points_per, low_stock_threshold, all_branches, created_by, custom_fields)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&b.code)
    .bind(&b.name)
    .bind(b.nickname.trim())
    .bind(b.description.trim())
    .bind(b.category_id)
    .bind(b.supplier_id)
    .bind(b.marked_price)
    .bind(b.max_discount)
    .bind(b.cost_price)
    .bind(&b.barcode)
    .bind(b.track_items)
    .bind(desired_active && !gated)
    .bind(b.available_for_orders)
    .bind(b.transfer_allowed)
    .bind(b.loyalty_eligible)
    .bind(b.loyalty_threshold)
    .bind(b.loyalty_points_per)
    .bind(b.low_stock_threshold)
    .bind(b.all_branches)
    .bind(ctx.user_id)
    .bind(Value::Object(b.custom_fields.clone()))
    .fetch_one(&mut *tx)
    .await?;
    write_branches(&mut tx, id, &b).await?;
    audit::record(&mut tx, &ctx, Entry::new("products", "create", "product", id).after(&b)).await?;

    if gated {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "product.create",
                entity_type: "product",
                entity_id: id,
                branch_id: None,
                summary: format!("New product: {} ({})", b.name, b.code.clone().unwrap_or_default()),
                amount: None,
                payload: json!({ "is_active": desired_active }),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, "product.create", None, approval).await;
        return Ok(Json(Outcome::pending_with(approval, json!({ "id": id, "code": b.code }))));
    }
    tx.commit().await?;
    Ok(Json(Outcome::done(json!({ "id": id, "code": b.code }))))
}

async fn update(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(mut b): Json<ProductBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("products.edit")?;
    let mut tx = state.db.begin().await?;
    let (current_active, current_code): (bool, String) = sqlx::query_as("SELECT is_active, code FROM products WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
    validate(&mut tx, &ctx, &mut b, Some(id)).await?;
    if b.code.is_none() {
        b.code = Some(current_code);
    }
    // Activation changes go through /status (their own permission and workflow).
    b.is_active = current_active;

    if workflow::needs_approval(&mut tx, ctx.tenant_id, "product.edit", None).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "product.edit",
                entity_type: "product",
                entity_id: id,
                branch_id: None,
                summary: format!("Edit product: {}", b.name),
                amount: None,
                payload: serde_json::to_value(&b).unwrap_or_default(),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, "product.edit", None, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    apply_update(&mut tx, &ctx, id, &b, None).await?;
    tx.commit().await?;
    Ok(Json(Outcome::done(json!({ "id": id }))))
}

async fn apply_update(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, b: &ProductBody, approval_id: Option<Uuid>) -> AppResult<()> {
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(p) FROM products p WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query(
        "UPDATE products SET code=$3, name=$4, nickname=$5, description=$6, category_id=$7, supplier_id=$8, marked_price=$9,
             max_discount=$10, cost_price=$11, barcode=$12, track_items=$13, available_for_orders=$14, transfer_allowed=$15,
             loyalty_eligible=$16, loyalty_threshold=$17, loyalty_points_per=$18, low_stock_threshold=$19, all_branches=$20, custom_fields=$21,
             updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(&b.code)
    .bind(&b.name)
    .bind(b.nickname.trim())
    .bind(b.description.trim())
    .bind(b.category_id)
    .bind(b.supplier_id)
    .bind(b.marked_price)
    .bind(b.max_discount)
    .bind(b.cost_price)
    .bind(&b.barcode)
    .bind(b.track_items)
    .bind(b.available_for_orders)
    .bind(b.transfer_allowed)
    .bind(b.loyalty_eligible)
    .bind(b.loyalty_threshold)
    .bind(b.loyalty_points_per)
    .bind(b.low_stock_threshold)
    .bind(b.all_branches)
    .bind(Value::Object(b.custom_fields.clone()))
    .execute(&mut *conn)
    .await?;
    write_branches(conn, id, b).await?;
    audit::record(conn, ctx, Entry::new("products", "update", "product", id).before(before).after(b).approval(approval_id)).await
}

#[derive(Deserialize)]
struct StatusBody {
    is_active: bool,
}

async fn set_status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<StatusBody>) -> AppResult<Json<Outcome<Value>>> {
    ctx.require("products.deactivate")?;
    let mut tx = state.db.begin().await?;
    let name: String = sqlx::query_scalar("SELECT name FROM products WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
    if !b.is_active && workflow::needs_approval(&mut tx, ctx.tenant_id, "product.deactivate", None).await? {
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "product.deactivate",
                entity_type: "product",
                entity_id: id,
                branch_id: None,
                summary: format!("Deactivate product: {name}"),
                amount: None,
                payload: json!({ "is_active": false }),
            },
        )
        .await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, "product.deactivate", None, approval).await;
        return Ok(Json(Outcome::pending(approval)));
    }
    apply_status(&mut tx, &ctx, id, b.is_active, None).await?;
    tx.commit().await?;
    if !b.is_active {
        crate::notify::to_permission(
            &state,
            ctx.tenant_id,
            None,
            "products.view",
            crate::notify::Note::new("product_deactivated", "Product deactivated", format!("{name} is no longer available for sale"), format!("/products/{id}")),
        )
        .await;
    }
    Ok(Json(Outcome::done(json!({ "id": id, "is_active": b.is_active }))))
}

async fn apply_status(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, active: bool, approval_id: Option<Uuid>) -> AppResult<()> {
    sqlx::query("UPDATE products SET is_active=$3, updated_at=now() WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(active)
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        ctx,
        Entry::new("products", if active { "activate" } else { "deactivate" }, "product", id).approval(approval_id),
    )
    .await
}

/// Executes an approved product workflow request.
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    match a.action.as_str() {
        "product.create" | "product.deactivate" => {
            let active = a.payload["is_active"].as_bool().unwrap_or(true);
            apply_status(conn, ctx, a.entity_id, active, Some(a.id)).await
        }
        "product.edit" => {
            let b: ProductBody = serde_json::from_value(a.payload.clone()).map_err(|_| bad("Stored product change is invalid"))?;
            apply_update(conn, ctx, a.entity_id, &b, Some(a.id)).await
        }
        _ => Ok(()),
    }
}

// ───────────────────────────── Photos ─────────────────────────────

async fn upload_photo(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, mut mp: Multipart) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.create", "products.edit"])?;
    let mut tx = state.db.begin().await?;
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM products WHERE id=$1 AND tenant_id=$2 FOR UPDATE")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    exists.ok_or(AppError::NotFound("Product"))?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_photos WHERE product_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= s.product.max_photos as i64 {
        return Err(rule(format!("A product can have at most {} photos", s.product.max_photos)));
    }
    let field = mp.next_field().await.map_err(|e| bad(e.to_string()))?.ok_or_else(|| bad("No photo uploaded"))?;
    let mime = field.content_type().unwrap_or("").to_string();
    if !["image/webp", "image/jpeg", "image/png"].contains(&mime.as_str()) {
        return Err(bad("Photos must be WebP, JPEG or PNG"));
    }
    let data: Bytes = field.bytes().await.map_err(|e| bad(e.to_string()))?;
    if data.len() > 3 * 1024 * 1024 {
        return Err(bad("Photo must be under 3 MB"));
    }
    let photo_id: Uuid = sqlx::query_scalar(
        "INSERT INTO product_photos (tenant_id, product_id, data, mime, is_primary, sort_order) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(data.to_vec())
    .bind(&mime)
    .bind(count == 0)
    .bind(count as i32)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE products SET updated_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("products", "add_photo", "product", id).comments("photo added")).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": photo_id, "url": format!("/api/photos/{photo_id}") })))
}

async fn delete_photo(State(state): State<AppState>, ctx: Ctx, Path((id, photo_id)): Path<(Uuid, Uuid)>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.create", "products.edit"])?;
    let mut tx = state.db.begin().await?;
    let was_primary: Option<bool> =
        sqlx::query_scalar("DELETE FROM product_photos WHERE id=$1 AND product_id=$2 AND tenant_id=$3 RETURNING is_primary")
            .bind(photo_id)
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?;
    if was_primary.ok_or(AppError::NotFound("Photo"))? {
        sqlx::query(
            "UPDATE product_photos SET is_primary = true WHERE id = (SELECT id FROM product_photos WHERE product_id = $1 ORDER BY sort_order LIMIT 1)",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(&mut tx, &ctx, Entry::new("products", "remove_photo", "product", id)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn set_primary(State(state): State<AppState>, ctx: Ctx, Path((id, photo_id)): Path<(Uuid, Uuid)>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.create", "products.edit"])?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE product_photos SET is_primary = false WHERE product_id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query("UPDATE product_photos SET is_primary = true WHERE id = $1 AND product_id = $2")
        .bind(photo_id)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Photo"));
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Public: photo ids are unguessable and photos are shown on the ordering portal.
async fn photo(State(state): State<AppState>, Path(id): Path<Uuid>) -> AppResult<impl IntoResponse> {
    let (data, mime): (Vec<u8>, String) = sqlx::query_as("SELECT data, mime FROM product_photos WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Photo"))?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "public, max-age=31536000, immutable".into())], data))
}

// ───────────────────────────── Barcode lookup ─────────────────────────────

#[derive(Deserialize)]
struct LookupQuery {
    code: String,
    branch_id: Option<Uuid>,
}

/// Resolve a scanned code: an individual stock item, a product barcode, or a product code.
async fn lookup(State(state): State<AppState>, ctx: Ctx, Query(q): Query<LookupQuery>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.view", "stock.view", "sales.create"])?;
    let code = q.code.trim();
    if code.is_empty() {
        return Err(bad("Scan or type a code"));
    }
    let branch = ctx.branch_or_current(q.branch_id)?;

    let item: Option<(Uuid, Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT si.id, si.product_id, si.branch_id, b.name, si.status FROM stock_items si JOIN branches b ON b.id = si.branch_id
         WHERE si.tenant_id = $1 AND si.barcode = $2
         ORDER BY (si.status IN ('in_stock','reserved','in_transit')) DESC, si.updated_at DESC LIMIT 1",
    )
    .bind(ctx.tenant_id)
    .bind(code)
    .fetch_optional(&state.db)
    .await?;

    let product_id: Option<Uuid> = match &item {
        Some((_, pid, ..)) => Some(*pid),
        None => sqlx::query_scalar(
            "SELECT id FROM products WHERE tenant_id = $1 AND (barcode = $2 OR upper(code) = upper($2)) ORDER BY is_active DESC LIMIT 1",
        )
        .bind(ctx.tenant_id)
        .bind(code)
        .fetch_optional(&state.db)
        .await?,
    };
    let Some(product_id) = product_id else {
        return Err(AppError::NotFound("Product for this barcode"));
    };
    let product: ProductRow = sqlx::query_as(&format!("{PRODUCT_SELECT} WHERE p.tenant_id = $1 AND p.id = $3"))
        .bind(ctx.tenant_id)
        .bind(branch)
        .bind(product_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({
        "product": product,
        "stock_item": item.map(|(id, _, bid, bname, status)| json!({
            "id": id, "barcode": code, "branch_id": bid, "branch_name": bname, "status": status,
            "in_current_branch": bid == branch,
        })),
    })))
}

// ───────────────────────────── Categories & suppliers ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct Category {
    id: Uuid,
    name: String,
    is_active: bool,
    product_count: i64,
}

async fn list_categories(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Category>>> {
    let rows = sqlx::query_as(
        "SELECT c.id, c.name, c.is_active, (SELECT COUNT(*) FROM products p WHERE p.category_id = c.id) AS product_count
         FROM categories c WHERE c.tenant_id = $1 ORDER BY c.name",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct NamedBody {
    name: String,
    is_active: Option<bool>,
}

async fn create_category(State(state): State<AppState>, ctx: Ctx, Json(b): Json<NamedBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.create", "settings.manage"])?;
    if b.name.trim().is_empty() {
        return Err(bad("Category name is required"));
    }
    let id: Uuid = sqlx::query_scalar("INSERT INTO categories (tenant_id, name) VALUES ($1, $2) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_category(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<NamedBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.edit", "settings.manage"])?;
    sqlx::query("UPDATE categories SET name = $3, is_active = COALESCE($4, is_active) WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.is_active)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, sqlx::FromRow)]
struct Supplier {
    id: Uuid,
    name: String,
    phone: String,
    email: String,
    notes: String,
    is_active: bool,
}

async fn list_suppliers(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<Supplier>>> {
    let rows = sqlx::query_as("SELECT id, name, phone, email, notes, is_active FROM suppliers WHERE tenant_id = $1 ORDER BY name")
        .bind(ctx.tenant_id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct SupplierBody {
    name: String,
    phone: Option<String>,
    email: Option<String>,
    notes: Option<String>,
    is_active: Option<bool>,
}

async fn create_supplier(State(state): State<AppState>, ctx: Ctx, Json(b): Json<SupplierBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.create", "stock.add", "settings.manage"])?;
    if b.name.trim().is_empty() {
        return Err(bad("Supplier name is required"));
    }
    let id: Uuid = sqlx::query_scalar("INSERT INTO suppliers (tenant_id, name, phone, email, notes) VALUES ($1,$2,$3,$4,$5) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.phone.as_deref().unwrap_or("").trim())
        .bind(b.email.as_deref().unwrap_or("").trim())
        .bind(b.notes.as_deref().unwrap_or("").trim())
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_supplier(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SupplierBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["products.edit", "settings.manage"])?;
    sqlx::query(
        "UPDATE suppliers SET name=$3, phone=$4, email=$5, notes=$6, is_active=COALESCE($7, is_active) WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(b.name.trim())
    .bind(b.phone.as_deref().unwrap_or("").trim())
    .bind(b.email.as_deref().unwrap_or("").trim())
    .bind(b.notes.as_deref().unwrap_or("").trim())
    .bind(b.is_active)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}
