//! Platform billing (roadmap 37–40, 41–46): packages and modules, tenant-specific pricing (base → discount → tax →
//! amount payable), free / trial / grace access, billing periods, a business's billing status, invoice numbering,
//! settling a verified payment (invoice → payment → receipt → period → next due), automatic renewal invoices and
//! billing suspension. Used by the platform owner screens (routes/platform.rs), the business's own Billing page
//! (routes/billing.rs), the session check (auth.rs), the Paystack webhook and the background jobs — one
//! implementation for all of them.
//!
//! Billing has a **service** dimension (roadmap 51): `platform` (the S'Shop subscription / licence) and `website` (the
//! Website Add-On) each have their own plan, documents, renewal, grace and suspension for the same business.

use chrono::{DateTime, Months, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::error::{rule, AppError, AppResult};
use crate::util::round2;

/// Billing dates follow the platform owner's calendar (Kenya).
pub const PLATFORM_TZ: &str = "Africa/Nairobi";
/// An invoice for the next period is issued this many days before it starts (auto-renew), and a payment is
/// "due" within this window.
pub const DUE_SOON_DAYS: i64 = 7;

pub fn today() -> NaiveDate {
    crate::util::today_in(crate::util::parse_tz(PLATFORM_TZ))
}

pub const FREQUENCIES: [&str; 5] = ["monthly", "quarterly", "semi_annual", "annual", "custom"];

/// Billable services.
pub const SERVICES: [&str; 2] = ["platform", "website"];

pub fn service_label(service: &str) -> &'static str {
    if service == "website" { "S'Shop Website" } else { "S'Shop" }
}

// ───────────────────────────── Modules (roadmap 41) ─────────────────────────────

/// A sellable part of S'Shop. `paths` are API prefixes the module owns (refused on the server when the business's
/// package excludes it); `perms` are the permissions that belong to it (hidden in the web app). Products, dashboard,
/// settings, users, branches, approvals, audit, notifications and search are core and always included.
pub struct Module {
    pub key: &'static str,
    pub label: &'static str,
    pub paths: &'static [&'static str],
    pub perms: &'static [&'static str],
}

pub const MODULES: &[Module] = &[
    Module { key: "sales", label: "Sales / POS", paths: &["/sales", "/mpesa"], perms: &["sales."] },
    Module { key: "orders", label: "Orders", paths: &["/orders"], perms: &["orders."] },
    Module { key: "stock", label: "Stock & Inventory", paths: &["/stock", "/transfers"], perms: &["stock."] },
    Module { key: "customers", label: "Customers", paths: &["/customers"], perms: &["customers.view", "customers.create", "customers.edit"] },
    Module { key: "loyalty", label: "Loyalty", paths: &["/loyalty"], perms: &["loyalty.", "customers.view_loyalty", "customers.redeem_points"] },
    Module { key: "credit", label: "Credit Sales", paths: &["/credit"], perms: &["credit.", "customers.view_credit"] },
    Module { key: "expenses", label: "Expenses", paths: &["/expenses"], perms: &["expenses."] },
    Module { key: "reports", label: "Reports & Analytics", paths: &["/reports", "/leaderboards"], perms: &["reports."] },
];

pub fn is_module(key: &str) -> bool {
    MODULES.iter().any(|m| m.key == key)
}

pub fn module_label(key: &str) -> &'static str {
    MODULES.iter().find(|m| m.key == key).map_or("This module", |m| m.label)
}

/// The module an API path belongs to (path as seen inside /api, with or without the prefix).
pub fn module_for_path(path: &str) -> Option<&'static str> {
    let p = path.strip_prefix("/api").unwrap_or(path);
    MODULES
        .iter()
        .find(|m| m.paths.iter().any(|pre| p == *pre || p.starts_with(&format!("{pre}/")) || p.starts_with(&format!("{pre}?"))))
        .map(|m| m.key)
}

pub fn modules_catalogue() -> Value {
    json!(MODULES.iter().map(|m| json!({ "key": m.key, "label": m.label, "perms": m.perms })).collect::<Vec<_>>())
}

/// What a session may use, read with the session on every request (auth.rs).
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct AccessRow {
    pub ownership: String,
    pub billing_suspended: bool,
    pub package: Option<String>,
    pub modules: Option<Vec<String>>,
    pub access_mode: Option<String>,
    pub trial_end: Option<NaiveDate>,
    pub trial_modules: Option<Vec<String>>,
}

impl AccessRow {
    /// Modules included (None = every module): the platform owner's business and businesses without a plan have
    /// everything; a running trial uses its own module list when one is set.
    pub fn modules(&self) -> Option<Vec<String>> {
        if self.ownership == "platform" || self.package.is_none() {
            return None;
        }
        if self.access_mode.as_deref() == Some("trial") && self.trial_end.is_some_and(|e| e >= today()) {
            if let Some(m) = self.trial_modules.as_ref().filter(|m| !m.is_empty()) {
                return Some(m.clone());
            }
        }
        match self.package.as_deref() {
            Some("modules") => Some(self.modules.clone().unwrap_or_default()),
            _ => None,
        }
    }

