//! Item barcodes (roadmap 133–135): S'Shop's seven-digit item barcodes, their platform-wide registry and the labels.
//!
//! - **Seven digits, 1000000–9999999**, drawn from the operating system's cryptographically secure generator — never
//!   sequential, so a code says nothing about its neighbours.
//! - **Issued once, platform-wide**: `barcode_registry.code` is the primary key; an issued code is never issued again
//!   (sold, returned, written off, replaced, deleted — the row stays). Codes a business enters itself in the same range
//!   are registered too, so S'Shop never issues them to anyone.
//! - **Never reused for a new item**: an item barcode this business used before is refused for a new unit (the same
//!   physical unit keeps its code through transfers, returns and stock takes).
//! - The barcode belongs to the physical unit (`stock_items`), not only to its product. Product-level (shared or
//!   manufacturer) barcodes are unchanged.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::access::require_platform_admin;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::state::AppState;

/// Size of the seven-digit namespace.
pub const CAPACITY: i64 = 9_000_000;
/// Share of the namespace in use when the platform owner is warned (and the move to longer codes is due).
const WARN_PERCENT: f64 = 70.0;
/// Past this share S'Shop stops issuing seven-digit codes (random draws would mostly collide).
const STOP_PERCENT: f64 = 95.0;
/// Most labels generated at once.
pub const MAX_BATCH: i32 = 500;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/barcodes/generate", post(generate))
        .route("/barcodes/labels", post(labels))
        .route("/barcodes/replace", post(replace))
        .route("/barcodes/label-existing", post(label_existing))
        .route("/barcodes/reveal", post(reveal))
        .route("/platform/barcodes/capacity", get(capacity))
}

/// A code in S'Shop's seven-digit item namespace.
pub fn is_item_code(code: &str) -> bool {
    code.len() == 7 && code.bytes().all(|b| b.is_ascii_digit()) && !code.starts_with('0')
}

fn random_code() -> String {
    rand::rngs::OsRng.gen_range(1_000_000u32..=9_999_999).to_string()
}

/// Issues `n` new codes to this business for `product` (status *allocated*: labels can be printed, no item yet).
/// Each code is unique in the registry and unused by any item or product barcode anywhere on S'Shop.
pub async fn allocate(conn: &mut PgConnection, tenant: Uuid, product: Uuid, by: Uuid, n: i32) -> AppResult<Vec<String>> {
    let used: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM barcode_registry").fetch_one(&mut *conn).await?;
    if (used + n as i64) as f64 * 100.0 / CAPACITY as f64 > STOP_PERCENT {
        tracing::error!(used, "seven-digit item barcode namespace nearly exhausted");
        return Err(refused("Barcodes unavailable", "S'Shop cannot issue more seven-digit barcodes right now — please contact S'Shop support"));
    }
    let mut out = Vec::with_capacity(n as usize);
    let mut tries = 0;
    while out.len() < n as usize {
        tries += 1;
        if tries > n * 20 + 50 {
            return Err(rule("Could not issue enough unique barcodes — please try again"));
        }
        let code = random_code();
        let got: Option<String> = sqlx::query_scalar(
            "INSERT INTO barcode_registry (code, tenant_id, product_id, allocated_by)
             SELECT $1, $2, $3, $4
              WHERE NOT EXISTS (SELECT 1 FROM stock_items WHERE barcode = $1)
                AND NOT EXISTS (SELECT 1 FROM products WHERE barcode = $1)
                AND NOT EXISTS (SELECT 1 FROM stock_item_barcode_history WHERE barcode = $1)
             ON CONFLICT (code) DO NOTHING RETURNING code",
        )
        .bind(&code)
        .bind(tenant)
        .bind(product)
        .bind(by)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(c) = got {
            out.push(c);
        }
    }
    Ok(out)
}

