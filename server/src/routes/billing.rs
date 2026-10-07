//! Billing (roadmap 37–40).
//!
//! * `/billing/*` — a business's own billing (Settings → Billing, permission `settings.billing`): plan, status,
//!   invoices, quotations, payments and receipts, the vendor's bank details (masked) and *Pay now* through Paystack.
//!   Every query is scoped to the caller's own business.
//! * `/platform/billing/*`, `/platform/tenants/{id}/billing-*` — the platform owner: billing plans, quotations,
//!   invoices, payments received outside S'Shop, the billing dashboard and the vendor details.
//! * `/webhooks/paystack` — signed Paystack events; the server re-verifies every payment with Paystack.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::access::require_platform_admin;
use super::platform::record_platform;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::billing::{self, FREQUENCIES};
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/billing", get(my_billing))
        .route("/billing/documents/{id}", get(my_document))
        .route("/billing/invoices/{id}/pay", post(pay))
        .route("/billing/paystack/verify", post(verify_return))
        .route("/billing/quotations/{id}/accept", post(accept_quotation))
        .route("/platform/billing", get(dashboard))
        .route("/platform/billing/vendor", get(vendor_get).put(vendor_put))
        .route("/platform/tenants/{id}/billing-plan", put(save_plan))
        .route("/platform/tenants/{id}/billing-documents", post(issue_document))
        .route("/platform/billing/documents/{id}", get(platform_document))
        .route("/platform/billing/documents/{id}/void", post(void_document))
        .route("/platform/billing/documents/{id}/invoice", post(invoice_quotation))
        .route("/platform/billing/documents/{id}/payments", post(record_payment))
        .route("/platform/billing/payments/{id}/verify", post(platform_verify))
        .route("/webhooks/paystack", post(paystack_webhook))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DocumentRow {
    id: Uuid,
    kind: String,
    number: String,
    category: String,
    description: String,
    amount: Decimal,
    currency: String,
    issue_date: NaiveDate,
    due_date: NaiveDate,
    period_start: Option<NaiveDate>,
    period_end: Option<NaiveDate>,
    status: String,
    quotation_id: Option<Uuid>,
    paid_at: Option<DateTime<Utc>>,
    void_reason: String,
    created_at: DateTime<Utc>,
    /// open invoice past its due date + grace period.
    overdue: bool,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    id: Uuid,
    invoice_id: Uuid,
    invoice_number: String,
    amount: Decimal,
    currency: String,
    method: String,
    reference: String,
    status: String,
    channel: String,
    receipt_no: Option<String>,
    paid_at: Option<DateTime<Utc>>,
    note: String,
    created_at: DateTime<Utc>,
}

const DOCUMENTS: &str = "SELECT d.id, d.kind, d.number, d.category, d.description, d.amount, d.currency, d.issue_date, d.due_date,
        d.period_start, d.period_end, d.status, d.quotation_id, d.paid_at, d.void_reason, d.created_at,
        (d.kind = 'invoice' AND d.status = 'open' AND d.due_date + COALESCE(p.grace_days, 0) < $2) AS overdue
 FROM billing_documents d LEFT JOIN billing_plans p ON p.tenant_id = d.tenant_id WHERE d.tenant_id = $1";

const PAYMENTS: &str = "SELECT b.id, b.invoice_id, d.number AS invoice_number, b.amount, b.currency, b.method, b.reference, b.status, b.channel,
        b.receipt_no, b.paid_at, b.note, b.created_at
 FROM billing_payments b JOIN billing_documents d ON d.id = b.invoice_id WHERE b.tenant_id = $1";

async fn documents(state: &AppState, tenant_id: Uuid) -> AppResult<Vec<DocumentRow>> {
    Ok(sqlx::query_as(&format!("{DOCUMENTS} ORDER BY d.created_at DESC LIMIT 200"))
        .bind(tenant_id)
        .bind(billing::today())
        .fetch_all(&state.db)
        .await?)
}