    pub fn suspended(&self) -> bool {
        self.ownership != "platform" && self.billing_suspended
    }
}

pub async fn access(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<AccessRow> {
    Ok(sqlx::query_as(
        "SELECT t.ownership, t.billing_suspended, p.package, p.modules, p.access_mode, p.trial_end, p.trial_modules
         FROM tenants t LEFT JOIN billing_plans p ON p.tenant_id = t.id AND p.service = 'platform' WHERE t.id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await?)
}

// ───────────────────────────── Plans & pricing (roadmap 42) ─────────────────────────────

pub fn months(frequency: &str, custom_months: i32) -> u32 {
    match frequency {
        "quarterly" => 3,
        "semi_annual" => 6,
        "annual" => 12,
        "custom" => custom_months.clamp(1, 60) as u32,
        _ => 1,
    }
}

/// The billing period that starts on `start`: [start, start + n months − 1 day].
pub fn period(start: NaiveDate, n: u32) -> (NaiveDate, NaiveDate) {
    let end = start.checked_add_months(Months::new(n)).unwrap_or(start).pred_opt().unwrap_or(start);
    (start, end)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Plan {
    pub tenant_id: Uuid,
    pub service: String,
    pub model: String,
    pub currency: String,
    pub one_off_amount: Decimal,
    pub one_off_paid_on: Option<NaiveDate>,
    pub recurring: bool,
    /// Recurring base price (for a module package: the sum of the included modules' prices).
    pub amount: Decimal,
    pub frequency: String,
    pub custom_months: i32,
    pub start_date: Option<NaiveDate>,
    pub next_due_date: Option<NaiveDate>,
    pub grace_days: i32,
    pub grace_until: Option<NaiveDate>,
    pub auto_renew: bool,
    pub auto_suspend: bool,
    pub package: String,
    pub modules: Vec<String>,
    pub module_prices: Value,
    pub discount_type: String,
    pub discount_value: Decimal,
    pub tax_enabled: bool,
    pub tax_rate: Decimal,
    pub access_mode: String,
    pub trial_start: Option<NaiveDate>,
    pub trial_end: Option<NaiveDate>,
    pub trial_modules: Vec<String>,
    pub notes: String,
    pub updated_at: DateTime<Utc>,
}

pub const PLAN_COLUMNS: &str = "tenant_id, service, model, currency, one_off_amount, one_off_paid_on, recurring, amount, frequency, custom_months,
    start_date, next_due_date, grace_days, grace_until, auto_renew, auto_suspend, package, modules, module_prices, discount_type,
    discount_value, tax_enabled, tax_rate, access_mode, trial_start, trial_end, trial_modules, notes, updated_at";

/// The platform plan of a business.
pub async fn plan(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<Option<Plan>> {
    plan_of(conn, tenant_id, "platform").await
}

pub async fn plan_of(conn: &mut PgConnection, tenant_id: Uuid, service: &str) -> AppResult<Option<Plan>> {
    Ok(sqlx::query_as(&format!("SELECT {PLAN_COLUMNS} FROM billing_plans WHERE tenant_id = $1 AND service = $2"))
        .bind(tenant_id)
        .bind(service)
        .fetch_optional(&mut *conn)
        .await?)
}

/// The recurring fee's category: a subscription, or maintenance after a one-off fee.
pub fn recurring_category(p: &Plan) -> &'static str {
    if p.model == "subscription" { "subscription" } else { "maintenance" }
}

/// Base price → discount → tax → amount payable.
#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq)]
pub struct Price {
    pub subtotal: Decimal,
    pub discount: Decimal,
    pub tax_rate: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
}

pub fn calculate(base: Decimal, discount_type: &str, discount_value: Decimal, tax_enabled: bool, tax_rate: Decimal) -> Price {
    let base = round2(base.max(Decimal::ZERO));
    let discount = match discount_type {
        "percent" => round2(base * discount_value.min(Decimal::from(100)) / Decimal::from(100)),
        "fixed" => round2(discount_value.min(base)),
        _ => Decimal::ZERO,
    };
    let rate = if tax_enabled { tax_rate } else { Decimal::ZERO };
    let tax = round2((base - discount) * rate / Decimal::from(100));
    Price { subtotal: base, discount, tax_rate: rate, tax, total: base - discount + tax }
}

/// The plan's pricing applied to `base`.
pub fn price(p: &Plan, base: Decimal) -> Price {
    calculate(base, &p.discount_type, p.discount_value, p.tax_enabled, p.tax_rate)
}

/// No discount or tax (no plan yet).
pub fn plain(amount: Decimal) -> Price {
    calculate(amount, "none", Decimal::ZERO, false, Decimal::ZERO)
}

// ───────────────────────────── Status (roadmap 44) ─────────────────────────────

/// A business's billing position — the same figures on the platform directory, dashboard, the business's own
/// Billing page and the session.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Summary {
    pub service: String,
    /// platform_owned | not_set | free | trial | suspended | active | one_off_paid | payment_due | maintenance_due |
    /// grace | overdue
    pub status: String,
    pub ownership: String,
    pub suspended: bool,
    pub model: Option<String>,
    pub access_mode: Option<String>,
    pub package: Option<String>,
    /// Modules included (None = all).
    pub modules: Option<Vec<String>>,
    pub currency: String,
    /// Recurring fee (subscription or maintenance): base and price after discount and tax.
    pub amount: Option<Decimal>,
    pub recurring_price: Option<Price>,
    pub frequency: Option<String>,
    pub custom_months: Option<i32>,
    pub next_due: Option<NaiveDate>,
    pub grace_days: i32,
    pub grace_until: Option<NaiveDate>,
    pub trial_start: Option<NaiveDate>,
    pub trial_end: Option<NaiveDate>,
    pub outstanding: Decimal,
    pub open_invoices: i64,
    pub oldest_open_due: Option<NaiveDate>,
    pub last_payment_at: Option<DateTime<Utc>>,
    pub last_payment_amount: Option<Decimal>,
    /// Latest paid recurring period.
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub one_off_amount: Option<Decimal>,
    pub one_off_price: Option<Price>,
    /// paid | pending (one-off model only).
    pub one_off_status: Option<String>,
    pub maintenance: bool,
    pub paid_total: Decimal,
}

/// SQL predicate: an open invoice `d` of plan `p` is overdue on `$today` (past due + grace days and any extension).
pub const OVERDUE_SQL: &str = "(d.kind = 'invoice' AND d.status = 'open' AND d.service = p.service
     AND d.due_date + COALESCE(p.grace_days, 0) < $2 AND (p.grace_until IS NULL OR p.grace_until < $2))";

/// The platform billing position of a business.
pub async fn summary(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<Summary> {
    summary_of(conn, tenant_id, "platform").await
}

pub async fn summary_of(conn: &mut PgConnection, tenant_id: Uuid, service: &str) -> AppResult<Summary> {
    let (ownership, platform_suspended): (String, bool) = sqlx::query_as("SELECT ownership, billing_suspended FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await?;
    let suspended = if service == "website" {
        sqlx::query_scalar::<_, bool>("SELECT billing_suspended FROM websites WHERE tenant_id = $1").bind(tenant_id).fetch_optional(&mut *conn).await?.unwrap_or(false)
    } else {
        platform_suspended
    };
    let (outstanding, open, oldest): (Option<Decimal>, i64, Option<NaiveDate>) = sqlx::query_as(
        "SELECT SUM(amount), COUNT(*), MIN(due_date) FROM billing_documents WHERE tenant_id = $1 AND service = $2 AND kind = 'invoice' AND status = 'open'",
    )
    .bind(tenant_id)
    .bind(service)
    .fetch_one(&mut *conn)
    .await?;
    let base = Summary {
        service: service.into(), ownership: ownership.clone(), suspended, currency: "KES".into(),
        outstanding: outstanding.unwrap_or_default(), open_invoices: open, ..Default::default()
    };
    if ownership == "platform" {
        return Ok(Summary { status: "platform_owned".into(), suspended: false, ..base });
    }
    let Some(p) = plan_of(conn, tenant_id, service).await? else {
        return Ok(Summary { status: if suspended { "suspended" } else { "not_set" }.into(), ..base });
    };
    let last: Option<(DateTime<Utc>, Decimal)> = sqlx::query_as(
        "SELECT b.paid_at, b.amount FROM billing_payments b JOIN billing_documents d ON d.id = b.invoice_id
         WHERE b.tenant_id = $1 AND d.service = $2 AND b.status = 'success' ORDER BY b.paid_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(service)
    .fetch_optional(&mut *conn)
    .await?;
    let paid_total: Option<Decimal> = sqlx::query_scalar(
        "SELECT SUM(b.amount) FROM billing_payments b JOIN billing_documents d ON d.id = b.invoice_id
         WHERE b.tenant_id = $1 AND d.service = $2 AND b.status = 'success'",
    )
    .bind(tenant_id)
    .bind(service)
    .fetch_one(&mut *conn)
    .await?;
    let covered: Option<(NaiveDate, NaiveDate)> = sqlx::query_as(
        "SELECT period_start, period_end FROM billing_documents
         WHERE tenant_id = $1 AND service = $3 AND kind = 'invoice' AND status = 'paid' AND category = $2 AND period_end IS NOT NULL
         ORDER BY period_end DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(recurring_category(&p))
    .bind(service)
    .fetch_optional(&mut *conn)
    .await?;
    let oldest_category: Option<String> = sqlx::query_scalar(
        "SELECT category FROM billing_documents WHERE tenant_id = $1 AND service = $2 AND kind = 'invoice' AND status = 'open' ORDER BY due_date LIMIT 1",
    )
    .bind(tenant_id)
    .bind(service)
    .fetch_optional(&mut *conn)
    .await?;
    let one_off_status = if p.model == "one_off" {
        let paid: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM billing_documents WHERE tenant_id = $1 AND service = $2 AND kind = 'invoice' AND category = 'one_off' AND status = 'paid')",
        )
        .bind(tenant_id)
        .bind(service)
        .fetch_one(&mut *conn)
        .await?;
        Some(if paid || p.one_off_paid_on.is_some() { "paid" } else { "pending" }.to_string())
    } else {
        None
    };

    let today = today();
    let trial_running = p.access_mode == "trial" && p.trial_end.is_some_and(|e| e >= today);
    let grace_end = oldest.map(|due| {
        let by_days = due + chrono::Duration::days(p.grace_days as i64);
        p.grace_until.map_or(by_days, |g| g.max(by_days))
    });
    let due_soon = |d: Option<NaiveDate>| d.is_some_and(|d| (d - today).num_days() <= DUE_SOON_DAYS);
    let status = if suspended {
        "suspended"
    } else if p.access_mode == "free" {
        "free"
    } else if trial_running {
        "trial"
    } else if let (Some(due), Some(end)) = (oldest, grace_end) {
        if end < today {
            "overdue"
        } else if due < today {
            "grace"
        } else if oldest_category.as_deref() == Some("maintenance") {
            "maintenance_due"
        } else {
            "payment_due"
        }
    } else if p.model == "one_off" {
        if one_off_status.as_deref() == Some("pending") {
            "payment_due"
        } else if p.recurring && due_soon(p.next_due_date) {
            "maintenance_due"
        } else {
            "one_off_paid"
        }
    } else if due_soon(p.next_due_date) || paid_total.is_none() {
        "payment_due"
    } else {
        "active"
    };
    let modules = AccessRow {
        ownership,
        billing_suspended: suspended,
        package: Some(p.package.clone()),
        modules: Some(p.modules.clone()),
        access_mode: Some(p.access_mode.clone()),
        trial_end: p.trial_end,
        trial_modules: Some(p.trial_modules.clone()),
    }
    .modules();
    Ok(Summary {
        status: status.into(),
        model: Some(p.model.clone()),
        access_mode: Some(p.access_mode.clone()),
        package: Some(p.package.clone()),
        modules,
        currency: p.currency.clone(),
        amount: p.recurring.then_some(p.amount),
        recurring_price: p.recurring.then(|| price(&p, p.amount)),
        frequency: p.recurring.then(|| p.frequency.clone()),
        custom_months: p.recurring.then_some(p.custom_months),
        next_due: if p.recurring { p.next_due_date } else { None },
        grace_days: p.grace_days,
        grace_until: p.grace_until,
        trial_start: p.trial_start,
        trial_end: p.trial_end,
        oldest_open_due: oldest,
        last_payment_at: last.map(|l| l.0),
        last_payment_amount: last.map(|l| l.1),
        period_start: covered.map(|c| c.0),
        period_end: covered.map(|c| c.1),
        one_off_amount: (p.model == "one_off").then_some(p.one_off_amount),
        one_off_price: (p.model == "one_off").then(|| price(&p, p.one_off_amount)),
        one_off_status,
        maintenance: p.model == "one_off" && p.recurring,
        paid_total: paid_total.unwrap_or_default(),
        ..base
    })
}

// ───────────────────────────── Documents ─────────────────────────────

pub async fn next_number(conn: &mut PgConnection, kind: &str) -> AppResult<String> {
    let (seq, prefix) = match kind {
        "quotation" => ("billing_quotation_no", "QUO"),
        "receipt" => ("billing_receipt_no", "RCT"),
        _ => ("billing_invoice_no", "INV"),
    };
    let n: i64 = sqlx::query_scalar(&format!("SELECT nextval('{seq}')")).fetch_one(&mut *conn).await?;
    Ok(format!("{prefix}-{}-{n:05}", today().format("%Y")))
}

pub struct NewDocument<'a> {
    pub tenant_id: Uuid,
    pub service: &'a str,
    pub kind: &'a str,
    pub category: &'a str,
    pub description: String,
    pub price: Price,
    pub currency: String,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub quotation_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
}

pub async fn insert_document(conn: &mut PgConnection, d: NewDocument<'_>) -> AppResult<(Uuid, String)> {
    let ownership: String = sqlx::query_scalar("SELECT ownership FROM tenants WHERE id = $1").bind(d.tenant_id).fetch_one(&mut *conn).await?;
    if ownership == "platform" {
        return Err(crate::error::refused("Not billable", "Platform billing does not apply to the platform owner's business"));
    }
    if d.price.total <= Decimal::ZERO {
        return Err(rule("The amount payable must be more than zero"));
    }
    let number = next_number(conn, d.kind).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_documents (tenant_id, kind, number, category, description, amount, currency, issue_date, due_date,
                                        period_start, period_end, quotation_id, created_by, subtotal, discount, tax_rate, tax, service)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) RETURNING id",
    )
    .bind(d.tenant_id)
    .bind(d.kind)
    .bind(&number)
    .bind(d.category)
    .bind(&d.description)
    .bind(d.price.total)
    .bind(&d.currency)
    .bind(d.issue_date)
    .bind(d.due_date)
    .bind(d.period.map(|p| p.0))
    .bind(d.period.map(|p| p.1))
    .bind(d.quotation_id)
    .bind(d.created_by)
    .bind(d.price.subtotal)
    .bind(d.price.discount)
    .bind(d.price.tax_rate)
    .bind(d.price.tax)
    .bind(d.service)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("billing_documents_period_uq") => {
            rule("An invoice for this billing period already exists")
        }
        sqlx::Error::Database(db) if db.constraint() == Some("billing_documents_quotation_uq") => {
            rule("This quotation has already been invoiced")
        }
        _ => AppError::from(e),
    })?;
    Ok((id, number))
}

