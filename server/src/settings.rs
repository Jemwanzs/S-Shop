//! Tenant configuration (Settings area). Stored as JSON on `tenants.settings`;
//! every field has a default so older rows keep working when fields are added.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::AppResult;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TenantSettings {
    pub product: ProductSettings,
    pub stock: StockSettings,
    pub sales: SalesSettings,
    pub orders: OrderSettings,
    pub customers: CustomerSettings,
    pub loyalty: LoyaltySettings,
    pub expenses: ExpenseSettings,
    pub reports: ReportSettings,
    pub notifications: NotificationSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProductSettings {
    pub max_photos: u8,
    pub auto_code_prefix: String,
}
impl Default for ProductSettings {
    fn default() -> Self {
        Self { max_photos: 5, auto_code_prefix: "PRD".into() }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeRequirement {
    Required,
    Optional,
    Disabled,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuantityEntry {
    Editable,
    Locked,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Valuation {
    Cost,
    Selling,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct StockSettings {
    pub barcode_requirement: BarcodeRequirement,
    pub quantity_entry: QuantityEntry,
    pub capture_cost: bool,
    pub valuation: Valuation,
    pub low_stock_threshold: i32,
    pub allow_negative: bool,
    /// When true, transfers sit "in transit" until the destination confirms receipt.
    pub transfer_receipt_control: bool,
}
impl Default for StockSettings {
    fn default() -> Self {
        Self {
            barcode_requirement: BarcodeRequirement::Optional,
            quantity_entry: QuantityEntry::Editable,
            capture_cost: true,
            valuation: Valuation::Cost,
            low_stock_threshold: 3,
            allow_negative: false,
            transfer_receipt_control: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentMethod {
    pub key: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SalesSettings {
    pub quantity_entry: QuantityEntry,
    pub require_barcode_clearance: bool,
    pub payment_methods: Vec<PaymentMethod>,
    /// Allow cashiers to confirm M-Pesa by typing the transaction code.
    pub mpesa_manual_confirmation: bool,
    pub credit_enabled: bool,
    pub credit_default_days: i64,
    pub receipt_footer: String,
}
impl Default for PaymentMethod {
    fn default() -> Self {
        Self { key: String::new(), label: String::new(), enabled: true }
    }
}
impl Default for SalesSettings {
    fn default() -> Self {
        Self {
            quantity_entry: QuantityEntry::Editable,
            require_barcode_clearance: false,
            payment_methods: vec![
                PaymentMethod { key: "mpesa".into(), label: "M-Pesa".into(), enabled: true },
                PaymentMethod { key: "cash".into(), label: "Cash".into(), enabled: true },
                PaymentMethod { key: "credit".into(), label: "Credit Sale".into(), enabled: true },
            ],
            mpesa_manual_confirmation: true,
            credit_enabled: true,
            credit_default_days: 30,
            receipt_footer: "Thank you for shopping with us!".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderSettings {
    pub portal_enabled: bool,
    pub default_branch_id: Option<Uuid>,
    /// Confirmed orders hold stock so the counter cannot sell the same units.
    pub reserve_stock: bool,
    /// "delivered" or "completed" — the status at which an order becomes a sale.
    pub sale_on_status: String,
    /// Verify customers with a one-time code (sent over WhatsApp) before showing orders.
    pub verify_with_otp: bool,
    pub show_out_of_stock: bool,
    pub notify_customer_whatsapp: bool,
    /// Order status names and which optional steps the business uses.
    pub statuses: Vec<OrderStatus>,
}

/// Every order status with its default label and whether a business may switch it off.
pub const ORDER_STATUSES: &[(&str, &str, bool)] = &[
    ("new", "Order received", false),
    ("confirmed", "Confirmed", false),
    ("preparing", "Being prepared", true),
    ("dispatched", "Ready / dispatched", true),
    ("on_delivery", "On delivery", true),
    ("delivered", "Delivered", false),
    ("completed", "Completed", true),
    ("cancelled", "Cancelled", false),
    ("rejected", "Rejected", false),
    ("returned", "Returned", false),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderStatus {
    pub key: String,
    pub label: String,
    pub enabled: bool,
}
impl Default for OrderStatus {
    fn default() -> Self {
        Self { key: String::new(), label: String::new(), enabled: true }
    }
}

impl Default for OrderSettings {
    fn default() -> Self {
        Self {
            portal_enabled: true,
            default_branch_id: None,
            reserve_stock: true,
            sale_on_status: "delivered".into(),
            verify_with_otp: false,
            show_out_of_stock: true,
            notify_customer_whatsapp: true,
            statuses: ORDER_STATUSES
                .iter()
                .map(|(k, l, _)| OrderStatus { key: k.to_string(), label: l.to_string(), enabled: true })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomerSettings {
    pub require_email: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Tier {
    pub name: String,
    pub min_spend: Decimal,
}
impl Default for Tier {
    fn default() -> Self {
        Self { name: String::new(), min_spend: Decimal::ZERO }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LoyaltySettings {
    pub enabled: bool,
    /// Spend per block of points (legacy "threshold"): every `threshold` spent earns `points_per` points.
    pub threshold: Decimal,
    pub points_per: i64,
    pub min_spend: Decimal,
    /// Monetary value of one point when redeemed.
    pub point_value: Decimal,
    pub redemption_enabled: bool,
    pub min_redemption_points: i64,
    /// Percentage of a referred customer's earned points credited to the referrer (legacy: 50%).
    pub referral_bonus_percent: i64,
    /// 0 = points never expire
    pub expiry_days: i64,
    pub tiers: Vec<Tier>,
    pub award_winners: usize,
    pub show_on_portal: bool,
    pub show_value_on_portal: bool,
}
impl Default for LoyaltySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: Decimal::new(500, 0),
            points_per: 1,
            min_spend: Decimal::ZERO,
            point_value: Decimal::ONE,
            redemption_enabled: true,
            min_redemption_points: 100,
            referral_bonus_percent: 50,
            expiry_days: 0,
            tiers: vec![
                Tier { name: "Bronze".into(), min_spend: Decimal::new(20_000, 0) },
                Tier { name: "Silver".into(), min_spend: Decimal::new(50_000, 0) },
                Tier { name: "Gold".into(), min_spend: Decimal::new(100_000, 0) },
            ],
            award_winners: 5,
            show_on_portal: true,
            show_value_on_portal: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExpenseSettings {
    pub require_attachment: bool,
    pub require_description: bool,
}
impl Default for ExpenseSettings {
    fn default() -> Self {
        Self { require_attachment: false, require_description: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportSettings {
    /// Hide cost/profit columns from users without `sales.view_financials`.
    pub hide_financials_without_permission: bool,
}
impl Default for ReportSettings {
    fn default() -> Self {
        Self { hide_financials_without_permission: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    pub whatsapp_receipts: bool,
    pub whatsapp_credit_reminders: bool,
    pub whatsapp_loyalty: bool,
}
impl Default for NotificationSettings {
    fn default() -> Self {
        Self { whatsapp_receipts: false, whatsapp_credit_reminders: false, whatsapp_loyalty: false }
    }
}

impl TenantSettings {
    pub fn tier_for(&self, total_spend: Decimal) -> String {
        let mut tiers = self.loyalty.tiers.clone();
        tiers.sort_by(|a, b| b.min_spend.cmp(&a.min_spend));
        tiers
            .into_iter()
            .find(|t| total_spend >= t.min_spend)
            .map(|t| t.name)
            .unwrap_or_default()
    }

    /// The business's name for an order status (falls back to the default label).
    pub fn order_label(&self, key: &str) -> String {
        self.orders
            .statuses
            .iter()
            .find(|s| s.key == key && !s.label.trim().is_empty())
            .map(|s| s.label.trim().to_string())
            .or_else(|| ORDER_STATUSES.iter().find(|(k, ..)| *k == key).map(|(_, l, _)| l.to_string()))
            .unwrap_or_else(|| key.replace('_', " "))
    }

    /// Optional steps can be switched off; core steps and the sale stage are always on.
    pub fn order_status_enabled(&self, key: &str) -> bool {
        let optional = ORDER_STATUSES.iter().any(|(k, _, opt)| *k == key && *opt);
        if !optional || key == self.orders.sale_on_status {
            return true;
        }
        self.orders.statuses.iter().find(|s| s.key == key).map(|s| s.enabled).unwrap_or(true)
    }

    pub fn payment_enabled(&self, key: &str) -> bool {
        self.sales.payment_methods.iter().any(|m| m.key == key && m.enabled)
    }
}

pub async fn load(conn: &mut PgConnection, tenant_id: Uuid) -> AppResult<TenantSettings> {
    let raw: serde_json::Value = sqlx::query_scalar("SELECT settings FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_fills_defaults() {
        let s: TenantSettings = serde_json::from_value(serde_json::json!({"stock": {"allow_negative": true}})).unwrap();
        assert!(s.stock.allow_negative);
        assert_eq!(s.product.max_photos, 5);
        assert_eq!(s.loyalty.referral_bonus_percent, 50);
    }

    #[test]
    fn order_status_config() {
        let mut s = TenantSettings::default();
        assert_eq!(s.order_label("preparing"), "Being prepared");
        s.orders.statuses.iter_mut().find(|x| x.key == "preparing").unwrap().label = "In the kitchen".into();
        s.orders.statuses.iter_mut().find(|x| x.key == "on_delivery").unwrap().enabled = false;
        s.orders.statuses.iter_mut().find(|x| x.key == "delivered").unwrap().enabled = false;
        assert_eq!(s.order_label("preparing"), "In the kitchen");
        assert!(!s.order_status_enabled("on_delivery"));
        assert!(s.order_status_enabled("delivered"), "core steps cannot be disabled");
        s.orders.sale_on_status = "completed".into();
        s.orders.statuses.iter_mut().find(|x| x.key == "completed").unwrap().enabled = false;
        assert!(s.order_status_enabled("completed"), "the sale stage cannot be disabled");
    }

    #[test]
    fn tiers_pick_highest_reached() {
        let s = TenantSettings::default();
        assert_eq!(s.tier_for(Decimal::new(150_000, 0)), "Gold");
        assert_eq!(s.tier_for(Decimal::new(60_000, 0)), "Silver");
        assert_eq!(s.tier_for(Decimal::new(100, 0)), "");
    }
}
