//! Loyalty engine — preserves the original Pablo Loyalty rules
//! (every `threshold` spent earns `points_per` points, product-level overrides,
//! referrer receives a share of the referred customer's points) and records
//! every change in `loyalty_ledger`.

use chrono::{Duration, NaiveDate};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{rule, AppResult};
use crate::settings::TenantSettings;

/// One line's part in a sale's points: whether it earns, its rate (product override or the business's) and the amount
/// it earns on (net or gross per the earning base, already reduced when redeemed value earns nothing).
pub struct EarnLine {
    pub eligible: bool,
    pub threshold: Option<Decimal>,
    pub points_per: Option<i32>,
    pub amount: Decimal,
}

/// Roadmap 106: a sale's points are worked out on the eligible amount per earning rate (not rounded line by line,
/// so 95,000 at 1 point per 1,000 is 95 even across several lines), then shared back over the lines (largest
/// remainder) so returns reverse exactly what each line earned. Returns (total, per line).
pub fn sale_points(s: &TenantSettings, lines: &[EarnLine]) -> (i64, Vec<i64>) {
    let mut per_line = vec![0i64; lines.len()];
    if !s.loyalty.enabled {
        return (0, per_line);
    }
    // Group by rate.
    let mut groups: Vec<((Decimal, i64), Vec<usize>)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let threshold = l.threshold.unwrap_or(s.loyalty.threshold);
        let per = l.points_per.map(i64::from).unwrap_or(s.loyalty.points_per);
        if !l.eligible || l.amount <= Decimal::ZERO || threshold <= Decimal::ZERO || per <= 0 {
            continue;
        }
        match groups.iter_mut().find(|(k, _)| *k == (threshold, per)) {
            Some((_, v)) => v.push(i),
            None => groups.push(((threshold, per), vec![i])),
        }
    }
    let mut total = 0i64;
    for ((threshold, per), idx) in groups {
        let amount: Decimal = idx.iter().map(|&i| lines[i].amount).sum();
        let blocks = amount / threshold;
        // "Nearest" rounds halves up (2.5 → 3), as people expect — not banker's rounding.
        let blocks = if s.loyalty.rounding == "nearest" { blocks.round_dp_with_strategy(0, rust_decimal::RoundingStrategy::MidpointAwayFromZero) } else { blocks.floor() };
        let points = blocks.to_i64().unwrap_or(0) * per;
        total += points;
        // Share the group's points over its lines by amount; leftovers to the largest remainders.
        let mut given = 0i64;
        let mut rest: Vec<(Decimal, usize)> = Vec::new();
        for &i in &idx {
            let exact = Decimal::from(points) * lines[i].amount / amount;
            let whole = exact.floor().to_i64().unwrap_or(0);
            per_line[i] = whole;
            given += whole;
            rest.push((exact - Decimal::from(whole), i));
        }
        rest.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, i) in rest.into_iter().take((points - given).max(0) as usize) {
            per_line[i] += 1;
        }
    }
    (total, per_line)
}

pub fn referral_bonus(s: &TenantSettings, points: i64) -> i64 {
    points * s.loyalty.referral_bonus_percent / 100
}

/// A purchase from before S'Shop (onboarding imports, roadmap 100): totals and tier as for any purchase, and the
/// last purchase only moves forward to the original date (never to today).
pub async fn record_past_purchase(conn: &mut PgConnection, s: &TenantSettings, customer_id: Uuid, spend: Decimal, at: chrono::DateTime<chrono::Utc>) -> AppResult<()> {
    let total: Decimal = sqlx::query_scalar(
        "UPDATE customers SET total_spend = GREATEST(total_spend + $2, 0), purchase_count = purchase_count + 1,
                last_purchase_at = GREATEST(COALESCE(last_purchase_at, $3), $3), updated_at = now()
         WHERE id = $1 RETURNING total_spend",
    )
    .bind(customer_id)
    .bind(spend)
    .bind(at)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE customers SET tier = $2 WHERE id = $1").bind(customer_id).bind(s.tier_for(total)).execute(&mut *conn).await?;
    Ok(())
}