/// Item barcodes about to go on new units of `product`: refused when S'Shop issued the code to another business or
/// another product, when it was already used (codes are never reused), or when this business used it on an item before.
pub async fn check_new_item_codes(conn: &mut PgConnection, tenant: Uuid, product: Uuid, codes: &[String]) -> AppResult<()> {
    if codes.is_empty() {
        return Ok(());
    }
    let reg: Vec<(String, Option<Uuid>, Option<Uuid>, String)> =
        sqlx::query_as("SELECT code, tenant_id, product_id, status FROM barcode_registry WHERE code = ANY($1)")
            .bind(codes)
            .fetch_all(&mut *conn)
            .await?;
    for (code, t, p, status) in &reg {
        if *t != Some(tenant) {
            return Err(rule(format!("Barcode {code} was issued to another business — use the labels S'Shop generated for you")));
        }
        if status != "allocated" {
            return Err(rule(format!("Barcode {code} has already been used — item barcodes are never reused")));
        }
        if *p != Some(product) {
            return Err(rule(format!("Barcode {code} was generated for another product — print this product's labels")));
        }
    }
    let used: Vec<String> = sqlx::query_scalar(
        "SELECT barcode FROM stock_items WHERE tenant_id = $1 AND barcode = ANY($2)
         UNION SELECT barcode FROM stock_item_barcode_history WHERE tenant_id = $1 AND barcode = ANY($2)",
    )
        .bind(tenant)
        .bind(codes)
        .fetch_all(&mut *conn)
        .await?;
    if !used.is_empty() {
        return Err(rule(format!("Already used on an item: {} — item barcodes are never reused", used.join(", "))));
    }
    Ok(())
}

/// How many of `codes` are S'Shop-generated codes of this business not yet on an item (labels just printed).
pub async fn generated_count(conn: &mut PgConnection, tenant: Uuid, codes: &[String]) -> AppResult<i64> {
    if codes.is_empty() {
        return Ok(0);
    }
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM barcode_registry WHERE tenant_id = $1 AND code = ANY($2) AND status = 'allocated' AND source = 'generated'")
        .bind(tenant)
        .bind(codes)
        .fetch_one(&mut *conn)
        .await?)
}

/// Records that `code` now identifies the unit `item` (after `check_new_item_codes`). S'Shop's own codes move from
/// allocated to assigned; codes the business entered in the seven-digit range are registered so they are never issued.
pub async fn assign(conn: &mut PgConnection, tenant: Uuid, product: Uuid, item: Uuid, code: &str) -> AppResult<()> {
    if !is_item_code(code) {
        return Ok(());
    }
    let n = sqlx::query(
        "INSERT INTO barcode_registry (code, tenant_id, product_id, stock_item_id, source, status, assigned_at)
         VALUES ($1, $2, $3, $4, 'entered', 'assigned', now())
         ON CONFLICT (code) DO UPDATE SET status = 'assigned', stock_item_id = EXCLUDED.stock_item_id, assigned_at = now()
          WHERE barcode_registry.tenant_id = EXCLUDED.tenant_id AND barcode_registry.product_id = EXCLUDED.product_id
            AND barcode_registry.status = 'allocated'",
    )
    .bind(code)
    .bind(tenant)
    .bind(product)
    .bind(item)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(rule(format!("Barcode {code} is no longer available — it was used or issued elsewhere")));
    }
    Ok(())
}

#[derive(Deserialize)]
struct GenerateBody {
    product_id: Uuid,
    quantity: i32,
}

/// Generates one barcode per physical unit of an individually tracked product, ready for printing.
async fn generate(State(state): State<AppState>, ctx: Ctx, Json(b): Json<GenerateBody>) -> AppResult<Json<Value>> {
    ctx.require("barcodes.generate")?;
    if !(1..=MAX_BATCH).contains(&b.quantity) {
        return Err(bad(format!("Generate between 1 and {MAX_BATCH} barcodes at a time")));
    }
    let mut tx = state.db.begin().await?;
    let s = crate::settings::load(&mut tx, ctx.tenant_id).await?;
    if s.stock.barcode_requirement == crate::settings::BarcodeRequirement::Disabled {
        return Err(rule("Barcode management is switched off (Settings → Stock)"));
    }
    if !s.stock.generate_item_barcodes {
        return Err(rule("Generating item barcodes is switched off (Settings → Stock → Barcode controls)"));
    }
    let (name, track): (String, bool) = sqlx::query_as("SELECT name, track_items FROM products WHERE id = $1 AND tenant_id = $2")
        .bind(b.product_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Product"))?;
    if !track {
        return Err(rule(format!("{name} uses one shared barcode — switch it to individual items to give each unit its own")));
    }
    let codes = allocate(&mut tx, ctx.tenant_id, b.product_id, ctx.user_id, b.quantity).await?;
    let business: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("barcodes", "generated", "product", b.product_id).after(json!({ "count": codes.len(), "product": name })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({
        "codes": codes,
        "label": { "business": business, "product": name },
    })))
}