async fn payments(state: &AppState, tenant_id: Uuid, all: bool) -> AppResult<Vec<PaymentRow>> {
    // Businesses see payments that went through (and ones still being confirmed); failed attempts stay with the platform.
    let filter = if all { "" } else { " AND b.status IN ('success', 'pending')" };
    Ok(sqlx::query_as(&format!("{PAYMENTS}{filter} ORDER BY b.created_at DESC LIMIT 200")).bind(tenant_id).fetch_all(&state.db).await?)
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct Vendor {
    #[serde(default)]
    bank_name: String,
    #[serde(default)]
    account_name: String,
    #[serde(default)]
    account_number: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    instructions: String,
}

async fn vendor(state: &AppState) -> AppResult<Vendor> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = 'vendor'").fetch_optional(&state.db).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}

/// Bank details as businesses see them: the account number masked to its last three digits.
fn vendor_public(v: &Vendor) -> Value {
    let digits: String = v.account_number.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let tail: String = digits.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
    json!({
        "bank_name": v.bank_name, "account_name": v.account_name, "branch": v.branch, "instructions": v.instructions,
        "account_masked": if tail.is_empty() { String::new() } else { format!("•••{tail}") },
    })
}

/// The plan as the business sees it (no internal notes).
fn plan_public(p: &billing::Plan) -> Value {
    json!({
        "model": p.model, "currency": p.currency, "one_off_amount": p.one_off_amount, "recurring": p.recurring, "amount": p.amount,
        "frequency": p.frequency, "custom_months": p.custom_months, "start_date": p.start_date, "next_due_date": p.next_due_date,
        "grace_days": p.grace_days, "auto_renew": p.auto_renew,
    })
}

// ───────────────────────────── The business's own billing ─────────────────────────────

async fn my_billing(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("settings.billing")?;
    let mut conn = state.db.acquire().await?;
    let summary = billing::summary(&mut conn, ctx.tenant_id).await?;
    let plan = billing::plan(&mut conn, ctx.tenant_id).await?;
    drop(conn);
    Ok(Json(json!({
        "summary": summary,
        "plan": plan.as_ref().map(plan_public),
        "documents": documents(&state, ctx.tenant_id).await?,
        "payments": payments(&state, ctx.tenant_id, false).await?,
        "vendor": vendor_public(&vendor(&state).await?),
        "paystack": state.cfg.paystack.is_some(),
    })))
}

/// One quotation / invoice with its payments and both parties' details, for viewing and the downloadable PDF
/// (invoice, quotation or receipt — built in the browser like sales receipts).
async fn my_document(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("settings.billing")?;
    document_view(&state, ctx.tenant_id, id, false).await.map(Json)
}

async fn document_view(state: &AppState, tenant_id: Uuid, id: Uuid, all_payments: bool) -> AppResult<Value> {
    let doc: DocumentRow = sqlx::query_as(&format!("{DOCUMENTS} AND d.id = $3"))
        .bind(tenant_id)
        .bind(billing::today())
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Billing document"))?;
    let filter = if all_payments { "" } else { " AND b.status IN ('success', 'pending')" };
    let pays: Vec<PaymentRow> = sqlx::query_as(&format!("{PAYMENTS} AND b.invoice_id = $2{filter} ORDER BY b.created_at"))
        .bind(tenant_id)
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let business: (String, String, String, String) = sqlx::query_as("SELECT name, phone, email, address FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await?;
    Ok(json!({
        "document": doc, "payments": pays,
        "business": { "name": business.0, "phone": business.1, "email": business.2, "address": business.3 },
        "vendor": vendor_public(&vendor(state).await?),
        "support_phones": super::access::SUPPORT_PHONES,
    }))
}

/// *Pay now*: a Paystack checkout for exactly one open invoice of the caller's business. The amount comes from
/// the invoice on the server; nothing the browser sends decides what is paid.
async fn pay(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("settings.billing")?;
    let cfg = state.cfg.paystack.as_ref().ok_or_else(|| refused("Online payment unavailable", "Online payments are not set up yet — pay by bank transfer using the details shown"))?;
    let (number, amount, currency, status): (String, Decimal, String, String) = sqlx::query_as(
        "SELECT number, amount, currency, status FROM billing_documents WHERE id = $1 AND tenant_id = $2 AND kind = 'invoice'",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Invoice"))?;
    if status != "open" {
        return Err(rule(if status == "paid" { "This invoice is already paid" } else { "This invoice is no longer payable" }));
    }
    // An earlier checkout for this invoice may have gone through: confirm it before opening another.
    let earlier: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM billing_payments WHERE invoice_id = $1 AND tenant_id = $2 AND method = 'paystack' AND status = 'pending'")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_all(&state.db)
        .await?;
    for p in earlier {
        match billing::verify_paystack(&state, p).await {
            Ok(s) if s == "success" => return Err(refused("Already paid", "A payment for this invoice has just been confirmed")),
            Ok(s) if s == "pending" => {
                sqlx::query("UPDATE billing_payments SET status = 'abandoned' WHERE id = $1 AND status = 'pending'").bind(p).execute(&state.db).await?;
            }
            _ => {}
        }
    }
    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1").bind(ctx.user_id).fetch_one(&state.db).await?;
    let reference = format!("SSB-{}", Uuid::new_v4().simple().to_string()[..20].to_uppercase());
    let mut tx = state.db.begin().await?;
    let pid: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_payments (tenant_id, invoice_id, amount, currency, method, reference, recorded_by) VALUES ($1,$2,$3,$4,'paystack',$5,$6) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(amount)
    .bind(&currency)
    .bind(&reference)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    let checkout = crate::integrations::paystack::initialize(
        &state.http,
        cfg,
        &email,
        amount,
        &currency,
        &reference,
        &format!("{}/settings/billing", state.cfg.public_url),
        json!({ "invoice": number, "invoice_id": id, "tenant_id": ctx.tenant_id, "payment_id": pid }),
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "paystack initialize");
        refused("Payment not started", "Paystack could not start the payment — please try again")
    })?;
    audit::record(&mut tx, &ctx, Entry::new("billing", "payment_started", "billing_payment", pid).after(json!({ "invoice": number, "amount": amount, "reference": reference }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "authorization_url": checkout.authorization_url, "access_code": checkout.access_code, "reference": reference })))
}

