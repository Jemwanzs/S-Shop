//! Digital receipts (roadmap 65–67): one receipt structure for every channel (view, PDF, image, print, email,
//! WhatsApp, share link), issued as an immutable snapshot.
//!
//! * **Original receipt**: issued in the same transaction as the sale (or, for older sales, the first time it is
//!   opened) and never changed afterwards. Later changes to product names, prices, the logo or the sale owner do not
//!   alter it.
//! * **Adjustment receipt**: issued when a return, exchange, cancellation or recall is executed (after any approval).
//!   It names the original receipt and shows what changed: items returned, refund, exchange, net sale value, loyalty.
//! * The business logo is stored once per version (`receipt_assets`, by content hash) and referenced by the snapshot.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::settings::TenantSettings;

fn label(s: &TenantSettings, method: &str) -> String {
    match method {
        "exchange" => "Exchange credit".into(),
        "credit" => "Credit".into(),
        "customer_credit" => "Customer credit".into(),
        _ => s.sales.payment_methods.iter().find(|m| m.key == method).map(|m| m.label.clone()).unwrap_or_else(|| {
            let mut c = method.replace('_', " ");
            if let Some(f) = c.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            c
        }),
    }
}

/// The current logo, stored as a receipt asset; returns its hash.
async fn logo_asset(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<Option<String>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as("SELECT logo, logo_mime FROM tenants WHERE id = $1").bind(tenant_id).fetch_optional(&mut *conn).await?;
    let Some((Some(data), mime)) = row else { return Ok(None) };
    let hash = hex::encode(Sha256::digest(&data));
    sqlx::query("INSERT INTO receipt_assets (hash, mime, data) VALUES ($1, $2, $3) ON CONFLICT (hash) DO NOTHING")
        .bind(&hash)
        .bind(mime.unwrap_or_else(|| "image/png".into()))
        .bind(&data)
        .execute(&mut *conn)
        .await?;
    Ok(Some(hash))
}

#[derive(sqlx::FromRow)]
struct Head {
    tenant_id: Uuid,
    receipt_no: String,
    created_at: DateTime<Utc>,
    business_date: NaiveDate,
    status: String,
    gross_total: Decimal,
    discount_total: Decimal,
    total: Decimal,
    redeemed_points: i64,
    redeemed_value: Decimal,
    amount_paid: Decimal,
    payment_method: String,
    points_earned: i64,
    notes: String,
    business: String,
    business_phone: String,
    currency: String,
    timezone: String,
    branch: String,
    branch_phone: String,
    customer: Option<String>,
    owner: Option<String>,
    settings: Value,
}

async fn head(conn: &mut PgConnection, sale_id: Uuid) -> AppResult<Head> {
    sqlx::query_as(
        "SELECT s.tenant_id, s.receipt_no, s.created_at, s.business_date, s.status, s.gross_total, s.discount_total, s.total,
                s.redeemed_points, s.redeemed_value, s.amount_paid, s.payment_method, s.points_earned, s.notes,
                t.name AS business, t.phone AS business_phone, t.currency, t.timezone, b.name AS branch, b.phone AS branch_phone,
                NULLIF(TRIM(c.first_name || ' ' || COALESCE(c.other_names, '')), '') AS customer, ow.name AS owner, t.settings
         FROM sales s JOIN tenants t ON t.id = s.tenant_id JOIN branches b ON b.id = s.branch_id
         LEFT JOIN customers c ON c.id = s.customer_id LEFT JOIN users ow ON ow.id = s.owner_id
         WHERE s.id = $1",
    )
    .bind(sale_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("Sale"))
}

