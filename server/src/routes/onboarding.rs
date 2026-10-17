//! Settings → Business → Onboarding (roadmap 99, phase 1): Excel uploads of products, customers, opening stock
//! balances and expenses.
//!
//! **Upload → map columns → preview & validate → (approval) → post.** A batch keeps the workbook, every row as read and
//! as understood, and what became of it. Validation reads the business's own data (branches by code, products by code
//! or barcode, customers by mobile, categories, payment methods) and is repeated inside the posting transaction, so a
//! batch posts exactly what it validated or nothing. Posting reuses the existing rules: product numbering and barcode
//! checks, customer matching by mobile, the stock ledger's opening receipt (`stock::execute_receipt`) and expenses.
//!
//! Never silently skipped: a batch with invalid rows posts only when the person explicitly asks to leave them out, and
//! only for products and customers — stock and money must be complete. The same workbook posts once per dataset;
//! opening stock never lands on top of stock already recorded for that product and branch; posted records point back
//! to their batch, and nothing sends notifications or payment requests.

use std::collections::{HashMap, HashSet};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::onboarding::{self as ob, Kind, Need};
use crate::settings;
use crate::state::AppState;
use crate::util::next_doc_no;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/onboarding", get(overview))
        .route("/onboarding/cutover", put(set_cutover))
        .route("/onboarding/{dataset}/template", get(template))
        .route("/onboarding/{dataset}/upload", post(upload).layer(DefaultBodyLimit::max(6 * 1024 * 1024)))
        .route("/onboarding/batches/{id}", get(batch))
        .route("/onboarding/batches/{id}/remap", post(remap))
        .route("/onboarding/batches/{id}/revalidate", post(revalidate))
        .route("/onboarding/batches/{id}/errors", get(error_report))
        .route("/onboarding/batches/{id}/post", post(post_batch))
        .route("/onboarding/batches/{id}/cancel", post(cancel))
        .route("/onboarding/batches/{id}/progress", get(progress))
        .route("/onboarding/batches/{id}/report", get(import_report))
        .route("/onboarding/reconciliation", get(reconciliation))
        .route("/onboarding/reconciliation/export", get(reconciliation_export))
        .route("/onboarding/complete", post(complete_onboarding))
}

fn can(ctx: &Ctx) -> AppResult<()> {
    ctx.require("settings.onboarding")
}

/// Stock and money must import complete; leaving invalid rows out is allowed only for these.
fn may_skip(dataset: &str) -> bool {
    matches!(dataset, "products" | "customers")
}

// ── Overview, cutover, templates ────────────────────────────────────────────────────────────────────────────────────