#[derive(Deserialize)]
struct VerifyBody {
    reference: String,
}

/// Back from Paystack: the server asks Paystack what happened. The success screen is never proof.
async fn verify_return(State(state): State<AppState>, ctx: Ctx, Json(b): Json<VerifyBody>) -> AppResult<Json<Value>> {
    ctx.require("settings.billing")?;
    let row: Option<(Uuid, String)> = sqlx::query_as("SELECT id, status FROM billing_payments WHERE reference = $1 AND tenant_id = $2")
        .bind(b.reference.trim())
        .bind(ctx.tenant_id)
        .fetch_optional(&state.db)
        .await?;
    let (id, status) = row.ok_or(AppError::NotFound("Payment"))?;
    let status = if status == "success" { status } else { billing::verify_paystack(&state, id).await? };
    let receipt: Option<String> = sqlx::query_scalar("SELECT receipt_no FROM billing_payments WHERE id = $1").bind(id).fetch_one(&state.db).await?;
    Ok(Json(json!({ "status": status, "receipt_no": receipt })))
}

/// The business accepts a quotation: it becomes an invoice it can pay.
async fn accept_quotation(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("settings.billing")?;
    let mut tx = state.db.begin().await?;
    let (invoice_id, number) = quotation_to_invoice(&mut tx, ctx.tenant_id, id, Some(ctx.user_id)).await?;
    audit::record(&mut tx, &ctx, Entry::new("billing", "quotation_accepted", "billing_document", id).after(json!({ "invoice": number }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "invoice_id": invoice_id, "number": number })))
}