/// Everything shared by original and adjustment receipts: who issued it, where, and the receipt options in force.
fn base(h: &Head, s: &TenantSettings, logo: Option<String>) -> Value {
    let r = &s.sales.receipt;
    json!({
        "currency": h.currency,
        "timezone": h.timezone,
        "business": { "name": h.business, "logo": if r.show_logo { logo } else { None }, "phone": if r.show_contact { Some(h.business_phone.clone()) } else { None } },
        "branch": { "name": if r.show_branch { Some(h.branch.clone()) } else { None }, "phone": if r.show_contact { Some(h.branch_phone.clone()) } else { None } },
        "customer": if r.show_customer { h.customer.clone() } else { None },
        "served_by": if r.show_salesperson { h.owner.clone() } else { None },
        "footer": s.sales.receipt_footer,
        "signed_by": h.business,
        "font": r.font,
    })
}

async fn items(conn: &mut PgConnection, sale_id: Uuid) -> AppResult<Vec<Value>> {
    let rows: Vec<(Uuid, String, i32, i32, Decimal, Decimal, Decimal)> = sqlx::query_as(
        "SELECT si.id, p.name, si.quantity, si.returned_qty, si.unit_price, si.line_total, si.marked_price
         FROM sale_items si JOIN products p ON p.id = si.product_id WHERE si.sale_id = $1 ORDER BY p.name, si.id",
    )
    .bind(sale_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, qty, returned, price, total, marked)| json!({ "id": id, "name": name, "qty": qty, "returned": returned, "price": price, "total": total, "marked": marked }))
        .collect())
}

/// Snapshot of a sale's original receipt, as the sale stands at issue time.
async fn original_snapshot(conn: &mut PgConnection, sale_id: Uuid) -> AppResult<Value> {
    let h = head(conn, sale_id).await?;
    let s: TenantSettings = serde_json::from_value(h.settings.clone()).unwrap_or_default();
    let logo = logo_asset(conn, h.tenant_id).await?;
    let lines = items(conn, sale_id).await?;
    let payments: Vec<(String, Decimal, String)> =
        sqlx::query_as("SELECT method, amount, reference FROM payments WHERE sale_id = $1 AND amount > 0 ORDER BY created_at").bind(sale_id).fetch_all(&mut *conn).await?;
    let balance: Option<Decimal> = sqlx::query_scalar("SELECT original_amount - amount_paid - adjustments FROM credit_sales WHERE sale_id = $1")
        .bind(sale_id)
        .fetch_optional(&mut *conn)
        .await?;
    let show_ref = s.sales.receipt.show_payment_ref;
    let mut v = base(&h, &s, logo);
    v["kind"] = json!("original");
    v["title"] = json!("SALES RECEIPT");
    v["number"] = json!(h.receipt_no);
    v["receipt_no"] = json!(h.receipt_no);
    v["at"] = json!(h.created_at);
    v["business_date"] = json!(h.business_date);
    v["status"] = json!(h.status);
    v["items"] = json!(lines.iter().map(|l| json!({ "name": l["name"], "qty": l["qty"], "price": l["price"], "total": l["total"] })).collect::<Vec<_>>());
    v["totals"] = json!({
        "subtotal": h.gross_total,
        "discount": h.discount_total,
        "redeemed_points": h.redeemed_points,
        "redeemed_value": h.redeemed_value,
        "total": h.total,
        "paid": h.amount_paid,
        "balance": balance.filter(|b| *b > Decimal::ZERO),
        "method": label(&s, &h.payment_method),
    });
    v["payments"] = json!(payments.iter().map(|(m, a, r)| json!({ "method": label(&s, m), "amount": a, "reference": if show_ref && !r.is_empty() { Some(r.clone()) } else { None } })).collect::<Vec<_>>());
    v["points_earned"] = json!(if s.sales.receipt.show_loyalty { h.points_earned } else { 0 });
    v["notes"] = json!(if h.notes.starts_with("Exchange for") { h.notes.clone() } else { String::new() });
    Ok(v)
}

