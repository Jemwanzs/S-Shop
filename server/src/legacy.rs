//! One-off import of a Pablo Loyalty (Supabase) database into S'Shop.
//!
//! Maps: app_users → users (PINs re-hashed, fixed roles → role templates),
//! products → products (threshold/points_per become product loyalty rules),
//! customers → customers (location/city/country kept as custom fields),
//! referrals, transactions → legacy sales (+ items), loyalty points → ledger,
//! award periods & winners. Legacy sales carry `is_legacy = true` and create
//! no stock movements (the old app did not track inventory).

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::auth::hash_pin;
use crate::perms::legacy_role;
use crate::settings::TenantSettings;
use crate::util::normalize_mobile;

fn dec(row: &sqlx::postgres::PgRow, col: &str) -> Decimal {
    row.try_get::<Option<Decimal>, _>(col).ok().flatten().unwrap_or_default()
}

fn int(d: Decimal) -> i64 {
    use rust_decimal::prelude::ToPrimitive;
    d.floor().to_i64().unwrap_or(0)
}

pub async fn import(db: &PgPool, source_url: &str, tenant_slug: Option<&str>) -> anyhow::Result<()> {
    let src = PgPoolOptions::new().max_connections(2).connect(source_url).await?;
    let mut tx = db.begin().await?;

    let slug = tenant_slug.unwrap_or("pablo-loyalty");
    let tenant_id: Uuid = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenants WHERE slug = $1").bind(slug).fetch_optional(&mut *tx).await? {
        Some(id) => id,
        None => crate::bootstrap::seed_tenant(&mut tx, "Pablo Loyalty", slug).await?,
    };
    let already: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sales WHERE tenant_id = $1 AND is_legacy)")
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    if already {
        anyhow::bail!("Legacy data has already been imported into '{slug}'");
    }
    let branch_id: Uuid = sqlx::query_scalar("SELECT id FROM branches WHERE tenant_id = $1 ORDER BY created_at LIMIT 1")
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let settings = TenantSettings::default();

    // Users
    let mut users = 0;
    for r in sqlx::query("SELECT name, email, pin, role, is_active FROM app_users").fetch_all(&src).await? {
        let email: String = r.try_get("email")?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE lower(email) = lower($1))")
            .bind(&email)
            .fetch_one(&mut *tx)
            .await?;
        if exists {
            continue;
        }
        let role_name = legacy_role(&r.try_get::<String, _>("role")?);
        let role_id: Uuid = sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND name = $2")
            .bind(tenant_id)
            .bind(role_name)
            .fetch_one(&mut *tx)
            .await?;
        let pin: String = r.try_get("pin")?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (tenant_id, name, email, pin_hash, role_id, is_active, all_branches) VALUES ($1,$2,lower($3),$4,$5,$6,$7) RETURNING id",
        )
        .bind(tenant_id)
        .bind(r.try_get::<String, _>("name")?)
        .bind(&email)
        .bind(hash_pin(&pin)?)
        .bind(role_id)
        .bind(r.try_get::<Option<bool>, _>("is_active")?.unwrap_or(true))
        .bind(role_name == crate::perms::ADMIN_ROLE)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO user_branches (user_id, branch_id) VALUES ($1,$2)").bind(id).bind(branch_id).execute(&mut *tx).await?;
        users += 1;
    }

    // Products
    let mut product_map: HashMap<Uuid, Uuid> = HashMap::new();
    for r in sqlx::query("SELECT id, code, name, is_active, threshold, points_per FROM products").fetch_all(&src).await? {
        let old: Uuid = r.try_get("id")?;
        let code: String = r.try_get("code")?;
        let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM products WHERE tenant_id = $1 AND code = $2")
            .bind(tenant_id)
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await?;
        let id = match existing {
            Some(id) => id,
            None => sqlx::query_scalar(
                "INSERT INTO products (tenant_id, code, name, is_active, loyalty_threshold, loyalty_points_per) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
            )
            .bind(tenant_id)
            .bind(&code)
            .bind(r.try_get::<String, _>("name")?)
            .bind(r.try_get::<Option<bool>, _>("is_active")?.unwrap_or(true))
            .bind(Some(dec(&r, "threshold")).filter(|d| *d > Decimal::ZERO))
            .bind(int(dec(&r, "points_per")) as i32)
            .fetch_one(&mut *tx)
            .await?,
        };
        product_map.insert(old, id);
    }

    // Referral bonuses received per referrer (legacy points_available includes them).
    let mut referral_bonus: HashMap<Uuid, i64> = HashMap::new();
    let legacy_refs = sqlx::query("SELECT id, referrer_id, referred_id, bonus_points_earned, created_at FROM referrals").fetch_all(&src).await?;
    for r in &legacy_refs {
        *referral_bonus.entry(r.try_get("referrer_id")?).or_default() += int(dec(r, "bonus_points_earned"));
    }

    // Customers
    let mut customer_map: HashMap<Uuid, Uuid> = HashMap::new();
    let mut skipped = Vec::new();
    for r in sqlx::query(
        "SELECT id, mobile, email, first_name, other_names, location, city, country, total_spend, total_points_earned,
                points_available, points_redeemed, last_purchase_date, created_at FROM customers",
    )
    .fetch_all(&src)
    .await?
    {
        let old: Uuid = r.try_get("id")?;
        let raw_mobile: String = r.try_get("mobile")?;
        let Ok(mobile) = normalize_mobile(&raw_mobile) else {
            skipped.push(raw_mobile);
            continue;
        };
        let spend = dec(&r, "total_spend");
        let referral = referral_bonus.get(&old).copied().unwrap_or(0);
        let earned = int(dec(&r, "total_points_earned"));
        let custom = serde_json::json!({
            "location": r.try_get::<Option<String>, _>("location")?.unwrap_or_default(),
            "city": r.try_get::<Option<String>, _>("city")?.unwrap_or_default(),
            "country": r.try_get::<Option<String>, _>("country")?.unwrap_or_default(),
        });
        let last: Option<NaiveDate> = r.try_get("last_purchase_date")?;
        let created: Option<DateTime<Utc>> = r.try_get("created_at")?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO customers (tenant_id, mobile, first_name, other_names, email, custom_fields, total_spend, own_points, referral_points,
                                    points_redeemed, points_available, last_purchase_at, tier, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,COALESCE($14, now()))
             ON CONFLICT (tenant_id, mobile) DO UPDATE SET first_name = EXCLUDED.first_name RETURNING id",
        )
        .bind(tenant_id)
        .bind(&mobile)
        .bind(r.try_get::<String, _>("first_name")?)
        .bind(r.try_get::<Option<String>, _>("other_names")?.unwrap_or_default())
        .bind(r.try_get::<Option<String>, _>("email")?.unwrap_or_default())
        .bind(custom)
        .bind(spend)
        .bind((earned - referral).max(0))
        .bind(referral)
        .bind(int(dec(&r, "points_redeemed")))
        .bind(int(dec(&r, "points_available")))
        .bind(last.and_then(|d| d.and_hms_opt(12, 0, 0)).map(|d| d.and_utc()))
        .bind(settings.tier_for(spend))
        .bind(created)
        .fetch_one(&mut *tx)
        .await?;
        customer_map.insert(old, id);
    }

    // Referrals
    let mut referral_map: HashMap<Uuid, Uuid> = HashMap::new();
    for r in &legacy_refs {
        let (Some(&a), Some(&b)) = (customer_map.get(&r.try_get::<Uuid, _>("referrer_id")?), customer_map.get(&r.try_get::<Uuid, _>("referred_id")?)) else {
            continue;
        };
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO referrals (tenant_id, referrer_id, referred_id, bonus_points_earned, created_at) VALUES ($1,$2,$3,$4,COALESCE($5, now())) RETURNING id",
        )
        .bind(tenant_id)
        .bind(a)
        .bind(b)
        .bind(int(dec(r, "bonus_points_earned")))
        .bind(r.try_get::<Option<DateTime<Utc>>, _>("created_at")?)
        .fetch_one(&mut *tx)
        .await?;
        referral_map.insert(r.try_get("id")?, id);
        let bonus = int(dec(r, "bonus_points_earned"));
        if bonus > 0 {
            sqlx::query("INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, referral_id, notes) VALUES ($1,$2,'referral',$3,$4,'Imported from Pablo Loyalty')")
                .bind(tenant_id)
                .bind(a)
                .bind(bonus)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }

    // Transactions → legacy sales
    let items = sqlx::query("SELECT transaction_id, product_id, quantity, unit_price FROM transaction_items").fetch_all(&src).await?;
    let mut items_by_tx: HashMap<Uuid, Vec<&sqlx::postgres::PgRow>> = HashMap::new();
    for it in &items {
        items_by_tx.entry(it.try_get("transaction_id")?).or_default().push(it);
    }
    let mut sales = 0;
    for (n, r) in sqlx::query("SELECT id, customer_id, total_amount, points_earned, created_at FROM transactions ORDER BY created_at")
        .fetch_all(&src)
        .await?
        .iter()
        .enumerate()
    {
        let Some(&customer) = customer_map.get(&r.try_get::<Uuid, _>("customer_id")?) else { continue };
        let total = dec(r, "total_amount");
        let points = int(dec(r, "points_earned"));
        let created: DateTime<Utc> = r.try_get::<Option<DateTime<Utc>>, _>("created_at")?.unwrap_or_else(Utc::now);
        let sale_id: Uuid = sqlx::query_scalar(
            "INSERT INTO sales (tenant_id, branch_id, receipt_no, customer_id, gross_total, total, amount_paid, payment_method, points_earned,
                                is_legacy, notes, created_at)
             VALUES ($1,$2,$3,$4,$5,$5,$5,'legacy',$6,true,'Imported from Pablo Loyalty',$7) RETURNING id",
        )
        .bind(tenant_id)
        .bind(branch_id)
        .bind(format!("LEG-{:06}", n + 1))
        .bind(customer)
        .bind(total)
        .bind(points)
        .bind(created)
        .fetch_one(&mut *tx)
        .await?;
        for it in items_by_tx.get(&r.try_get::<Uuid, _>("id")?).map(|v| v.as_slice()).unwrap_or_default() {
            let Some(&product) = product_map.get(&it.try_get::<Uuid, _>("product_id")?) else { continue };
            let qty: i32 = it.try_get("quantity")?;
            let price = dec(it, "unit_price");
            sqlx::query(
                "INSERT INTO sale_items (tenant_id, sale_id, product_id, quantity, marked_price, unit_price, line_total) VALUES ($1,$2,$3,$4,$5,$5,$6)",
            )
            .bind(tenant_id)
            .bind(sale_id)
            .bind(product)
            .bind(qty.max(1))
            .bind(price)
            .bind(price * Decimal::from(qty.max(1)))
            .execute(&mut *tx)
            .await?;
        }
        if points > 0 {
            sqlx::query("INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, sale_id, notes, created_at) VALUES ($1,$2,'earn',$3,$4,'Imported from Pablo Loyalty',$5)")
                .bind(tenant_id)
                .bind(customer)
                .bind(points)
                .bind(sale_id)
                .bind(created)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE customers SET purchase_count = purchase_count + 1 WHERE id = $1").bind(customer).execute(&mut *tx).await?;
        sales += 1;
    }

    // Reconcile: the ledger must explain every customer's balance.
    sqlx::query(
        "INSERT INTO loyalty_ledger (tenant_id, customer_id, kind, points, notes)
         SELECT c.tenant_id, c.id, 'adjust', c.points_available - COALESCE(SUM(l.points),0), 'Opening balance reconciliation (Pablo Loyalty import)'
         FROM customers c LEFT JOIN loyalty_ledger l ON l.customer_id = c.id
         WHERE c.tenant_id = $1 GROUP BY c.id HAVING c.points_available <> COALESCE(SUM(l.points),0)",
    )
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;

    // Award periods & winners (replace the seeded default period).
    let periods = sqlx::query("SELECT id, name, start_date, end_date, status FROM award_periods ORDER BY start_date").fetch_all(&src).await?;
    if !periods.is_empty() {
        sqlx::query("DELETE FROM award_periods WHERE tenant_id = $1 AND NOT EXISTS (SELECT 1 FROM award_winners w WHERE w.period_id = award_periods.id)")
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    let winners = sqlx::query("SELECT period_id, customer_id, customer_name, tier, total_spend, points FROM award_winners").fetch_all(&src).await?;
    for p in periods {
        let old: Uuid = p.try_get("id")?;
        let id: Uuid = sqlx::query_scalar("INSERT INTO award_periods (tenant_id, name, start_date, end_date, status) VALUES ($1,$2,$3,$4,$5) RETURNING id")
            .bind(tenant_id)
            .bind(p.try_get::<String, _>("name")?)
            .bind(p.try_get::<NaiveDate, _>("start_date")?)
            .bind(p.try_get::<Option<NaiveDate>, _>("end_date")?)
            .bind(p.try_get::<String, _>("status")?)
            .fetch_one(&mut *tx)
            .await?;
        let mut rank = 0;
        for w in winners.iter().filter(|w| w.try_get::<Uuid, _>("period_id").ok() == Some(old)) {
            let Some(&customer) = customer_map.get(&w.try_get::<Uuid, _>("customer_id")?) else { continue };
            rank += 1;
            sqlx::query("INSERT INTO award_winners (period_id, customer_id, customer_name, tier, rank, total_spend, points) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(id)
                .bind(customer)
                .bind(w.try_get::<String, _>("customer_name")?)
                .bind(w.try_get::<String, _>("tier")?)
                .bind(rank)
                .bind(dec(w, "total_spend"))
                .bind(int(dec(w, "points")))
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    println!(
        "Imported into '{slug}': {users} users, {} products, {} customers, {} referrals, {sales} sales.",
        product_map.len(),
        customer_map.len(),
        referral_map.len()
    );
    if !skipped.is_empty() {
        println!("Skipped {} customer(s) with unusable mobile numbers: {}", skipped.len(), skipped.join(", "));
    }
    Ok(())
}