async fn quotation_to_invoice(tx: &mut sqlx::PgConnection, tenant_id: Uuid, id: Uuid, by: Option<Uuid>) -> AppResult<(Uuid, String)> {
    let q: Option<(String, String, String, Decimal, String, Option<NaiveDate>, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT status, category, description, amount, currency, period_start, period_end FROM billing_documents
         WHERE id = $1 AND tenant_id = $2 AND kind = 'quotation' FOR UPDATE",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, category, description, amount, currency, ps, pe) = q.ok_or(AppError::NotFound("Quotation"))?;
    if status != "open" {
        return Err(rule(format!("This quotation is already {status}")));
    }
    let today = billing::today();
    let grace: i32 = sqlx::query_scalar("SELECT grace_days FROM billing_plans WHERE tenant_id = $1").bind(tenant_id).fetch_optional(&mut *tx).await?.unwrap_or(7);
    let created = billing::insert_document(
        tx,
        billing::NewDocument {
            tenant_id,
            kind: "invoice",
            category: &category,
            description,
            amount,
            currency,
            issue_date: today,
            due_date: today + chrono::Duration::days(grace.max(1) as i64),
            period: ps.zip(pe),
            quotation_id: Some(id),
            created_by: by,
        },
    )
    .await?;
    sqlx::query("UPDATE billing_documents SET status = 'accepted' WHERE id = $1").bind(id).execute(&mut *tx).await?;
    Ok(created)
}

// ───────────────────────────── Platform owner ─────────────────────────────

/// Plan, summary, documents and every payment attempt of one business (platform tenant detail).
pub async fn platform_view(state: &AppState, tenant_id: Uuid) -> AppResult<Value> {
    let mut conn = state.db.acquire().await?;
    let plan = billing::plan(&mut conn, tenant_id).await?;
    let summary = billing::summary(&mut conn, tenant_id).await?;
    drop(conn);
    Ok(json!({
        "plan": plan, "summary": summary,
        "documents": documents(state, tenant_id).await?,
        "payments": payments(state, tenant_id, true).await?,
        "paystack": state.cfg.paystack.is_some(),
    }))
}

/// Platform billing dashboard (roadmap 40): businesses by status, revenue, maintenance due, with every business's
/// billing position for drill-down.
async fn dashboard(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let rows = super::platform::tenant_rows(&state, None).await?;
    let today = billing::today();
    let count = |f: &dyn Fn(&super::platform::TenantRow) -> bool| rows.iter().filter(|r| f(r)).count();
    let month_start = today.with_day(1).unwrap_or(today);
    let year_start = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today);
    let revenue: Vec<(String, Decimal, Decimal, Decimal)> = sqlx::query_as(
        "SELECT d.category,
                COALESCE(SUM(b.amount) FILTER (WHERE b.paid_at >= $1), 0),
                COALESCE(SUM(b.amount) FILTER (WHERE b.paid_at >= $2), 0),
                COALESCE(SUM(b.amount), 0)
         FROM billing_payments b JOIN billing_documents d ON d.id = b.invoice_id WHERE b.status = 'success' GROUP BY d.category",
    )
    .bind(month_start.and_hms_opt(0, 0, 0).map(|d| d.and_utc()))
    .bind(year_start.and_hms_opt(0, 0, 0).map(|d| d.and_utc()))
    .fetch_all(&state.db)
    .await?;
    let monthly_recurring: Option<Decimal> = sqlx::query_scalar(
        "SELECT SUM(p.amount / CASE p.frequency WHEN 'quarterly' THEN 3 WHEN 'semi_annual' THEN 6 WHEN 'annual' THEN 12
                                                WHEN 'custom' THEN p.custom_months ELSE 1 END)
         FROM billing_plans p JOIN tenants t ON t.id = p.tenant_id WHERE p.recurring AND t.status = 'active' AND p.model = 'subscription'",
    )
    .fetch_one(&state.db)
    .await?;
    let maintenance: (i64, Option<Decimal>) = sqlx::query_as(
        "SELECT COUNT(*), SUM(p.amount) FROM billing_plans p JOIN tenants t ON t.id = p.tenant_id
         WHERE p.model = 'one_off' AND p.recurring AND t.status = 'active' AND p.next_due_date <= $1",
    )
    .bind(today + chrono::Duration::days(30))
    .fetch_one(&state.db)
    .await?;
    let attention: Vec<PaymentAttention> = sqlx::query_as(
        "SELECT b.id, t.name AS business, d.number AS invoice_number, b.amount, b.reference, b.receipt_no, b.note, b.paid_at
         FROM billing_payments b JOIN billing_documents d ON d.id = b.invoice_id JOIN tenants t ON t.id = b.tenant_id
         WHERE b.status = 'success'
           AND (d.status = 'void' OR EXISTS (SELECT 1 FROM billing_payments o WHERE o.invoice_id = b.invoice_id AND o.status = 'success'
                                             AND (o.paid_at, o.id) < (b.paid_at, b.id)))
         ORDER BY b.paid_at DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await?;
    let by = |cat: &str, i: usize| revenue.iter().filter(|r| r.0 == cat).map(|r| [r.1, r.2, r.3][i]).sum::<Decimal>();
    let billable = |r: &super::platform::TenantRow| !r.is_demo;
    Ok(Json(json!({
        "counts": {
            "businesses": count(&|r| billable(r)),
            "active": count(&|r| billable(r) && r.status == "active"),
            "deactivated": count(&|r| billable(r) && r.status != "active"),
            "paid": count(&|r| billable(r) && r.billing.status == "paid"),
            "due_soon": count(&|r| billable(r) && r.billing.status == "due_soon"),
            "grace": count(&|r| billable(r) && r.billing.status == "grace"),
            "overdue": count(&|r| billable(r) && r.billing.status == "overdue"),
            "pending": count(&|r| billable(r) && r.billing.status == "pending"),
            "not_set": count(&|r| billable(r) && r.billing.status == "not_set"),
        },
        "revenue": {
            "subscription": { "month": by("subscription", 0), "year": by("subscription", 1), "all": by("subscription", 2) },
            "one_off": { "month": by("one_off", 0), "year": by("one_off", 1), "all": by("one_off", 2) },
            "maintenance": { "month": by("maintenance", 0), "year": by("maintenance", 1), "all": by("maintenance", 2) },
            "other": { "month": by("other", 0), "year": by("other", 1), "all": by("other", 2) },
            "monthly_recurring": monthly_recurring.map(crate::util::round2).unwrap_or_default(),
        },
        "outstanding": rows.iter().map(|r| r.billing.outstanding).sum::<Decimal>(),
        "maintenance_due": { "count": maintenance.0, "amount": maintenance.1.unwrap_or_default() },
        "attention": attention,
        "items": rows,
        "paystack": state.cfg.paystack.is_some(),
    })))
}

