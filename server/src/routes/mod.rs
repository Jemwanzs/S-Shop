//! HTTP API. Each module owns one functional area of the scope.

pub mod access;
pub mod admin;
pub mod approvals;
pub mod audit;
pub mod auth;
pub mod billing;
pub mod catalog;
pub mod credit;
pub mod customers;
pub mod dashboard;
pub mod expenses;
pub mod fields;
pub mod leaderboards;
pub mod loyalty;
pub mod notifications;
pub mod orders;
pub mod payments;
pub mod platform;
pub mod portal;
pub mod prefs;
pub mod reports;
pub mod sales;
pub mod search;
pub mod site;
pub mod stock;
pub mod transfers;
pub mod webhooks;
pub mod receipts;
pub mod tenants;
pub mod quickpin;
pub mod campaigns;
pub mod recovery;
pub mod website;

use axum::Router;
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

pub fn api() -> Router<AppState> {
    Router::new()
        .merge(auth::routes())
        .merge(access::routes())
        .merge(recovery::routes())
        .merge(receipts::routes())
        .merge(tenants::routes())
        .merge(quickpin::routes())
        .merge(campaigns::routes())
        .merge(prefs::routes())
        .merge(platform::routes())
        .merge(billing::routes())
        .merge(admin::routes())
        .merge(catalog::routes())
        .merge(stock::routes())
        .merge(transfers::routes())
        .merge(customers::routes())
        .merge(fields::routes())
        .merge(loyalty::routes())
        .merge(sales::routes())
        .merge(payments::routes())
        .merge(credit::routes())
        .merge(orders::routes())
        .merge(portal::routes())
        .merge(expenses::routes())
        .merge(approvals::routes())
        .merge(dashboard::routes())
        .merge(leaderboards::routes())
        .merge(reports::routes())
        .merge(notifications::routes())
        .merge(audit::routes())
        .merge(search::routes())
        .merge(webhooks::routes())
        .merge(website::routes())
        .merge(site::routes())
}

/// Date filter shared by lists, dashboard and reports.
/// `period` presets: today | yesterday | week | month | year | last7 | last30 | all;
/// explicit `from`/`to` (inclusive) override the preset.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct Period {
    pub period: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

impl Period {
    /// `today` is the business day (see `Ctx::today`), so "today" after midnight still means the open trading day.
    pub fn resolve(&self, today: NaiveDate, default: &str) -> (NaiveDate, NaiveDate) {
        if let (Some(f), Some(t)) = (self.from, self.to) {
            return if f <= t { (f, t) } else { (t, f) };
        }
        if let Some(f) = self.from {
            return (f, f);
        }
        match self.period.as_deref().unwrap_or(default) {
            "today" => (today, today),
            "yesterday" => {
                let y = today - Duration::days(1);
                (y, y)
            }
            "week" => (today - Duration::days(today.weekday().num_days_from_monday() as i64), today),
            "month" => (today.with_day(1).unwrap(), today),
            "year" => (NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap(), today),
            "last7" => (today - Duration::days(6), today),
            "last30" => (today - Duration::days(29), today),
            "all" => (NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(), today),
            _ => (today - Duration::days(today.weekday().num_days_from_monday() as i64), today),
        }
    }
}

/// Query-string helpers. `#[serde(flatten)]` buffers query values as strings,
/// so numeric/boolean fields in flattened query structs must accept strings.
pub mod de {
    use serde::{Deserialize, Deserializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw<T> {
        Value(T),
        Text(String),
    }

    fn parse<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        match Option::<Raw<T>>::deserialize(d)? {
            None => Ok(None),
            Some(Raw::Value(v)) => Ok(Some(v)),
            Some(Raw::Text(s)) if s.trim().is_empty() => Ok(None),
            Some(Raw::Text(s)) => s.trim().parse().map(Some).map_err(serde::de::Error::custom),
        }
    }

    pub fn opt_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        parse(d)
    }

    pub fn opt_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
        parse(d)
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct Page {
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub limit: Option<i64>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    pub offset: Option<i64>,
}

impl Page {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 500)
    }
    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

#[derive(Serialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// A row plus `COUNT(*) OVER() AS total_count`, so one query yields a page and its total.
pub struct Counted<T> {
    pub row: T,
    pub total_count: i64,
}

impl<'r, T: sqlx::FromRow<'r, sqlx::postgres::PgRow>> sqlx::FromRow<'r, sqlx::postgres::PgRow> for Counted<T> {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self { total_count: row.try_get("total_count")?, row: T::from_row(row)? })
    }
}

impl<T> From<Vec<Counted<T>>> for Paged<T> {
    fn from(rows: Vec<Counted<T>>) -> Self {
        let total = rows.first().map(|r| r.total_count).unwrap_or(0);
        Self { items: rows.into_iter().map(|r| r.row).collect(), total }
    }
}

/// Response for actions that may be deferred to the approval engine.
#[derive(Serialize)]
pub struct Outcome<T: Serialize> {
    pub pending_approval: bool,
    pub approval_id: Option<uuid::Uuid>,
    pub result: Option<T>,
}

impl<T: Serialize> Outcome<T> {
    pub fn done(result: T) -> Self {
        Self { pending_approval: false, approval_id: None, result: Some(result) }
    }
    pub fn pending(approval_id: uuid::Uuid) -> Self {
        Self { pending_approval: true, approval_id: Some(approval_id), result: None }
    }
    /// Parked for approval, but the record already exists (e.g. an inactive new product).
    pub fn pending_with(approval_id: uuid::Uuid, result: T) -> Self {
        Self { pending_approval: true, approval_id: Some(approval_id), result: Some(result) }
    }
}

/// `%term%` for ILIKE searches.
pub fn like(q: &Option<String>) -> Option<String> {
    q.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")))
}

/// Cost prices (and values derived from them) are financial figures: hidden from users without
/// "View cost & profit figures" while Settings → Reports → "Hide cost & profit" is on (the default).
pub async fn costs_hidden(conn: &mut sqlx::PgConnection, ctx: &crate::auth::Ctx) -> crate::error::AppResult<bool> {
    if ctx.can("sales.view_financials") {
        return Ok(false);
    }
    Ok(crate::settings::load(conn, ctx.tenant_id).await?.reports.hide_financials_without_permission)
}