/// The original receipt (issuing it now if this sale has none yet, e.g. a sale from before receipts were stored).
pub async fn issue_original(conn: &mut PgConnection, tenant_id: Uuid, sale_id: Uuid, by: Option<Uuid>) -> AppResult<Uuid> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM receipts WHERE sale_id = $1 AND kind = 'original'").bind(sale_id).fetch_optional(&mut *conn).await? {
        return Ok(id);
    }
    let snap = original_snapshot(conn, sale_id).await?;
    let number = snap["number"].as_str().unwrap_or_default().to_string();
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO receipts (tenant_id, sale_id, kind, number, snapshot, created_by) VALUES ($1,$2,'original',$3,$4,$5)
         ON CONFLICT (sale_id) WHERE kind = 'original' DO NOTHING RETURNING id",
    )
    .bind(tenant_id)
    .bind(sale_id)
    .bind(&number)
    .bind(&snap)
    .bind(by)
    .fetch_optional(&mut *conn)
    .await?;
    match id {
        Some(id) => Ok(id),
        None => Ok(sqlx::query_scalar("SELECT id FROM receipts WHERE sale_id = $1 AND kind = 'original'").bind(sale_id).fetch_one(&mut *conn).await?),
    }
}

/// Adjustment receipt for an executed return / exchange / cancellation / recall, linked to the original receipt.
pub async fn issue_adjustment(conn: &mut PgConnection, tenant_id: Uuid, sale_id: Uuid, return_id: Uuid, by: Option<Uuid>) -> AppResult<Uuid> {
    // The original receipt exists before any adjustment (it shows the sale as first issued).
    issue_original(conn, tenant_id, sale_id, by).await?;
    let h = head(conn, sale_id).await?;
    let s: TenantSettings = serde_json::from_value(h.settings.clone()).unwrap_or_default();
    let logo = logo_asset(conn, h.tenant_id).await?;
    #[allow(clippy::type_complexity)]
    let (return_no, kind, reason, refund, refund_method, points_reversed, points_unrecovered, customer_credit, at, approver): (
        String, String, String, Decimal, String, i64, i64, Decimal, DateTime<Utc>, Option<String>,
    ) = sqlx::query_as(
        "SELECT r.return_no, r.kind, r.reason, r.refund_amount, r.refund_method, r.points_reversed, r.points_unrecovered, r.customer_credit,
                r.created_at, au.name
         FROM sale_returns r LEFT JOIN users au ON au.id = r.approved_by WHERE r.id = $1 AND r.sale_id = $2",
    )
    .bind(return_id)
    .bind(sale_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("Return"))?;
    let returned: Vec<(String, i32, Decimal)> = sqlx::query_as(
        "SELECT p.name, ri.quantity, ri.amount FROM sale_return_items ri JOIN sale_items si ON si.id = ri.sale_item_id
         JOIN products p ON p.id = si.product_id WHERE ri.return_id = $1 ORDER BY p.name",
    )
    .bind(return_id)
    .fetch_all(&mut *conn)
    .await?;
    // Exchange: the replacement sale paid (partly) by this return's value.
    let exchange: Option<(String, Decimal)> = sqlx::query_as(
        "SELECT s.receipt_no, s.total FROM payments p JOIN sales s ON s.id = p.sale_id
         WHERE p.method = 'exchange' AND p.reference = $1 AND p.tenant_id = $2 AND p.sale_id <> $3 LIMIT 1",
    )
    .bind(&return_no)
    .bind(tenant_id)
    .bind(sale_id)
    .fetch_optional(&mut *conn)
    .await?;
    let refunded_total: Decimal = sqlx::query_scalar("SELECT COALESCE(SUM(refund_amount), 0) FROM sale_returns WHERE sale_id = $1 AND created_at <= $2")
        .bind(sale_id)
        .bind(at)
        .fetch_one(&mut *conn)
        .await?;
    let original_points: i64 = sqlx::query_scalar("SELECT points_earned FROM sales WHERE id = $1").bind(sale_id).fetch_one(&mut *conn).await?;
    let reversed_total: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(points_reversed), 0)::bigint FROM sale_returns WHERE sale_id = $1 AND created_at <= $2")
        .bind(sale_id)
        .bind(at)
        .fetch_one(&mut *conn)
        .await?;
    let lines = items(conn, sale_id).await?;
    let status = match (kind.as_str(), exchange.is_some(), h.status.as_str()) {
        ("cancellation", _, _) => "Cancelled",
        ("recall", _, _) => "Recalled",
        (_, true, _) => "Exchanged",
        (_, _, "returned") => "Fully Returned",
        _ => "Partially Returned",
    };
    let show_loyalty = s.sales.receipt.show_loyalty;
    let mut v = base(&h, &s, logo);
    v["kind"] = json!("adjustment");
    v["title"] = json!(if exchange.is_some() { "EXCHANGE RECEIPT" } else { "ADJUSTMENT RECEIPT" });
    v["number"] = json!(return_no);
    v["receipt_no"] = json!(h.receipt_no);
    v["at"] = json!(at);
    v["business_date"] = json!(h.business_date);
    v["status"] = json!(status);
    v["adjustment"] = json!({
        "reference": return_no,
        "kind": kind,
        "reason": reason,
        "original_receipt": h.receipt_no,
        "original_at": h.created_at,
        "returned": returned.iter().map(|(n, q, a)| json!({ "name": n, "qty": q, "total": a })).collect::<Vec<_>>(),
        "remaining": lines.iter().filter(|l| l["qty"].as_i64().unwrap_or(0) > l["returned"].as_i64().unwrap_or(0)).map(|l| {
            let left = l["qty"].as_i64().unwrap_or(0) - l["returned"].as_i64().unwrap_or(0);
            json!({ "name": l["name"], "qty": left, "price": l["price"] })
        }).collect::<Vec<_>>(),
        "original_total": h.total,
        "refund": refund,
        "refund_method": label(&s, &refund_method),
        "refunded_total": refunded_total,
        "customer_credit": customer_credit,
        "net_sale_value": (h.total - refunded_total).max(Decimal::ZERO),
        "exchange": exchange.as_ref().map(|(no, total)| json!({ "receipt_no": no, "total": total, "difference": *total - refund })),
        "approved_by": approver,
        "loyalty": if show_loyalty { Some(json!({
            "original": original_points, "reversed": points_reversed, "unrecovered": points_unrecovered,
            "net": (original_points - reversed_total).max(0),
        })) } else { None },
    });
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO receipts (tenant_id, sale_id, kind, number, return_id, snapshot, created_by) VALUES ($1,$2,'adjustment',$3,$4,$5,$6)
         ON CONFLICT (return_id) WHERE kind = 'adjustment' DO NOTHING RETURNING id",
    )
    .bind(tenant_id)
    .bind(sale_id)
    .bind(v["number"].as_str().unwrap_or_default())
    .bind(return_id)
    .bind(&v)
    .bind(by)
    .fetch_optional(&mut *conn)
    .await?;
    match id {
        Some(id) => Ok(id),
        None => Ok(sqlx::query_scalar("SELECT id FROM receipts WHERE return_id = $1 AND kind = 'adjustment'").bind(return_id).fetch_one(&mut *conn).await?),
    }
}

/// All receipts of a sale, original first.
pub async fn of_sale(conn: &mut PgConnection, tenant_id: Uuid, sale_id: Uuid, by: Option<Uuid>) -> AppResult<Vec<Value>> {
    issue_original(conn, tenant_id, sale_id, by).await?;
    let rows: Vec<(Uuid, String, String, Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, kind, number, snapshot, created_at FROM receipts WHERE sale_id = $1 AND tenant_id = $2 ORDER BY (kind = 'original') DESC, created_at",
    )
    .bind(sale_id)
    .bind(tenant_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(id, kind, number, snapshot, at)| json!({ "id": id, "kind": kind, "number": number, "created_at": at, "snapshot": snapshot })).collect())
}