/// Roadmap 136: an item barcode as ordinary screens show it — only the last three characters (`****047`).
pub fn mask(code: &str) -> String {
    let n = code.chars().count();
    if n <= 3 {
        return "***".into();
    }
    format!("****{}", code.chars().skip(n - 3).collect::<String>())
}

/// Masks every `barcode` / `barcodes` value inside a JSON document (approval payloads shown to people).
pub fn mask_payload(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                match (k.as_str(), &mut *x) {
                    ("barcode", Value::String(s)) => *s = mask(s),
                    ("barcodes", Value::Array(list)) => {
                        for c in list.iter_mut() {
                            if let Value::String(s) = c {
                                *s = mask(s);
                            }
                        }
                    }
                    _ => mask_payload(x),
                }
            }
        }
        Value::Array(list) => list.iter_mut().for_each(mask_payload),
        _ => {}
    }
}

async fn business_name(conn: &mut PgConnection, tenant: Uuid) -> AppResult<String> {
    Ok(sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(tenant).fetch_one(&mut *conn).await?)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LabelsBody {
    /// Codes generated for a receipt not yet completed (reprint before receiving).
    codes: Vec<String>,
    /// Particular units.
    stock_item_ids: Vec<Uuid>,
    /// Every unit of this product in stock (at `branch_id`, or every branch the person works in).
    product_id: Option<Uuid>,
    branch_id: Option<Uuid>,
}

/// Label data with the full codes — only for printing (permission *Print & reprint labels*), recorded in the audit.
async fn labels(State(state): State<AppState>, ctx: Ctx, Json(b): Json<LabelsBody>) -> AppResult<Json<Value>> {
    ctx.require("barcodes.print")?;
    let mut tx = state.db.begin().await?;
    let items: Vec<(String, String, Uuid)> = if !b.codes.is_empty() {
        sqlx::query_as(
            "SELECT r.code, p.name, p.id FROM barcode_registry r JOIN products p ON p.id = r.product_id
              WHERE r.tenant_id = $1 AND r.code = ANY($2) AND r.status = 'allocated' ORDER BY r.allocated_at, r.code",
        )
        .bind(ctx.tenant_id)
        .bind(&b.codes)
        .fetch_all(&mut *tx)
        .await?
    } else {
        if let Some(br) = b.branch_id {
            ctx.ensure_branch(br)?;
        }
        if b.stock_item_ids.is_empty() && b.product_id.is_none() {
            return Err(bad("Choose the items to print labels for"));
        }
        sqlx::query_as(
            "SELECT i.barcode, p.name, p.id FROM stock_items i JOIN products p ON p.id = i.product_id
              WHERE i.tenant_id = $1 AND i.status IN ('in_stock','in_transit') AND i.branch_id = ANY($2)
                AND (cardinality($3::uuid[]) = 0 OR i.id = ANY($3))
                AND ($4::uuid IS NULL OR i.product_id = $4) AND ($5::uuid IS NULL OR i.branch_id = $5)
              ORDER BY p.name, i.created_at LIMIT 2000",
        )
        .bind(ctx.tenant_id)
        .bind(&ctx.branch_ids)
        .bind(&b.stock_item_ids)
        .bind(b.product_id)
        .bind(b.branch_id)
        .fetch_all(&mut *tx)
        .await?
    };
    if items.is_empty() {
        return Err(rule("No labels to print — the items may have been sold, moved or not received yet"));
    }
    let business = business_name(&mut tx, ctx.tenant_id).await?;
    let mut products: Vec<Uuid> = items.iter().map(|i| i.2).collect();
    products.sort();
    products.dedup();
    let action = if b.codes.is_empty() { "labels_reprinted" } else { "labels_printed" };
    let masked: Vec<String> = items.iter().map(|i| mask(&i.0)).collect();
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("barcodes", action, "product", products[0]).after(json!({ "count": items.len(), "products": products, "barcodes": masked })),
    )
    .await?;
    tx.commit().await?;
    let labels: Vec<Value> = items.into_iter().map(|(code, product, _)| json!({ "code": code, "product": product })).collect();
    Ok(Json(json!({ "business": business, "labels": labels })))
}

