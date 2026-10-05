//! Geofencing (roadmap 17): selected actions only from devices at the Current Branch.
//!
//! The browser reports its position in the `X-Location: lat,lng,accuracy_m` header. Browser locations can be faked
//! by a determined user, so this is a control plus a record (the location is kept in the audit trail), not proof.

use serde::Serialize;
use sqlx::PgConnection;

use crate::auth::Ctx;
use crate::error::{rule, AppResult};
use crate::settings::{self, LocationMode};

/// Areas a business can restrict to the branch (Settings → Workspace).
pub const AREAS: [&str; 7] = ["sales", "returns", "stock", "transfers", "expenses", "orders", "credit"];

/// Readings less precise than this are refused rather than trusted.
const MAX_ACCURACY_M: f64 = 500.0;
/// At most this much of the reported accuracy counts in the user's favour.
const ACCURACY_ALLOWANCE_M: f64 = 100.0;

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub struct Location {
    pub lat: f64,
    pub lng: f64,
    pub accuracy_m: f64,
}

impl Location {
    pub fn parse(v: &str) -> Option<Self> {
        let mut it = v.split(',').map(|p| p.trim().parse::<f64>());
        let (lat, lng, acc) = (it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
        let ok = (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lng) && acc.is_finite() && acc >= 0.0 && it.next().is_none();
        ok.then_some(Self { lat, lng, accuracy_m: acc })
    }
}

/// Great-circle distance in metres.
pub fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, la2) = (a.0.to_radians(), b.0.to_radians());
    let (dla, dlo) = ((b.0 - a.0).to_radians(), (b.1 - a.1).to_radians());
    let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

/// Inside the fence? `None` when the reading is too imprecise to decide.
pub fn within(loc: &Location, point: (f64, f64), radius_m: f64) -> Option<bool> {
    if loc.accuracy_m > MAX_ACCURACY_M {
        return None;
    }
    Some(distance_m((loc.lat, loc.lng), point) <= radius_m + loc.accuracy_m.min(ACCURACY_ALLOWANCE_M))
}

/// Refuses `area` unless the user is at the Current Branch, when the business and the branch require it.
pub async fn require_on_site(conn: &mut PgConnection, ctx: &Ctx, area: &str) -> AppResult<()> {
    if ctx.can("location.bypass") {
        return Ok(());
    }
    let s = settings::load(conn, ctx.tenant_id).await?;
    let policy = &s.workspace.location;
    if policy.mode != LocationMode::Branch || !policy.areas.iter().any(|a| a == area) {
        return Ok(());
    }
    let fence: Option<(Option<f64>, Option<f64>, i32, String)> = sqlx::query_as(
        "SELECT latitude, longitude, geofence_radius_m, name FROM branches WHERE id = $1 AND tenant_id = $2 AND geofence_enabled",
    )
    .bind(ctx.branch_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((Some(lat), Some(lng), radius, name)) = fence else { return Ok(()) };
    let Some(loc) = ctx.location else {
        return Err(rule(format!("{name} requires your location for this. Allow location access in your browser and try again.")));
    };
    match within(&loc, (lat, lng), radius as f64) {
        Some(true) => Ok(()),
        Some(false) => Err(rule(format!(
            "You appear to be about {} m from {name}. This action is only allowed at the branch.",
            distance_m((loc.lat, loc.lng), (lat, lng)).round()
        ))),
        None => Err(rule("Your location is not precise enough. Turn on precise location (GPS) and try again.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_measures() {
        assert_eq!(Location::parse("-1.2633,36.8045,12"), Some(Location { lat: -1.2633, lng: 36.8045, accuracy_m: 12.0 }));
        assert!(Location::parse("91,0,1").is_none() && Location::parse("1,2").is_none() && Location::parse("1,2,3,4").is_none());
        assert!(Location::parse("1,2,NaN").is_none() && Location::parse("1,2,-5").is_none());
        // Kimathi Street → Westlands is roughly 3.5 km.
        let d = distance_m((-1.2841, 36.8233), (-1.2649, 36.8028));
        assert!((3_000.0..4_200.0).contains(&d), "{d}");
        let at = Location { lat: -1.2841, lng: 36.8233, accuracy_m: 20.0 };
        assert_eq!(within(&at, (-1.2841, 36.8233), 150.0), Some(true));
        assert_eq!(within(&at, (-1.2649, 36.8028), 150.0), Some(false));
        // ~200 m away with 80 m accuracy and a 150 m radius: inside the allowance.
        let near = Location { lat: -1.2841 + 0.0018, lng: 36.8233, accuracy_m: 80.0 };
        assert_eq!(within(&near, (-1.2841, 36.8233), 150.0), Some(true));
        assert_eq!(within(&Location { accuracy_m: 900.0, ..at }, (-1.2841, 36.8233), 150.0), None);
    }
}
