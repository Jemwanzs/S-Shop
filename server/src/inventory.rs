//! Inventory ledger.
//!
//! Every stock change is a row in `stock_movements`; `Current Stock = Σ movements`.
//! `stock_levels` is a locked projection updated in the same transaction so
//! concurrent sales cannot oversell. Nothing ever writes `on_hand` directly.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{rule, AppResult};

#[derive(Debug, Clone, Copy)]
pub struct Level {
    pub on_hand: i32,
    pub reserved: i32,
}

impl Level {
    pub fn available(&self) -> i32 {
        self.on_hand - self.reserved
    }
}

/// Lock (creating if needed) the stock level row for a branch/product.
pub async fn lock(conn: &mut PgConnection, tenant_id: Uuid, branch_id: Uuid, product_id: Uuid) -> AppResult<Level> {
    sqlx::query(
        "INSERT INTO stock_levels (tenant_id, branch_id, product_id) VALUES ($1,$2,$3)
         ON CONFLICT (branch_id, product_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(branch_id)
    .bind(product_id)
    .execute(&mut *conn)
    .await?;
    let (on_hand, reserved): (i32, i32) = sqlx::query_as(
        "SELECT on_hand, reserved FROM stock_levels WHERE branch_id = $1 AND product_id = $2 FOR UPDATE",
    )
    .bind(branch_id)
    .bind(product_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Level { on_hand, reserved })
}

pub struct Movement<'a> {
    pub branch_id: Uuid,
    pub product_id: Uuid,
    pub stock_item_id: Option<Uuid>,
    pub kind: &'a str,
    /// Signed: positive adds stock, negative removes it.
    pub quantity: i32,
    pub unit_cost: Option<Decimal>,
    pub unit_price: Option<Decimal>,
    pub ref_type: Option<&'a str>,
    pub ref_id: Option<Uuid>,
    pub supplier_id: Option<Uuid>,
    pub notes: &'a str,
    pub occurred_on: Option<NaiveDate>,
}

impl<'a> Movement<'a> {
    pub fn new(branch_id: Uuid, product_id: Uuid, kind: &'a str, quantity: i32) -> Self {
        Self {
            branch_id,
            product_id,
            stock_item_id: None,
            kind,
            quantity,
            unit_cost: None,
            unit_price: None,
            ref_type: None,
            ref_id: None,
            supplier_id: None,
            notes: "",
            occurred_on: None,
        }
    }
    pub fn item(mut self, id: Option<Uuid>) -> Self {
        self.stock_item_id = id;
        self
    }
    pub fn reference(mut self, ref_type: &'a str, ref_id: Uuid) -> Self {
        self.ref_type = Some(ref_type);
        self.ref_id = Some(ref_id);
        self
    }
    pub fn cost(mut self, c: Option<Decimal>) -> Self {
        self.unit_cost = c;
        self
    }
    pub fn price(mut self, p: Option<Decimal>) -> Self {
        self.unit_price = p;
        self
    }
    pub fn notes(mut self, n: &'a str) -> Self {
        self.notes = n;
        self
    }
}

/// How a removal is checked against the stock level.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Must not dip into units reserved for orders (counter sales, transfers, write-offs).
    Available,
    /// Consumes a reservation this caller holds (order completion) — checks on_hand only.
    OnHand,
    /// Correction that may take stock negative (stock counts, explicit config).
    None,
}

/// Apply a movement: lock the level, enforce the check, write the ledger row, update the projection.
pub async fn apply(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Option<Uuid>,
    allow_negative: bool,
    check: Check,
    m: Movement<'_>,
) -> AppResult<(Uuid, Level)> {
    let level = lock(conn, tenant_id, m.branch_id, m.product_id).await?;
    if m.quantity < 0 && !allow_negative {
        let need = -m.quantity;
        let have = match check {
            Check::Available => level.available(),
            Check::OnHand => level.on_hand,
            Check::None => i32::MAX,
        };
        if have < need {
            let name: String = sqlx::query_scalar("SELECT name FROM products WHERE id = $1")
                .bind(m.product_id)
                .fetch_one(&mut *conn)
                .await?;
            return Err(rule(format!("Not enough stock for {name}: {} available, {need} needed", have.max(0))));
        }
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO stock_movements (tenant_id, branch_id, product_id, stock_item_id, kind, quantity, unit_cost,
                                      unit_price, ref_type, ref_id, supplier_id, notes, user_id, occurred_on)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13, COALESCE($14, CURRENT_DATE))
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(m.branch_id)
    .bind(m.product_id)
    .bind(m.stock_item_id)
    .bind(m.kind)
    .bind(m.quantity)
    .bind(m.unit_cost)
    .bind(m.unit_price)
    .bind(m.ref_type)
    .bind(m.ref_id)
    .bind(m.supplier_id)
    .bind(m.notes)
    .bind(user_id)
    .bind(m.occurred_on)
    .fetch_one(&mut *conn)
    .await?;

    let after: (i32, i32) = sqlx::query_as(
        "UPDATE stock_levels SET on_hand = on_hand + $3, updated_at = now()
         WHERE branch_id = $1 AND product_id = $2 RETURNING on_hand, reserved",
    )
    .bind(m.branch_id)
    .bind(m.product_id)
    .bind(m.quantity)
    .fetch_one(&mut *conn)
    .await?;

    Ok((id, Level { on_hand: after.0, reserved: after.1 }))
}

/// Hold units for a confirmed order (Physical − Reserved = Available-to-Sell).
pub async fn reserve(conn: &mut PgConnection, tenant_id: Uuid, branch_id: Uuid, product_id: Uuid, qty: i32, allow_negative: bool) -> AppResult<()> {
    let level = lock(conn, tenant_id, branch_id, product_id).await?;
    if !allow_negative && level.available() < qty {
        let name: String = sqlx::query_scalar("SELECT name FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_one(&mut *conn)
            .await?;
        return Err(rule(format!("Cannot reserve {qty} × {name}: only {} available", level.available().max(0))));
    }
    sqlx::query("UPDATE stock_levels SET reserved = reserved + $3, updated_at = now() WHERE branch_id = $1 AND product_id = $2")
        .bind(branch_id)
        .bind(product_id)
        .bind(qty)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn release(conn: &mut PgConnection, tenant_id: Uuid, branch_id: Uuid, product_id: Uuid, qty: i32) -> AppResult<()> {
    lock(conn, tenant_id, branch_id, product_id).await?;
    sqlx::query(
        "UPDATE stock_levels SET reserved = GREATEST(reserved - $3, 0), updated_at = now()
         WHERE branch_id = $1 AND product_id = $2",
    )
    .bind(branch_id)
    .bind(product_id)
    .bind(qty)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Product must be active and sellable in this branch.
pub async fn ensure_product_in_branch(conn: &mut PgConnection, tenant_id: Uuid, product_id: Uuid, branch_id: Uuid) -> AppResult<()> {
    let ok: Option<bool> = sqlx::query_scalar(
        "SELECT p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = $3)
         FROM products p WHERE p.id = $1 AND p.tenant_id = $2",
    )
    .bind(product_id)
    .bind(tenant_id)
    .bind(branch_id)
    .fetch_optional(&mut *conn)
    .await?;
    match ok {
        None => Err(crate::error::AppError::NotFound("Product")),
        Some(false) => Err(rule("This product is not available in the selected branch")),
        Some(true) => Ok(()),
    }
}
