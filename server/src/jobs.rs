//! Background housekeeping, every 15 minutes:
//! points expiry, overdue-credit alerts (+ optional WhatsApp reminders),
//! stale M-Pesa requests and old portal codes.

use std::time::Duration;

use rust_decimal::Decimal;
use uuid::Uuid;

use crate::notify::{self, Note};
use crate::settings::TenantSettings;
use crate::state::AppState;
use crate::util::{money_str, parse_tz, today_in};

pub fn spawn(state: AppState) {
    // Test runs switch the scheduler off so results never depend on when it happens to run (BACKGROUND_JOBS=off).
    if std::env::var("BACKGROUND_JOBS").is_ok_and(|v| v.eq_ignore_ascii_case("off")) {
        tracing::warn!("background jobs are switched off (BACKGROUND_JOBS=off) — never do this in production");
        return;
    }
    // Roadmap 91: sales waiting for an STK Push are checked every minute (late or lost callbacks).
    let sweeper = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(e) = crate::routes::stk_sales::sweep(&sweeper).await {
                tracing::warn!(error = %e, "stk sales sweep");
                crate::alerts::report("job", "M-Pesa payment check failed", format!("{e:#}"), "", "STK sales sweep (every minute)");
            }
        }
    });
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut tick = tokio::time::interval(Duration::from_secs(15 * 60));
        loop {
            tick.tick().await;
            if let Err(e) = run_once(&state).await {
                tracing::error!(error = ?e, "background job failed");
                crate::alerts::report("job", "Background job failed", format!("{e:#}"), "", "Housekeeping (every 15 min)");
            }
        }
    });
}

async fn run_once(state: &AppState) -> anyhow::Result<()> {
    if let Err(e) = crate::billing::run_jobs(state).await {
        tracing::warn!(error = %format!("{e:#}"), "billing jobs");
        crate::alerts::report("job", "Billing job failed", format!("{e:#}"), "", "Billing renewals, trials and reminders");
    }
    sqlx::query(
        "UPDATE mpesa_requests SET status = 'timeout', result_desc = 'No response from M-Pesa', updated_at = now()
         WHERE status = 'pending' AND purpose <> 'sale' AND created_at < now() - interval '10 minutes'",
    )
    .execute(&state.db)
    .await?;
    sqlx::query("DELETE FROM portal_otps WHERE created_at < now() - interval '1 day'").execute(&state.db).await?;
    // Roadmap 147: resolved errors and old start records are kept for 90 days.
    sqlx::query("DELETE FROM error_events WHERE resolved_at < now() - interval '90 days'").execute(&state.db).await?;
    sqlx::query("DELETE FROM service_starts WHERE started_at < now() - interval '90 days'").execute(&state.db).await?;

    let tenants: Vec<(Uuid, String, String, serde_json::Value)> = sqlx::query_as("SELECT id, name, timezone, settings FROM tenants")
        .fetch_all(&state.db)
        .await?;
    for (tenant_id, business, tz, raw) in tenants {
        let settings: TenantSettings = serde_json::from_value(raw).unwrap_or_default();
        let today = today_in(parse_tz(&tz));

        if settings.loyalty.enabled && settings.loyalty.expiry_days > 0 {
            let mut tx = state.db.begin().await?;
            let expired = crate::loyalty::expire_due(&mut tx, tenant_id, today).await?;
            tx.commit().await?;
            if expired > 0 {
                tracing::info!(%tenant_id, expired, "loyalty points expired");
            }
        }

        let overdue: Vec<(Uuid, Uuid, String, String, String, Decimal, chrono::NaiveDate)> = sqlx::query_as(
            "SELECT cs.id, cs.branch_id, TRIM(c.first_name || ' ' || c.other_names), c.mobile, s.receipt_no,
                    cs.original_amount - cs.amount_paid - cs.adjustments, cs.due_date
             FROM credit_sales cs JOIN customers c ON c.id = cs.customer_id JOIN sales s ON s.id = cs.sale_id
                  LEFT JOIN import_batches ib ON ib.id = cs.import_batch_id
             WHERE cs.tenant_id = $1 AND cs.status IN ('outstanding','partially_paid') AND cs.due_date < $2
               AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.tenant_id = $1 AND n.dedupe_key = 'credit_overdue:' || cs.id)
               -- Roadmap 100: debts imported already overdue never trigger automatic alerts or customer reminders.
               AND (ib.id IS NULL OR cs.due_date >= (ib.posted_at AT TIME ZONE 'UTC')::date)",
        )
        .bind(tenant_id)
        .bind(today)
        .fetch_all(&state.db)
        .await?;
        for (id, branch, name, mobile, receipt, balance, due) in overdue {
            notify::to_permission(
                state,
                tenant_id,
                Some(branch),
                "credit.view",
                Note::new("credit_overdue", format!("Overdue credit: {name}"), format!("KSh {} on {receipt} was due {}", money_str(balance), due.format("%d/%m")), format!("/credit/{id}"))
                    .dedupe(format!("credit_overdue:{id}")),
            )
            .await;
            if settings.notifications.whatsapp_credit_reminders {
                let first = name.split(' ').next().unwrap_or_default().to_string();
                notify::whatsapp(
                    state,
                    tenant_id,
                    mobile,
                    format!("Hi {first}, a friendly reminder from {business}: KSh {} on receipt {receipt} was due on {}. Thank you! 🙏", money_str(balance), due.format("%d/%m/%Y")),
                );
            }
        }
    }
    Ok(())
}
