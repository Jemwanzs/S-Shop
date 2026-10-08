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
    pub workspace: WorkspaceSettings,
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
    /// Receipt configuration (roadmap 66). The structure is fixed; these choose what it shows.
    pub receipt: ReceiptSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReceiptSettings {
    pub show_logo: bool,
    pub show_branch: bool,
    pub show_contact: bool,
    pub show_customer: bool,
    pub show_salesperson: bool,
    pub show_loyalty: bool,
    pub show_payment_ref: bool,
    /// sans | thermal
    pub font: String,
}

impl Default for ReceiptSettings {
    fn default() -> Self {
        Self {
            show_logo: true,
            show_branch: true,
            show_contact: true,
            show_customer: true,
            show_salesperson: true,
            show_loyalty: true,
            show_payment_ref: true,
            font: "sans".into(),
        }
    }
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
            receipt: ReceiptSettings::default(),
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
    /// Show product prices on the customer ordering link (staff screens always show them).
    pub show_prices: bool,
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
            show_prices: true,
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
    pub medals: MedalSettings,
}
impl Default for ReportSettings {
    fn default() -> Self {
        Self { hide_financials_without_permission: true, medals: MedalSettings::default() }
    }
}

/// How dashboard leaderboards (products, staff) award Gold / Silver / Bronze.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MedalMode {
    /// Top three by position (1st Gold, 2nd Silver, 3rd Bronze).
    #[default]
    Rank,
    /// Anyone reaching a target earns the medal, regardless of position.
    Targets,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MedalBasis {
    #[default]
    Revenue,
    Units,
}

/// Targets are **per day** and scale with the length of the period viewed. 0 switches a medal off.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MedalTargets {
    pub basis: MedalBasis,
    pub gold: Decimal,
    pub silver: Decimal,
    pub bronze: Decimal,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MedalSettings {
    pub mode: MedalMode,
    pub products: MedalTargets,
    pub staff: MedalTargets,
}

pub const MEDALS: [&str; 3] = ["Gold", "Silver", "Bronze"];

