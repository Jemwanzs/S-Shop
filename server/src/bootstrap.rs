//! First-run seeding and recovery commands.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::auth::{hash_pin, validate_pin};
use crate::config::Config;
use crate::perms::{ADMIN_ROLE, DEFAULT_ROLES};
use crate::workflow::ACTIONS;

/// On an empty database, create the first business from BOOTSTRAP_* variables.
pub async fn ensure(db: &PgPool, cfg: &Config) -> anyhow::Result<()> {
    let tenants: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenants").fetch_one(db).await?;
    if tenants > 0 {
        return Ok(());
    }
    let Some(b) = &cfg.bootstrap else {
        tracing::warn!("Database is empty. Set BOOTSTRAP_ADMIN_EMAIL and BOOTSTRAP_ADMIN_PIN to create the first business.");
        return Ok(());
    };
    validate_pin(&b.admin_pin).map_err(|e| anyhow::anyhow!("BOOTSTRAP_ADMIN_PIN: {e}"))?;
    let mut tx = db.begin().await?;
    let tenant_id = seed_tenant(&mut tx, &b.business_name, &b.slug).await?;
    create_admin(&mut tx, tenant_id, &b.admin_name, &b.admin_email, &b.admin_pin).await?;
    tx.commit().await?;
    tracing::info!(business = %b.business_name, slug = %b.slug, admin = %b.admin_email, "created first business");
    Ok(())
}

/// Create a business with default roles, a main branch, workflow rows (disabled),
/// expense categories and an open award period.
pub async fn seed_tenant(conn: &mut PgConnection, name: &str, slug: &str) -> anyhow::Result<Uuid> {
    let tenant_id: Uuid = sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(slug)
        .fetch_one(&mut *conn)
        .await?;

    for r in DEFAULT_ROLES {
        sqlx::query("INSERT INTO roles (tenant_id, name, description, permissions, is_system) VALUES ($1,$2,$3,$4,$5)")
            .bind(tenant_id)
            .bind(r.name)
            .bind(r.description)
            .bind(r.permissions.iter().map(|p| p.to_string()).collect::<Vec<_>>())
            .bind(r.name == ADMIN_ROLE)
            .execute(&mut *conn)
            .await?;
    }

    sqlx::query("INSERT INTO branches (tenant_id, name, code) VALUES ($1, 'Main Branch', 'MAIN')")
        .bind(tenant_id)
        .execute(&mut *conn)
        .await?;

    for a in ACTIONS {
        sqlx::query("INSERT INTO workflows (tenant_id, action) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(a.key)
            .execute(&mut *conn)
            .await?;
    }

    for c in ["Rent", "Utilities", "Salaries & Wages", "Transport", "Supplies", "Marketing", "Repairs", "Other"] {
        sqlx::query("INSERT INTO expense_categories (tenant_id, name) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(c)
            .execute(&mut *conn)
            .await?;
    }

    sqlx::query("INSERT INTO award_periods (tenant_id, name, start_date) VALUES ($1, $2, CURRENT_DATE)")
        .bind(tenant_id)
        .bind(format!("Awards {}", chrono::Utc::now().format("%Y")))
        .execute(&mut *conn)
        .await?;

    Ok(tenant_id)
}

pub async fn create_admin(conn: &mut PgConnection, tenant_id: Uuid, name: &str, email: &str, pin: &str) -> anyhow::Result<Uuid> {
    let role_id: Uuid = sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND name = $2")
        .bind(tenant_id)
        .bind(ADMIN_ROLE)
        .fetch_one(&mut *conn)
        .await?;
    let id = sqlx::query_scalar(
        "INSERT INTO users (tenant_id, name, email, pin_hash, role_id, all_branches) VALUES ($1,$2,lower($3),$4,$5,true) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(email.trim())
    .bind(hash_pin(pin)?)
    .bind(role_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

pub async fn reset_pin(db: &PgPool, email: &str, pin: &str) -> anyhow::Result<()> {
    validate_pin(pin).map_err(|e| anyhow::anyhow!("{e}"))?;
    let n = sqlx::query("UPDATE users SET pin_hash = $2, failed_attempts = 0, locked_until = NULL WHERE lower(email) = lower($1)")
        .bind(email.trim())
        .bind(hash_pin(pin)?)
        .execute(db)
        .await?
        .rows_affected();
    if n == 0 {
        anyhow::bail!("No user with email {email}");
    }
    println!("PIN reset for {email}");
    Ok(())
}