#[derive(Serialize, sqlx::FromRow)]
struct PaymentAttention {
    id: Uuid,
    business: String,
    invoice_number: String,
    amount: Decimal,
    reference: String,
    receipt_no: Option<String>,
    note: String,
    paid_at: Option<DateTime<Utc>>,
}

async fn vendor_get(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let v = vendor(&state).await?;
    Ok(Json(json!({ "vendor": v, "public": vendor_public(&v) })))
}

async fn vendor_put(State(state): State<AppState>, ctx: Ctx, Json(b): Json<Vendor>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let clip = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    let v = Vendor {
        bank_name: clip(&b.bank_name, 80),
        account_name: clip(&b.account_name, 120),
        account_number: clip(&b.account_number, 40),
        branch: clip(&b.branch, 80),
        instructions: clip(&b.instructions, 500),
    };
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO platform_settings (key, value, updated_by) VALUES ('vendor', $1, $2)
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()")
        .bind(json!(v))
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    // The full account number stays out of the audit trail.
    audit::record(&mut tx, &ctx, Entry::new("platform", "vendor_details", "platform_settings", ctx.tenant_id).after(vendor_public(&v))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "public": vendor_public(&v) })))
}

#[derive(Deserialize)]
struct PlanBody {
    model: String,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    one_off_amount: Decimal,
    /// One-off model: maintenance fee required.
    #[serde(default)]
    maintenance: bool,
    #[serde(default)]
    amount: Decimal,
    #[serde(default)]
    frequency: Option<String>,
    #[serde(default)]
    custom_months: Option<i32>,
    start_date: Option<NaiveDate>,
    next_due_date: Option<NaiveDate>,
    #[serde(default)]
    grace_days: Option<i32>,
    #[serde(default)]
    auto_renew: Option<bool>,
    #[serde(default)]
    notes: String,
}