async fn overview(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    can(&ctx)?;
    fail_interrupted(&state.db, ctx.tenant_id).await?;
    let (cutover, completed_at): (Option<NaiveDate>, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as("SELECT migration_cutover, onboarding_completed_at FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let batches: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object('id', b.id, 'dataset', b.dataset, 'mode', b.mode, 'status', b.status, 'file_name', b.file_name,
                'total_rows', b.total_rows, 'valid_rows', b.valid_rows, 'warning_rows', b.warning_rows, 'invalid_rows', b.invalid_rows,
                'posted_rows', b.posted_rows, 'created_at', b.created_at, 'posted_at', b.posted_at, 'uploaded_by', u.name,
                'decided_by', d.name, 'error', b.error, 'reference', b.reference)
         FROM import_batches b LEFT JOIN users u ON u.id = b.uploaded_by LEFT JOIN users d ON d.id = b.decided_by
         WHERE b.tenant_id = $1 ORDER BY b.created_at DESC LIMIT 100",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let totals: Value = sqlx::query_scalar(
        "SELECT json_build_object(
            'products', (SELECT count(*) FROM products WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'customers', (SELECT count(*) FROM customers WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'opening_quantity', (SELECT COALESCE(sum(quantity), 0) FROM stock_movements WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'opening_value', (SELECT COALESCE(sum(quantity * COALESCE(unit_cost, 0)), 0) FROM stock_movements WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'expenses_count', (SELECT count(*) FROM expenses WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'expenses_amount', (SELECT COALESCE(sum(amount), 0) FROM expenses WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'sales_count', (SELECT count(*) FROM sales WHERE tenant_id = $1 AND import_batch_id IS NOT NULL AND payment_method <> 'credit'),
            'sales_value', (SELECT COALESCE(sum(total), 0) FROM sales WHERE tenant_id = $1 AND import_batch_id IS NOT NULL AND payment_method <> 'credit'),
            'credit_outstanding', (SELECT COALESCE(sum(original_amount - amount_paid - adjustments), 0) FROM credit_sales
                                   WHERE tenant_id = $1 AND import_batch_id IS NOT NULL AND status IN ('outstanding','partially_paid')),
            'loyalty_points', (SELECT COALESCE(sum(points), 0) FROM loyalty_ledger WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'orders_history', (SELECT count(*) FROM orders WHERE tenant_id = $1 AND import_batch_id IS NOT NULL AND is_legacy),
            'orders_outstanding', (SELECT count(*) FROM orders WHERE tenant_id = $1 AND import_batch_id IS NOT NULL AND NOT is_legacy),
            'transfers', (SELECT count(*) FROM transfers WHERE tenant_id = $1 AND import_batch_id IS NOT NULL),
            'transfers_in_transit', (SELECT count(*) FROM transfers WHERE tenant_id = $1 AND import_batch_id IS NOT NULL AND status = 'dispatched'))",
    )
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let branches: i64 = sqlx::query_scalar("SELECT count(*) FROM branches WHERE tenant_id = $1 AND is_active").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    let datasets: Vec<Value> = ob::DATASETS
        .iter()
        .map(|(k, l)| {
            let cols: Vec<Value> = ob::columns(k)
                .iter()
                .map(|c| json!({ "key": c.key, "header": c.header, "required": c.need == Need::Required, "help": c.help }))
                .collect();
            json!({ "key": k, "label": l, "columns": cols, "may_skip_invalid": may_skip(k) })
        })
        .collect();
    Ok(Json(json!({ "cutover": cutover, "completed_at": completed_at, "datasets": datasets, "batches": batches, "totals": totals, "branches": branches })))
}

#[derive(Deserialize)]
struct CutoverBody {
    date: Option<NaiveDate>,
}

/// The migration cutover: history before it, balances as of it, S'Shop after it.
async fn set_cutover(State(state): State<AppState>, ctx: Ctx, Json(b): Json<CutoverBody>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    ctx.require_full()?;
    if b.date.is_some_and(|d| d > Utc::now().with_timezone(&ctx.tz).date_naive()) {
        return Err(bad("The cutover date cannot be in the future"));
    }
    let mut tx = state.db.begin().await?;
    let before: Option<NaiveDate> = sqlx::query_scalar("SELECT migration_cutover FROM tenants WHERE id = $1 FOR UPDATE").bind(ctx.tenant_id).fetch_one(&mut *tx).await?;
    sqlx::query("UPDATE tenants SET migration_cutover = $2 WHERE id = $1").bind(ctx.tenant_id).bind(b.date).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("settings", "migration_cutover", "tenant", ctx.tenant_id).before(json!({ "cutover": before })).after(json!({ "cutover": b.date })))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn template(State(state): State<AppState>, ctx: Ctx, Path(dataset): Path<String>) -> AppResult<impl IntoResponse> {
    can(&ctx)?;
    let label = ob::label(&dataset).ok_or(AppError::NotFound("Dataset"))?;
    let mut extra: HashMap<&'static str, Vec<String>> = HashMap::new();
    let codes: Vec<String> = sqlx::query_scalar("SELECT code FROM branches WHERE tenant_id = $1 AND is_active ORDER BY code").bind(ctx.tenant_id).fetch_all(&state.db).await?;
    extra.insert("branch", codes);
    let s = settings::load(&mut *state.db.acquire().await?, ctx.tenant_id).await?;
    extra.insert("payment_method", s.sales.payment_methods.iter().filter(|m| m.enabled && m.key != "credit").map(|m| m.label.clone()).collect());
    let bytes = ob::template(&dataset, &extra)?;
    let name = format!("SShop_{}_template.xlsx", label.replace(' ', "_"));
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        bytes,
    ))
}

// ── Validation ──────────────────────────────────────────────────────────────────────────────────────────────────────

/// The business's data a batch is checked against (read once per validation).
struct Refs {
    branches: HashMap<String, Uuid>,
    categories: HashMap<String, Uuid>,
    suppliers: HashMap<String, Uuid>,
    expense_categories: HashMap<String, Uuid>,
    /// upper(code) → (id, track_items, barcode, name)
    products: HashMap<String, (Uuid, bool, Option<String>, String)>,
    /// barcode → product id
    product_barcodes: HashMap<String, Uuid>,
    item_barcodes: HashSet<String>,
    customers: HashSet<String>,
    /// product id + branch id with any stock recorded already
    stocked: HashSet<(Uuid, Uuid)>,
    /// lower(label or key) → key
    payment_methods: HashMap<String, String>,
    /// expense references already imported
    expense_refs: HashSet<String>,
    /// upper(receipt_no) of every sale (historical references must not collide)
    receipts: HashSet<String>,
    /// mobile → customer id
    customer_ids: HashMap<String, Uuid>,
    /// lower(email) → user id (salespeople)
    users: HashMap<String, Uuid>,
    /// customers whose loyalty opening points were already imported
    loyalty_imported: HashSet<Uuid>,
    loyalty_on: bool,
    order_nos: HashSet<String>,
    transfer_nos: HashSet<String>,
    /// product + branch → units available to sell (on hand − reserved)
    available: HashMap<(Uuid, Uuid), i64>,
    /// the order stage that turns an order into a sale, whether confirming reserves stock, negative stock allowed
    sale_rank: usize,
    reserve_stock: bool,
    allow_negative: bool,
    order_statuses: HashSet<String>,
    barcodes_on: bool,
    cutover: Option<NaiveDate>,
    today: NaiveDate,
}

async fn refs(conn: &mut PgConnection, ctx: &Ctx) -> AppResult<Refs> {
    let t = ctx.tenant_id;
    let pairs = |rows: Vec<(String, Uuid)>| rows.into_iter().map(|(k, v)| (k.trim().to_lowercase(), v)).collect::<HashMap<_, _>>();
    let branches = sqlx::query_as::<_, (String, Uuid)>("SELECT code, id FROM branches WHERE tenant_id = $1 AND is_active").bind(t).fetch_all(&mut *conn).await?;
    let categories = sqlx::query_as::<_, (String, Uuid)>("SELECT name, id FROM categories WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let suppliers = sqlx::query_as::<_, (String, Uuid)>("SELECT name, id FROM suppliers WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let ecats = sqlx::query_as::<_, (String, Uuid)>("SELECT name, id FROM expense_categories WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let prods: Vec<(Uuid, String, bool, Option<String>, String)> =
        sqlx::query_as("SELECT id, code, track_items, barcode, name FROM products WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let items: Vec<String> = sqlx::query_scalar("SELECT barcode FROM stock_items WHERE tenant_id = $1 AND status IN ('in_stock','reserved','in_transit')")
        .bind(t)
        .fetch_all(&mut *conn)
        .await?;
    let customer_rows: Vec<(String, Uuid)> = sqlx::query_as("SELECT mobile, id FROM customers WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let customers: Vec<String> = customer_rows.iter().map(|c| c.0.clone()).collect();
    let receipts: Vec<String> = sqlx::query_scalar("SELECT upper(receipt_no) FROM sales WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let users: Vec<(String, Uuid)> = sqlx::query_as("SELECT lower(email), id FROM users WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let loyalty_imported: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT customer_id FROM loyalty_ledger WHERE tenant_id = $1 AND import_batch_id IS NOT NULL")
        .bind(t)
        .fetch_all(&mut *conn)
        .await?;
    let stocked: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT DISTINCT product_id, branch_id FROM stock_movements WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let expense_refs: Vec<String> = sqlx::query_scalar(
        "SELECT r.data->>'reference' FROM import_rows r JOIN import_batches b ON b.id = r.batch_id
         WHERE b.tenant_id = $1 AND b.dataset = 'expenses' AND r.status = 'posted' AND r.data->>'reference' IS NOT NULL",
    )
    .bind(t)
    .fetch_all(&mut *conn)
    .await?;
    let order_nos: Vec<String> = sqlx::query_scalar("SELECT upper(order_no) FROM orders WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let transfer_nos: Vec<String> = sqlx::query_scalar("SELECT upper(transfer_no) FROM transfers WHERE tenant_id = $1").bind(t).fetch_all(&mut *conn).await?;
    let available: Vec<(Uuid, Uuid, i64)> = sqlx::query_as("SELECT product_id, branch_id, (on_hand - reserved)::bigint FROM stock_levels WHERE tenant_id = $1")
        .bind(t)
        .fetch_all(&mut *conn)
        .await?;
    let s = settings::load(conn, t).await?;
    let cutover: Option<NaiveDate> = sqlx::query_scalar("SELECT migration_cutover FROM tenants WHERE id = $1").bind(t).fetch_one(&mut *conn).await?;
    let mut payment_methods = HashMap::new();
    for m in s.sales.payment_methods.iter().filter(|m| m.enabled && m.key != "credit") {
        payment_methods.insert(m.label.trim().to_lowercase(), m.key.clone());
        payment_methods.insert(m.key.to_lowercase(), m.key.clone());
    }
    Ok(Refs {
        branches: pairs(branches),
        categories: pairs(categories),
        suppliers: pairs(suppliers),
        expense_categories: pairs(ecats),
        product_barcodes: prods.iter().filter_map(|p| p.3.clone().map(|b| (b, p.0))).collect(),
        products: prods.into_iter().map(|(id, code, track, barcode, name)| (code.to_uppercase(), (id, track, barcode, name))).collect(),
        item_barcodes: items.into_iter().collect(),
        customers: customers.into_iter().collect(),
        stocked: stocked.into_iter().collect(),
        payment_methods,
        expense_refs: expense_refs.into_iter().map(|r| r.to_uppercase()).collect(),
        receipts: receipts.into_iter().collect(),
        customer_ids: customer_rows.into_iter().collect(),
        users: users.into_iter().collect(),
        loyalty_imported: loyalty_imported.into_iter().collect(),
        loyalty_on: s.loyalty.enabled,
        order_nos: order_nos.into_iter().collect(),
        transfer_nos: transfer_nos.into_iter().collect(),
        available: available.into_iter().map(|(p, b, a)| ((p, b), a)).collect(),
        sale_rank: super::orders::FLOW.iter().position(|f| *f == s.orders.sale_on_status).unwrap_or(5),
        reserve_stock: s.orders.reserve_stock,
        allow_negative: s.stock.allow_negative,
        order_statuses: super::orders::FLOW.iter().filter(|f| s.order_status_enabled(f)).map(|f| f.to_string()).collect(),
        barcodes_on: s.stock.barcode_requirement != settings::BarcodeRequirement::Disabled,
        cutover,
        today: Utc::now().with_timezone(&ctx.tz).date_naive(),
    })
}

/// One row's problems: errors stop it, warnings are shown and allowed.
#[derive(Default)]
struct Check {
    msgs: Vec<Value>,
    errors: usize,
    warnings: usize,
}
impl Check {
    fn err(&mut self, field: &str, m: impl Into<String>) {
        self.errors += 1;
        self.msgs.push(json!({ "level": "error", "field": field, "message": m.into() }));
    }
    fn warn(&mut self, field: &str, m: impl Into<String>) {
        self.warnings += 1;
        self.msgs.push(json!({ "level": "warning", "field": field, "message": m.into() }));
    }
}

fn s<'a>(d: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    d.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty())
}

/// Generic checks from the column definitions: required values present, numbers / dates / yes-no readable.
fn basic(dataset: &str, d: &Map<String, Value>, c: &mut Check) {
    for col in ob::columns(dataset) {
        let v = d.get(col.key).unwrap_or(&Value::Null);
        if let Some(bad) = v.get("invalid").and_then(|x| x.as_str()) {
            let what = match col.kind {
                Kind::Money => "an amount",
                Kind::Int => "a whole number",
                Kind::Date => "a date (e.g. 2026-10-01)",
                Kind::YesNo => "YES or NO",
                _ => "valid",
            };
            c.err(col.key, format!("{}: “{bad}” is not {what}", col.header));
        } else if v.is_null() && col.need == Need::Required {
            c.err(col.key, format!("{} is required", col.header));
        }
    }
}

fn money(d: &Map<String, Value>, k: &str) -> Option<Decimal> {
    d.get(k).and_then(|v| v.as_f64()).and_then(|f| Decimal::try_from(f).ok()).map(|v| v.round_dp(2))
}

/// Validates every row; returns the per-row (normalised data, check). `seen` catches duplicates within the file.
fn validate(dataset: &str, mode: &str, rows: &[(u32, Map<String, Value>)], r: &Refs) -> Vec<(Map<String, Value>, Check)> {
    let mut seen: HashMap<String, u32> = HashMap::new();
    // Sales history: every line of a receipt must agree on these (first line wins).
    let mut receipts: HashMap<String, (u32, Vec<Option<String>>)> = HashMap::new();
    let mut out = Vec::with_capacity(rows.len());
    for (row_no, data) in rows {
        let mut d = data.clone();
        let mut c = Check::default();
        basic(dataset, &d, &mut c);
        let mut dup = |key: String, field: &str, what: &str, c: &mut Check| {
            if let Some(first) = seen.get(&key) {
                c.err(field, format!("{what} already on row {first} of this file"));
            } else {
                seen.insert(key, *row_no);
            }
        };
        match dataset {
            "products" => {
                if let Some(code) = s(&d, "code").map(str::to_uppercase) {
                    d.insert("code".into(), json!(code));
                    dup(format!("code:{code}"), "code", &format!("Product code {code}"), &mut c);
                    match (mode, r.products.get(&code)) {
                        ("create", Some((_, _, _, name))) => c.err("code", format!("{code} already exists ({name}) — use “Update existing products” to change it")),
                        ("update", None) => c.err("code", format!("{code} is not an existing product")),
                        _ => {}
                    }
                } else if mode == "update" {
                    c.err("code", "Product Code is required to update a product");
                }
                let target = s(&d, "code").and_then(|code| r.products.get(code)).map(|p| p.0);
                if let Some(p) = money(&d, "selling_price") {
                    if p < Decimal::ZERO {
                        c.err("selling_price", "Selling Price cannot be negative");
                    }
                    if let Some(m) = money(&d, "max_discount") {
                        if m < Decimal::ZERO || m > p {
                            c.err("max_discount", "Maximum Discount must be between 0 and the selling price");
                        }
                    }
                }
                if money(&d, "cost_price").is_some_and(|v| v < Decimal::ZERO) {
                    c.err("cost_price", "Cost Price cannot be negative");
                }
                if d.get("low_stock").and_then(|v| v.as_i64()).is_some_and(|v| v < 0) {
                    c.err("low_stock", "Low-Stock Alert cannot be negative");
                }
                let tracking = s(&d, "tracking").map(str::to_lowercase);
                let individual = match tracking.as_deref() {
                    None | Some("product barcode") | Some("product") => false,
                    Some("individual items") | Some("individual") | Some("individual item") => true,
                    Some(other) => {
                        c.err("tracking", format!("Stock Tracking “{other}”: Product barcode or Individual items"));
                        false
                    }
                };
                if individual && !r.barcodes_on {
                    c.err("tracking", "Individual items need barcode management (Settings → Stock → Barcode settings)");
                }
                d.insert("individual".into(), json!(individual));
                let required = d.get("barcode_required").and_then(|v| v.as_bool()).unwrap_or(r.barcodes_on && !individual);
                match s(&d, "barcode").map(str::to_string) {
                    Some(code) if !r.barcodes_on => {
                        c.warn("barcode", format!("Barcode management is off — {code} is not saved"));
                        d.remove("barcode");
                    }
                    Some(code) if individual => {
                        c.err("barcode", format!("{code}: individually tracked products have no product barcode (each unit gets its own when stock is added)"));
                    }
                    Some(code) => match crate::util::clean_barcode(&code) {
                        Err(e) => c.err("barcode", e.to_string()),
                        Ok(code) => {
                            dup(format!("barcode:{code}"), "barcode", &format!("Barcode {code}"), &mut c);
                            match r.product_barcodes.get(&code) {
                                Some(owner) if Some(*owner) != target => {
                                    let name = r.products.values().find(|p| p.0 == *owner).map(|p| p.3.clone()).unwrap_or_default();
                                    c.err("barcode", format!("Barcode {code} already belongs to {name}"));
                                }
                                _ => {}
                            }
                            if r.item_barcodes.contains(&code) {
                                c.err("barcode", format!("Barcode {code} is an individual item's label in stock"));
                            }
                            d.insert("barcode".into(), json!(code));
                        }
                    },
                    None if required && r.barcodes_on && !individual && mode == "create" => {
                        c.err("barcode", "Barcode Number is required (Barcode Required During Sale = YES) — or set it to NO");
                    }
                    None => {}
                }
                if let Some(cat) = s(&d, "category") {
                    if !r.categories.contains_key(&cat.to_lowercase()) {
                        c.warn("category", format!("New category “{cat}” will be created"));
                    }
                }
                if let Some(sup) = s(&d, "supplier") {
                    if !r.suppliers.contains_key(&sup.to_lowercase()) {
                        c.warn("supplier", format!("New supplier “{sup}” will be created"));
                    }
                }
                match s(&d, "status").map(str::to_lowercase).as_deref() {
                    None | Some("active") => {
                        d.insert("active".into(), json!(true));
                    }
                    Some("inactive") => {
                        d.insert("active".into(), json!(false));
                    }
                    Some(o) => c.err("status", format!("Status “{o}”: Active or Inactive")),
                }
                let branches = s(&d, "branches").unwrap_or("ALL").to_string();
                if branches.eq_ignore_ascii_case("all") {
                    d.insert("branch_ids".into(), Value::Null);
                } else {
                    let mut ids = Vec::new();
                    for code in branches.split([';', ',']).map(str::trim).filter(|x| !x.is_empty()) {
                        match r.branches.get(&code.to_lowercase()) {
                            Some(id) => ids.push(json!(id)),
                            None => c.err("branches", format!("Branch code {code} does not exist (Settings → Branches)")),
                        }
                    }
                    if ids.is_empty() && c.errors == 0 {
                        c.err("branches", "Branch Applicability: ALL or branch codes separated by ;");
                    }
                    d.insert("branch_ids".into(), Value::Array(ids));
                }
            }
            "customers" => {
                if let Some(m) = s(&d, "mobile") {
                    match crate::util::normalize_mobile(m) {
                        Err(_) => c.err("mobile", format!("{m} is not a valid Kenyan mobile number (07…, 01… or 254…)")),
                        Ok(n) => {
                            dup(format!("mobile:{n}"), "mobile", &format!("Mobile {n}"), &mut c);
                            if r.customers.contains(&n) {
                                c.warn("mobile", format!("{n} is already a customer — matched, not duplicated"));
                                d.insert("existing".into(), json!(true));
                            }
                            d.insert("mobile".into(), json!(n));
                        }
                    }
                }
                if let Some(e) = s(&d, "email") {
                    if !e.contains('@') || e.len() > 120 {
                        c.err("email", format!("{e} is not an email address"));
                    }
                }
            }
            "opening_stock" => {
                let product = s(&d, "product").map(str::to_string);
                let p = product.as_deref().and_then(|code| {
                    r.products.get(&code.to_uppercase()).cloned().or_else(|| r.product_barcodes.get(code).and_then(|id| r.products.values().find(|p| p.0 == *id).cloned()))
                });
                let b = s(&d, "branch").and_then(|code| r.branches.get(&code.to_lowercase()).copied());
                if let (Some(code), None) = (&product, &p) {
                    c.err("product", format!("No product with code or barcode {code} — import products first"));
                }
                if let (Some(code), None) = (s(&d, "branch"), b) {
                    c.err("branch", format!("Branch code {code} does not exist (Settings → Branches)"));
                }
                let qty = d.get("quantity").and_then(|v| v.as_i64());
                if qty.is_some_and(|q| q < 0) {
                    c.err("quantity", "Opening Quantity cannot be negative");
                }
                if money(&d, "unit_cost").is_some_and(|v| v < Decimal::ZERO) {
                    c.err("unit_cost", "Unit Cost cannot be negative");
                }
                if let Some(date) = s(&d, "as_of").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()) {
                    if date > r.today {
                        c.err("as_of", "As-of Date cannot be in the future");
                    } else if r.cutover.is_some_and(|cut| cut != date) {
                        c.warn("as_of", format!("As-of Date differs from the migration cutover ({})", r.cutover.unwrap_or(date)));
                    }
                }
                if let (Some(p), Some(b)) = (&p, b) {
                    d.insert("product_id".into(), json!(p.0));
                    d.insert("branch_id".into(), json!(b));
                    dup(format!("stock:{}:{b}", p.0), "product", &format!("{} at this branch", p.3), &mut c);
                    if r.stocked.contains(&(p.0, b)) {
                        c.err("product", format!("{} already has stock recorded at this branch — opening stock is not added on top; use a stock take to correct it", p.3));
                    }
                    let units: Vec<String> = s(&d, "item_barcodes").map(|x| x.split([';', ',', '\n']).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default();
                    if p.1 {
                        if units.len() as i64 != qty.unwrap_or(-1) {
                            c.err("item_barcodes", format!("{} is tracked per item: give one Item Barcode per unit ({} given for {})", p.3, units.len(), qty.unwrap_or(0)));
                        }
                        for u in &units {
                            match crate::util::clean_barcode(u) {
                                Err(e) => c.err("item_barcodes", e.to_string()),
                                Ok(u) => {
                                    dup(format!("item:{u}"), "item_barcodes", &format!("Item barcode {u}"), &mut c);
                                    if r.item_barcodes.contains(&u) || r.product_barcodes.contains_key(&u) {
                                        c.err("item_barcodes", format!("Item barcode {u} is already in use"));
                                    }
                                }
                            }
                        }
                    } else if !units.is_empty() {
                        c.err("item_barcodes", format!("{} uses one product barcode — leave Item Barcodes empty", p.3));
                    }
                    d.insert("units".into(), json!(units));
                    if qty == Some(0) {
                        c.warn("quantity", "Zero: nothing is recorded for this row");
                    }
                }
            }
            "expenses" => {
                if let Some(rf) = s(&d, "reference").map(str::to_uppercase) {
                    dup(format!("ref:{rf}"), "reference", &format!("Reference {rf}"), &mut c);
                    if r.expense_refs.contains(&rf) {
                        c.err("reference", format!("Expense {rf} was already imported"));
                    }
                    d.insert("reference".into(), json!(rf));
                }
                if let Some(a) = money(&d, "amount") {
                    if a <= Decimal::ZERO {
                        c.err("amount", "Amount must be more than 0");
                    }
                }
                if let Some(date) = s(&d, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()) {
                    if date > r.today {
                        c.err("date", "Expense Date cannot be in the future");
                    }
                }
                match s(&d, "branch").map(|code| (code, r.branches.get(&code.to_lowercase()))) {
                    Some((_, Some(id))) => {
                        d.insert("branch_id".into(), json!(id));
                    }
                    Some((code, None)) => c.err("branch", format!("Branch code {code} does not exist (Settings → Branches)")),
                    None => {}
                }
                if let Some(cat) = s(&d, "category") {
                    if !r.expense_categories.contains_key(&cat.to_lowercase()) {
                        c.warn("category", format!("New expense category “{cat}” will be created"));
                    }
                }
                let pm = s(&d, "payment_method").unwrap_or("cash").to_lowercase();
                match r.payment_methods.get(&pm) {
                    Some(key) => {
                        d.insert("method_key".into(), json!(key));
                    }
                    None => c.err("payment_method", format!("Payment method “{pm}” is not one of your payment methods (Settings → Sales & payments)")),
                }
                if s(&d, "reference").is_none() {
                    c.warn("reference", "No reference: re-importing the same expense would not be detected");
                }
            }
            "sales" | "credit" => {
                let credit = dataset == "credit";
                let ref_key = if credit { "reference" } else { "receipt" };
                let date_key = if credit { "sale_date" } else { "date" };
                let reference = s(&d, ref_key).map(str::to_uppercase);
                if let Some(rf) = &reference {
                    d.insert(ref_key.into(), json!(rf));
                    if r.receipts.contains(rf) {
                        c.err(ref_key, format!("{rf} is already a receipt in S'Shop — it was imported or recorded before"));
                    }
                    if credit {
                        dup(format!("credit:{rf}"), ref_key, &format!("Credit {rf}"), &mut c);
                    }
                }
                let date = s(&d, date_key).and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                if let Some(date) = date {
                    if date > r.today {
                        c.err(date_key, "The date cannot be in the future");
                    } else if let Some(cut) = r.cutover {
                        if date >= cut {
                            c.err(date_key, format!("On or after the migration cutover ({cut}) — record it in S'Shop instead of importing it"));
                        }
                    } else {
                        c.warn(date_key, "Set the migration cutover (above) so history and S'Shop's own records stay apart");
                    }
                }
                let branch = s(&d, "branch").map(|code| (code.to_string(), r.branches.get(&code.to_lowercase()).copied()));
                match &branch {
                    Some((_, Some(id))) => {
                        d.insert("branch_id".into(), json!(id));
                    }
                    Some((code, None)) => c.err("branch", format!("Branch code {code} does not exist (Settings → Branches)")),
                    None => {}
                }
                let customer = s(&d, "customer").map(|m| (m.to_string(), crate::util::normalize_mobile(m).ok()));
                match &customer {
                    Some((m, None)) => c.err("customer", format!("{m} is not a valid mobile number")),
                    Some((_, Some(n))) => match r.customer_ids.get(n) {
                        Some(id) => {
                            d.insert("customer_id".into(), json!(id));
                        }
                        None => c.err("customer", format!("No customer with mobile {n} — import customers first")),
                    },
                    None => {}
                }
                match s(&d, "salesperson").map(|e| (e.to_string(), r.users.get(&e.to_lowercase()))) {
                    Some((_, Some(id))) => {
                        d.insert("owner_id".into(), json!(id));
                    }
                    Some((e, None)) => c.err("salesperson", format!("{e} is not a team member's sign-in email")),
                    None => {}
                }
                let product = s(&d, "product").map(str::to_string);
                if let Some(code) = &product {
                    let p = r.products.get(&code.to_uppercase()).cloned().or_else(|| r.product_barcodes.get(code).and_then(|id| r.products.values().find(|p| p.0 == *id).cloned()));
                    match p {
                        Some(p) => {
                            d.insert("product_id".into(), json!(p.0));
                        }
                        None => c.err("product", format!("No product with code or barcode {code} — import products first")),
                    }
                }
                let qty = d.get("quantity").and_then(|v| v.as_i64());
                if qty.is_some_and(|q| q <= 0) {
                    c.err("quantity", "Quantity must be more than 0");
                }
                if credit {
                    let original = money(&d, "original_amount");
                    let paid = money(&d, "amount_paid").unwrap_or(Decimal::ZERO);
                    if original.is_some_and(|o| o <= Decimal::ZERO) {
                        c.err("original_amount", "Original Amount must be more than 0");
                    }
                    if paid < Decimal::ZERO || original.is_some_and(|o| paid > o) {
                        c.err("amount_paid", "Amount Paid must be between 0 and the Original Amount");
                    } else if original == Some(paid) {
                        c.warn("amount_paid", "Fully paid — imported as settled credit history");
                    }
                    let due = s(&d, "due_date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                    if let (Some(due), Some(date)) = (due, date) {
                        if due < date {
                            c.err("due_date", "The Due Date is before the Original Sale Date");
                        }
                    }
                    if product.is_none() {
                        if qty.is_some() {
                            c.err("quantity", "Quantity needs a Product Code");
                        }
                        c.warn("product", "No product: imported as an Opening Credit Balance");
                    }
                    if customer.is_none() {
                        c.err("customer", "Customer Mobile is required");
                    }
                } else {
                    if money(&d, "unit_price").is_some_and(|v| v < Decimal::ZERO) {
                        c.err("unit_price", "Unit Selling Price cannot be negative");
                    }
                    if money(&d, "discount").is_some_and(|v| v < Decimal::ZERO) {
                        c.err("discount", "Discount cannot be negative");
                    }
                    let pm = s(&d, "payment_method").map(str::to_lowercase);
                    match pm.as_deref().map(|m| (m, r.payment_methods.get(m))) {
                        Some((m, _)) if m == "credit" || m == "credit sale" => c.err("payment_method", "Credit sales go in the Credit sales template (so receivables are never counted twice)"),
                        Some((_, Some(key))) => {
                            d.insert("method_key".into(), json!(key));
                        }
                        Some((m, None)) => c.err("payment_method", format!("Payment method “{m}” is not one of your payment methods")),
                        None => {}
                    }
                    // One receipt = one sale: its lines must agree.
                    if let Some(rf) = &reference {
                        let sig: Vec<Option<String>> = ["date", "branch", "payment_method", "payment_reference", "customer", "salesperson"]
                            .iter()
                            .map(|k| s(&d, k).map(|v| v.to_lowercase()))
                            .collect();
                        match receipts.get(rf) {
                            Some((first, sig0)) if *sig0 != sig => {
                                c.err(ref_key, format!("Receipt {rf}: date, branch, payment and customer must match its first line (row {first})"));
                            }
                            Some(_) => {}
                            None => {
                                receipts.insert(rf.clone(), (*row_no, sig));
                            }
                        }
                    }
                }
            }
            "orders" => {
                let rf = s(&d, "order").map(str::to_uppercase);
                if let Some(rf) = &rf {
                    d.insert("order".into(), json!(rf));
                    if r.order_nos.contains(rf) {
                        c.err("order", format!("Order {rf} is already in S'Shop"));
                    }
                }
                let status = s(&d, "status").map(|x| x.trim().to_lowercase().replace(' ', "_"));
                let rank = status.as_deref().and_then(|x| super::orders::FLOW.iter().position(|f| *f == x));
                let closed = match (status.as_deref(), rank) {
                    (Some("cancelled" | "rejected" | "returned"), _) => Some(true),
                    (_, Some(rk)) => Some(rk >= r.sale_rank),
                    (Some(other), None) => {
                        c.err("status", format!("Order Status “{other}”: New, Confirmed, Preparing, Dispatched, On delivery, Delivered, Completed, Cancelled, Rejected or Returned"));
                        None
                    }
                    (None, _) => None,
                };
                if let Some(st) = &status {
                    d.insert("status".into(), json!(st));
                }
                if closed == Some(false) {
                    if let Some(st) = &status {
                        if !r.order_statuses.contains(st) {
                            c.err("status", format!("The {st} stage is switched off for this business (Settings → Orders)"));
                        }
                    }
                }
                d.insert("closed".into(), json!(closed.unwrap_or(false)));
                d.insert("reserve".into(), json!(closed == Some(false) && rank.is_some_and(|rk| rk >= 1) && r.reserve_stock));
                let date = s(&d, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                if let Some(date) = date {
                    if date > r.today {
                        c.err("date", "The Order Date cannot be in the future");
                    } else if closed == Some(true) {
                        match r.cutover {
                            Some(cut) if date >= cut => c.err("date", format!("A closed order on or after the migration cutover ({cut}) belongs in S'Shop, not in history")),
                            None => c.warn("date", "Set the migration cutover (above) so history and S'Shop's own records stay apart"),
                            _ => {}
                        }
                    }
                }
                if closed == Some(true) {
                    c.warn("status", "History: no sale is created and no stock is held (import its sale in Sales history)");
                }
                order_or_transfer_refs(&mut d, &mut c, r, true);
                if d.get("quantity").and_then(|v| v.as_i64()).is_some_and(|q| q <= 0) {
                    c.err("quantity", "Quantity must be more than 0");
                }
                if money(&d, "unit_price").is_some_and(|v| v < Decimal::ZERO) {
                    c.err("unit_price", "Unit Price cannot be negative");
                }
            }
            "transfers" => {
                let rf = s(&d, "transfer").map(str::to_uppercase);
                if let Some(rf) = &rf {
                    d.insert("transfer".into(), json!(rf));
                    if r.transfer_nos.contains(rf) {
                        c.err("transfer", format!("Transfer {rf} is already in S'Shop"));
                    }
                }
                let status = match s(&d, "status").map(|x| x.trim().to_lowercase()).as_deref() {
                    Some("received" | "completed") => Some("received"),
                    Some("in transit" | "in_transit" | "dispatched") => Some("dispatched"),
                    Some("pending" | "draft") => Some("draft"),
                    Some("cancelled" | "canceled") => Some("cancelled"),
                    Some(other) => {
                        c.err("status", format!("Transfer Status “{other}”: Received, In transit, Pending or Cancelled"));
                        None
                    }
                    None => None,
                };
                if let Some(st) = status {
                    d.insert("status".into(), json!(st));
                }
                for (key, id_key) in [("from_branch", "from_id"), ("to_branch", "to_id")] {
                    match s(&d, key).map(|code| (code.to_string(), r.branches.get(&code.to_lowercase()).copied())) {
                        Some((_, Some(id))) => {
                            d.insert(id_key.into(), json!(id));
                        }
                        Some((code, None)) => c.err(key, format!("Branch code {code} does not exist (Settings → Branches)")),
                        None => {}
                    }
                }
                if d.get("from_id").is_some() && d.get("from_id") == d.get("to_id") {
                    c.err("to_branch", "The destination must differ from the origin");
                }
                order_or_transfer_refs(&mut d, &mut c, r, false);
                let sent = d.get("dispatched").and_then(|v| v.as_i64());
                if sent.is_some_and(|q| q <= 0) {
                    c.err("dispatched", "Quantity Dispatched must be more than 0");
                }
                let got = d.get("received").and_then(|v| v.as_i64());
                let date = s(&d, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                let rdate = s(&d, "received_date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                if date.is_some_and(|x| x > r.today) {
                    c.err("date", "The Transfer Date cannot be in the future");
                }
                match status {
                    Some("received") => {
                        if got.is_some_and(|g| g < 0 || sent.is_some_and(|s| g > s)) {
                            c.err("received", "Quantity Received must be between 0 and the quantity dispatched");
                        }
                        if rdate.is_none() {
                            c.err("received_date", "A received transfer needs its Receipt Date");
                        }
                        if let (Some(a), Some(b)) = (date, rdate) {
                            if b < a {
                                c.err("received_date", "The Receipt Date is before the Transfer Date");
                            }
                        }
                        match (r.cutover, rdate.or(date)) {
                            (Some(cut), Some(x)) if x >= cut => c.err("received_date", format!("Received on or after the migration cutover ({cut}) — receive it in S'Shop instead")),
                            (None, _) => c.warn("date", "Set the migration cutover (above) so history and S'Shop's own records stay apart"),
                            _ => {}
                        }
                        c.warn("status", "History: stock does not move again (opening stock already reflects it)");
                    }
                    Some("dispatched") => {
                        if got.is_some() {
                            c.err("received", "An in-transit transfer has not been received — leave Quantity Received empty");
                        }
                        if let Some(true) = s(&d, "product_id").and_then(|x| Uuid::parse_str(x).ok()).and_then(|id| r.products.values().find(|p| p.0 == id).map(|p| p.1)) {
                            c.err("product", "Individually tracked items cannot be in transit here — record each unit as opening stock where it is");
                        }
                        c.warn("dispatched", "In transit: receive it in S'Shop when it arrives — it must not be counted in either branch's opening stock");
                    }
                    Some("draft") => c.warn("status", "Pending: imported as a draft to dispatch in S'Shop (stock leaves the origin then)"),
                    Some("cancelled") => {
                        if r.cutover.is_some_and(|cut| date.is_some_and(|x| x >= cut)) {
                            c.err("date", "A cancelled transfer on or after the migration cutover belongs in S'Shop");
                        }
                    }
                    _ => {}
                }
            }
            "loyalty" => {
                match s(&d, "customer").map(|m| (m.to_string(), crate::util::normalize_mobile(m).ok())) {
                    Some((m, None)) => c.err("customer", format!("{m} is not a valid mobile number")),
                    Some((_, Some(n))) => {
                        dup(format!("loyalty:{n}"), "customer", &format!("Customer {n}"), &mut c);
                        match r.customer_ids.get(&n) {
                            Some(id) => {
                                d.insert("customer_id".into(), json!(id));
                                if r.loyalty_imported.contains(id) {
                                    c.err("customer", format!("{n}'s opening points were already imported — points are never allocated twice"));
                                }
                            }
                            None => c.err("customer", format!("No customer with mobile {n} — import customers first")),
                        }
                    }
                    None => {}
                }
                let n = |k: &str| d.get(k).and_then(|v| v.as_i64());
                let (balance, earned, redeemed) = (n("balance"), n("earned"), n("redeemed"));
                if [balance, earned, redeemed].iter().any(|v| v.is_some_and(|v| v < 0)) {
                    c.err("balance", "Points cannot be negative");
                }
                match (earned, redeemed) {
                    (Some(e), Some(rd)) if balance.is_some_and(|b| e - rd != b) => {
                        c.err("balance", format!("Points Earned − Points Redeemed ({}) must equal the Points Balance ({})", e - rd, balance.unwrap_or(0)));
                    }
                    (Some(_), None) | (None, Some(_)) => c.err("earned", "Give Points Earned and Points Redeemed together (or neither)"),
                    _ => {}
                }
                let eff = s(&d, "effective").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                if eff.is_some_and(|e| e > r.today) {
                    c.err("effective", "The Effective Date cannot be in the future");
                } else if let (Some(e), Some(cut)) = (eff, r.cutover) {
                    if e != cut {
                        c.warn("effective", format!("Effective Date differs from the migration cutover ({cut})"));
                    }
                }
                if let (Some(x), Some(e)) = (s(&d, "expiry").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()), eff) {
                    if x <= e {
                        c.err("expiry", "The Expiry Date must be after the Effective Date");
                    }
                }
                if !r.loyalty_on {
                    c.warn("balance", "Loyalty is switched off (Settings → Loyalty) — points are kept for when it is on");
                }
            }
            _ => {}
        }
        out.push((d, c));
    }
    if matches!(dataset, "orders" | "transfers") {
        documents(dataset, rows, &mut out, r);
    }
    out
}

/// Product, branch and customer references shared by orders and transfers.
fn order_or_transfer_refs(d: &mut Map<String, Value>, c: &mut Check, r: &Refs, order: bool) {
    if let Some(code) = s(d, "product").map(str::to_string) {
        let p = r.products.get(&code.to_uppercase()).cloned().or_else(|| r.product_barcodes.get(&code).and_then(|id| r.products.values().find(|p| p.0 == *id).cloned()));
        match p {
            Some(p) => {
                d.insert("product_id".into(), json!(p.0));
            }
            None => c.err("product", format!("No product with code or barcode {code} — import products first")),
        }
    }
    if order {
        match s(d, "branch").map(|code| (code.to_string(), r.branches.get(&code.to_lowercase()).copied())) {
            Some((_, Some(id))) => {
                d.insert("branch_id".into(), json!(id));
            }
            Some((code, None)) => c.err("branch", format!("Branch code {code} does not exist (Settings → Branches)")),
            None => {}
        }
        match s(d, "customer").map(|m| (m.to_string(), crate::util::normalize_mobile(m).ok())) {
            Some((m, None)) => c.err("customer", format!("{m} is not a valid mobile number")),
            Some((_, Some(n))) => match r.customer_ids.get(&n) {
                Some(id) => {
                    d.insert("customer_id".into(), json!(id));
                }
                None => c.err("customer", format!("No customer with mobile {n} — import customers first")),
            },
            None => {}
        }
    }
}

/// One reference = one order / transfer: its lines must agree on the header; outstanding orders that hold stock must
/// fit what is available at their branch (unless negative stock is allowed).
fn documents(dataset: &str, rows: &[(u32, Map<String, Value>)], out: &mut [(Map<String, Value>, Check)], r: &Refs) {
    let (ref_key, header, names): (&str, &[&str], &str) = if dataset == "orders" {
        ("order", &["date", "customer_id", "branch_id", "status", "delivery"], "date, customer, branch, status and delivery location")
    } else {
        ("transfer", &["from_id", "to_id", "date", "status", "received_date"], "branches, dates and status")
    };
    let mut first: HashMap<String, (u32, Vec<Option<String>>)> = HashMap::new();
    for ((row_no, _), (d, c)) in rows.iter().zip(out.iter_mut()) {
        let Some(rf) = s(d, ref_key).map(String::from) else { continue };
        let sig: Vec<Option<String>> = header.iter().map(|k| d.get(*k).filter(|v| !v.is_null()).map(|v| v.to_string().to_lowercase())).collect();
        match first.get(&rf) {
            Some((row0, sig0)) if *sig0 != sig => c.err(ref_key, format!("{rf}: every line must have the same {names} as its first line (row {row0})")),
            Some(_) => {}
            None => {
                first.insert(rf, (*row_no, sig));
            }
        }
    }
    if dataset == "orders" && !r.allow_negative {
        // In file order: the line that takes the running total past what is available is the one refused.
        let mut held: HashMap<(Uuid, Uuid), i64> = HashMap::new();
        for (d, c) in out.iter_mut() {
            if d.get("reserve").and_then(|v| v.as_bool()) != Some(true) {
                continue;
            }
            if let (Some(p), Some(b)) = (s(d, "product_id").and_then(|x| Uuid::parse_str(x).ok()), s(d, "branch_id").and_then(|x| Uuid::parse_str(x).ok())) {
                let have = r.available.get(&(p, b)).copied().unwrap_or(0);
                let want = held.entry((p, b)).or_default();
                *want += d.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0);
                if *want > have {
                    c.err("quantity", format!("Outstanding orders up to this line hold {want} of this product at this branch but only {have} are available — import the opening stock first or correct the orders"));
                }
            }
        }
    }
}

fn row_status(c: &Check) -> &'static str {
    if c.errors > 0 {
        "invalid"
    } else if c.warnings > 0 {
        "warning"
    } else {
        "valid"
    }
}

/// (Re)validates a batch's rows from its stored data and saves rows and counts.
async fn store_validation(conn: &mut PgConnection, ctx: &Ctx, batch: Uuid, dataset: &str, mode: &str) -> AppResult<()> {
    let rows: Vec<(u32, Value)> = sqlx::query_as::<_, (i32, Value)>("SELECT row_no, data FROM import_rows WHERE batch_id = $1 ORDER BY row_no")
        .bind(batch)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|(n, v)| (n as u32, v))
        .collect();
    let parsed: Vec<(u32, Map<String, Value>)> = rows.into_iter().map(|(n, v)| (n, raw_fields(dataset, v))).collect();
    let r = refs(conn, ctx).await?;
    let checked = validate(dataset, mode, &parsed, &r);
    let (mut valid, mut warn, mut invalid) = (0, 0, 0);
    for ((row_no, _), (d, c)) in parsed.iter().zip(checked) {
        let st = row_status(&c);
        match st {
            "valid" => valid += 1,
            "warning" => warn += 1,
            _ => invalid += 1,
        }
        sqlx::query("UPDATE import_rows SET data = $3, status = $4, messages = $5 WHERE batch_id = $1 AND row_no = $2")
            .bind(batch)
            .bind(*row_no as i32)
            .bind(Value::Object(d))
            .bind(st)
            .bind(Value::Array(c.msgs))
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("UPDATE import_batches SET valid_rows = $2, warning_rows = $3, invalid_rows = $4 WHERE id = $1")
        .bind(batch)
        .bind(valid)
        .bind(warn)
        .bind(invalid)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Every header of the workbook with the field it is read as ("" = ignored), for the column-mapping screen.
fn all_headers(sheet: &ob::Sheet) -> Value {
    let m: Map<String, Value> = sheet.headers.iter().map(|h| (h.clone(), json!(sheet.mapping.get(h).cloned().unwrap_or_default()))).collect();
    Value::Object(m)
}

/// The fields as read (drops what validation added, so re-validation starts from the workbook's values).
fn raw_fields(dataset: &str, v: Value) -> Map<String, Value> {
    let keys: HashSet<&str> = ob::columns(dataset).iter().map(|c| c.key).collect();
    match v {
        Value::Object(m) => m.into_iter().filter(|(k, _)| keys.contains(k.as_str())).collect(),
        _ => Map::new(),
    }
}

// ── Upload, view, remap, error report ───────────────────────────────────────────────────────────────────────────────

async fn upload(State(state): State<AppState>, ctx: Ctx, Path(dataset): Path<String>, mut mp: Multipart) -> AppResult<Json<Value>> {
    can(&ctx)?;
    ob::label(&dataset).ok_or(AppError::NotFound("Dataset"))?;
    let (mut file, mut name, mut mode, mut mapping) = (None::<Bytes>, String::from("upload.xlsx"), String::from("create"), HashMap::<String, String>::new());
    while let Some(f) = mp.next_field().await.map_err(|_| bad("The file could not be read. Try again."))? {
        match f.name() {
            Some("file") => {
                name = f.file_name().unwrap_or("upload.xlsx").chars().take(120).collect();
                file = Some(f.bytes().await.map_err(|_| bad("The file could not be read. Try again."))?);
            }
            Some("mode") => mode = f.text().await.unwrap_or_default(),
            Some("mapping") => mapping = serde_json::from_str(&f.text().await.unwrap_or_default()).unwrap_or_default(),
            _ => {}
        }
    }
    let file = file.ok_or_else(|| bad("Choose an Excel (.xlsx) file"))?;
    if !matches!(mode.as_str(), "create" | "update") || (mode == "update" && dataset != "products") {
        return Err(bad("Unknown import mode"));
    }
    if mode == "update" {
        ctx.require("products.edit")?;
    }
    let sheet = ob::read(&dataset, &file, &mapping)?;
    let sha = crate::secrets::digest_bytes(&file);
    let posted: Option<String> = sqlx::query_scalar("SELECT file_name FROM import_batches WHERE tenant_id = $1 AND dataset = $2 AND file_sha256 = $3 AND status = 'posted' LIMIT 1")
        .bind(ctx.tenant_id)
        .bind(&dataset)
        .bind(&sha)
        .fetch_optional(&state.db)
        .await?;
    if let Some(f) = posted {
        return Err(refused("Already imported", format!("This workbook ({f}) was already imported — nothing would change. Upload a corrected or new workbook.")));
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO import_batches (tenant_id, dataset, mode, file_name, file_sha256, file, mapping, total_rows, uploaded_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(&dataset)
    .bind(&mode)
    .bind(&name)
    .bind(&sha)
    .bind(file.to_vec())
    .bind(all_headers(&sheet))
    .bind(sheet.rows.len() as i32)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    for (row_no, data, source) in &sheet.rows {
        sqlx::query("INSERT INTO import_rows (batch_id, row_no, source, data, status) VALUES ($1, $2, $3, $4, 'valid')")
            .bind(id)
            .bind(*row_no as i32)
            .bind(Value::Object(source.clone()))
            .bind(Value::Object(data.clone()))
            .execute(&mut *tx)
            .await?;
    }
    store_validation(&mut tx, &ctx, id, &dataset, &mode).await?;
    audit::record(&mut tx, &ctx, Entry::new("onboarding", "uploaded", "import_batch", id).after(json!({ "dataset": dataset, "file": name, "rows": sheet.rows.len() }))).await?;
    tx.commit().await?;
    Ok(Json(batch_view(&state, &ctx, id, None, 0).await?))
}

#[derive(Deserialize)]
struct RowsQuery {
    status: Option<String>,
    #[serde(default)]
    offset: i64,
}

async fn batch(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Query(q): Query<RowsQuery>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    Ok(Json(batch_view(&state, &ctx, id, q.status.as_deref(), q.offset).await?))
}

async fn batch_view(state: &AppState, ctx: &Ctx, id: Uuid, status: Option<&str>, offset: i64) -> AppResult<Value> {
    let b: Value = sqlx::query_scalar(
        "SELECT json_build_object('id', b.id, 'dataset', b.dataset, 'mode', b.mode, 'status', b.status, 'file_name', b.file_name,
                'mapping', b.mapping, 'total_rows', b.total_rows, 'valid_rows', b.valid_rows, 'warning_rows', b.warning_rows,
                'invalid_rows', b.invalid_rows, 'posted_rows', b.posted_rows, 'skip_invalid', b.skip_invalid, 'summary', b.summary,
                'created_at', b.created_at, 'posted_at', b.posted_at, 'uploaded_by', u.name, 'decided_by', d.name, 'error', b.error,
                'approval_id', b.approval_id, 'reference', b.reference)
         FROM import_batches b LEFT JOIN users u ON u.id = b.uploaded_by LEFT JOIN users d ON d.id = b.decided_by
         WHERE b.id = $1 AND b.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Import"))?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object('row_no', row_no, 'status', status, 'data', data, 'source', source, 'messages', messages, 'target_id', target_id)
         FROM import_rows WHERE batch_id = $1 AND ($2::text IS NULL OR status = $2) ORDER BY row_no LIMIT 200 OFFSET $3",
    )
    .bind(id)
    .bind(status)
    .bind(offset.max(0))
    .fetch_all(&state.db)
    .await?;
    let dataset = b["dataset"].as_str().unwrap_or_default().to_string();
    let headers: Vec<Value> = ob::columns(&dataset).iter().map(|c| json!({ "key": c.key, "header": c.header, "required": c.need == Need::Required })).collect();
    Ok(json!({ "batch": b, "rows": rows, "columns": headers, "may_skip_invalid": may_skip(&dataset) }))
}

#[derive(Deserialize)]
struct RemapBody {
    mapping: HashMap<String, String>,
}

/// Re-reads the stored workbook with another column mapping (source header → field, "" = ignore).
async fn remap(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(body): Json<RemapBody>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    let mut tx = state.db.begin().await?;
    let (dataset, mode, status, file): (String, String, String, Vec<u8>) =
        sqlx::query_as("SELECT dataset, mode, status, file FROM import_batches WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("Import"))?;
    if !matches!(status.as_str(), "validated" | "failed") {
        return Err(rule("Only an import not yet submitted can be re-mapped"));
    }
    let sheet = ob::read(&dataset, &file, &body.mapping)?;
    sqlx::query("DELETE FROM import_rows WHERE batch_id = $1").bind(id).execute(&mut *tx).await?;
    for (row_no, data, source) in &sheet.rows {
        sqlx::query("INSERT INTO import_rows (batch_id, row_no, source, data, status) VALUES ($1, $2, $3, $4, 'valid')")
            .bind(id)
            .bind(*row_no as i32)
            .bind(Value::Object(source.clone()))
            .bind(Value::Object(data.clone()))
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE import_batches SET mapping = $2, total_rows = $3 WHERE id = $1").bind(id).bind(all_headers(&sheet)).bind(sheet.rows.len() as i32).execute(&mut *tx).await?;
    store_validation(&mut tx, &ctx, id, &dataset, &mode).await?;
    tx.commit().await?;
    Ok(Json(batch_view(&state, &ctx, id, None, 0).await?))
}

/// Checks again (e.g. after importing the products an opening-stock file refers to).
async fn revalidate(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    let mut tx = state.db.begin().await?;
    let (dataset, mode, status): (String, String, String) = sqlx::query_as("SELECT dataset, mode, status FROM import_batches WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Import"))?;
    if !matches!(status.as_str(), "validated" | "failed") {
        return Err(rule("Only an import not yet submitted can be checked again"));
    }
    store_validation(&mut tx, &ctx, id, &dataset, &mode).await?;
    sqlx::query("UPDATE import_batches SET status = 'validated', error = NULL WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(batch_view(&state, &ctx, id, None, 0).await?))
}

/// The rows with errors or warnings, as uploaded, plus what to fix — correct them and upload this file again.
async fn error_report(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<impl IntoResponse> {
    can(&ctx)?;
    let (dataset, file_name): (String, String) = sqlx::query_as("SELECT dataset, file_name FROM import_batches WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Import"))?;
    let rows: Vec<(i32, String, Value, Value)> = sqlx::query_as(
        "SELECT row_no, status, data, messages FROM import_rows WHERE batch_id = $1 AND status IN ('invalid','warning','skipped') ORDER BY row_no",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let x = |e: rust_xlsxwriter::XlsxError| AppError::Other(anyhow::anyhow!(e));
    let mut wb = rust_xlsxwriter::Workbook::new();
    rows_sheet(wb.add_worksheet().set_name("Rows to fix").map_err(x)?, &dataset, &rows).map_err(x)?;
    let bytes = wb.save_to_buffer().map_err(x)?;
    let name = format!("Fix_{}", file_name.trim_end_matches(".xlsx").chars().take(60).collect::<String>()) + ".xlsx";
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        bytes,
    ))
}

/// The import report: a summary (reference, who, when, counts) and every row with what happened to it.
async fn import_report(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<impl IntoResponse> {
    can(&ctx)?;
    type Head = (String, String, String, String, Option<String>, Option<Value>, Option<chrono::DateTime<Utc>>, Option<String>, Option<String>, Option<String>);
    let (dataset, mode, status, file_name, reference, summary, posted_at, uploader, approver, error): Head = sqlx::query_as(
        "SELECT b.dataset, b.mode, b.status, b.file_name, b.reference, b.summary, b.posted_at, u.name, d.name, b.error
         FROM import_batches b LEFT JOIN users u ON u.id = b.uploaded_by LEFT JOIN users d ON d.id = b.decided_by
         WHERE b.id = $1 AND b.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Import"))?;
    let rows: Vec<(i32, String, Value, Value)> = sqlx::query_as("SELECT row_no, status, data, messages FROM import_rows WHERE batch_id = $1 ORDER BY row_no")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let x = |e: rust_xlsxwriter::XlsxError| AppError::Other(anyhow::anyhow!(e));
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet().set_name("Summary").map_err(x)?;
    let bold = rust_xlsxwriter::Format::new().set_bold();
    ws.set_column_width(0, 26).map_err(x)?;
    ws.set_column_width(1, 44).map_err(x)?;
    let sm = summary.unwrap_or(Value::Null);
    let num = |k: &str| sm[k].as_i64().map(|n| n.to_string()).unwrap_or_else(|| "—".into());
    let at = posted_at.map(|a| a.with_timezone(&ctx.tz).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "—".into());
    let lines: Vec<(&str, String)> = vec![
        ("Import reference", reference.unwrap_or_else(|| "—".into())),
        ("Dataset", ob::label(&dataset).unwrap_or("Data").to_string()),
        ("Mode", if mode == "update" { "Update existing" } else { "Add new" }.to_string()),
        ("File", file_name.clone()),
        ("Status", match status.as_str() { "posted" => "Imported", "failed" => "Failed — nothing saved", "posting" => "Importing", "pending_approval" => "Awaiting approval", other => other }.to_string()),
        ("Imported at", at),
        ("Uploaded by", uploader.unwrap_or_default()),
        ("Approved by", approver.unwrap_or_else(|| "—".into())),
        ("Records processed", num("total")),
        ("Imported", num("posted")),
        ("Created", num("created")),
        ("Updated", num("updated")),
        ("Matched existing (not duplicated)", num("matched")),
        ("Left out (invalid)", num("skipped")),
        ("Failed", num("failed")),
    ];
    for (r, (k, v)) in lines.iter().enumerate() {
        ws.write_string_with_format(r as u32, 0, *k, &bold).map_err(x)?;
        ws.write_string(r as u32, 1, v).map_err(x)?;
    }
    if let Some(e) = error {
        ws.write_string_with_format(lines.len() as u32, 0, "Error", &bold).map_err(x)?;
        ws.write_string(lines.len() as u32, 1, e).map_err(x)?;
    }
    rows_sheet(wb.add_worksheet().set_name("Rows").map_err(x)?, &dataset, &rows).map_err(x)?;
    let bytes = wb.save_to_buffer().map_err(x)?;
    let name = format!("Import_report_{}", file_name.trim_end_matches(".xlsx").chars().take(60).collect::<String>()) + ".xlsx";
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        bytes,
    ))
}

/// Rows as uploaded, with their status and messages (error report and import report).
fn rows_sheet(ws: &mut rust_xlsxwriter::Worksheet, dataset: &str, rows: &[(i32, String, Value, Value)]) -> Result<(), rust_xlsxwriter::XlsxError> {
    let x = |e: rust_xlsxwriter::XlsxError| e;
    let bold = rust_xlsxwriter::Format::new().set_bold().set_background_color("F9E3CF");
    let text = rust_xlsxwriter::Format::new().set_num_format("@");
    let cols = ob::columns(dataset);
    for (i, c) in cols.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, c.header, &bold).map_err(x)?;
        ws.set_column_format(i as u16, &text).map_err(x)?;
        ws.set_column_width(i as u16, 18).map_err(x)?;
    }
    let n = cols.len() as u16;
    ws.write_string_with_format(0, n, "Status", &bold).map_err(x)?;
    ws.write_string_with_format(0, n + 1, "Row in your file", &bold).map_err(x)?;
    ws.write_string_with_format(0, n + 2, "What to fix", &bold).map_err(x)?;
    ws.set_column_width(n + 2, 80).map_err(x)?;
    for (r, (row_no, status, data, msgs)) in rows.iter().enumerate() {
        let r = r as u32 + 1;
        for (i, c) in cols.iter().enumerate() {
            let v = match data.get(c.key) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                Some(Value::Bool(b)) => if *b { "YES" } else { "NO" }.into(),
                Some(o) if o.get("invalid").is_some() => o["invalid"].as_str().unwrap_or_default().to_string(),
                _ => String::new(),
            };
            ws.write_string(r, i as u16, v).map_err(x)?;
        }
        let label = match status.as_str() {
            "invalid" => "Error",
            "warning" => "Warning",
            "posted" => "Imported",
            "skipped" => "Left out",
            _ => "Valid",
        };
        ws.write_string(r, n, label).map_err(x)?;
        ws.write_number(r, n + 1, *row_no as f64).map_err(x)?;
        let what: Vec<String> = msgs.as_array().into_iter().flatten().filter_map(|m| m["message"].as_str().map(String::from)).collect();
        ws.write_string(r, n + 2, what.join(" · ")).map_err(x)?;
    }
    Ok(())
}

async fn cancel(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    let n = sqlx::query("UPDATE import_batches SET status = 'cancelled', decided_by = $3, decided_at = now(), file = '' WHERE id = $1 AND tenant_id = $2 AND status IN ('validated', 'failed')")
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(ctx.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(rule("Only an import not yet submitted can be discarded"));
    }
    Ok(Json(json!({ "ok": true })))
}

// ── Reconciliation ─────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct RecQuery {
    dataset: Option<String>,
    batch_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

/// Per posted batch: what the workbook said (its posted rows) against what S'Shop holds for it now, plus what is
/// unfinished (imports not posted, rows left out). Branch-filtered where the dataset has branches.
async fn reconcile_data(conn: &mut PgConnection, tenant: Uuid, q: &RecQuery) -> AppResult<Value> {
    let batches: Vec<(Uuid, String, String, i32, i32, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT id, dataset, file_name, posted_rows, total_rows, posted_at FROM import_batches
         WHERE tenant_id = $1 AND status = 'posted' AND ($2::text IS NULL OR dataset = $2) AND ($3::uuid IS NULL OR id = $3)
           AND ($4::date IS NULL OR posted_at::date >= $4) AND ($5::date IS NULL OR posted_at::date <= $5)
         ORDER BY posted_at",
    )
    .bind(tenant)
    .bind(&q.dataset)
    .bind(q.batch_id)
    .bind(q.from)
    .bind(q.to)
    .fetch_all(&mut *conn)
    .await?;
    let br = q.branch_id;
    let mut lines = Vec::new();
    let mut differences = 0usize;
    for (id, dataset, file, posted, total, at) in batches {
        // (measure, source total from the posted rows, total S'Shop holds for this batch)
        let (measure, src, held): (&str, Decimal, Decimal) = match dataset.as_str() {
            "products" | "customers" => {
                let n: i64 = sqlx::query_scalar(&format!(
                    "SELECT count(*) FROM import_rows r JOIN {} x ON x.id = r.target_id WHERE r.batch_id = $1 AND r.status = 'posted'",
                    if dataset == "products" { "products" } else { "customers" }
                ))
                .bind(id)
                .fetch_one(&mut *conn)
                .await?;
                ("records", Decimal::from(posted), Decimal::from(n))
            }
            "opening_stock" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum((data->>'quantity')::numeric), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted' AND ($2::uuid IS NULL OR data->>'branch_id' = $2::text)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(quantity), 0)::numeric FROM stock_movements WHERE import_batch_id = $1 AND ($2::uuid IS NULL OR branch_id = $2)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                ("units", src, held)
            }
            "expenses" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum((data->>'amount')::numeric), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted' AND ($2::uuid IS NULL OR data->>'branch_id' = $2::text)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(amount), 0) FROM expenses WHERE import_batch_id = $1 AND status <> 'void' AND ($2::uuid IS NULL OR branch_id = $2)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                ("amount", src, held)
            }
            "sales" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(round((data->>'unit_price')::numeric * (data->>'quantity')::numeric, 2)), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted' AND ($2::uuid IS NULL OR data->>'branch_id' = $2::text)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(total), 0) FROM sales WHERE import_batch_id = $1 AND ($2::uuid IS NULL OR branch_id = $2)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                ("amount", src, held)
            }
            "credit" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum((data->>'original_amount')::numeric - COALESCE((data->>'amount_paid')::numeric, 0)), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted' AND ($2::uuid IS NULL OR data->>'branch_id' = $2::text)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                // As imported (later repayments in S'Shop reduce the live balance; they are not differences).
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(cs.original_amount - COALESCE(p.paid, 0)), 0) FROM credit_sales cs
                         LEFT JOIN (SELECT credit_sale_id, sum(amount) AS paid FROM payments WHERE method = 'legacy' GROUP BY credit_sale_id) p ON p.credit_sale_id = cs.id
                         WHERE cs.import_batch_id = $1 AND ($2::uuid IS NULL OR cs.branch_id = $2)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                ("balance", src, held)
            }
            "loyalty" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum((data->>'balance')::numeric), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted'")
                    .bind(id).fetch_one(&mut *conn).await?;
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(points), 0)::numeric FROM loyalty_ledger WHERE import_batch_id = $1").bind(id).fetch_one(&mut *conn).await?;
                ("points", src, held)
            }
            "orders" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(round((data->>'unit_price')::numeric * (data->>'quantity')::numeric, 2)), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted' AND ($2::uuid IS NULL OR data->>'branch_id' = $2::text)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(total), 0) FROM orders WHERE import_batch_id = $1 AND ($2::uuid IS NULL OR branch_id = $2)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                ("amount", src, held)
            }
            "transfers" => {
                let src: Decimal = sqlx::query_scalar("SELECT COALESCE(sum((data->>'dispatched')::numeric), 0) FROM import_rows WHERE batch_id = $1 AND status = 'posted' AND ($2::uuid IS NULL OR data->>'from_id' = $2::text OR data->>'to_id' = $2::text)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                let held: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(ti.quantity), 0)::numeric FROM transfer_items ti JOIN transfers t ON t.id = ti.transfer_id WHERE t.import_batch_id = $1 AND ($2::uuid IS NULL OR t.from_branch_id = $2 OR t.to_branch_id = $2)")
                    .bind(id).bind(br).fetch_one(&mut *conn).await?;
                ("units", src, held)
            }
            _ => continue,
        };
        if held != src {
            differences += 1;
        }
        lines.push(json!({
            "batch_id": id, "dataset": dataset, "label": ob::label(&dataset), "file_name": file, "posted_at": at,
            "rows_posted": posted, "rows_left_out": total - posted, "measure": measure, "source": src, "imported": held, "difference": held - src,
        }));
    }
    let unfinished: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object('id', id, 'dataset', dataset, 'file_name', file_name, 'status', status, 'created_at', created_at)
         FROM import_batches WHERE tenant_id = $1 AND status IN ('validated', 'pending_approval', 'posting', 'failed') ORDER BY created_at",
    )
    .bind(tenant)
    .fetch_all(&mut *conn)
    .await?;
    let left_out: i64 = lines.iter().map(|l| l["rows_left_out"].as_i64().unwrap_or(0)).sum();
    let completed: Option<chrono::DateTime<Utc>> = sqlx::query_scalar("SELECT onboarding_completed_at FROM tenants WHERE id = $1").bind(tenant).fetch_one(&mut *conn).await?;
    Ok(json!({
        "lines": lines, "unfinished": unfinished, "differences": differences, "rows_left_out": left_out,
        "reconciled": differences == 0 && unfinished.is_empty(), "completed_at": completed,
    }))
}