#[derive(Deserialize)]
struct ReplaceBody {
    stock_item_id: Uuid,
    #[serde(default)]
    reason: String,
}

/// A damaged or lost label: the unit gets a new seven-digit code; the old one is retired, never reused.
async fn replace(State(state): State<AppState>, ctx: Ctx, Json(b): Json<ReplaceBody>) -> AppResult<Json<Value>> {
    ctx.require("barcodes.replace")?;
    let reason = b.reason.trim().to_string();
    if reason.is_empty() {
        return Err(bad("Say why the label is being replaced"));
    }
    let mut tx = state.db.begin().await?;
    let (old, product, branch, status, name): (String, Uuid, Uuid, String, String) = sqlx::query_as(
        "SELECT i.barcode, i.product_id, i.branch_id, i.status, p.name FROM stock_items i JOIN products p ON p.id = i.product_id
          WHERE i.id = $1 AND i.tenant_id = $2 FOR UPDATE OF i",
    )
    .bind(b.stock_item_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("Item"))?;
    ctx.ensure_branch(branch)?;
    if status != "in_stock" {
        return Err(rule("Only an item in stock can be relabelled"));
    }
    let code = allocate(&mut tx, ctx.tenant_id, product, ctx.user_id, 1).await?.remove(0);
    sqlx::query("INSERT INTO stock_item_barcode_history (tenant_id, stock_item_id, barcode, replaced_by, reason) VALUES ($1, $2, $3, $4, $5)")
        .bind(ctx.tenant_id)
        .bind(b.stock_item_id)
        .bind(&old)
        .bind(ctx.user_id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE stock_items SET barcode = $2, updated_at = now() WHERE id = $1").bind(b.stock_item_id).bind(&code).execute(&mut *tx).await?;
    sqlx::query("UPDATE barcode_registry SET status = 'retired', retired_at = now(), retire_reason = $3 WHERE code = $1 AND tenant_id = $2")
        .bind(&old)
        .bind(ctx.tenant_id)
        .bind(format!("Replaced: {reason}"))
        .execute(&mut *tx)
        .await?;
    assign(&mut tx, ctx.tenant_id, product, b.stock_item_id, &code).await?;
    let business = business_name(&mut tx, ctx.tenant_id).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("barcodes", "replaced", "stock_item", b.stock_item_id)
            .branch(branch)
            .before(json!({ "barcode": mask(&old) }))
            .after(json!({ "barcode": mask(&code), "reason": reason })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "code": code, "masked": mask(&code), "business": business, "product": name })))
}

#[derive(Deserialize)]
struct LabelExistingBody {
    product_id: Uuid,
    branch_id: Uuid,
}

/// A product switched to individual items with stock already on hand: one new code per unlabelled unit at the branch.
/// The stock count does not change — the units only get their identity.
async fn label_existing(State(state): State<AppState>, ctx: Ctx, Json(b): Json<LabelExistingBody>) -> AppResult<Json<Value>> {
    ctx.require("barcodes.generate")?;
    ctx.require("stock.add")?;
    ctx.ensure_branch(b.branch_id)?;
    let mut tx = state.db.begin().await?;
    let (name, track, cost): (String, bool, Option<rust_decimal::Decimal>) =
        sqlx::query_as("SELECT name, track_items, cost_price FROM products WHERE id = $1 AND tenant_id = $2")
            .bind(b.product_id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("Product"))?;
    if !track {
        return Err(rule(format!("{name} uses one shared barcode")));
    }
    let on_hand: i32 = sqlx::query_scalar("SELECT on_hand FROM stock_levels WHERE tenant_id = $1 AND branch_id = $2 AND product_id = $3 FOR UPDATE")
        .bind(ctx.tenant_id)
        .bind(b.branch_id)
        .bind(b.product_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
    let units: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_items WHERE tenant_id = $1 AND branch_id = $2 AND product_id = $3 AND status = 'in_stock'")
        .bind(ctx.tenant_id)
        .bind(b.branch_id)
        .bind(b.product_id)
        .fetch_one(&mut *tx)
        .await?;
    let missing = on_hand as i64 - units;
    if missing <= 0 {
        return Err(rule("Every unit in stock at this branch already has its own barcode"));
    }
    if missing > MAX_BATCH as i64 {
        return Err(rule(format!("{missing} units need labels — S'Shop labels up to {MAX_BATCH} at a time; run it again for the rest")));
    }
    let codes = allocate(&mut tx, ctx.tenant_id, b.product_id, ctx.user_id, missing as i32).await?;
    for code in &codes {
        let item: Uuid = sqlx::query_scalar("INSERT INTO stock_items (tenant_id, product_id, branch_id, barcode, cost_price) VALUES ($1,$2,$3,$4,$5) RETURNING id")
            .bind(ctx.tenant_id)
            .bind(b.product_id)
            .bind(b.branch_id)
            .bind(code)
            .bind(cost)
            .fetch_one(&mut *tx)
            .await?;
        assign(&mut tx, ctx.tenant_id, b.product_id, item, code).await?;
    }
    let business = business_name(&mut tx, ctx.tenant_id).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("barcodes", "existing_stock_labelled", "product", b.product_id).branch(b.branch_id).after(json!({ "count": codes.len() })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "codes": codes, "label": { "business": business, "product": name } })))
}

