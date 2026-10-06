//! Customers → Loyalty & Rewards: overview, points ledger, redemption,
//! manual adjustments, referrals and award periods (Gold / Silver / Bronze).

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{like, Counted, Page, Paged};
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, rule, AppError, AppResult};
use crate::integrations::whatsapp;
use crate::loyalty;
use crate::settings;
use crate::state::AppState;
use crate::util::{local_range, money_str};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/loyalty/overview", get(overview))
        .route("/customers/{id}/loyalty", get(ledger))
        .route("/customers/{id}/redeem", post(redeem))
        .route("/customers/{id}/points", post(adjust))
        .route("/referrals", get(list_referrals).post(create_referral))
        .route("/referrals/{id}/deactivate", post(deactivate_referral))
        .route("/awards", get(list_awards).post(open_period))
        .route("/awards/{id}/close", post(close_period))
        .route("/awards/message/{customer_id}", post(message_customer))
}

async fn overview(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("customers.view_loyalty")?;
    let (issued, referral, redeemed, expired, outstanding, members): (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(own_points),0)::bigint, COALESCE(SUM(referral_points),0)::bigint, COALESCE(SUM(points_redeemed),0)::bigint,
                COALESCE(SUM(points_expired),0)::bigint, COALESCE(SUM(points_available),0)::bigint,
                COUNT(*) FILTER (WHERE points_available > 0)
         FROM customers WHERE tenant_id = $1",
    )
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let tiers: Vec<(String, i64)> = sqlx::query_as(
        "SELECT COALESCE(NULLIF(tier,''),'—'), COUNT(*) FROM customers WHERE tenant_id = $1 AND is_active GROUP BY 1 ORDER BY 2 DESC",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let top: Vec<(Uuid, String, String, Decimal, i64, i64, String)> = sqlx::query_as(
        "SELECT id, TRIM(first_name || ' ' || other_names), mobile, total_spend, own_points, referral_points, tier
         FROM customers WHERE tenant_id = $1 AND is_active ORDER BY total_spend DESC LIMIT 10",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let mut conn = state.db.acquire().await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;
    Ok(Json(json!({
        "totals": {
            "own_points": issued, "referral_points": referral, "redeemed": redeemed, "expired": expired,
            "outstanding": outstanding, "outstanding_value": (Decimal::from(outstanding) * s.loyalty.point_value).round_dp(2),
            "members_with_points": members,
        },
        "tiers": tiers.into_iter().map(|(t, n)| json!({ "tier": t, "count": n })).collect::<Vec<_>>(),
        "top_customers": top.into_iter().map(|(id, name, mobile, spend, own, refp, tier)| json!({
            "id": id, "name": name, "mobile": mobile, "total_spend": spend, "own_points": own, "referral_points": refp, "tier": tier,
        })).collect::<Vec<_>>(),
        "rules": s.loyalty,
    })))
}

#[derive(Serialize, sqlx::FromRow)]
struct LedgerRow {
    id: Uuid,
    kind: String,
    points: i64,
    receipt_no: Option<String>,
    notes: String,
    expires_at: Option<NaiveDate>,
    user_name: Option<String>,
    created_at: DateTime<Utc>,
}

async fn ledger(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Query(page): Query<Page>) -> AppResult<Json<Paged<LedgerRow>>> {
    ctx.require("customers.view_loyalty")?;
    let rows: Vec<Counted<LedgerRow>> = sqlx::query_as(
        "SELECT COUNT(*) OVER() AS total_count, l.id, l.kind, l.points, s.receipt_no, l.notes, l.expires_at, u.name AS user_name, l.created_at
         FROM loyalty_ledger l LEFT JOIN sales s ON s.id = l.sale_id LEFT JOIN users u ON u.id = l.user_id
         WHERE l.customer_id = $1 AND l.tenant_id = $2 ORDER BY l.created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into()))
}

#[derive(Deserialize)]
struct PointsBody {
    points: i64,
    #[serde(default)]
    notes: String,
}