pub fn frequency_label(frequency: &str, custom_months: i32) -> String {
    match frequency {
        "quarterly" => "Quarterly".into(),
        "semi_annual" => "Semi-annual".into(),
        "annual" => "Annual".into(),
        "custom" => format!("Every {custom_months} months"),
        _ => "Monthly".into(),
    }
}

fn package_label(p: &Plan) -> String {
    if p.package == "modules" {
        p.modules.iter().map(|m| module_label(m)).collect::<Vec<_>>().join(", ")
    } else {
        "Full platform".into()
    }
}

/// The invoice for the plan's next recurring period (starting at `next_due_date`, due that day). A one-off plan
/// only ever produces maintenance invoices here, and only when maintenance is switched on.
pub async fn issue_next_period(conn: &mut PgConnection, p: &Plan, created_by: Option<Uuid>) -> AppResult<(Uuid, String)> {
    if !p.recurring {
        return Err(rule("This billing plan has no recurring fee"));
    }
    let start = p.next_due_date.ok_or_else(|| rule("Set the next due date first"))?;
    let (start, end) = period(start, months(&p.frequency, p.custom_months));
    let what = match (p.service.as_str(), p.model.as_str()) {
        ("website", "subscription") => "S'Shop Website subscription".to_string(),
        ("website", _) => "S'Shop Website maintenance".to_string(),
        (_, "subscription") => format!("S'Shop subscription ({})", package_label(p)),
        _ => "S'Shop maintenance".to_string(),
    };
    let issue = today().min(start);
    insert_document(
        conn,
        NewDocument {
            tenant_id: p.tenant_id,
            service: &p.service,
            kind: "invoice",
            category: recurring_category(p),
            description: format!("{what} — {} ({} – {})", frequency_label(&p.frequency, p.custom_months), start.format("%d %b %Y"), end.format("%d %b %Y")),
            price: price(p, p.amount),
            currency: p.currency.clone(),
            issue_date: issue,
            due_date: start.max(issue),
            period: Some((start, end)),
            quotation_id: None,
            created_by,
        },
    )
    .await
}