async fn reconciliation(State(state): State<AppState>, ctx: Ctx, Query(q): Query<RecQuery>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    Ok(Json(reconcile_data(&mut *state.db.acquire().await?, ctx.tenant_id, &q).await?))
}

/// The Migration Reconciliation Report as Excel.
async fn reconciliation_export(State(state): State<AppState>, ctx: Ctx, Query(q): Query<RecQuery>) -> AppResult<impl IntoResponse> {
    can(&ctx)?;
    let data = reconcile_data(&mut *state.db.acquire().await?, ctx.tenant_id, &q).await?;
    let x = |e: rust_xlsxwriter::XlsxError| AppError::Other(anyhow::anyhow!(e));
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet().set_name("Reconciliation").map_err(x)?;
    let bold = rust_xlsxwriter::Format::new().set_bold().set_background_color("F9E3CF");
    let red = rust_xlsxwriter::Format::new().set_font_color("B42318").set_bold();
    let heads = ["Dataset", "File", "Posted", "Rows imported", "Rows left out", "Measure", "Workbook", "In S'Shop", "Difference"];
    for (i, h) in heads.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, *h, &bold).map_err(x)?;
        ws.set_column_width(i as u16, if i == 1 { 34 } else { 15 }).map_err(x)?;
    }
    for (r, l) in data["lines"].as_array().into_iter().flatten().enumerate() {
        let r = r as u32 + 1;
        let num = |k: &str| l[k].as_str().and_then(|v| v.parse::<f64>().ok()).or_else(|| l[k].as_f64()).unwrap_or(0.0);
        ws.write_string(r, 0, l["label"].as_str().unwrap_or_default()).map_err(x)?;
        ws.write_string(r, 1, l["file_name"].as_str().unwrap_or_default()).map_err(x)?;
        ws.write_string(r, 2, l["posted_at"].as_str().unwrap_or_default().get(..10).unwrap_or_default()).map_err(x)?;
        ws.write_number(r, 3, l["rows_posted"].as_f64().unwrap_or(0.0)).map_err(x)?;
        ws.write_number(r, 4, l["rows_left_out"].as_f64().unwrap_or(0.0)).map_err(x)?;
        ws.write_string(r, 5, l["measure"].as_str().unwrap_or_default()).map_err(x)?;
        ws.write_number(r, 6, num("source")).map_err(x)?;
        ws.write_number(r, 7, num("imported")).map_err(x)?;
        let diff = num("difference");
        if diff != 0.0 {
            ws.write_number_with_format(r, 8, diff, &red).map_err(x)?;
        } else {
            ws.write_number(r, 8, diff).map_err(x)?;
        }
    }
    let n = data["lines"].as_array().map_or(0, |a| a.len()) as u32 + 2;
    ws.write_string_with_format(n, 0, if data["reconciled"].as_bool() == Some(true) { "Reconciled" } else { "Not reconciled" }, &bold).map_err(x)?;
    ws.write_string(n + 1, 0, format!("Imports not finished: {}", data["unfinished"].as_array().map_or(0, |a| a.len()))).map_err(x)?;
    let bytes = wb.save_to_buffer().map_err(x)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"SShop_Migration_Reconciliation.xlsx\"".to_string()),
        ],
        bytes,
    ))
}