/// Redeem points outside a sale (e.g. a gift).
async fn redeem(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<PointsBody>) -> AppResult<Json<Value>> {
    ctx.require("customers.redeem_points")?;
    let mut tx = state.db.begin().await?;
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let note = if b.notes.trim().is_empty() { "Redeemed" } else { b.notes.trim() };
    let value = loyalty::redeem(&mut tx, &s, ctx.tenant_id, id, b.points, None, Some(ctx.user_id), note).await?;
    audit::record(&mut tx, &ctx, Entry::new("loyalty", "redeem", "customer", id).after(json!({ "points": b.points, "value": value })).comments(note))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "redeemed": b.points, "value": value })))
}

/// Manual correction (signed). Never takes the balance below zero.
async fn adjust(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<PointsBody>) -> AppResult<Json<Value>> {
    ctx.require("loyalty.manage")?;
    if b.points == 0 {
        return Err(bad("Enter the points to add or remove"));
    }
    if b.notes.trim().is_empty() {
        return Err(bad("A reason is required"));
    }
    let mut tx = state.db.begin().await?;
    let available: i64 = sqlx::query_scalar("SELECT points_available FROM customers WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(id)
        .bind(ctx.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound("Customer"))?;
    if available + b.points < 0 {
        return Err(rule(format!("The customer only has {available} points")));
    }
    sqlx::query("INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, notes, user_id) VALUES ($1,$2,'adjust',$3,$4,$5)")
        .bind(ctx.tenant_id)
        .bind(id)
        .bind(b.points)
        .bind(b.notes.trim())
        .bind(ctx.user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE customers SET own_points = GREATEST(own_points + $2, 0), points_available = points_available + $2 WHERE id = $1")
        .bind(id)
        .bind(b.points)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("loyalty", "adjust", "customer", id)
            .before(json!({ "points_available": available }))
            .after(json!({ "points_available": available + b.points }))
            .comments(b.notes.trim()),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "points_available": available + b.points })))
}

// ───────────────────────────── Referrals ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct ReferralRow {
    id: Uuid,
    referrer_id: Uuid,
    referrer_name: String,
    referred_id: Uuid,
    referred_name: String,
    bonus_points_earned: i64,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct RefQuery {
    q: Option<String>,
}

async fn list_referrals(State(state): State<AppState>, ctx: Ctx, Query(q): Query<RefQuery>) -> AppResult<Json<Value>> {
    ctx.require("customers.view")?;
    let rows: Vec<ReferralRow> = sqlx::query_as(
        "SELECT r.id, r.referrer_id, TRIM(a.first_name || ' ' || a.other_names) AS referrer_name,
                r.referred_id, TRIM(b.first_name || ' ' || b.other_names) AS referred_name, r.bonus_points_earned, r.created_at
         FROM referrals r JOIN customers a ON a.id = r.referrer_id JOIN customers b ON b.id = r.referred_id
         WHERE r.tenant_id = $1 AND r.is_active
           AND ($2::text IS NULL OR a.first_name ILIKE $2 OR a.other_names ILIKE $2 OR b.first_name ILIKE $2 OR b.other_names ILIKE $2)
         ORDER BY r.created_at DESC",
    )
    .bind(ctx.tenant_id)
    .bind(like(&q.q))
    .fetch_all(&state.db)
    .await?;
    let referrers = rows.iter().map(|r| r.referrer_id).collect::<std::collections::HashSet<_>>().len();
    let bonus: i64 = rows.iter().map(|r| r.bonus_points_earned).sum();
    Ok(Json(json!({ "items": rows, "summary": { "total": rows.len(), "referrers": referrers, "bonus_points": bonus } })))
}

#[derive(Deserialize)]
struct ReferralBody {
    referrer_id: Uuid,
    referred_id: Uuid,
}

async fn create_referral(State(state): State<AppState>, ctx: Ctx, Json(b): Json<ReferralBody>) -> AppResult<Json<Value>> {
    ctx.require_any(&["loyalty.manage", "customers.create"])?;
    if b.referrer_id == b.referred_id {
        return Err(rule("A customer cannot refer themselves"));
    }
    let mut tx = state.db.begin().await?;
    let found: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE tenant_id = $1 AND id IN ($2, $3)")
        .bind(ctx.tenant_id)
        .bind(b.referrer_id)
        .bind(b.referred_id)
        .fetch_one(&mut *tx)
        .await?;
    if found != 2 {
        return Err(AppError::NotFound("Customer"));
    }
    let circular: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM referrals WHERE referrer_id = $1 AND referred_id = $2 AND is_active)")
        .bind(b.referred_id)
        .bind(b.referrer_id)
        .fetch_one(&mut *tx)
        .await?;
    if circular {
        return Err(rule("These customers already refer each other the other way round"));
    }
    let id: Uuid = sqlx::query_scalar("INSERT INTO referrals (tenant_id, referrer_id, referred_id, created_by) VALUES ($1,$2,$3,$4) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(b.referrer_id)
        .bind(b.referred_id)
        .bind(ctx.user_id)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("loyalty", "create_referral", "referral", id).after(json!({
        "referrer_id": b.referrer_id, "referred_id": b.referred_id
    })))
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

