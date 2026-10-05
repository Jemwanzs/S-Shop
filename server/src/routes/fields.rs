//! Configurable custom fields for customers (Settings → Customer Configuration)
//! and products (Settings → Product Configuration). One implementation, two tables.

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, AppError, AppResult};
use crate::state::AppState;

#[derive(Clone, Copy)]
pub enum Kind {
    Customer,
    Product,
}

impl Kind {
    /// Settings area that manages this kind of field.
    fn settings_permission(self) -> &'static str {
        match self {
            Kind::Customer => "settings.customers",
            Kind::Product => "settings.products",
        }
    }

    fn table(self) -> &'static str {
        match self {
            Kind::Customer => "customer_fields",
            Kind::Product => "product_fields",
        }
    }
    fn entity(self) -> &'static str {
        match self {
            Kind::Customer => "customer_field",
            Kind::Product => "product_field",
        }
    }
    /// Keys of built-in fields that custom fields may not shadow.
    fn reserved(self) -> &'static [&'static str] {
        match self {
            Kind::Customer => &["mobile", "first_name", "other_names", "nickname", "email"],
            Kind::Product => &["name", "nickname", "code", "description", "barcode", "category", "supplier", "price"],
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/customer-fields",
            get(|s: State<AppState>, c: Ctx| list(s, c, Kind::Customer)).post(|s: State<AppState>, c: Ctx, b: Json<FieldBody>| create(s, c, b, Kind::Customer)),
        )
        .route("/customer-fields/{id}", put(|s: State<AppState>, c: Ctx, p: Path<Uuid>, b: Json<FieldBody>| update(s, c, p, b, Kind::Customer)))
        .route(
            "/product-fields",
            get(|s: State<AppState>, c: Ctx| list(s, c, Kind::Product)).post(|s: State<AppState>, c: Ctx, b: Json<FieldBody>| create(s, c, b, Kind::Product)),
        )
        .route("/product-fields/{id}", put(|s: State<AppState>, c: Ctx, p: Path<Uuid>, b: Json<FieldBody>| update(s, c, p, b, Kind::Product)))
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

/// Validate values against the active field definitions; unknown/inactive keys are dropped.
pub async fn clean(conn: &mut PgConnection, tenant_id: Uuid, kind: Kind, input: &Map<String, Value>) -> AppResult<Map<String, Value>> {
    let defs: Vec<FieldDef> = sqlx::query_as(&format!(
        "SELECT id, key, label, field_type, options, required, is_active, display_order FROM {} WHERE tenant_id = $1 AND is_active",
        kind.table()
    ))
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

async fn list(State(state): State<AppState>, ctx: Ctx, kind: Kind) -> AppResult<Json<Vec<FieldDef>>> {
    let rows = sqlx::query_as(&format!(
        "SELECT id, key, label, field_type, options, required, is_active, display_order FROM {}
         WHERE tenant_id = $1 ORDER BY display_order, label",
        kind.table()
    ))
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, Serialize)]
pub struct FieldBody {
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

fn validate(b: &FieldBody) -> AppResult<Vec<String>> {
    if b.label.trim().is_empty() {
        return Err(bad("Field name is required"));
    }
    if !["text", "number", "date", "dropdown", "boolean", "email"].contains(&b.field_type.as_str()) {
        return Err(bad("Unknown field type"));
    }
    let options: Vec<String> = b.options.iter().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
    if b.field_type == "dropdown" && options.is_empty() {
        return Err(bad("Add at least one dropdown option"));
    }
    Ok(options)
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(b): Json<FieldBody>, kind: Kind) -> AppResult<Json<Value>> {
    ctx.require(kind.settings_permission())?;
    let options = validate(&b)?;
    let key = crate::util::slugify(&b.label).replace('-', "_");
    if kind.reserved().contains(&key.as_str()) {
        return Err(bad("That name is reserved for a built-in field"));
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(&format!(
        "INSERT INTO {} (tenant_id, key, label, field_type, options, required, is_active, display_order)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        kind.table()
    ))
    .bind(ctx.tenant_id)
    .bind(&key)
    .bind(b.label.trim())
    .bind(&b.field_type)
    .bind(&options)
    .bind(b.required)
    .bind(b.is_active)
    .bind(b.display_order)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "create_field", kind.entity(), id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "key": key })))
}

async fn update(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<FieldBody>, kind: Kind) -> AppResult<Json<Value>> {
    ctx.require(kind.settings_permission())?;
    let options = validate(&b)?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query(&format!(
        "UPDATE {} SET label=$3, field_type=$4, options=$5, required=$6, is_active=$7, display_order=$8 WHERE id=$1 AND tenant_id=$2",
        kind.table()
    ))
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(b.label.trim())
    .bind(&b.field_type)
    .bind(&options)
    .bind(b.required)
    .bind(b.is_active)
    .bind(b.display_order)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Field"));
    }
    audit::record(&mut tx, &ctx, Entry::new("settings", "update_field", kind.entity(), id).after(&b)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