/// Marks onboarding complete — refused while anything differs or an import is unfinished.
async fn complete_onboarding(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    can(&ctx)?;
    ctx.require_full()?;
    let mut tx = state.db.begin().await?;
    let data = reconcile_data(&mut tx, ctx.tenant_id, &RecQuery::default()).await?;
    if data["reconciled"].as_bool() != Some(true) {
        return Err(refused(
            "Not reconciled yet",
            format!(
                "{} difference(s) and {} unfinished import(s) remain — resolve them (Reconciliation) before completing onboarding",
                data["differences"], data["unfinished"].as_array().map_or(0, |a| a.len())
            ),
        ));
    }
    sqlx::query("UPDATE tenants SET onboarding_completed_at = now() WHERE id = $1").bind(ctx.tenant_id).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("onboarding", "completed", "tenant", ctx.tenant_id).after(json!({ "batches": data["lines"].as_array().map_or(0, |a| a.len()) }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Posting ─────────────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PostBody {
    /// Leave the invalid rows out (products and customers only).
    #[serde(default)]
    skip_invalid: bool,
}

/// Posts now, or sends the batch for approval when the *Data import* workflow is on.
async fn post_batch(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<PostBody>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    ctx.require_full()?;
    let mut tx = state.db.begin().await?;
    let (dataset, status, total, invalid, file_name): (String, String, i32, i32, String) =
        sqlx::query_as("SELECT dataset, status, total_rows, invalid_rows, file_name FROM import_batches WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("Import"))?;
    match status.as_str() {
        "validated" | "failed" => {}
        "posting" => return Err(rule("This import is already running")),
        _ => return Err(rule("This import was already submitted")),
    }
    if invalid > 0 && !(b.skip_invalid && may_skip(&dataset)) {
        return Err(refused(
            "Fix the invalid rows",
            if may_skip(&dataset) {
                format!("{invalid} row(s) have errors — fix them, or choose to import only the valid rows")
            } else {
                format!("{invalid} row(s) have errors — stock and money import complete or not at all: fix them and upload again")
            },
        ));
    }
    if invalid == total {
        return Err(rule("No valid rows to import"));
    }
    let has_ref: bool = sqlx::query_scalar("SELECT reference IS NOT NULL FROM import_batches WHERE id = $1").bind(id).fetch_one(&mut *tx).await?;
    let reference = if has_ref { None } else { Some(next_doc_no(&mut tx, ctx.tenant_id, "IMP", ctx.tz).await?) };
    sqlx::query("UPDATE import_batches SET skip_invalid = $2, error = NULL, reference = COALESCE(reference, $3) WHERE id = $1")
        .bind(id)
        .bind(b.skip_invalid)
        .bind(&reference)
        .execute(&mut *tx)
        .await?;
    if workflow::needs_approval(&mut tx, &ctx, "data.import", workflow::Gate::default()).await? {
        let label = ob::label(&dataset).unwrap_or("Data");
        let approval = workflow::submit(
            &mut tx,
            &ctx,
            workflow::Request {
                action: "data.import",
                entity_type: "import_batch",
                entity_id: id,
                branch_id: None,
                summary: format!("{label} import: {} row(s) from {file_name}", total - if b.skip_invalid { invalid } else { 0 }),
                amount: None,
                payload: json!({ "batch_id": id, "dataset": dataset }),
            },
        )
        .await?;
        sqlx::query("UPDATE import_batches SET status = 'pending_approval', approval_id = $2 WHERE id = $1").bind(id).bind(approval).execute(&mut *tx).await?;
        tx.commit().await?;
        super::approvals::notify_approvers(&state, &ctx, approval).await;
        return Ok(Json(json!({ "status": "pending_approval", "approval_id": approval })));
    }
    // Runs in the background: the request returns at once and the screen follows the real progress.
    let rows = total - if b.skip_invalid && may_skip(&dataset) { invalid } else { 0 };
    sqlx::query("UPDATE import_batches SET status = 'posting', started_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO import_progress (batch_id, stage, done, total, updated_at) VALUES ($1, 'preparing', 0, $2, now())
         ON CONFLICT (batch_id) DO UPDATE SET stage = 'preparing', done = 0, total = $2, updated_at = now()",
    )
    .bind(id)
    .bind(rows)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tokio::spawn(run_import(state.clone(), ctx.clone(), id));
    Ok(Json(json!({ "status": "posting", "batch_id": id })))
}

/// Reports real progress on its own connection (throttled), so the import's transaction never waits on it.
/// Without a pool (posting inside an approval), it records nothing.
pub struct Progress {
    db: Option<sqlx::PgPool>,
    batch: Uuid,
    done: i32,
    last: std::time::Instant,
}

impl Progress {
    fn live(db: sqlx::PgPool, batch: Uuid) -> Self {
        Self { db: Some(db), batch, done: 0, last: std::time::Instant::now() }
    }
    fn off(batch: Uuid) -> Self {
        Self { db: None, batch, done: 0, last: std::time::Instant::now() }
    }
    async fn stage(&mut self, stage: &str) {
        if let Some(db) = &self.db {
            let _ = sqlx::query("UPDATE import_progress SET stage = $2, done = $3, updated_at = now() WHERE batch_id = $1")
                .bind(self.batch)
                .bind(stage)
                .bind(self.done)
                .execute(db)
                .await;
            self.last = std::time::Instant::now();
        }
    }
    async fn step(&mut self, rows: i32) {
        self.done += rows;
        if self.db.is_some() && self.last.elapsed() >= std::time::Duration::from_millis(250) {
            self.stage("importing").await;
        }
    }
}

/// What a failed import tells the person: business refusals as they are, anything unexpected without internals.
fn failure_text(e: &AppError) -> String {
    match e {
        AppError::Refused { title, message } => format!("{title}: {message}"),
        AppError::Rule(m) | AppError::BadRequest(m) => m.clone(),
        _ => "Something went wrong while importing. Nothing was saved — try again.".into(),
    }
}

/// The import itself: one transaction (everything or nothing), progress reported as it goes, a heartbeat while it
/// runs, and a clear failed state (never a stuck one) if anything goes wrong — including a crash of the task.
async fn run_import(state: AppState, ctx: Ctx, id: Uuid) {
    let beat_db = state.db.clone();
    let heartbeat = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let _ = sqlx::query("UPDATE import_progress SET updated_at = now() WHERE batch_id = $1").bind(id).execute(&beat_db).await;
        }
    });
    let (st, c) = (state.clone(), ctx.clone());
    let work = tokio::spawn(async move {
        let mut p = Progress::live(st.db.clone(), id);
        let mut tx = st.db.begin().await?;
        post_rows(&mut tx, &c, id, &mut p).await?;
        tx.commit().await?;
        AppResult::Ok(())
    });
    let outcome = match work.await {
        Ok(r) => r,
        Err(_) => Err(AppError::Other(anyhow::anyhow!("import task stopped"))),
    };
    heartbeat.abort();
    match outcome {
        Ok(()) => {
            let _ = sqlx::query("UPDATE import_progress SET stage = 'completed', done = total, updated_at = now() WHERE batch_id = $1").bind(id).execute(&state.db).await;
            state.emit(ctx.tenant_id, None, "stock", json!({}));
        }
        Err(e) => {
            tracing::warn!(batch = %id, error = %e, "import failed");
            let _ = sqlx::query("UPDATE import_batches SET status = 'failed', error = $2 WHERE id = $1 AND status = 'posting'")
                .bind(id)
                .bind(failure_text(&e))
                .execute(&state.db)
                .await;
            let _ = sqlx::query("UPDATE import_progress SET stage = 'failed', updated_at = now() WHERE batch_id = $1").bind(id).execute(&state.db).await;
        }
    }
}

/// An import whose worker stopped (the server restarted mid-way) rolled back: mark it failed rather than running forever.
async fn fail_interrupted(db: &sqlx::PgPool, tenant: Uuid) -> AppResult<()> {
    sqlx::query(
        "UPDATE import_batches b SET status = 'failed', error = 'The import was interrupted. Nothing was saved — import it again.'
         FROM import_progress p
         WHERE p.batch_id = b.id AND b.tenant_id = $1 AND b.status = 'posting' AND p.updated_at < now() - interval '2 minutes'",
    )
    .bind(tenant)
    .execute(db)
    .await?;
    Ok(())
}

/// Live state of one import for the progress screen (polled while it runs; works again after a refresh).
async fn progress(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    can(&ctx)?;
    fail_interrupted(&state.db, ctx.tenant_id).await?;
    let v: Value = sqlx::query_scalar(
        "SELECT json_build_object('id', b.id, 'dataset', b.dataset, 'mode', b.mode, 'status', b.status, 'file_name', b.file_name,
                'reference', b.reference, 'total_rows', b.total_rows, 'invalid_rows', b.invalid_rows, 'posted_rows', b.posted_rows,
                'skip_invalid', b.skip_invalid, 'summary', b.summary, 'error', b.error, 'started_at', b.started_at, 'posted_at', b.posted_at,
                'stage', p.stage, 'done', COALESCE(p.done, 0), 'total', COALESCE(p.total, 0))
         FROM import_batches b LEFT JOIN import_progress p ON p.batch_id = b.id
         WHERE b.id = $1 AND b.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Import"))?;
    Ok(Json(v))
}

/// Approved by the *Data import* workflow (inside the approval's transaction).
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &super::approvals::ApprovalRow) -> AppResult<()> {
    sqlx::query("UPDATE import_batches SET decided_by = $2, decided_at = now(), started_at = now() WHERE id = $1").bind(a.entity_id).bind(ctx.user_id).execute(&mut *conn).await?;
    post_rows(conn, ctx, a.entity_id, &mut Progress::off(a.entity_id)).await.map(|_| ())
}