/// Sets a business's billing model (roadmap 37).
async fn save_plan(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<PlanBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if !matches!(b.model.as_str(), "subscription" | "one_off") {
        return Err(bad("Billing model must be Subscription or One-off"));
    }
    let recurring = b.model == "subscription" || b.maintenance;
    let frequency = b.frequency.clone().unwrap_or_else(|| "monthly".into());
    if !FREQUENCIES.contains(&frequency.as_str()) {
        return Err(bad("Choose a billing frequency"));
    }
    let custom_months = b.custom_months.unwrap_or(1);
    if frequency == "custom" && !(1..=60).contains(&custom_months) {
        return Err(bad("A custom frequency is between 1 and 60 months"));
    }
    let grace = b.grace_days.unwrap_or(7);
    if !(0..=90).contains(&grace) {
        return Err(bad("Grace period must be 0–90 days"));
    }
    if b.model == "one_off" && b.one_off_amount <= Decimal::ZERO {
        return Err(bad("Enter the one-off amount"));
    }
    if recurring {
        let what = if b.model == "subscription" { "subscription" } else { "maintenance" };
        if b.amount <= Decimal::ZERO {
            return Err(bad(format!("Enter the {what} amount")));
        }
        if b.start_date.is_none() {
            return Err(bad(format!("Enter the {what} start date")));
        }
    }
    if b.one_off_amount < Decimal::ZERO || b.amount < Decimal::ZERO {
        return Err(bad("Amounts cannot be negative"));
    }
    let currency = b.currency.clone().unwrap_or_else(|| "KES".into()).trim().to_uppercase();
    if currency.len() != 3 {
        return Err(bad("Currency must be a 3-letter code"));
    }
    let next_due = if recurring { b.next_due_date.or(b.start_date) } else { None };
    let mut tx = state.db.begin().await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tenants WHERE id = $1)").bind(id).fetch_one(&mut *tx).await?;
    if !exists {
        return Err(AppError::NotFound("Business"));
    }
    let before = billing::plan(&mut tx, id).await?;
    sqlx::query(
        "INSERT INTO billing_plans (tenant_id, model, currency, one_off_amount, recurring, amount, frequency, custom_months, start_date,
                                    next_due_date, grace_days, auto_renew, notes, updated_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT (tenant_id) DO UPDATE SET model = EXCLUDED.model, currency = EXCLUDED.currency, one_off_amount = EXCLUDED.one_off_amount,
             recurring = EXCLUDED.recurring, amount = EXCLUDED.amount, frequency = EXCLUDED.frequency, custom_months = EXCLUDED.custom_months,
             start_date = EXCLUDED.start_date, next_due_date = EXCLUDED.next_due_date, grace_days = EXCLUDED.grace_days,
             auto_renew = EXCLUDED.auto_renew, notes = EXCLUDED.notes, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(id)
    .bind(&b.model)
    .bind(&currency)
    .bind(if b.model == "one_off" { b.one_off_amount } else { Decimal::ZERO })
    .bind(recurring)
    .bind(if recurring { b.amount } else { Decimal::ZERO })
    .bind(&frequency)
    .bind(custom_months)
    .bind(if recurring { b.start_date } else { None })
    .bind(next_due)
    .bind(grace)
    .bind(b.auto_renew.unwrap_or(true))
    .bind(b.notes.trim().chars().take(1000).collect::<String>())
    .bind(ctx.user_id)
    .execute(&mut *tx)
    .await?;
    let after = billing::plan(&mut tx, id).await?;
    record_platform(&mut tx, &ctx, id, || Entry::new("billing", "plan_updated", "tenant", id).before(before.clone()).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "plan": after })))
}

#[derive(Deserialize)]
struct DocumentBody {
    /// "quotation" | "invoice"
    kind: String,
    /// "next_period" (the plan's next recurring period) | "one_off" | "other"
    category: String,
    #[serde(default)]
    description: String,
    amount: Option<Decimal>,
    due_date: Option<NaiveDate>,
}

