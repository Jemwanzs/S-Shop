use chrono::{DateTime, Datelike, NaiveDate, Utc};
use chrono_tz::Tz;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{bad, AppResult};

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Normalise Kenyan mobile numbers to 2547XXXXXXXX / 2541XXXXXXXX.
/// Other international numbers (+CC…) are kept as digits only.
pub fn normalize_mobile(input: &str) -> AppResult<String> {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    let normalized = if digits.len() == 10 && digits.starts_with('0') {
        format!("254{}", &digits[1..])
    } else if digits.len() == 9 && (digits.starts_with('7') || digits.starts_with('1')) {
        format!("254{digits}")
    } else {
        digits
    };
    if normalized.len() < 9 || normalized.len() > 15 {
        return Err(bad("Enter a valid mobile number, e.g. 0712345678"));
    }
    Ok(normalized)
}

/// Sequential document numbers per tenant / kind / year: PREFIX-YYYY-000001.
pub async fn next_doc_no(conn: &mut PgConnection, tenant_id: Uuid, prefix: &str, tz: Tz) -> AppResult<String> {
    let year = Utc::now().with_timezone(&tz).year();
    let value: i64 = sqlx::query_scalar(
        "INSERT INTO doc_counters (tenant_id, kind, year, value) VALUES ($1, $2, $3, 1)
         ON CONFLICT (tenant_id, kind, year) DO UPDATE SET value = doc_counters.value + 1
         RETURNING value",
    )
    .bind(tenant_id)
    .bind(prefix)
    .bind(year)
    .fetch_one(&mut *conn)
    .await?;
    Ok(format!("{prefix}-{year}-{value:06}"))
}

pub fn parse_tz(name: &str) -> Tz {
    name.parse().unwrap_or(chrono_tz::Africa::Nairobi)
}

pub fn today_in(tz: Tz) -> NaiveDate {
    Utc::now().with_timezone(&tz).date_naive()
}

/// Business date now, for a day that runs `shift_minutes` past midnight (0 = calendar day).
pub fn business_today(tz: Tz, shift_minutes: i32) -> NaiveDate {
    (Utc::now().with_timezone(&tz).naive_local() - chrono::Duration::minutes(shift_minutes as i64)).date()
}

pub fn round2(d: Decimal) -> Decimal {
    d.round_dp(2)
}

pub fn money_str(d: Decimal) -> String {
    let rounded = d.round_dp(0);
    let s = rounded.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let body: String = out.chars().rev().collect();
    if rounded.is_sign_negative() && !rounded.is_zero() {
        format!("-{body}")
    } else {
        body
    }
}

/// Start/end (exclusive) instants for an inclusive local date range.
pub fn local_range(from: NaiveDate, to: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = from.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(tz).earliest().unwrap();
    let end = to.succ_opt().unwrap_or(to).and_hms_opt(0, 0, 0).unwrap().and_local_timezone(tz).earliest().unwrap();
    (start.with_timezone(&Utc), end.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobiles_normalise() {
        assert_eq!(normalize_mobile("0712 345 678").unwrap(), "254712345678");
        assert_eq!(normalize_mobile("+254712345678").unwrap(), "254712345678");
        assert_eq!(normalize_mobile("712345678").unwrap(), "254712345678");
        assert_eq!(normalize_mobile("0110345678").unwrap(), "254110345678");
        assert!(normalize_mobile("12").is_err());
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Mali-Safi  Kibandaski!"), "mali-safi-kibandaski");
        assert_eq!(slugify("  Pablo Niche "), "pablo-niche");
    }

    #[test]
    fn money_formatting() {
        assert_eq!(money_str(Decimal::new(76800000, 2)), "768,000");
        assert_eq!(money_str(Decimal::new(-50000, 2)), "-500");
        assert_eq!(money_str(Decimal::ZERO), "0");
    }
}