pub async fn on_rejected(conn: &mut PgConnection, ctx: &Ctx, a: &super::approvals::ApprovalRow, outcome: &str) -> AppResult<()> {
    sqlx::query("UPDATE import_batches SET status = $3, decided_by = $2, decided_at = now() WHERE id = $1 AND status = 'pending_approval'")
        .bind(a.entity_id)
        .bind(ctx.user_id)
        .bind(if outcome == "cancelled" { "cancelled" } else { "rejected" })
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Validates again (data may have changed since the upload) and writes every valid row — all in the caller's
/// transaction: the batch posts completely or not at all.
async fn post_rows(conn: &mut PgConnection, ctx: &Ctx, id: Uuid, p: &mut Progress) -> AppResult<Value> {
    let (dataset, mode, status, skip, uploader): (String, String, String, bool, Option<Uuid>) =
        sqlx::query_as("SELECT dataset, mode, status, skip_invalid, uploaded_by FROM import_batches WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_one(&mut *conn)
            .await?;
    if !matches!(status.as_str(), "validated" | "pending_approval" | "posting") {
        return Err(rule("This import was already processed"));
    }
    p.stage("validating").await;
    store_validation(conn, ctx, id, &dataset, &mode).await?;
    let invalid: i64 = sqlx::query_scalar("SELECT count(*) FROM import_rows WHERE batch_id = $1 AND status = 'invalid'").bind(id).fetch_one(&mut *conn).await?;
    if invalid > 0 && !(skip && may_skip(&dataset)) {
        return Err(refused("Data changed", format!("{invalid} row(s) are no longer valid (the business's data changed since the upload) — check the import again")));
    }
    let rows: Vec<(i32, Value)> = sqlx::query_as("SELECT row_no, data FROM import_rows WHERE batch_id = $1 AND status IN ('valid','warning') ORDER BY row_no")
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    let cfg = settings::load(conn, ctx.tenant_id).await?;
    p.stage("importing").await;
    let mut posted = 0;
    let mut created = 0;
    let mut matched = 0;
    let mut quantity: i64 = 0;
    let mut amount = Decimal::ZERO;
    let grouped = matches!(dataset.as_str(), "sales" | "orders" | "transfers");
    if grouped {
        let (n, total, units) = match dataset.as_str() {
            "sales" => post_sales(conn, ctx, &cfg, id, uploader, &rows, p).await.map(|(n, v)| (n, v, 0))?,
            "orders" => post_orders(conn, ctx, &cfg, id, uploader, &rows, p).await?,
            _ => post_transfers(conn, ctx, id, uploader, &rows, p).await?,
        };
        posted = rows.len() as i32;
        created = n;
        amount = total;
        quantity = units;
    }
    for (row_no, data) in rows.iter().filter(|_| !grouped) {
        let d = data.as_object().cloned().unwrap_or_default();
        let target = match dataset.as_str() {
            "products" => {
                let (pid, new) = post_product(conn, ctx, id, &mode, &d).await?;
                if new {
                    created += 1;
                } else {
                    matched += 1;
                }
                Some(pid)
            }
            "customers" => {
                let mobile = s_req(&d, "mobile")?;
                let (cid, new) = super::customers::upsert_by_mobile(conn, ctx.tenant_id, uploader, &mobile, s(&d, "first_name").unwrap_or(""), s(&d, "nickname").unwrap_or("")).await?;
                if new {
                    sqlx::query("UPDATE customers SET other_names = $2, email = $3, import_batch_id = $4 WHERE id = $1")
                        .bind(cid)
                        .bind(s(&d, "other_names").unwrap_or(""))
                        .bind(s(&d, "email").unwrap_or(""))
                        .bind(id)
                        .execute(&mut *conn)
                        .await?;
                    created += 1;
                } else {
                    matched += 1;
                }
                Some(cid)
            }
            "opening_stock" => {
                let qty = d.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                if qty == 0 {
                    None
                } else {
                    let product_id = uuid_of(&d, "product_id")?;
                    let branch_id = uuid_of(&d, "branch_id")?;
                    let mut body = super::stock::ReceiveBody {
                        product_id,
                        branch_id: Some(branch_id),
                        quantity: qty,
                        barcodes: d.get("units").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
                        cost_price: money(&d, "unit_cost"),
                        marked_price: None,
                        max_discount: None,
                        supplier_id: None,
                        reference: format!("Opening stock (import, row {row_no})"),
                        date_received: s(&d, "as_of").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()),
                        activate: false,
                        kind: Some("opening".into()),
                    };
                    super::stock::receive_in_tx(conn, ctx, &cfg, &mut body).await?;
                    sqlx::query(
                        "UPDATE stock_movements SET import_batch_id = $1 WHERE tenant_id = $2 AND product_id = $3 AND branch_id = $4
                           AND kind = 'opening' AND import_batch_id IS NULL AND created_at = now()",
                    )
                    .bind(id)
                    .bind(ctx.tenant_id)
                    .bind(product_id)
                    .bind(branch_id)
                    .execute(&mut *conn)
                    .await?;
                    quantity += qty as i64;
                    amount += money(&d, "unit_cost").unwrap_or(Decimal::ZERO) * Decimal::from(qty);
                    Some(product_id)
                }
            }
            "expenses" => {
                let cat = s_req(&d, "category")?;
                let cat_id = find_or_create(conn, ctx.tenant_id, "expense_categories", &cat).await?;
                let value = money(&d, "amount").ok_or_else(|| bad("Amount"))?;
                let desc = match s(&d, "reference") {
                    Some(r) => format!("{} [{r}]", s(&d, "description").unwrap_or("")).trim().to_string(),
                    None => s(&d, "description").unwrap_or("").to_string(),
                };
                let eid: Uuid = sqlx::query_scalar(
                    "INSERT INTO expenses (tenant_id, branch_id, category_id, amount, expense_date, description, payee, payment_method, status, user_id, import_batch_id)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'approved',$9,$10) RETURNING id",
                )
                .bind(ctx.tenant_id)
                .bind(uuid_of(&d, "branch_id")?)
                .bind(cat_id)
                .bind(value)
                .bind(s(&d, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Expense Date"))?)
                .bind(desc)
                .bind(s(&d, "payee").unwrap_or(""))
                .bind(s(&d, "method_key").unwrap_or("cash"))
                .bind(uploader)
                .bind(id)
                .fetch_one(&mut *conn)
                .await?;
                amount += value;
                created += 1;
                Some(eid)
            }
            "credit" => {
                let (cs, value) = post_credit(conn, ctx, &cfg, id, uploader, &d).await?;
                amount += value;
                created += 1;
                Some(cs)
            }
            "loyalty" => {
                let customer = uuid_of(&d, "customer_id")?;
                let at = noon(ctx, s(&d, "effective").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Effective Date"))?);
                let expiry = s(&d, "expiry").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok());
                let int = |k: &str| d.get(k).and_then(|v| v.as_i64());
                let balance = int("balance").unwrap_or(0);
                let (earned, redeemed) = match (int("earned"), int("redeemed")) {
                    (Some(e), Some(rd)) => (e, rd),
                    _ => (balance, 0),
                };
                let note = s(&d, "notes").map(|n| format!(" — {n}")).unwrap_or_default();
                if earned > 0 {
                    sqlx::query(
                        "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, expires_at, notes, user_id, created_at, import_batch_id)
                         VALUES ($1,$2,'adjust',$3,$4,$5,$6,$7,$8)",
                    )
                    .bind(ctx.tenant_id)
                    .bind(customer)
                    .bind(earned)
                    .bind(expiry)
                    .bind(format!("Opening points (earned before S'Shop){note}"))
                    .bind(uploader)
                    .bind(at)
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
                }
                if redeemed > 0 {
                    sqlx::query(
                        "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, notes, user_id, created_at, import_batch_id)
                         VALUES ($1,$2,'redeem',$3,'Opening points (redeemed before S''Shop)',$4,$5,$6)",
                    )
                    .bind(ctx.tenant_id)
                    .bind(customer)
                    .bind(-redeemed)
                    .bind(uploader)
                    .bind(at)
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
                }
                sqlx::query(
                    "UPDATE customers SET own_points = own_points + $2, points_redeemed = points_redeemed + $3,
                            points_available = points_available + $2 - $3, updated_at = now() WHERE id = $1",
                )
                .bind(customer)
                .bind(earned)
                .bind(redeemed)
                .execute(&mut *conn)
                .await?;
                quantity += balance;
                created += 1;
                Some(customer)
            }
            _ => return Err(bad("Unknown dataset")),
        };
        sqlx::query("UPDATE import_rows SET status = 'posted', target_id = $3 WHERE batch_id = $1 AND row_no = $2")
            .bind(id)
            .bind(row_no)
            .bind(target)
            .execute(&mut *conn)
            .await?;
        posted += 1;
        p.step(1).await;
    }
    p.stage("finalizing").await;
    let skipped = sqlx::query("UPDATE import_rows SET status = 'skipped' WHERE batch_id = $1 AND status = 'invalid'").bind(id).execute(&mut *conn).await?.rows_affected();
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM import_rows WHERE batch_id = $1").bind(id).fetch_one(&mut *conn).await?;
    // Updating existing products changes them; matching an existing customer leaves them as they were (not duplicated).
    let updated = if dataset == "products" && mode == "update" { std::mem::take(&mut matched) } else { 0 };
    let summary = json!({
        "total": total, "posted": posted, "created": created, "updated": updated, "matched": matched, "skipped": skipped,
        "failed": 0, "quantity": quantity, "amount": amount,
    });
    sqlx::query("UPDATE import_batches SET status = 'posted', posted_rows = $2, posted_at = now(), summary = $3, file = '' WHERE id = $1")
        .bind(id)
        .bind(posted)
        .bind(&summary)
        .execute(&mut *conn)
        .await?;
    audit::record(conn, ctx, Entry::new("onboarding", "posted", "import_batch", id).after(json!({ "dataset": dataset, "mode": mode, "summary": summary }))).await?;
    Ok(summary)
}

/// A historical date as a moment: midday in the business's time zone, so the business day never shifts.
fn noon(ctx: &Ctx, d: NaiveDate) -> chrono::DateTime<Utc> {
    use chrono::TimeZone;
    ctx.tz.from_local_datetime(&d.and_hms_opt(12, 0, 0).expect("noon")).single().map(|t| t.with_timezone(&Utc)).unwrap_or_else(|| d.and_hms_opt(9, 0, 0).expect("9am").and_utc())
}

/// Historical sales, one per receipt reference: original date and receipt number, lines, payment and the customer's
/// totals — never stock movements or loyalty points (`is_legacy`). Returns (sales created, value).
async fn post_sales(conn: &mut PgConnection, ctx: &Ctx, cfg: &settings::TenantSettings, batch: Uuid, uploader: Option<Uuid>, rows: &[(i32, Value)], p: &mut Progress) -> AppResult<(i32, Decimal)> {
    let mut order: Vec<String> = Vec::new();
    let mut by_receipt: HashMap<String, Vec<(i32, Map<String, Value>)>> = HashMap::new();
    for (row_no, v) in rows {
        let d = v.as_object().cloned().unwrap_or_default();
        let rf = s_req(&d, "receipt")?;
        if !by_receipt.contains_key(&rf) {
            order.push(rf.clone());
        }
        by_receipt.entry(rf).or_default().push((*row_no, d));
    }
    let mut value = Decimal::ZERO;
    for rf in &order {
        let lines = &by_receipt[rf];
        let head = &lines[0].1;
        let at = noon(ctx, s(head, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Sale Date"))?);
        let branch = uuid_of(head, "branch_id")?;
        let customer = s(head, "customer_id").and_then(|x| Uuid::parse_str(x).ok());
        let owner = s(head, "owner_id").and_then(|x| Uuid::parse_str(x).ok());
        let method = s_req(head, "method_key")?;
        let mut gross = Decimal::ZERO;
        let mut discount = Decimal::ZERO;
        let mut total = Decimal::ZERO;
        let mut items = Vec::new();
        for (_, d) in lines {
            let qty = d.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0).max(1);
            let unit = money(d, "unit_price").unwrap_or_default();
            let disc = money(d, "discount").unwrap_or_default();
            let line = (unit * Decimal::from(qty)).round_dp(2);
            let marked = ((line + disc) / Decimal::from(qty)).round_dp(2);
            gross += line + disc;
            discount += disc;
            total += line;
            items.push((uuid_of(d, "product_id")?, qty as i32, marked, unit, line));
        }
        let notes = match s(head, "notes") {
            Some(n) => format!("Imported sales history — {n}"),
            None => "Imported sales history".to_string(),
        };
        let sale: Uuid = sqlx::query_scalar(
            "INSERT INTO sales (tenant_id, branch_id, receipt_no, customer_id, user_id, owner_id, status, gross_total, discount_total, total,
                    amount_paid, payment_method, points_earned, is_legacy, notes, created_at, import_batch_id)
             VALUES ($1,$2,$3,$4,$5,$6,'completed',$7,$8,$9,$9,$10,0,true,$11,$12,$13) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(branch)
        .bind(rf)
        .bind(customer)
        .bind(uploader)
        .bind(owner)
        .bind(gross)
        .bind(discount)
        .bind(total)
        .bind(&method)
        .bind(notes)
        .bind(at)
        .bind(batch)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => rule(format!("Receipt {rf} already exists")),
            _ => e.into(),
        })?;
        for (product, qty, marked, unit, line) in items {
            sqlx::query("INSERT INTO sale_items (tenant_id, sale_id, product_id, quantity, marked_price, unit_price, line_total) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(ctx.tenant_id)
                .bind(sale)
                .bind(product)
                .bind(qty)
                .bind(marked)
                .bind(unit)
                .bind(line)
                .execute(&mut *conn)
                .await?;
        }
        if total > Decimal::ZERO {
            sqlx::query("INSERT INTO payments (tenant_id, branch_id, sale_id, method, amount, reference, user_id, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(ctx.tenant_id)
                .bind(branch)
                .bind(sale)
                .bind(&method)
                .bind(total)
                .bind(s(head, "payment_reference").unwrap_or(""))
                .bind(uploader)
                .bind(at)
                .execute(&mut *conn)
                .await?;
        }
        if let Some(c) = customer {
            crate::loyalty::record_past_purchase(conn, cfg, c, total, at).await?;
        }
        for (row_no, _) in lines {
            sqlx::query("UPDATE import_rows SET status = 'posted', target_id = $3 WHERE batch_id = $1 AND row_no = $2").bind(batch).bind(row_no).bind(sale).execute(&mut *conn).await?;
        }
        p.step(lines.len() as i32).await;
        value += total;
    }
    Ok((order.len() as i32, value))
}

/// Rows grouped by a reference, in file order.
fn group(rows: &[(i32, Value)], key: &str) -> AppResult<Vec<(String, Vec<(i32, Map<String, Value>)>)>> {
    let mut out: Vec<(String, Vec<(i32, Map<String, Value>)>)> = Vec::new();
    for (row_no, v) in rows {
        let d = v.as_object().cloned().unwrap_or_default();
        let rf = s_req(&d, key)?;
        match out.iter_mut().find(|(k, _)| *k == rf) {
            Some((_, lines)) => lines.push((*row_no, d)),
            None => out.push((rf, vec![(*row_no, d)])),
        }
    }
    Ok(out)
}

async fn mark_posted(conn: &mut PgConnection, batch: Uuid, lines: &[(i32, Map<String, Value>)], target: Uuid) -> AppResult<()> {
    for (row_no, _) in lines {
        sqlx::query("UPDATE import_rows SET status = 'posted', target_id = $3 WHERE batch_id = $1 AND row_no = $2").bind(batch).bind(row_no).bind(target).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Orders: history (closed: no sale, no stock held) or outstanding (into Orders with the original date; confirmed
/// stages hold stock like the live flow). No messages are sent. Returns (orders, value, units held).
async fn post_orders(conn: &mut PgConnection, ctx: &Ctx, cfg: &settings::TenantSettings, batch: Uuid, uploader: Option<Uuid>, rows: &[(i32, Value)], p: &mut Progress) -> AppResult<(i32, Decimal, i64)> {
    let groups = group(rows, "order")?;
    let mut value = Decimal::ZERO;
    let mut held = 0i64;
    for (rf, lines) in &groups {
        let head = &lines[0].1;
        let branch = uuid_of(head, "branch_id")?;
        let reserve = head.get("reserve").and_then(|v| v.as_bool()).unwrap_or(false);
        let closed = head.get("closed").and_then(|v| v.as_bool()).unwrap_or(false);
        let at = noon(ctx, s(head, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Order Date"))?);
        let total: Decimal = lines.iter().map(|(_, d)| money(d, "unit_price").unwrap_or_default() * Decimal::from(d.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0))).sum();
        let notes = format!("{}{}", if closed { "Imported order history" } else { "Imported outstanding order" }, s(head, "notes").map(|n| format!(" — {n}")).unwrap_or_default());
        let order: Uuid = sqlx::query_scalar(
            "INSERT INTO orders (tenant_id, branch_id, order_no, customer_id, status, source, delivery_location, notes, total, reserved, created_by, created_at, updated_at, import_batch_id, is_legacy)
             VALUES ($1,$2,$3,$4,$5,'internal',$6,$7,$8,$9,$10,$11,$11,$12,$13) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(branch)
        .bind(rf)
        .bind(uuid_of(head, "customer_id")?)
        .bind(s_req(head, "status")?)
        .bind(s(head, "delivery").unwrap_or(""))
        .bind(notes)
        .bind(total.round_dp(2))
        .bind(reserve)
        .bind(uploader)
        .bind(at)
        .bind(batch)
        .bind(closed)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => rule(format!("Order {rf} already exists")),
            _ => e.into(),
        })?;
        for (_, d) in lines {
            let product = uuid_of(d, "product_id")?;
            let qty = d.get("quantity").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
            let price = money(d, "unit_price").unwrap_or_default();
            sqlx::query("INSERT INTO order_items (order_id, product_id, quantity, unit_price, line_total) VALUES ($1,$2,$3,$4,$5)")
                .bind(order)
                .bind(product)
                .bind(qty)
                .bind(price)
                .bind((price * Decimal::from(qty)).round_dp(2))
                .execute(&mut *conn)
                .await?;
            if reserve {
                crate::inventory::reserve(conn, ctx.tenant_id, branch, product, qty, cfg.stock.allow_negative).await?;
                held += qty as i64;
            }
        }
        mark_posted(conn, batch, lines, order).await?;
        p.step(lines.len() as i32).await;
        value += total;
    }
    Ok((groups.len() as i32, value, held))
}

/// Transfers: received / cancelled history (no stock moves: opening stock already reflects it), in transit (sent
/// before the cutover; receiving it in S'Shop adds it at the destination) or pending (a draft to dispatch).
/// Returns (transfers, 0, units).
async fn post_transfers(conn: &mut PgConnection, ctx: &Ctx, batch: Uuid, uploader: Option<Uuid>, rows: &[(i32, Value)], p: &mut Progress) -> AppResult<(i32, Decimal, i64)> {
    let groups = group(rows, "transfer")?;
    let mut units = 0i64;
    for (rf, lines) in &groups {
        let head = &lines[0].1;
        let status = s_req(head, "status")?;
        let date = s(head, "date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Transfer Date"))?;
        let at = noon(ctx, date);
        let received_at = s(head, "received_date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).map(|d| noon(ctx, d));
        let sent = matches!(status.as_str(), "received" | "dispatched");
        let notes = format!(
            "{}{}",
            match status.as_str() {
                "dispatched" => "Imported in transit (sent before S'Shop)",
                "draft" => "Imported pending transfer",
                _ => "Imported transfer history",
            },
            s(head, "notes").map(|n| format!(" — {n}")).unwrap_or_default()
        );
        let transfer: Uuid = sqlx::query_scalar(
            "INSERT INTO transfers (tenant_id, transfer_no, from_branch_id, to_branch_id, status, transfer_date, notes, created_by,
                    dispatched_by, dispatched_at, received_by, received_at, created_at, import_batch_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(rf)
        .bind(uuid_of(head, "from_id")?)
        .bind(uuid_of(head, "to_id")?)
        .bind(&status)
        .bind(date)
        .bind(notes)
        .bind(uploader)
        .bind(if sent { uploader } else { None })
        .bind(if sent { Some(at) } else { None })
        .bind(if status == "received" { uploader } else { None })
        .bind(if status == "received" { received_at } else { None })
        .bind(at)
        .bind(batch)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => rule(format!("Transfer {rf} already exists")),
            _ => e.into(),
        })?;
        for (_, d) in lines {
            let qty = d.get("dispatched").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
            let got = if status == "received" { d.get("received").and_then(|v| v.as_i64()).map(|g| g as i32).unwrap_or(qty) } else { qty };
            sqlx::query("INSERT INTO transfer_items (transfer_id, product_id, quantity, short_qty) VALUES ($1,$2,$3,$4)")
                .bind(transfer)
                .bind(uuid_of(d, "product_id")?)
                .bind(qty)
                .bind((qty - got).max(0))
                .execute(&mut *conn)
                .await?;
            units += qty as i64;
        }
        mark_posted(conn, batch, lines, transfer).await?;
        p.step(lines.len() as i32).await;
    }
    Ok((groups.len() as i32, Decimal::ZERO, units))
}

/// A historical credit sale (or an Opening Credit Balance without item history): the sale, its credit with the
/// original due date and what was paid before S'Shop. Never stock or loyalty. Returns (credit id, original amount).
async fn post_credit(conn: &mut PgConnection, ctx: &Ctx, cfg: &settings::TenantSettings, batch: Uuid, uploader: Option<Uuid>, d: &Map<String, Value>) -> AppResult<(Uuid, Decimal)> {
    let rf = s_req(d, "reference")?;
    let branch = uuid_of(d, "branch_id")?;
    let customer = uuid_of(d, "customer_id")?;
    let owner = s(d, "owner_id").and_then(|x| Uuid::parse_str(x).ok());
    let at = noon(ctx, s(d, "sale_date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Original Sale Date"))?);
    let due = s(d, "due_date").and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()).ok_or_else(|| bad("Due Date"))?;
    let original = money(d, "original_amount").ok_or_else(|| bad("Original Amount"))?;
    let paid = money(d, "amount_paid").unwrap_or(Decimal::ZERO);
    let product = s(d, "product_id").and_then(|x| Uuid::parse_str(x).ok());
    let opening = product.is_none();
    let notes = format!(
        "{}{}",
        if opening { "Opening credit balance (imported)" } else { "Imported credit sale" },
        s(d, "notes").map(|n| format!(" — {n}")).unwrap_or_default()
    );
    let sale: Uuid = sqlx::query_scalar(
        "INSERT INTO sales (tenant_id, branch_id, receipt_no, customer_id, user_id, owner_id, status, gross_total, discount_total, total,
                amount_paid, payment_method, points_earned, is_legacy, notes, created_at, import_batch_id)
         VALUES ($1,$2,$3,$4,$5,$6,'completed',$7,0,$7,$8,'credit',0,true,$9,$10,$11) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(&rf)
    .bind(customer)
    .bind(uploader)
    .bind(owner)
    .bind(original)
    .bind(paid)
    .bind(&notes)
    .bind(at)
    .bind(batch)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => rule(format!("Receipt {rf} already exists")),
        _ => e.into(),
    })?;
    if let Some(p) = product {
        let qty = d.get("quantity").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
        let unit = (original / Decimal::from(qty)).round_dp(2);
        sqlx::query("INSERT INTO sale_items (tenant_id, sale_id, product_id, quantity, marked_price, unit_price, line_total) VALUES ($1,$2,$3,$4,$5,$5,$6)")
            .bind(ctx.tenant_id)
            .bind(sale)
            .bind(p)
            .bind(qty as i32)
            .bind(unit)
            .bind(original)
            .execute(&mut *conn)
            .await?;
    }
    let status = if paid >= original { "paid" } else if paid > Decimal::ZERO { "partially_paid" } else { "outstanding" };
    let credit: Uuid = sqlx::query_scalar(
        "INSERT INTO credit_sales (tenant_id, branch_id, sale_id, customer_id, user_id, original_amount, amount_paid, due_date, status, created_at, import_batch_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(branch)
    .bind(sale)
    .bind(customer)
    .bind(owner.or(uploader))
    .bind(original)
    .bind(paid)
    .bind(due)
    .bind(status)
    .bind(at)
    .bind(batch)
    .fetch_one(&mut *conn)
    .await?;
    if paid > Decimal::ZERO {
        // What was paid before S'Shop: recorded once, as paid before the cutover (not a new payment).
        sqlx::query(
            "INSERT INTO payments (tenant_id, branch_id, sale_id, credit_sale_id, method, amount, reference, user_id, created_at)
             VALUES ($1,$2,$3,$4,'legacy',$5,'Paid before S''Shop (imported)',$6,$7)",
        )
        .bind(ctx.tenant_id)
        .bind(branch)
        .bind(sale)
        .bind(credit)
        .bind(paid)
        .bind(uploader)
        .bind(at)
        .execute(&mut *conn)
        .await?;
    }
    crate::loyalty::record_past_purchase(conn, cfg, customer, original, at).await?;
    Ok((credit, original))
}

fn s_req(d: &Map<String, Value>, k: &str) -> AppResult<String> {
    s(d, k).map(String::from).ok_or_else(|| bad(format!("{k} missing")))
}

fn uuid_of(d: &Map<String, Value>, k: &str) -> AppResult<Uuid> {
    s(d, k).and_then(|v| Uuid::parse_str(v).ok()).ok_or_else(|| bad(format!("{k} missing")))
}

/// A category / supplier / expense category by name, created when new.
async fn find_or_create(conn: &mut PgConnection, tenant: Uuid, table: &str, name: &str) -> AppResult<Uuid> {
    let name = name.trim();
    let found: Option<Uuid> = sqlx::query_scalar(&format!("SELECT id FROM {table} WHERE tenant_id = $1 AND lower(name) = lower($2) LIMIT 1"))
        .bind(tenant)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(id) = found {
        return Ok(id);
    }
    Ok(sqlx::query_scalar(&format!("INSERT INTO {table} (tenant_id, name) VALUES ($1, $2) RETURNING id")).bind(tenant).bind(name).fetch_one(&mut *conn).await?)
}

/// Creates a product (S'Shop's numbering when no code is given) or updates an existing one by code.
async fn post_product(conn: &mut PgConnection, ctx: &Ctx, batch: Uuid, mode: &str, d: &Map<String, Value>) -> AppResult<(Uuid, bool)> {
    let cat = find_or_create(conn, ctx.tenant_id, "categories", &s_req(d, "category")?).await?;
    let sup = match s(d, "supplier") {
        Some(n) => Some(find_or_create(conn, ctx.tenant_id, "suppliers", n).await?),
        None => None,
    };
    let all = d.get("branch_ids").is_none_or(|v| v.is_null());
    let branch_ids: Vec<Uuid> = d.get("branch_ids").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let individual = d.get("individual").and_then(|v| v.as_bool()).unwrap_or(false);
    let barcode = s(d, "barcode").map(String::from);
    let id: Uuid;
    let new;
    if mode == "update" {
        let code = s_req(d, "code")?;
        id = sqlx::query_scalar(
            "UPDATE products SET name = $3, nickname = COALESCE($4, nickname), description = COALESCE($5, description), category_id = $6,
                    supplier_id = COALESCE($7, supplier_id), marked_price = COALESCE($8, marked_price), cost_price = COALESCE($9, cost_price),
                    max_discount = COALESCE($10, max_discount), barcode = COALESCE($11, barcode), low_stock_threshold = COALESCE($12, low_stock_threshold),
                    is_active = $13, available_for_orders = $14, all_branches = $15, updated_at = now()
             WHERE tenant_id = $1 AND upper(code) = $2 RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(&code)
        .bind(s_req(d, "name")?)
        .bind(s(d, "nickname"))
        .bind(s(d, "description"))
        .bind(cat)
        .bind(sup)
        .bind(money(d, "selling_price"))
        .bind(money(d, "cost_price"))
        .bind(money(d, "max_discount"))
        .bind(&barcode)
        .bind(d.get("low_stock").and_then(|v| v.as_i64()).map(|v| v as i32))
        .bind(d.get("active").and_then(|v| v.as_bool()).unwrap_or(true))
        .bind(d.get("publish_online").and_then(|v| v.as_bool()).unwrap_or(true))
        .bind(all)
        .fetch_one(&mut *conn)
        .await?;
        new = false;
    } else {
        let code = match s(d, "code") {
            Some(c) => c.to_uppercase(),
            None => {
                let prefix = settings::load(conn, ctx.tenant_id).await?.product.auto_code_prefix;
                let n: i64 = sqlx::query_scalar(
                    "INSERT INTO doc_counters (tenant_id, kind, year, value) VALUES ($1, 'product_code', 0, 1)
                     ON CONFLICT (tenant_id, kind, year) DO UPDATE SET value = doc_counters.value + 1 RETURNING value",
                )
                .bind(ctx.tenant_id)
                .fetch_one(&mut *conn)
                .await?;
                format!("{prefix}{n:04}")
            }
        };
        id = sqlx::query_scalar(
            "INSERT INTO products (tenant_id, code, name, nickname, description, category_id, supplier_id, marked_price, max_discount, cost_price,
                    barcode, track_items, is_active, available_for_orders, low_stock_threshold, all_branches, created_by, import_batch_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(&code)
        .bind(s_req(d, "name")?)
        .bind(s(d, "nickname").unwrap_or(""))
        .bind(s(d, "description").unwrap_or(""))
        .bind(cat)
        .bind(sup)
        .bind(money(d, "selling_price"))
        .bind(money(d, "max_discount"))
        .bind(money(d, "cost_price"))
        .bind(&barcode)
        .bind(individual)
        .bind(d.get("active").and_then(|v| v.as_bool()).unwrap_or(true))
        .bind(d.get("publish_online").and_then(|v| v.as_bool()).unwrap_or(true))
        .bind(d.get("low_stock").and_then(|v| v.as_i64()).map(|v| v as i32))
        .bind(all)
        .bind(ctx.user_id)
        .bind(batch)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => rule(format!("Product code {code} or its barcode is already used")),
            _ => e.into(),
        })?;
        new = true;
    }
    // Catalogue branches (stock is never created here).
    sqlx::query("DELETE FROM product_branches WHERE product_id = $1").bind(id).execute(&mut *conn).await?;
    if !all {
        for b in branch_ids {
            sqlx::query("INSERT INTO product_branches (product_id, branch_id) VALUES ($1, $2)").bind(id).bind(b).execute(&mut *conn).await?;
        }
    }
    Ok((id, new))
}