// ───────────────────────────── Suspension (roadmap 43–44) ─────────────────────────────

/// Brings `tenants.billing_suspended` in line with the plan: suspended when the plan allows automatic suspension,
/// billing applies (not free, not a running trial, not the platform owner) and an invoice is overdue past its
/// grace period and any extension. Lifted as soon as that is no longer true (paid, voided, grace extended,
/// switched to free …). Audited when it changes.
pub async fn refresh_suspension(conn: &mut PgConnection, tenant_id: Uuid, actor: Option<Uuid>) -> AppResult<bool> {
    let platform = refresh_suspension_of(conn, tenant_id, "platform", actor).await?;
    refresh_suspension_of(conn, tenant_id, "website", actor).await?;
    Ok(platform)
}

/// Suspension of one service: the platform suspends the business's S'Shop access, the website only takes the public
/// website offline ("temporarily unavailable") — POS and operations carry on.
pub async fn refresh_suspension_of(conn: &mut PgConnection, tenant_id: Uuid, service: &str, actor: Option<Uuid>) -> AppResult<bool> {
    let should: bool = sqlx::query_scalar(&format!(
        "SELECT t.ownership = 'customer' AND COALESCE(p.auto_suspend, false) AND p.access_mode = 'billed'
                AND EXISTS (SELECT 1 FROM billing_documents d WHERE d.tenant_id = t.id AND {OVERDUE_SQL})
         FROM tenants t LEFT JOIN billing_plans p ON p.tenant_id = t.id AND p.service = $3 WHERE t.id = $1"
    ))
    .bind(tenant_id)
    .bind(today())
    .bind(service)
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or(false);
    let changed: Option<bool> = if service == "website" {
        sqlx::query_scalar("UPDATE websites SET billing_suspended = $2 WHERE tenant_id = $1 AND billing_suspended <> $2 RETURNING billing_suspended")
            .bind(tenant_id)
            .bind(should)
            .fetch_optional(&mut *conn)
            .await?
    } else {
        sqlx::query_scalar(
            "UPDATE tenants SET billing_suspended = $2 WHERE id = $1 AND billing_suspended <> $2 AND ownership = 'customer' RETURNING billing_suspended",
        )
        .bind(tenant_id)
        .bind(should)
        .fetch_optional(&mut *conn)
        .await?
    };
    if let Some(now) = changed {
        audit::system(
            conn,
            tenant_id,
            actor,
            Entry::new("billing", if now { "billing_suspended" } else { "billing_restored" }, "tenant", tenant_id)
                .before(json!({ "service": service, "billing_suspended": !now }))
                .after(json!({ "service": service, "billing_suspended": now })),
            "",
            "",
        )
        .await?;
    }
    Ok(should)
}

