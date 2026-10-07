//! Platform billing (roadmap 37–40): billing periods, a business's billing status, invoice numbering, settling a
//! verified payment (invoice → payment → receipt → period → next due) and automatic renewal invoices.
//! Used by the platform owner screens (routes/platform.rs), the business's own Billing page (routes/billing.rs),
//! the Paystack webhook and the background jobs — one implementation for all of them.

use chrono::{DateTime, Months, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::error::{rule, AppError, AppResult};

/// Billing dates follow the platform owner's calendar (Kenya).
pub const PLATFORM_TZ: &str = "Africa/Nairobi";
/// An invoice for the next period is issued this many days before it starts (auto-renew), and a business is
/// "due soon" within this window.
pub const DUE_SOON_DAYS: i64 = 7;

pub fn today() -> NaiveDate {
    crate::util::today_in(crate::util::parse_tz(PLATFORM_TZ))
}

pub const FREQUENCIES: [&str; 5] = ["monthly", "quarterly", "semi_annual", "annual", "custom"];

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
    pub model: String,
    pub currency: String,
    pub one_off_amount: Decimal,
    pub recurring: bool,
    pub amount: Decimal,
    pub frequency: String,
    pub custom_months: i32,
    pub start_date: Option<NaiveDate>,
    pub next_due_date: Option<NaiveDate>,
    pub grace_days: i32,
    pub auto_renew: bool,
    pub notes: String,
    pub updated_at: DateTime<Utc>,
}

