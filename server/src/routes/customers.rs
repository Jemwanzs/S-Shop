//! Customer book, profiles and configurable customer fields.

use axum::extract::{Path, Query, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{like, Counted, Page, Paged};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, AppError, AppResult};
use crate::settings;
use crate::state::AppState;
use crate::util::normalize_mobile;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/customers", get(list).post(create))
        .route("/customers/lookup", get(lookup))
        .route("/customers/{id}", get(profile).put(update))
        .route("/customer-fields", get(list_fields).post(create_field))
        .route("/customer-fields/{id}", put(update_field))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CustomerRow {
    pub id: Uuid,
    pub mobile: String,
    pub first_name: String,
    pub other_names: String,
    pub nickname: String,
    pub email: String,
    pub custom_fields: Value,
    pub total_spend: Decimal,
    pub purchase_count: i32,
    pub last_purchase_at: Option<DateTime<Utc>>,
    pub own_points: i64,
    pub referral_points: i64,
    pub points_redeemed: i64,
    pub points_expired: i64,
    pub points_available: i64,
    pub tier: String,
    pub is_active: bool,
    pub credit_balance: Decimal,
    pub created_at: DateTime<Utc>,
}

pub const CUSTOMER_SELECT: &str = "SELECT c.id, c.mobile, c.first_name, c.other_names, c.nickname, c.email, c.custom_fields,
        c.total_spend, c.purchase_count, c.last_purchase_at, c.own_points, c.referral_points, c.points_redeemed,
        c.points_expired, c.points_available, c.tier, c.is_active,
        COALESCE((SELECT SUM(cs.original_amount - cs.amount_paid - cs.adjustments) FROM credit_sales cs
                  WHERE cs.customer_id = c.id AND cs.status IN ('outstanding','partially_paid')), 0)::numeric(14,2) AS credit_balance,
        c.created_at
    FROM customers c";