/// Issues a quotation or invoice to a business (roadmap 37).
async fn issue_document(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<DocumentBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if !matches!(b.kind.as_str(), "quotation" | "invoice") {
        return Err(bad("Choose quotation or invoice"));
    }
    let mut tx = state.db.begin().await?;
    let plan = billing::plan(&mut tx, id).await?;
    let today = billing::today();
    let grace = plan.as_ref().map_or(7, |p| p.grace_days).max(1) as i64;
    let currency = plan.as_ref().map_or_else(|| "KES".to_string(), |p| p.currency.clone());
    let due = |d: Option<NaiveDate>| -> AppResult<NaiveDate> {
        let d = d.unwrap_or(today + chrono::Duration::days(grace));
        if d < today {
            return Err(bad("The due date cannot be in the past"));
        }
        Ok(d)
    };
    let (doc_id, number, amount) = match b.category.as_str() {
        "next_period" => {
            let p = plan.as_ref().ok_or_else(|| rule("Set the business's billing plan first"))?;
            if b.kind == "invoice" {
                let (i, n) = billing::issue_next_period(&mut tx, p, Some(ctx.user_id)).await?;
                (i, n, p.amount)
            } else {
                let start = p.next_due_date.ok_or_else(|| rule("This billing plan has no recurring fee"))?;
                let period = billing::period(start, billing::months(&p.frequency, p.custom_months));
                let (i, n) = billing::insert_document(&mut tx, billing::NewDocument {
                    tenant_id: id, kind: "quotation", category: billing::recurring_category(p),
                    description: format!("{} — {}", if p.model == "subscription" { "S'Shop subscription" } else { "S'Shop maintenance" }, billing::frequency_label(&p.frequency, p.custom_months)),
                    amount: p.amount, currency: currency.clone(), issue_date: today, due_date: due(b.due_date)?, period: Some(period), quotation_id: None, created_by: Some(ctx.user_id),
                }).await?;
                (i, n, p.amount)
            }
        }
        "one_off" => {
            let p = plan.as_ref().filter(|p| p.model == "one_off").ok_or_else(|| rule("This business is not on the one-off model"))?;
            let amount = b.amount.unwrap_or(p.one_off_amount);
            let description = if b.description.trim().is_empty() { "S'Shop one-off licence".to_string() } else { b.description.trim().chars().take(300).collect() };
            let (i, n) = billing::insert_document(&mut tx, billing::NewDocument {
                tenant_id: id, kind: if b.kind == "invoice" { "invoice" } else { "quotation" }, category: "one_off", description,
                amount, currency: currency.clone(), issue_date: today, due_date: due(b.due_date)?, period: None, quotation_id: None, created_by: Some(ctx.user_id),
            }).await?;
            (i, n, amount)
        }
        "other" => {
            let amount = b.amount.filter(|a| *a > Decimal::ZERO).ok_or_else(|| bad("Enter the amount"))?;
            let description: String = b.description.trim().chars().take(300).collect();
            if description.len() < 3 {
                return Err(bad("Describe what this is for"));
            }
            let (i, n) = billing::insert_document(&mut tx, billing::NewDocument {
                tenant_id: id, kind: if b.kind == "invoice" { "invoice" } else { "quotation" }, category: "other", description,
                amount, currency: currency.clone(), issue_date: today, due_date: due(b.due_date)?, period: None, quotation_id: None, created_by: Some(ctx.user_id),
            }).await?;
            (i, n, amount)
        }
        _ => return Err(bad("Choose what to bill")),
    };
    if amount <= Decimal::ZERO {
        return Err(bad("The amount must be more than zero"));
    }
    let action = if b.kind == "invoice" { "invoice_issued" } else { "quotation_issued" };
    let after = json!({ "number": number, "amount": amount, "category": b.category });
    record_platform(&mut tx, &ctx, id, || Entry::new("billing", action, "billing_document", doc_id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "id": doc_id, "number": number })))
}

#[derive(Deserialize)]
struct VoidBody {
    #[serde(default)]
    reason: String,
}

async fn void_document(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<VoidBody>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let reason: String = b.reason.trim().chars().take(300).collect();
    if reason.chars().count() < 3 {
        return Err(refused("Reason required", "Give the reason for voiding this document"));
    }
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "UPDATE billing_documents SET status = 'void', void_reason = $2 WHERE id = $1 AND status = 'open' RETURNING tenant_id, number, kind",
    )
    .bind(id)
    .bind(&reason)
    .fetch_optional(&mut *tx)
    .await?;
    let (tenant, number, _kind) = row.ok_or_else(|| rule("Only open quotations and invoices can be voided"))?;
    let after = json!({ "number": number, "reason": reason });
    record_platform(&mut tx, &ctx, tenant, || Entry::new("billing", "document_void", "billing_document", id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Quotation → invoice, by the platform owner.
async fn invoice_quotation(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM billing_documents WHERE id = $1").bind(id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound("Quotation"))?;
    let mut tx = state.db.begin().await?;
    let (invoice_id, number) = quotation_to_invoice(&mut tx, tenant, id, Some(ctx.user_id)).await?;
    let after = json!({ "invoice": number });
    record_platform(&mut tx, &ctx, tenant, || Entry::new("billing", "invoice_issued", "billing_document", invoice_id).after(after.clone())).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "invoice_id": invoice_id, "number": number })))
}