pub async fn plan(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<Option<Plan>> {
    Ok(sqlx::query_as(
        "SELECT tenant_id, model, currency, one_off_amount, recurring, amount, frequency, custom_months, start_date, next_due_date,
                grace_days, auto_renew, notes, updated_at FROM billing_plans WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The recurring fee's category: a subscription, or maintenance after a one-off fee.
pub fn recurring_category(p: &Plan) -> &'static str {
    if p.model == "subscription" { "subscription" } else { "maintenance" }
}

/// A business's billing position — the same figures on the platform directory, dashboard and the business's
/// own Billing page.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Summary {
    /// not_set | paid | pending | due_soon | grace | overdue
    pub status: String,
    pub model: Option<String>,
    pub currency: String,
    /// Recurring fee (subscription or maintenance) and its frequency.
    pub amount: Option<Decimal>,
    pub frequency: Option<String>,
    pub custom_months: Option<i32>,
    pub next_due: Option<NaiveDate>,
    pub grace_days: i32,
    pub outstanding: Decimal,
    pub open_invoices: i64,
    pub oldest_open_due: Option<NaiveDate>,
    pub last_payment_at: Option<DateTime<Utc>>,
    pub last_payment_amount: Option<Decimal>,
    /// Latest paid recurring period.
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub one_off_amount: Option<Decimal>,
    /// paid | pending (one-off model only).
    pub one_off_status: Option<String>,
    pub maintenance: bool,
    pub paid_total: Decimal,
}

pub async fn summary(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<Summary> {
    let Some(p) = plan(conn, tenant_id).await? else {
        let (outstanding, open): (Option<Decimal>, i64) =
            sqlx::query_as("SELECT SUM(amount), COUNT(*) FROM billing_documents WHERE tenant_id = $1 AND kind = 'invoice' AND status = 'open'")
                .bind(tenant_id)
                .fetch_one(&mut *conn)
                .await?;
        return Ok(Summary { status: "not_set".into(), currency: "KES".into(), outstanding: outstanding.unwrap_or_default(), open_invoices: open, ..Default::default() });
    };
    let (outstanding, open, oldest): (Option<Decimal>, i64, Option<NaiveDate>) = sqlx::query_as(
        "SELECT SUM(amount), COUNT(*), MIN(due_date) FROM billing_documents WHERE tenant_id = $1 AND kind = 'invoice' AND status = 'open'",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await?;
    let last: Option<(DateTime<Utc>, Decimal)> = sqlx::query_as(
        "SELECT paid_at, amount FROM billing_payments WHERE tenant_id = $1 AND status = 'success' ORDER BY paid_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let paid_total: Option<Decimal> = sqlx::query_scalar("SELECT SUM(amount) FROM billing_payments WHERE tenant_id = $1 AND status = 'success'")
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await?;
    let covered: Option<(NaiveDate, NaiveDate)> = sqlx::query_as(
        "SELECT period_start, period_end FROM billing_documents
         WHERE tenant_id = $1 AND kind = 'invoice' AND status = 'paid' AND category = $2 AND period_end IS NOT NULL
         ORDER BY period_end DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(recurring_category(&p))
    .fetch_optional(&mut *conn)
    .await?;
    let one_off_status = if p.model == "one_off" {
        let paid: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM billing_documents WHERE tenant_id = $1 AND kind = 'invoice' AND category = 'one_off' AND status = 'paid')",
        )
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await?;
        Some(if paid { "paid" } else { "pending" }.to_string())
    } else {
        None
    };

    let today = today();
    let grace = chrono::Duration::days(p.grace_days as i64);
    let status = match oldest {
        Some(due) if due + grace < today => "overdue",
        Some(due) if due < today => "grace",
        Some(_) => "due_soon",
        None if p.recurring && p.next_due_date.is_some_and(|d| (d - today).num_days() <= DUE_SOON_DAYS) => "due_soon",
        None if one_off_status.as_deref() == Some("pending") => "pending",
        None if paid_total.is_none() && p.recurring => "pending",
        None => "paid",
    };
    Ok(Summary {
        status: status.into(),
        model: Some(p.model.clone()),
        currency: p.currency.clone(),
        amount: p.recurring.then_some(p.amount),
        frequency: p.recurring.then(|| p.frequency.clone()),
        custom_months: p.recurring.then_some(p.custom_months),
        next_due: if p.recurring { p.next_due_date } else { None },
        grace_days: p.grace_days,
        outstanding: outstanding.unwrap_or_default(),
        open_invoices: open,
        oldest_open_due: oldest,
        last_payment_at: last.map(|l| l.0),
        last_payment_amount: last.map(|l| l.1),
        period_start: covered.map(|c| c.0),
        period_end: covered.map(|c| c.1),
        one_off_amount: (p.model == "one_off").then_some(p.one_off_amount),
        one_off_status,
        maintenance: p.model == "one_off" && p.recurring,
        paid_total: paid_total.unwrap_or_default(),
    })
}

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
    pub kind: &'a str,
    pub category: &'a str,
    pub description: String,
    pub amount: Decimal,
    pub currency: String,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub quotation_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
}

pub async fn insert_document(conn: &mut PgConnection, d: NewDocument<'_>) -> AppResult<(Uuid, String)> {
    let number = next_number(conn, d.kind).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_documents (tenant_id, kind, number, category, description, amount, currency, issue_date, due_date,
                                        period_start, period_end, quotation_id, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING id",
    )
    .bind(d.tenant_id)
    .bind(d.kind)
    .bind(&number)
    .bind(d.category)
    .bind(&d.description)
    .bind(d.amount)
    .bind(&d.currency)
    .bind(d.issue_date)
    .bind(d.due_date)
    .bind(d.period.map(|p| p.0))
    .bind(d.period.map(|p| p.1))
    .bind(d.quotation_id)
    .bind(d.created_by)
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

/// The invoice for the plan's next recurring period (starting at `next_due_date`, due that day).
pub async fn issue_next_period(conn: &mut PgConnection, p: &Plan, created_by: Option<Uuid>) -> AppResult<(Uuid, String)> {
    if !p.recurring {
        return Err(rule("This billing plan has no recurring fee"));
    }
    let start = p.next_due_date.ok_or_else(|| rule("Set the next due date first"))?;
    let (start, end) = period(start, months(&p.frequency, p.custom_months));
    let what = if p.model == "subscription" { "S'Shop subscription" } else { "S'Shop maintenance" };
    let issue = today().min(start);
    insert_document(
        conn,
        NewDocument {
            tenant_id: p.tenant_id,
            kind: "invoice",
            category: recurring_category(p),
            description: format!("{what} — {} ({} – {})", frequency_label(&p.frequency, p.custom_months), start.format("%d %b %Y"), end.format("%d %b %Y")),
            amount: p.amount,
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
/// recurring period → next due date moved past the period. Idempotent: a payment already settled (webhook and
/// verify racing) returns `Ok(None)`. A second successful payment for an already-paid invoice is kept as
/// a success with a note so it can be refunded — money that arrived is never discarded.
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
    let (inv_status, inv_number, category, period_end): (String, String, String, Option<NaiveDate>) = sqlx::query_as(
        "SELECT status, number, category, period_end FROM billing_documents WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
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
                    "UPDATE billing_plans SET next_due_date = GREATEST(COALESCE(next_due_date, $2), $2), updated_at = now() WHERE tenant_id = $1",
                )
                .bind(p.tenant_id)
                .bind(end.succ_opt().unwrap_or(end))
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
            "receipt": receipt, "note": note,
        })),
        ip,
        "",
    )
    .await?;
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

/// Background job: renewal invoices for plans with auto-renew (issued DUE_SOON_DAYS before the period starts)
/// and reconciliation of Paystack payments left pending (closed browser, missed webhook).
pub async fn run_jobs(state: &crate::state::AppState) -> anyhow::Result<()> {
    let soon = today() + chrono::Duration::days(DUE_SOON_DAYS);
    let due: Vec<Uuid> = sqlx::query_scalar(
        "SELECT p.tenant_id FROM billing_plans p JOIN tenants t ON t.id = p.tenant_id
         WHERE p.recurring AND p.auto_renew AND t.status = 'active' AND p.next_due_date <= $1
           AND NOT EXISTS (SELECT 1 FROM billing_documents d WHERE d.tenant_id = p.tenant_id AND d.kind = 'invoice'
                           AND d.status <> 'void' AND d.period_start = p.next_due_date
                           AND d.category = CASE WHEN p.model = 'subscription' THEN 'subscription' ELSE 'maintenance' END)",
    )
    .bind(soon)
    .fetch_all(&state.db)
    .await?;
    for tenant_id in due {
        let mut tx = state.db.begin().await?;
        let Some(p) = plan(&mut tx, tenant_id).await? else { continue };
        match issue_next_period(&mut tx, &p, None).await {
            Ok((id, number)) => {
                audit::system(&mut tx, tenant_id, None, Entry::new("billing", "invoice_issued", "billing_document", id).after(json!({ "number": number, "amount": p.amount, "auto": true })), "", "").await?;
                tx.commit().await?;
                tracing::info!(%tenant_id, number, "renewal invoice issued");
            }
            Err(e) => tracing::warn!(%tenant_id, error = %e, "renewal invoice not issued"),
        }
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
}