// ───────────────────────────── Payments ─────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct PaymentLock {
    tenant_id: Uuid,
    invoice_id: Uuid,
    amount: Decimal,
    currency: String,
    status: String,
    method: String,
    reference: String,
}

/// Settles a payment the server has verified: payment → success with a receipt number, invoice → paid,
/// recurring period → next due date moved past the period, suspension lifted when nothing is overdue any more.
/// Idempotent: a payment already settled (webhook and verify racing) returns `Ok(None)`. A second successful
/// payment for an already-paid invoice is kept as a success with a note so it can be refunded — money that
/// arrived is never discarded.
pub async fn settle(
    conn: &mut PgConnection,
    payment_id: Uuid,
    paid_at: DateTime<Utc>,
    channel: &str,
    gateway: Option<Value>,
    actor: Option<Uuid>,
    ip: &str,
) -> AppResult<Option<String>> {
    let p: PaymentLock = sqlx::query_as(
        "SELECT tenant_id, invoice_id, amount, currency, status, method, reference FROM billing_payments WHERE id = $1 FOR UPDATE",
    )
    .bind(payment_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("Payment"))?;
    if !matches!(p.status.as_str(), "pending" | "abandoned") {
        return Ok(None);
    }
    let (inv_status, inv_number, category, period_end, service): (String, String, String, Option<NaiveDate>, String) = sqlx::query_as(
        "SELECT status, number, category, period_end, service FROM billing_documents WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(p.invoice_id)
    .bind(p.tenant_id)
    .fetch_one(&mut *conn)
    .await?;
    let receipt = next_number(conn, "receipt").await?;
    let note = match inv_status.as_str() {
        "paid" => "Invoice was already paid — duplicate payment, refund or credit it",
        "void" => "Invoice was voided — refund or credit this payment",
        _ => "",
    };
    sqlx::query(
        "UPDATE billing_payments SET status = 'success', receipt_no = $2, paid_at = $3, verified_at = now(), channel = $4,
                gateway = COALESCE($5, gateway), note = CASE WHEN $6 = '' THEN note ELSE $6 END WHERE id = $1",
    )
    .bind(payment_id)
    .bind(&receipt)
    .bind(paid_at)
    .bind(channel)
    .bind(gateway)
    .bind(note)
    .execute(&mut *conn)
    .await?;
    if inv_status == "open" {
        sqlx::query("UPDATE billing_documents SET status = 'paid', paid_at = $2 WHERE id = $1").bind(p.invoice_id).bind(paid_at).execute(&mut *conn).await?;
        if matches!(category.as_str(), "subscription" | "maintenance") {
            if let Some(end) = period_end {
                sqlx::query(
                    "UPDATE billing_plans SET next_due_date = GREATEST(COALESCE(next_due_date, $2), $2), updated_at = now()
                     WHERE tenant_id = $1 AND service = $3",
                )
                .bind(p.tenant_id)
                .bind(end.succ_opt().unwrap_or(end))
                .bind(&service)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    audit::system(
        conn,
        p.tenant_id,
        actor,
        Entry::new("billing", "payment_received", "billing_payment", payment_id).after(json!({
            "invoice": inv_number, "amount": p.amount, "currency": p.currency, "method": p.method, "reference": p.reference,
            "receipt": receipt, "note": note, "service": service,
        })),
        ip,
        "",
    )
    .await?;
    refresh_suspension(conn, p.tenant_id, actor).await?;
    Ok(Some(receipt))
}

/// Verifies a pending Paystack payment with Paystack and settles or closes it. Used by the browser return,
/// the webhook and reconciliation; the browser's word is never taken.
/// Returns the payment's status afterwards.
pub async fn verify_paystack(state: &crate::state::AppState, payment_id: Uuid) -> AppResult<String> {
    let cfg = state.cfg.paystack.as_ref().ok_or_else(|| rule("Online payments are not configured"))?;
    let (reference, amount, currency, status, created_at): (String, Decimal, String, String, DateTime<Utc>) = sqlx::query_as(
        "SELECT reference, amount, currency, status, created_at FROM billing_payments WHERE id = $1 AND method = 'paystack'",
    )
    .bind(payment_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Payment"))?;
    if !matches!(status.as_str(), "pending" | "abandoned") {
        return Ok(status);
    }
    let v = crate::integrations::paystack::verify(&state.http, cfg, &reference).await.map_err(|e| {
        tracing::warn!(error = %e, %reference, "paystack verify failed");
        rule("The payment could not be confirmed with Paystack yet — try again in a moment")
    })?;
    let mut tx = state.db.begin().await?;
    let result = match v.status.as_str() {
        "success" => {
            if v.amount != crate::integrations::paystack::subunits(amount) || !v.currency.eq_ignore_ascii_case(&currency) {
                tracing::error!(%reference, paid = v.amount, %v.currency, "paystack amount/currency mismatch");
                sqlx::query("UPDATE billing_payments SET status = 'failed', gateway = $2, note = 'Amount or currency did not match the invoice' WHERE id = $1 AND status IN ('pending', 'abandoned')")
                    .bind(payment_id)
                    .bind(&v.raw)
                    .execute(&mut *tx)
                    .await?;
                "failed".to_string()
            } else {
                settle(&mut tx, payment_id, v.paid_at.unwrap_or_else(Utc::now), &v.channel, Some(v.raw.clone()), None, "").await?;
                "success".to_string()
            }
        }
        "failed" | "reversed" => {
            sqlx::query("UPDATE billing_payments SET status = 'failed', gateway = $2 WHERE id = $1 AND status IN ('pending', 'abandoned')")
                .bind(payment_id)
                .bind(&v.raw)
                .execute(&mut *tx)
                .await?;
            "failed".to_string()
        }
        // Still open at Paystack: abandoned after a day so a stale checkout never blocks a new one.
        _ if status == "pending" && Utc::now() - created_at > chrono::Duration::hours(24) => {
            sqlx::query("UPDATE billing_payments SET status = 'abandoned', gateway = $2 WHERE id = $1 AND status = 'pending'")
                .bind(payment_id)
                .bind(&v.raw)
                .execute(&mut *tx)
                .await?;
            "abandoned".to_string()
        }
        _ => status.clone(),
    };
    tx.commit().await?;
    Ok(result)
}

// ───────────────────────────── Background job ─────────────────────────────

/// Every 15 minutes: trials that have ended move to billing (audited), renewal invoices for billed plans with
/// auto-renew (issued DUE_SOON_DAYS before the period starts — never for free, trial or platform-owned
/// businesses, and never a subscription invoice for a one-off plan), suspension kept in line with overdue
/// invoices, and reconciliation of Paystack payments left pending (closed browser, missed webhook).
pub async fn run_jobs(state: &crate::state::AppState) -> anyhow::Result<()> {
    let today = today();
    let ended: Vec<(Uuid, String, NaiveDate)> = sqlx::query_as(
        "SELECT p.tenant_id, p.service, p.trial_end FROM billing_plans p JOIN tenants t ON t.id = p.tenant_id
         WHERE p.access_mode = 'trial' AND p.trial_end < $1 AND t.ownership = 'customer'",
    )
    .bind(today)
    .fetch_all(&state.db)
    .await?;
    for (tenant_id, service, end) in ended {
        let mut tx = state.db.begin().await?;
        let first_due = end.succ_opt().unwrap_or(end);
        sqlx::query(
            "UPDATE billing_plans SET access_mode = 'billed', updated_at = now(),
                    next_due_date = CASE WHEN recurring THEN GREATEST(COALESCE(next_due_date, $2), $2) ELSE next_due_date END
             WHERE tenant_id = $1 AND service = $3 AND access_mode = 'trial'",
        )
        .bind(tenant_id)
        .bind(first_due)
        .bind(&service)
        .execute(&mut *tx)
        .await?;
        audit::system(
            &mut tx,
            tenant_id,
            None,
            Entry::new("billing", "trial_ended", "tenant", tenant_id)
                .before(json!({ "service": service, "access_mode": "trial", "trial_end": end }))
                .after(json!({ "service": service, "access_mode": "billed", "first_due": first_due })),
            "",
            "",
        )
        .await?;
        tx.commit().await?;
        tracing::info!(%tenant_id, "trial ended — billing applies");
    }

    let soon = today + chrono::Duration::days(DUE_SOON_DAYS);
    let due: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT p.tenant_id, p.service FROM billing_plans p JOIN tenants t ON t.id = p.tenant_id
         WHERE p.recurring AND p.auto_renew AND p.access_mode = 'billed' AND t.status = 'active' AND t.ownership = 'customer'
           AND p.next_due_date <= $1
           AND NOT EXISTS (SELECT 1 FROM billing_documents d WHERE d.tenant_id = p.tenant_id AND d.service = p.service AND d.kind = 'invoice'
                           AND d.status <> 'void' AND d.period_start = p.next_due_date
                           AND d.category = CASE WHEN p.model = 'subscription' THEN 'subscription' ELSE 'maintenance' END)",
    )
    .bind(soon)
    .fetch_all(&state.db)
    .await?;
    for (tenant_id, service) in due {
        let mut tx = state.db.begin().await?;
        let Some(p) = plan_of(&mut tx, tenant_id, &service).await? else { continue };
        match issue_next_period(&mut tx, &p, None).await {
            Ok((id, number)) => {
                audit::system(&mut tx, tenant_id, None, Entry::new("billing", "invoice_issued", "billing_document", id).after(json!({ "number": number, "price": price(&p, p.amount), "auto": true })), "", "").await?;
                tx.commit().await?;
                tracing::info!(%tenant_id, number, "renewal invoice issued");
            }
            Err(e) => tracing::warn!(%tenant_id, error = %e, "renewal invoice not issued"),
        }
    }

    let tenants: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT t.id FROM tenants t LEFT JOIN billing_plans p ON p.tenant_id = t.id LEFT JOIN websites w ON w.tenant_id = t.id
         WHERE t.ownership = 'customer' AND (t.billing_suspended OR COALESCE(w.billing_suspended, false) OR COALESCE(p.auto_suspend, false))",
    )
    .fetch_all(&state.db)
    .await?;
    for tenant_id in tenants {
        let mut tx = state.db.begin().await?;
        refresh_suspension(&mut tx, tenant_id, None).await?;
        tx.commit().await?;
    }

    if state.cfg.paystack.is_some() {
        let pending: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM billing_payments WHERE status = 'pending' AND method = 'paystack' AND created_at < now() - interval '10 minutes' LIMIT 50",
        )
        .fetch_all(&state.db)
        .await?;
        for id in pending {
            if let Err(e) = verify_paystack(state, id).await {
                tracing::warn!(%id, error = %e, "paystack reconciliation");
            }
        }
    }
    Ok(())
}

/// Start-up: every business with an active platform administrator is the platform owner's own (protected) business.
pub async fn mark_platform_tenants(db: &sqlx::PgPool, platform_admins: &[String]) -> anyhow::Result<()> {
    if platform_admins.is_empty() {
        return Ok(());
    }
    let marked: Vec<(Uuid, String)> = sqlx::query_as(
        "UPDATE tenants SET ownership = 'platform', status = 'active', billing_suspended = false
         WHERE ownership <> 'platform' AND id IN (SELECT tenant_id FROM users WHERE is_active AND lower(email) = ANY($1))
         RETURNING id, name",
    )
    .bind(platform_admins)
    .fetch_all(db)
    .await?;
    for (id, name) in marked {
        tracing::info!(%id, name, "business marked as platform owned");
        let mut conn = db.acquire().await?;
        audit::system(&mut conn, id, None, Entry::new("platform", "platform_owned", "tenant", id).after(json!({ "ownership": "platform" })), "", "").await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(period(d(2026, 1, 1), 1), (d(2026, 1, 1), d(2026, 1, 31)));
        assert_eq!(period(d(2026, 1, 31), 1), (d(2026, 1, 31), d(2026, 2, 27)));
        assert_eq!(period(d(2026, 3, 15), months("quarterly", 1)), (d(2026, 3, 15), d(2026, 6, 14)));
        assert_eq!(period(d(2026, 1, 1), months("annual", 1)).1, d(2026, 12, 31));
        assert_eq!(months("custom", 2), 2);
        assert_eq!(months("custom", 99), 60);
    }

    #[test]
    fn pricing() {
        let dec = |s: &str| s.parse::<Decimal>().unwrap();
        // 120,000 annual maintenance + 16% tax
        let p = calculate(dec("120000"), "none", Decimal::ZERO, true, dec("16"));
        assert_eq!((p.discount, p.tax, p.total), (Decimal::ZERO, dec("19200"), dec("139200")));
        // 10% discount before tax
        let p = calculate(dec("10000"), "percent", dec("10"), true, dec("16"));
        assert_eq!((p.discount, p.tax, p.total), (dec("1000"), dec("1440"), dec("10440")));
        // fixed discount never exceeds the base; no tax when disabled
        let p = calculate(dec("500"), "fixed", dec("800"), false, dec("16"));
        assert_eq!((p.discount, p.tax, p.total), (dec("500"), Decimal::ZERO, Decimal::ZERO));
    }

    #[test]
    fn module_paths() {
        assert_eq!(module_for_path("/api/credit/123/payments"), Some("credit"));
        assert_eq!(module_for_path("/sales"), Some("sales"));
        assert_eq!(module_for_path("/transfers/1"), Some("stock"));
        assert_eq!(module_for_path("/products"), None);
        assert_eq!(module_for_path("/billing"), None);
        assert_eq!(module_for_path("/stockist"), None);
    }

    #[test]
    fn access_modules() {
        let mut a = AccessRow { ownership: "customer".into(), package: Some("modules".into()), modules: Some(vec!["sales".into()]), access_mode: Some("billed".into()), ..Default::default() };
        assert_eq!(a.modules(), Some(vec!["sales".to_string()]));
        a.access_mode = Some("trial".into());
        a.trial_end = Some(today());
        a.trial_modules = Some(vec!["sales".into(), "credit".into()]);
        assert_eq!(a.modules().map(|m| m.len()), Some(2));
        a.ownership = "platform".into();
        assert_eq!(a.modules(), None);
        assert!(!AccessRow { ownership: "platform".into(), billing_suspended: true, ..Default::default() }.suspended());
    }
}