/// Referrals are deactivated, never deleted, so bonus history stays intact.
async fn deactivate_referral(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("loyalty.manage")?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE referrals SET is_active = false WHERE id = $1 AND tenant_id = $2 AND is_active")
        .bind(id)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("Referral"));
    }
    audit::record(&mut tx, &ctx, Entry::new("loyalty", "deactivate_referral", "referral", id)).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ───────────────────────────── Award periods ─────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct Standing {
    customer_id: Uuid,
    name: String,
    mobile: String,
    period_spend: Decimal,
    period_points: i64,
    period_referral_points: i64,
}

async fn standings(conn: &mut sqlx::PgConnection, ctx: &Ctx, start: NaiveDate, end: NaiveDate, limit: i64) -> AppResult<Vec<Standing>> {
    let (from, to) = local_range(start, end, ctx.tz);
    Ok(sqlx::query_as(
        "SELECT c.id AS customer_id, TRIM(c.first_name || ' ' || c.other_names) AS name, c.mobile,
                COALESCE(SUM(s.total),0)::numeric(14,2) AS period_spend,
                COALESCE((SELECT SUM(points) FROM loyalty_ledger l WHERE l.customer_id = c.id AND l.kind IN ('earn','reversal')
                          AND l.referral_id IS NULL AND l.created_at >= $2 AND l.created_at < $3),0)::bigint AS period_points,
                COALESCE((SELECT SUM(points) FROM loyalty_ledger l WHERE l.customer_id = c.id AND l.kind = 'referral'
                          AND l.created_at >= $2 AND l.created_at < $3),0)::bigint AS period_referral_points
         FROM customers c JOIN sales s ON s.customer_id = c.id AND s.status <> 'cancelled' AND s.created_at >= $2 AND s.created_at < $3
         WHERE c.tenant_id = $1 GROUP BY c.id HAVING SUM(s.total) > 0
         ORDER BY period_spend DESC LIMIT $4",
    )
    .bind(ctx.tenant_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?)
}

async fn list_awards(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    ctx.require("customers.view_loyalty")?;
    let periods: Vec<(Uuid, String, NaiveDate, Option<NaiveDate>, String)> = sqlx::query_as(
        "SELECT id, name, start_date, end_date, status FROM award_periods WHERE tenant_id = $1 ORDER BY start_date DESC, created_at DESC",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&state.db)
    .await?;
    let mut conn = state.db.acquire().await?;
    let s = settings::load(&mut conn, ctx.tenant_id).await?;
    let mut out = Vec::new();
    for (id, name, start, end, status) in periods {
        let winners: Vec<(Uuid, String, String, i32, Decimal, i64)> = sqlx::query_as(
            "SELECT customer_id, customer_name, tier, rank, total_spend, points FROM award_winners WHERE period_id = $1 ORDER BY rank",
        )
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
        let live = if status == "open" {
            standings(&mut conn, &ctx, start, ctx.today(), 20).await?
        } else {
            vec![]
        };
        out.push(json!({
            "id": id, "name": name, "start_date": start, "end_date": end, "status": status,
            "winners": winners.into_iter().map(|(cid, cname, tier, rank, spend, pts)| json!({
                "customer_id": cid, "customer_name": cname, "tier": tier, "rank": rank, "total_spend": spend, "points": pts,
            })).collect::<Vec<_>>(),
            "standings": live,
        }));
    }
    Ok(Json(json!({ "periods": out, "winners_per_period": s.loyalty.award_winners })))
}

#[derive(Deserialize)]
struct OpenBody {
    name: String,
    start_date: Option<NaiveDate>,
}

/// Opens a new award round. Unlike the legacy app this never resets balances
/// or deletes sales — standings are computed from activity within the period.
async fn open_period(State(state): State<AppState>, ctx: Ctx, Json(b): Json<OpenBody>) -> AppResult<Json<Value>> {
    ctx.require("loyalty.manage")?;
    if b.name.trim().is_empty() {
        return Err(bad("Name the award period"));
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO award_periods (tenant_id, name, start_date) VALUES ($1,$2,$3) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(b.name.trim())
        .bind(b.start_date.unwrap_or_else(|| ctx.today()))
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("loyalty", "open_award_period", "award_period", id).after(json!({ "name": b.name }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

const TIERS: [&str; 3] = ["Gold", "Silver", "Bronze"];

async fn close_period(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("loyalty.manage")?;
    let mut tx = state.db.begin().await?;
    let (name, start, status): (String, NaiveDate, String) =
        sqlx::query_as("SELECT name, start_date, status FROM award_periods WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("Award period"))?;
    if status != "open" {
        return Err(rule("This award period is already closed"));
    }
    let s = settings::load(&mut tx, ctx.tenant_id).await?;
    let end = ctx.today();
    let top = standings(&mut tx, &ctx, start, end, s.loyalty.award_winners.max(1) as i64).await?;
    let mut winners = Vec::new();
    for (i, st) in top.iter().enumerate() {
        let tier = TIERS.get(i).copied().unwrap_or("Bronze");
        sqlx::query(
            "INSERT INTO award_winners (period_id, customer_id, customer_name, tier, rank, total_spend, points) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(id)
        .bind(st.customer_id)
        .bind(&st.name)
        .bind(tier)
        .bind(i as i32 + 1)
        .bind(st.period_spend)
        .bind(st.period_points + st.period_referral_points)
        .execute(&mut *tx)
        .await?;
        winners.push(json!({ "customer_id": st.customer_id, "name": st.name, "tier": tier, "spend": st.period_spend, "mobile": st.mobile }));
    }
    sqlx::query("UPDATE award_periods SET status = 'closed', end_date = $2 WHERE id = $1 AND tenant_id = $3")
        .bind(id)
        .bind(end)
        .bind(ctx.tenant_id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("loyalty", "close_award_period", "award_period", id).after(json!({ "winners": winners })))
        .await?;
    tx.commit().await?;

    if s.notifications.whatsapp_loyalty {
        let business: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(ctx.tenant_id).fetch_one(&state.db).await?;
        for w in &winners {
            let medal = match w["tier"].as_str() {
                Some("Gold") => "🥇",
                Some("Silver") => "🥈",
                _ => "🥉",
            };
            let text = format!(
                "{medal} Congratulations {}! You are a {} winner in {business}'s {name}. Thank you for being a valued customer! 🎁",
                w["name"].as_str().unwrap_or_default(),
                w["tier"].as_str().unwrap_or_default()
            );
            crate::notify::whatsapp(&state, ctx.tenant_id, w["mobile"].as_str().unwrap_or_default().to_string(), text);
        }
    }
    Ok(Json(json!({ "winners": winners })))
}

/// Personal loyalty message (WhatsApp API if configured, otherwise a wa.me link).
async fn message_customer(State(state): State<AppState>, ctx: Ctx, Path(customer_id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("customers.view_loyalty")?;
    let (first, mobile, spend, points, tier, business): (String, String, Decimal, i64, String, String) = sqlx::query_as(
        "SELECT c.first_name, c.mobile, c.total_spend, c.points_available, c.tier, t.name FROM customers c JOIN tenants t ON t.id = c.tenant_id
         WHERE c.id = $1 AND c.tenant_id = $2",
    )
    .bind(customer_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Customer"))?;
    let tier_line = if tier.is_empty() { String::new() } else { format!("• Tier: {tier}\n") };
    let text = format!(
        "🏆 Hi {first}!\n\nThank you for being a valued {business} customer.\n\nYour current stats:\n• Total spend: KSh {}\n• Points: {points}\n{tier_line}\nKeep shopping to climb the ranks! 🎁",
        money_str(spend)
    );
    let sent = if whatsapp::is_configured(&state) {
        whatsapp::send_notification(&state, Some(ctx.tenant_id), &mobile, &text).await.unwrap_or(false)
    } else {
        false
    };
    Ok(Json(json!({ "sent": sent, "link": whatsapp::deep_link(&mobile, &text) })))
}