/// Hide loyalty / credit figures from users without the matching permission.
pub fn redact(ctx: &Ctx, c: &mut CustomerRow) {
    if !ctx.can("customers.view_loyalty") {
        c.own_points = 0;
        c.referral_points = 0;
        c.points_redeemed = 0;
        c.points_expired = 0;
        c.points_available = 0;
    }
    if !ctx.can("customers.view_credit") {
        c.credit_balance = Decimal::ZERO;
    }
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    tier: Option<String>,
    /// name | spend | points | recent
    sort: Option<String>,
    #[serde(default, deserialize_with = "super::de::opt_bool")]
    with_credit: Option<bool>,
    #[serde(flatten)]
    page: Page,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Paged<CustomerRow>>> {
    ctx.require("customers.view")?;
    let order = match q.sort.as_deref() {
        Some("spend") => "c.total_spend DESC",
        Some("points") => "c.points_available DESC",
        Some("recent") => "c.last_purchase_at DESC NULLS LAST",
        _ => "c.first_name, c.other_names",
    };
    let digits: String = q.q.as_deref().unwrap_or("").chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = (digits.len() >= 3).then(|| format!("%{}%", digits.trim_start_matches('0')));
    let select = CUSTOMER_SELECT.replacen("SELECT", "SELECT COUNT(*) OVER() AS total_count,", 1);
    let rows: Vec<Counted<CustomerRow>> = sqlx::query_as(&format!(
        "{select} WHERE c.tenant_id = $1
           AND ($2::text IS NULL OR c.first_name ILIKE $2 OR c.other_names ILIKE $2 OR c.nickname ILIKE $2
                OR (c.first_name || ' ' || c.other_names) ILIKE $2 OR ($3::text IS NOT NULL AND c.mobile LIKE $3))
           AND ($4::text IS NULL OR c.tier = $4)
           AND (NOT $5 OR EXISTS (SELECT 1 FROM credit_sales cs WHERE cs.customer_id = c.id AND cs.status IN ('outstanding','partially_paid')))
         ORDER BY {order} LIMIT $6 OFFSET $7"
    ))
    .bind(ctx.tenant_id)
    .bind(like(&q.q))
    .bind(digits)
    .bind(&q.tier)
    .bind(q.with_credit.unwrap_or(false))
    .bind(q.page.limit())
    .bind(q.page.offset())
    .fetch_all(&state.db)
    .await?;
    let mut page: Paged<CustomerRow> = rows.into();
    page.items.iter_mut().for_each(|c| redact(&ctx, c));
    Ok(Json(page))
}

#[derive(Deserialize)]
struct LookupQuery {
    mobile: String,
}

/// Exact mobile lookup (POS credit sale / quick identify).
async fn lookup(State(state): State<AppState>, ctx: Ctx, Query(q): Query<LookupQuery>) -> AppResult<Json<Value>> {
    ctx.require_any(&["customers.view", "sales.create"])?;
    let mobile = normalize_mobile(&q.mobile)?;
    let row: Option<CustomerRow> = sqlx::query_as(&format!("{CUSTOMER_SELECT} WHERE c.tenant_id = $1 AND c.mobile = $2"))
        .bind(ctx.tenant_id)
        .bind(&mobile)
        .fetch_optional(&state.db)
        .await?;
    Ok(Json(json!({ "mobile": mobile, "customer": row.map(|mut c| { redact(&ctx, &mut c); c }) })))
}

async fn profile(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("customers.view")?;
    let mut c: CustomerRow = sqlx::query_as(&format!("{CUSTOMER_SELECT} WHERE c.tenant_id = $1 AND c.id = $2"))
        .bind(ctx.tenant_id)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Customer"))?;
    redact(&ctx, &mut c);

    let sales: Vec<(Uuid, String, DateTime<Utc>, Decimal, String, i64, String, String)> = sqlx::query_as(
        "SELECT s.id, s.receipt_no, s.created_at, s.total, s.status, s.points_earned, s.payment_method, b.name
         FROM sales s JOIN branches b ON b.id = s.branch_id WHERE s.customer_id = $1 ORDER BY s.created_at DESC LIMIT 25",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let orders: Vec<(Uuid, String, String, Decimal, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, order_no, status, total, created_at FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT 10",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let referred_by: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT r.referrer_id, c.first_name || ' ' || c.other_names FROM referrals r JOIN customers c ON c.id = r.referrer_id
         WHERE r.referred_id = $1 AND r.is_active",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let referred: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT r.referred_id, c.first_name || ' ' || c.other_names, r.bonus_points_earned FROM referrals r
         JOIN customers c ON c.id = r.referred_id WHERE r.referrer_id = $1 AND r.is_active ORDER BY r.created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let credit: Vec<(Uuid, String, Decimal, Decimal, NaiveDate, String)> = if ctx.can("customers.view_credit") {
        sqlx::query_as(
            "SELECT cs.id, s.receipt_no, cs.original_amount - cs.adjustments, cs.amount_paid, cs.due_date, cs.status
             FROM credit_sales cs JOIN sales s ON s.id = cs.sale_id WHERE cs.customer_id = $1 ORDER BY cs.created_at DESC",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await?
    } else {
        vec![]
    };
    let mut conn = state.db.acquire().await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;

    Ok(Json(json!({
        "customer": c,
        "points_value": (Decimal::from(c.points_available) * s.loyalty.point_value).round_dp(2),
        "sales": sales.into_iter().map(|(id, no, at, total, status, pts, method, branch)| json!({
            "id": id, "receipt_no": no, "created_at": at, "total": total, "status": status, "points_earned": pts,
            "payment_method": method, "branch_name": branch,
        })).collect::<Vec<_>>(),
        "orders": orders.into_iter().map(|(id, no, status, total, at)| json!({
            "id": id, "order_no": no, "status": status, "total": total, "created_at": at,
        })).collect::<Vec<_>>(),
        "referred_by": referred_by.map(|(id, name)| json!({ "id": id, "name": name.trim() })),
        "referrals": referred.into_iter().map(|(id, name, pts)| json!({ "id": id, "name": name.trim(), "bonus_points": pts })).collect::<Vec<_>>(),
        "credit": credit.into_iter().map(|(id, no, amount, paid, due, status)| json!({
            "id": id, "receipt_no": no, "amount": amount, "paid": paid, "balance": amount - paid, "due_date": due, "status": status,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct CustomerBody {
    pub mobile: String,
    pub first_name: String,
    #[serde(default)]
    pub other_names: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub custom_fields: Map<String, Value>,
    pub is_active: Option<bool>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct FieldDef {
    pub id: Uuid,
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub options: Vec<String>,
    pub required: bool,
    pub is_active: bool,
    pub display_order: i32,
}

/// Validate configured custom fields; unknown/inactive keys are dropped.
async fn clean_custom_fields(conn: &mut PgConnection, tenant_id: Uuid, input: &Map<String, Value>) -> AppResult<Map<String, Value>> {
    let defs: Vec<FieldDef> = sqlx::query_as(
        "SELECT id, key, label, field_type, options, required, is_active, display_order FROM customer_fields WHERE tenant_id = $1 AND is_active",
    )
    .bind(tenant_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Map::new();
    for d in defs {
        let v = input.get(&d.key).cloned().unwrap_or(Value::Null);
        let empty = v.is_null() || v.as_str().map(|s| s.trim().is_empty()).unwrap_or(false);
        if empty {
            if d.required {
                return Err(bad(format!("{} is required", d.label)));
            }
            continue;
        }
        let ok = match d.field_type.as_str() {
            "number" => v.is_number() || v.as_str().map(|s| s.trim().parse::<f64>().is_ok()).unwrap_or(false),
            "boolean" => v.is_boolean(),
            "date" => v.as_str().map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()).unwrap_or(false),
            "email" => v.as_str().map(|s| s.contains('@')).unwrap_or(false),
            "dropdown" => v.as_str().map(|s| d.options.iter().any(|o| o == s)).unwrap_or(false),
            _ => v.is_string(),
        };
        if !ok {
            return Err(bad(format!("{} has an invalid value", d.label)));
        }
        out.insert(d.key, v);
    }
    Ok(out)
}

/// Create or reuse a customer by mobile (used by POS, orders and the portal).
pub async fn upsert_by_mobile(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
    mobile: &str,
    first_name: &str,
    nickname: &str,
) -> AppResult<(Uuid, bool)> {
    let mobile = normalize_mobile(mobile)?;
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM customers WHERE tenant_id = $1 AND mobile = $2")
        .bind(tenant_id)
        .bind(&mobile)
        .fetch_optional(&mut *conn)
        .await?
    {
        return Ok((id, false));
    }
    if first_name.trim().is_empty() {
        return Err(bad("First name is required for a new customer"));
    }
    let s = settings::load(conn, tenant_id).await?;
    let id = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, mobile, first_name, nickname, tier, created_by) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&mobile)
    .bind(first_name.trim())
    .bind(nickname.trim())
    .bind(s.tier_for(Decimal::ZERO))
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok((id, true))
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CustomerBody>) -> AppResult<Json<Value>> {
    ctx.require("customers.create")?;
    let mobile = normalize_mobile(&b.mobile)?;
    if b.first_name.trim().is_empty() {
        return Err(bad("First name is required"));
    }
    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    if s.customers.require_email && !b.email.contains('@') {
        return Err(bad("A valid email is required"));
    }
    let custom = clean_custom_fields(&mut tx, ctx.tenant_id, &b.custom_fields).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, mobile, first_name, other_names, nickname, email, custom_fields, tier, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&mobile)
    .bind(b.first_name.trim())
    .bind(b.other_names.trim())
    .bind(b.nickname.trim())
    .bind(b.email.trim().to_lowercase())
    .bind(Value::Object(custom))
    .bind(s.tier_for(Decimal::ZERO))
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("customers", "create", "customer", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "mobile": mobile })))
}

async fn update(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<CustomerBody>) -> AppResult<Json<Value>> {
    ctx.require("customers.edit")?;
    let mobile = normalize_mobile(&b.mobile)?;
    if b.first_name.trim().is_empty() {
        return Err(bad("First name is required"));
    }
    let mut tx = state.db.begin().await?;
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM customers c WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Customer"))?;
    let custom = clean_custom_fields(&mut tx, ctx.tenant_id, &b.custom_fields).await?;
    sqlx::query(
        "UPDATE customers SET mobile=$3, first_name=$4, other_names=$5, nickname=$6, email=$7, custom_fields=$8,
                is_active=COALESCE($9, is_active), updated_at=now()
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(&mobile)
    .bind(b.first_name.trim())
    .bind(b.other_names.trim())
    .bind(b.nickname.trim())
    .bind(b.email.trim().to_lowercase())
    .bind(Value::Object(custom))
    .bind(b.is_active)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("customers", "update", "customer", id).before(before).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Custom fields ─────────────────────────────

async fn list_fields(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Vec<FieldDef>>> {
    let rows = sqlx::query_as(
        "SELECT id, key, label, field_type, options, required, is_active, display_order FROM customer_fields
         WHERE tenant_id = $1 ORDER BY display_order, label",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, Serialize)]
struct FieldBody {
    label: String,
    field_type: String,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    required: bool,
    #[serde(default = "yes")]
    is_active: bool,
    #[serde(default)]
    display_order: i32,
}

fn yes() -> bool {
    true
}

fn validate_field(b: &FieldBody) -> AppResult<()> {
    if b.label.trim().is_empty() {
        return Err(bad("Field name is required"));
    }
    if !["text", "number", "date", "dropdown", "boolean", "email"].contains(&b.field_type.as_str()) {
        return Err(bad("Unknown field type"));
    }
    if b.field_type == "dropdown" && b.options.iter().all(|o| o.trim().is_empty()) {
        return Err(bad("Add at least one dropdown option"));
    }
    Ok(())
}

async fn create_field(State(state): State<AppState>, ctx: Ctx, Json(b): Json<FieldBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.manage")?;
    validate_field(&b)?;
    let key = crate::util::slugify(&b.label).replace('-', "_");
    if ["mobile", "first_name", "other_names", "nickname", "email"].contains(&key.as_str()) {
        return Err(bad("That name is reserved for a built-in field"));
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO customer_fields (tenant_id, key, label, field_type, options, required, is_active, display_order)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&key)
    .bind(b.label.trim())
    .bind(&b.field_type)
    .bind(b.options.iter().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect::<Vec<_>>())
    .bind(b.required)
    .bind(b.is_active)
    .bind(b.display_order)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "create_customer_field", "customer_field", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "key": key })))
}

async fn update_field(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<FieldBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.manage")?;
    validate_field(&b)?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query(
        "UPDATE customer_fields SET label=$3, field_type=$4, options=$5, required=$6, is_active=$7, display_order=$8
         WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(b.label.trim())
    .bind(&b.field_type)
    .bind(b.options.iter().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect::<Vec<_>>())
    .bind(b.required)
    .bind(b.is_active)
    .bind(b.display_order)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Field"));
    }
    audit::record(&mut tx, &ctx, Entry::new("settings", "update_customer_field", "customer_field", id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