#[derive(Deserialize)]
struct ManualPayment {
    /// bank | mpesa | cash | other
    method: String,
    reference: String,
    paid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    note: String,
}

/// A payment received outside S'Shop (bank transfer, M-Pesa …), recorded by the platform owner. Settles the
/// invoice exactly like a verified Paystack payment.
async fn record_payment(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<ManualPayment>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    if !matches!(b.method.as_str(), "bank" | "mpesa" | "cash" | "other") {
        return Err(bad("Choose how it was paid"));
    }
    let reference: String = b.reference.trim().chars().take(80).collect();
    if reference.len() < 3 {
        return Err(bad("Enter the payment reference (bank or M-Pesa code)"));
    }
    let paid_at = b.paid_at.unwrap_or_else(Utc::now);
    if paid_at > Utc::now() + chrono::Duration::minutes(5) {
        return Err(bad("The payment date cannot be in the future"));
    }
    let mut tx = state.db.begin().await?;
    let inv: Option<(Uuid, Decimal, String, String)> = sqlx::query_as(
        "SELECT tenant_id, amount, currency, status FROM billing_documents WHERE id = $1 AND kind = 'invoice' FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (tenant, amount, currency, status) = inv.ok_or(AppError::NotFound("Invoice"))?;
    if status != "open" {
        return Err(rule(if status == "paid" { "This invoice is already paid" } else { "This invoice is no longer payable" }));
    }
    let pid: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_payments (tenant_id, invoice_id, amount, currency, method, reference, recorded_by, note)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
    )
    .bind(tenant)
    .bind(id)
    .bind(amount)
    .bind(&currency)
    .bind(&b.method)
    .bind(&reference)
    .bind(ctx.user_id)
    .bind(b.note.trim().chars().take(300).collect::<String>())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => rule("This payment reference has already been used"),
        _ => AppError::from(e),
    })?;
    let receipt = billing::settle(&mut tx, pid, paid_at, &b.method, None, Some(ctx.user_id), &ctx.ip).await?;
    let after = json!({ "invoice_id": id, "amount": amount, "method": b.method, "reference": reference, "receipt": receipt });
    let home = ctx.acting_from.unwrap_or(ctx.tenant_id);
    if home != tenant {
        let mut c = ctx.clone();
        c.tenant_id = home;
        audit::record(&mut tx, &c, Entry::new("billing", "payment_received", "billing_payment", pid).after(after)).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "payment_id": pid, "receipt_no": receipt })))
}

async fn platform_document(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM billing_documents WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Billing document"))?;
    document_view(&state, tenant, id, true).await.map(Json)
}

async fn platform_verify(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_platform_admin(&state, &ctx).await?;
    let status = billing::verify_paystack(&state, id).await?;
    Ok(Json(json!({ "status": status })))
}

/// Paystack events, authenticated by the HMAC-SHA512 signature of the raw body. The event itself is not trusted:
/// the payment is re-verified with Paystack before anything is settled. Always 200 for a valid signature so
/// Paystack does not retry forever; an invalid signature gets 401.
async fn paystack_webhook(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let Some(cfg) = &state.cfg.paystack else { return StatusCode::NOT_FOUND };
    let sig = headers.get("x-paystack-signature").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !crate::integrations::paystack::signature_ok(cfg, &body, sig) {
        tracing::warn!("paystack webhook with a bad signature");
        return StatusCode::UNAUTHORIZED;
    }
    let Ok(event) = serde_json::from_slice::<Value>(&body) else { return StatusCode::OK };
    let reference = event["data"]["reference"].as_str().unwrap_or_default();
    if !matches!(event["event"].as_str(), Some("charge.success")) || reference.is_empty() {
        return StatusCode::OK;
    }
    let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM billing_payments WHERE reference = $1 AND method = 'paystack'")
        .bind(reference)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    match id {
        Some(id) => {
            if let Err(e) = billing::verify_paystack(&state, id).await {
                tracing::warn!(%reference, error = %e, "paystack webhook verification");
            }
        }
        None => tracing::warn!(%reference, "paystack webhook for an unknown reference"),
    }
    StatusCode::OK
}