impl MedalSettings {
    /// Medal for the entry at `rank` (0-based) with the given revenue and units over `days`.
    pub fn award(&self, t: &MedalTargets, rank: usize, revenue: Decimal, units: i64, days: i64) -> Option<&'static str> {
        match self.mode {
            MedalMode::Rank => MEDALS.get(rank).copied(),
            MedalMode::Targets => {
                let value = match t.basis {
                    MedalBasis::Revenue => revenue,
                    MedalBasis::Units => Decimal::from(units),
                };
                let days = Decimal::from(days.max(1));
                [(t.gold, MEDALS[0]), (t.silver, MEDALS[1]), (t.bronze, MEDALS[2])]
                    .into_iter()
                    .find(|(per_day, _)| *per_day > Decimal::ZERO && value >= *per_day * days)
                    .map(|(_, m)| m)
            }
        }
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

/// Working days and trading hours. Business-wide here; a branch may override `hours` (branches.hours).
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct WorkspaceSettings {
    pub hours: Hours,
    /// What happens to a sale outside trading hours: `allow` (default) or `block` (unless `sales.outside_hours`).
    pub outside_hours: OutsideHours,
    /// Where selected actions may be done from (geofencing).
    pub location: LocationPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LocationPolicy {
    /// `anywhere` (default) or `branch`: the areas below only within the Current Branch's radius.
    pub mode: LocationMode,
    pub areas: Vec<String>,
}

impl Default for LocationPolicy {
    fn default() -> Self {
        Self { mode: LocationMode::Anywhere, areas: crate::geo::AREAS.iter().map(|a| a.to_string()).collect() }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LocationMode {
    #[default]
    Anywhere,
    Branch,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutsideHours {
    #[default]
    Allow,
    Block,
}

/// Trading days Monday..Sunday with one opening and closing time ("HH:MM"). A closing time at or before the
/// opening time runs past midnight: 06:00 → 02:00 trades until 2 a.m., and those hours belong to the day that opened.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Hours {
    pub days: [bool; 7],
    pub open: String,
    pub close: String,
}

impl Default for Hours {
    fn default() -> Self {
        Self { days: [true; 7], open: "00:00".into(), close: "00:00".into() }
    }
}

fn minutes(hhmm: &str) -> Option<i32> {
    let (h, m) = hhmm.split_once(':')?;
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m) && hhmm.len() == 5).then_some(h * 60 + m)
}

impl Hours {
    pub fn validate(&self) -> Result<(), &'static str> {
        if minutes(&self.open).is_none() || minutes(&self.close).is_none() {
            return Err("Trading hours must be times like 08:00");
        }
        if !self.days.iter().any(|d| *d) {
            return Err("Choose at least one working day");
        }
        Ok(())
    }

    /// Minutes after midnight that still belong to the previous business day.
    pub fn day_shift(&self) -> i32 {
        match (minutes(&self.open), minutes(&self.close)) {
            (Some(o), Some(c)) if c <= o => c,
            _ => 0,
        }
    }

    /// Open at local time `now`? Days are judged by the business day, so 01:00 on Saturday after a Friday that
    /// trades until 02:00 is still open even when Saturday is a day off.
    pub fn is_open(&self, now: chrono::NaiveDateTime) -> bool {
        use chrono::{Datelike, Timelike};
        let (Some(o), Some(c)) = (minutes(&self.open), minutes(&self.close)) else { return true };
        let t = (now.hour() * 60 + now.minute()) as i32;
        let business_day = (now - chrono::Duration::minutes(self.day_shift() as i64)).date();
        let in_hours = if c > o { t >= o && t < c } else { t >= o || t < c };
        in_hours && self.days[business_day.weekday().num_days_from_monday() as usize]
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

/// A branch's trading hours: its own override, else the business hours.
pub fn effective_hours(branch_hours: Option<&serde_json::Value>, s: &TenantSettings) -> Hours {
    branch_hours.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_else(|| s.workspace.hours.clone())
}

/// Trading hours of one branch (for the open/closed check at the till).
pub async fn branch_hours(conn: &mut PgConnection, tenant_id: Uuid, branch_id: Uuid, s: &TenantSettings) -> AppResult<Hours> {
    let v: Option<serde_json::Value> = sqlx::query_scalar("SELECT hours FROM branches WHERE id = $1 AND tenant_id = $2")
        .bind(branch_id)
        .bind(tenant_id)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    Ok(effective_hours(v.as_ref(), s))
}

/// Re-derive the business-day shift of the business and every branch after hours change. Only new records
/// use it: business dates already stored are snapshots and never move.
pub async fn apply_day_shifts(conn: &mut PgConnection, tenant_id: Uuid, s: &TenantSettings) -> AppResult<()> {
    sqlx::query("UPDATE tenants SET day_shift_minutes = $2 WHERE id = $1")
        .bind(tenant_id)
        .bind(s.workspace.hours.day_shift())
        .execute(&mut *conn)
        .await?;
    let branches: Vec<(Uuid, Option<serde_json::Value>)> = sqlx::query_as("SELECT id, hours FROM branches WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_all(&mut *conn)
        .await?;
    for (id, hours) in branches {
        sqlx::query("UPDATE branches SET day_shift_minutes = $2 WHERE id = $1 AND day_shift_minutes <> $2")
            .bind(id)
            .bind(effective_hours(hours.as_ref(), s).day_shift())
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
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
    fn medal_targets_scale_with_period() {
        let mut m = MedalSettings::default();
        let t = MedalTargets { basis: MedalBasis::Revenue, gold: Decimal::new(10_000, 0), silver: Decimal::new(5_000, 0), bronze: Decimal::ZERO };
        assert_eq!(m.award(&t, 0, Decimal::ZERO, 0, 1), Some("Gold"), "rank mode ignores figures");
        assert_eq!(m.award(&t, 3, Decimal::ZERO, 0, 1), None);
        m.mode = MedalMode::Targets;
        assert_eq!(m.award(&t, 4, Decimal::new(12_000, 0), 0, 1), Some("Gold"));
        assert_eq!(m.award(&t, 0, Decimal::new(12_000, 0), 0, 7), None, "a week needs 7× the daily target");
        assert_eq!(m.award(&t, 0, Decimal::new(40_000, 0), 0, 7), Some("Silver"));
        assert_eq!(m.award(&t, 0, Decimal::new(4_000, 0), 0, 1), None, "bronze 0 is switched off");
        let units = MedalTargets { basis: MedalBasis::Units, gold: Decimal::new(20, 0), ..Default::default() };
        assert_eq!(m.award(&units, 0, Decimal::new(1_000_000, 0), 19, 1), None);
        assert_eq!(m.award(&units, 0, Decimal::ZERO, 20, 1), Some("Gold"));
    }

    #[test]
    fn trading_hours_and_business_day() {
        let at = |d: u32, hm: (u32, u32)| chrono::NaiveDate::from_ymd_opt(2026, 10, d).unwrap().and_hms_opt(hm.0, hm.1, 0).unwrap();
        // 2026-10-05 is a Monday.
        let mut h = Hours { days: [true, true, true, true, true, false, false], open: "06:00".into(), close: "02:00".into() };
        assert_eq!(h.day_shift(), 120);
        assert!(h.is_open(at(5, (23, 0))));
        assert!(h.is_open(at(6, (1, 30))), "01:30 Tuesday is Monday's late trade");
        assert!(!h.is_open(at(6, (3, 0))), "closed between 02:00 and 06:00");
        assert!(h.is_open(at(10, (1, 0))), "Saturday 01:00 still belongs to Friday");
        assert!(!h.is_open(at(10, (9, 0))), "Saturday is a day off");
        h = Hours { days: [true; 7], open: "08:00".into(), close: "20:00".into() };
        assert_eq!(h.day_shift(), 0);
        assert!(h.is_open(at(5, (8, 0))) && !h.is_open(at(5, (20, 0))) && !h.is_open(at(5, (7, 59))));
        let all_day = Hours::default();
        assert_eq!(all_day.day_shift(), 0);
        assert!(all_day.is_open(at(5, (3, 0))));
        assert!(Hours { open: "6:00".into(), ..Hours::default() }.validate().is_err());
        assert!(Hours { days: [false; 7], ..Hours::default() }.validate().is_err());
    }

    #[test]
    fn tiers_pick_highest_reached() {
        let s = TenantSettings::default();
        assert_eq!(s.tier_for(Decimal::new(150_000, 0)), "Gold");
        assert_eq!(s.tier_for(Decimal::new(60_000, 0)), "Silver");
        assert_eq!(s.tier_for(Decimal::new(100, 0)), "");
    }
}