#[derive(Deserialize)]
struct RevealBody {
    stock_item_id: Uuid,
}

/// The full barcode of one item, for a stock investigation — permission *Reveal full item barcodes*, always audited.
async fn reveal(State(state): State<AppState>, ctx: Ctx, Json(b): Json<RevealBody>) -> AppResult<Json<Value>> {
    ctx.require("barcodes.reveal")?;
    let mut tx = state.db.begin().await?;
    let (code, branch): (String, Uuid) = sqlx::query_as("SELECT barcode, branch_id FROM stock_items WHERE id = $1 AND tenant_id = $2")
        .bind(b.stock_item_id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Item"))?;
    ctx.ensure_branch(branch)?;
    audit::record(&mut tx, &ctx, Entry::new("barcodes", "revealed", "stock_item", b.stock_item_id).branch(branch).after(json!({ "barcode": mask(&code) }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "code": code })))
}

/// Platform owner: how much of the seven-digit namespace is used.
async fn capacity(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let (used, assigned, allocated): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'assigned'), COUNT(*) FILTER (WHERE status = 'allocated') FROM barcode_registry",
    )
    .fetch_one(&state.db)
    .await?;
    let percent = used as f64 * 100.0 / CAPACITY as f64;
    Ok(Json(json!({
        "used": used, "assigned": assigned, "allocated": allocated, "capacity": CAPACITY,
        "remaining": CAPACITY - used, "percent": (percent * 100.0).round() / 100.0,
        "warning": percent >= WARN_PERCENT, "stop_percent": STOP_PERCENT,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_digit_codes() {
        for _ in 0..2000 {
            let c = random_code();
            assert!(is_item_code(&c), "{c}");
        }
        assert!(!is_item_code("0123456"), "no leading zero");
        assert!(!is_item_code("123456"), "seven digits");
        assert!(!is_item_code("12345678"));
        assert!(!is_item_code("12a4567"));
        assert!(is_item_code("1000000") && is_item_code("9999999"));
    }

    #[test]
    fn payloads_masked() {
        let mut v = json!({ "barcodes": ["5831047", "7294618"], "items": [{ "barcode": "PN-W03-0007", "barcodes": ["4068352"] }], "quantity": 2 });
        mask_payload(&mut v);
        assert_eq!(v, json!({ "barcodes": ["****047", "****618"], "items": [{ "barcode": "****007", "barcodes": ["****352"] }], "quantity": 2 }));
    }

    #[test]
    fn masked() {
        assert_eq!(mask("5831047"), "****047");
        assert_eq!(mask("ABC"), "***");
        assert_eq!(mask("PN-W03-0007"), "****007");
    }

    #[test]
    fn codes_are_not_sequential() {
        let a: Vec<u32> = (0..50).map(|_| random_code().parse().unwrap()).collect();
        let steps = a.windows(2).filter(|w| w[1] == w[0] + 1).count();
        assert!(steps < 3, "random, not sequential");
    }
}