/// Record purchase totals and recompute the customer's tier.
pub async fn record_purchase(conn: &mut PgConnection, s: &TenantSettings, customer_id: Uuid, spend_delta: Decimal, count_delta: i32) -> AppResult<()> {
    let total: Decimal = sqlx::query_scalar(
        "UPDATE customers SET total_spend = GREATEST(total_spend + $2, 0), purchase_count = GREATEST(purchase_count + $3, 0),
                last_purchase_at = CASE WHEN $3 > 0 THEN now() ELSE last_purchase_at END, updated_at = now()
         WHERE id = $1 RETURNING total_spend",
    )
    .bind(customer_id)
    .bind(spend_delta)
    .bind(count_delta)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("UPDATE customers SET tier = $2 WHERE id = $1")
        .bind(customer_id)
        .bind(s.tier_for(total))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

fn expiry(s: &TenantSettings, today: NaiveDate) -> Option<NaiveDate> {
    (s.loyalty.expiry_days > 0).then(|| today + Duration::days(s.loyalty.expiry_days))
}

/// Award points for a completed sale, plus the referrer's bonus. Returns (customer points, referral bonus).
pub async fn award_sale(
    conn: &mut PgConnection,
    s: &TenantSettings,
    tenant_id: Uuid,
    customer_id: Uuid,
    sale_id: Uuid,
    points: i64,
    user_id: Option<Uuid>,
    today: NaiveDate,
) -> AppResult<(i64, i64)> {
    if points <= 0 {
        return Ok((0, 0));
    }
    let expires = expiry(s, today);
    sqlx::query(
        "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, expires_at, user_id)
         VALUES ($1,$2,'earn',$3,$4,$5,$6)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(points)
    .bind(sale_id)
    .bind(expires)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE customers SET own_points = own_points + $2, points_available = points_available + $2 WHERE id = $1")
        .bind(customer_id)
        .bind(points)
        .execute(&mut *conn)
        .await?;

    let referral: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, referrer_id FROM referrals WHERE tenant_id = $1 AND referred_id = $2 AND is_active",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_optional(&mut *conn)
    .await?;

    let mut bonus = 0;
    if let Some((referral_id, referrer_id)) = referral {
        bonus = referral_bonus(s, points);
        if bonus > 0 {
            sqlx::query(
                "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, referral_id, expires_at, user_id)
                 VALUES ($1,$2,'referral',$3,$4,$5,$6,$7)",
            )
            .bind(tenant_id)
            .bind(referrer_id)
            .bind(bonus)
            .bind(sale_id)
            .bind(referral_id)
            .bind(expires)
            .bind(user_id)
            .execute(&mut *conn)
            .await?;
            sqlx::query("UPDATE customers SET referral_points = referral_points + $2, points_available = points_available + $2 WHERE id = $1")
                .bind(referrer_id)
                .bind(bonus)
                .execute(&mut *conn)
                .await?;
            sqlx::query("UPDATE referrals SET bonus_points_earned = bonus_points_earned + $2 WHERE id = $1")
                .bind(referral_id)
                .bind(bonus)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok((points, bonus))
}

/// Reverse a share of the points a sale awarded (returns / cancellations),
/// including the matching share of any referral bonus. Never takes a balance below zero.
pub async fn reverse_sale(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    sale_id: Uuid,
    points_to_reverse: i64,
    user_id: Option<Uuid>,
    note: &str,
) -> AppResult<i64> {
    if points_to_reverse <= 0 {
        return Ok(0);
    }
    let earned: Vec<(Uuid, String, i64, Option<Uuid>)> = sqlx::query_as(
        "SELECT customer_id, kind, SUM(points)::bigint, MAX(referral_id::text)::uuid FROM loyalty_ledger
         WHERE sale_id = $1 AND kind IN ('earn','referral') GROUP BY customer_id, kind",
    )
    .bind(sale_id)
    .fetch_all(&mut *conn)
    .await?;
    let own_earned: i64 = earned.iter().filter(|e| e.1 == "earn").map(|e| e.2).sum();
    if own_earned <= 0 {
        return Ok(0);
    }
    let mut reversed_own = 0;
    for (customer_id, kind, pts, referral_id) in earned {
        let share = if kind == "earn" { points_to_reverse.min(pts) } else { pts * points_to_reverse.min(own_earned) / own_earned };
        if share <= 0 {
            continue;
        }
        let available: i64 = sqlx::query_scalar("SELECT points_available FROM customers WHERE id = $1 FOR UPDATE")
            .bind(customer_id)
            .fetch_one(&mut *conn)
            .await?;
        let take = share.min(available.max(0));
        if take <= 0 {
            continue;
        }
        sqlx::query(
            "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, referral_id, notes, user_id)
             VALUES ($1,$2,'reversal',$3,$4,$5,$6,$7)",
        )
        .bind(tenant_id)
        .bind(customer_id)
        .bind(-take)
        .bind(sale_id)
        .bind(referral_id)
        .bind(note)
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
        let col = if kind == "earn" { "own_points" } else { "referral_points" };
        sqlx::query(&format!(
            "UPDATE customers SET {col} = GREATEST({col} - $2, 0), points_available = points_available - $2 WHERE id = $1"
        ))
        .bind(customer_id)
        .bind(take)
        .execute(&mut *conn)
        .await?;
        if let (Some(rid), "referral") = (referral_id, kind.as_str()) {
            sqlx::query("UPDATE referrals SET bonus_points_earned = GREATEST(bonus_points_earned - $2, 0) WHERE id = $1")
                .bind(rid)
                .bind(take)
                .execute(&mut *conn)
                .await?;
        }
        if kind == "earn" {
            reversed_own = take;
        }
    }
    Ok(reversed_own)
}

/// Spend points (sale redemption or manual). Returns the monetary value.
pub async fn redeem(
    conn: &mut PgConnection,
    s: &TenantSettings,
    tenant_id: Uuid,
    customer_id: Uuid,
    points: i64,
    sale_id: Option<Uuid>,
    user_id: Option<Uuid>,
    note: &str,
) -> AppResult<Decimal> {
    s.loyalty.redemption_open(chrono::Utc::now()).map_err(rule)?;
    if points <= 0 {
        return Err(rule("Enter the number of points to redeem"));
    }
    let available: i64 = sqlx::query_scalar("SELECT points_available FROM customers WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(customer_id)
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await?;
    if available < s.loyalty.min_redemption_points {
        return Err(rule(format!("A minimum balance of {} points is required to redeem", s.loyalty.min_redemption_points)));
    }
    if points > available {
        return Err(rule(format!("Only {available} points available")));
    }
    sqlx::query(
        "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, notes, user_id) VALUES ($1,$2,'redeem',$3,$4,$5,$6)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(-points)
    .bind(sale_id)
    .bind(note)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE customers SET points_redeemed = points_redeemed + $2, points_available = points_available - $2 WHERE id = $1")
        .bind(customer_id)
        .bind(points)
        .execute(&mut *conn)
        .await?;
    Ok((Decimal::from(points) * s.loyalty.point_value).round_dp(2))
}

/// FIFO expiry: points earned before today whose expiry date passed and that
/// have not already been consumed by redemptions/expiries/reversals.
pub async fn expire_due(conn: &mut PgConnection, tenant_id: Uuid, today: NaiveDate) -> AppResult<i64> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "WITH due AS (
            SELECT customer_id, SUM(points) AS due FROM loyalty_ledger
            WHERE tenant_id = $1 AND kind IN ('earn','referral') AND expires_at IS NOT NULL AND expires_at <= $2
            GROUP BY customer_id),
         used AS (
            SELECT customer_id, -SUM(points) AS used FROM loyalty_ledger
            WHERE tenant_id = $1 AND kind IN ('redeem','expire','reversal') GROUP BY customer_id)
         SELECT d.customer_id, LEAST(d.due - COALESCE(u.used, 0), c.points_available)::bigint
         FROM due d LEFT JOIN used u USING (customer_id) JOIN customers c ON c.id = d.customer_id
         WHERE d.due - COALESCE(u.used, 0) > 0 AND c.points_available > 0",
    )
    .bind(tenant_id)
    .bind(today)
    .fetch_all(&mut *conn)
    .await?;

    let mut total = 0;
    for (customer_id, pts) in rows {
        if pts <= 0 {
            continue;
        }
        sqlx::query("INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, notes) VALUES ($1,$2,'expire',$3,'Points expired')")
            .bind(tenant_id)
            .bind(customer_id)
            .bind(-pts)
            .execute(&mut *conn)
            .await?;
        sqlx::query("UPDATE customers SET points_expired = points_expired + $2, points_available = points_available - $2 WHERE id = $1")
            .bind(customer_id)
            .bind(pts)
            .execute(&mut *conn)
            .await?;
        total += pts;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(eligible: bool, threshold: Option<i64>, per: Option<i32>, amount: i64) -> EarnLine {
        EarnLine { eligible, threshold: threshold.map(|t| Decimal::new(t, 0)), points_per: per, amount: Decimal::new(amount, 0) }
    }

    #[test]
    fn legacy_points_rule() {
        let s = TenantSettings::default(); // 1 point per 500
        assert_eq!(sale_points(&s, &[line(true, None, None, 1499)]).0, 2);
        // product override: 2 points per 1000 (legacy "Classic Watch")
        assert_eq!(sale_points(&s, &[line(true, Some(1000), Some(2), 5000)]).0, 10);
        assert_eq!(sale_points(&s, &[line(false, None, None, 5000)]).0, 0);
    }

    #[test]
    fn points_on_the_payable_amount_not_line_by_line() {
        // Roadmap 106 brief: 100,000 − 5,000 discount = 95,000 payable at 1 point per 1,000 → 95.
        let mut s = TenantSettings::default();
        s.loyalty.threshold = Decimal::new(1000, 0);
        assert_eq!(sale_points(&s, &[line(true, None, None, 95_000)]).0, 95);
        // Spread over lines that each round down (3 × 999 = 2,997 → 2 points, not 0); shared back without loss.
        let (total, per_line) = sale_points(&s, &[line(true, None, None, 999), line(true, None, None, 999), line(true, None, None, 999)]);
        assert_eq!(total, 2);
        assert_eq!(per_line.iter().sum::<i64>(), 2);
        // Different rates are kept apart; ineligible lines earn nothing.
        let (total, per_line) = sale_points(&s, &[line(true, None, None, 2500), line(true, Some(500), Some(3), 1000), line(false, None, None, 9000)]);
        assert_eq!((total, per_line), (2 + 6, vec![2, 6, 0]));
        s.loyalty.rounding = "nearest".into();
        assert_eq!(sale_points(&s, &[line(true, None, None, 2500)]).0, 3);
    }

    #[test]
    fn redemption_window_and_cap() {
        let mut l = TenantSettings::default().loyalty;
        let now = chrono::Utc::now();
        assert!(l.redemption_open(now).is_ok());
        l.redemption_window_open = false;
        assert!(l.redemption_open(now).is_err());
        l.redemption_window_open = true;
        l.redemption_opens_at = Some(now + chrono::Duration::hours(1));
        assert!(l.redemption_open(now).is_err());
        l.redemption_opens_at = Some(now - chrono::Duration::hours(1));
        l.redemption_closes_at = Some(now - chrono::Duration::minutes(1));
        assert!(l.redemption_open(now).is_err());
        l.max_redemption_percent = 90;
        assert_eq!(l.redemption_percent(), 50);
    }

    #[test]
    fn referral_half() {
        let s = TenantSettings::default();
        assert_eq!(referral_bonus(&s, 57), 28);
    }
}
