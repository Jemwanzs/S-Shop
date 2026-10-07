//! Per-user preferences (language, font, display currency) and exchange rates for display conversion.
//! Amounts are always stored and entered in the business currency (KES); conversion is display-only.

use std::time::{Duration, Instant};

use axum::extract::State;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/auth/preferences", put(save)).route("/fx", get(fx))
}

pub const LANGUAGES: [&str; 4] = ["en", "sw", "fr", "ar"];
pub const FONTS: [&str; 5] = ["Outfit", "Poppins", "Inter", "Roboto", "Nunito"];
pub const CURRENCIES: [&str; 3] = ["KES", "USD", "EUR"];
/// Business currency all amounts are recorded in.
pub const BASE_CURRENCY: &str = "KES";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub language: String,
    pub font: String,
    pub currency: String,
    /// Quick actions (roadmap 48): the floating Record Sale bubble may be dragged to another edge position.
    pub quick_sale_draggable: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self { language: "en".into(), font: "Outfit".into(), currency: BASE_CURRENCY.into(), quick_sale_draggable: false }
    }
}

impl Preferences {
    /// Reads stored preferences, replacing anything no longer supported with the default.
    pub fn from_stored(v: Value) -> Self {
        let mut p: Self = serde_json::from_value(v).unwrap_or_default();
        let d = Self::default();
        if !LANGUAGES.contains(&p.language.as_str()) {
            p.language = d.language;
        }
        if !FONTS.contains(&p.font.as_str()) {
            p.font = d.font;
        }
        if !CURRENCIES.contains(&p.currency.as_str()) {
            p.currency = d.currency;
        }
        p
    }
}

async fn save(State(state): State<AppState>, ctx: Ctx, Json(p): Json<Preferences>) -> AppResult<Json<Preferences>> {
    if !LANGUAGES.contains(&p.language.as_str()) {
        return Err(bad("Unsupported language"));
    }
    if !FONTS.contains(&p.font.as_str()) {
        return Err(bad("Unsupported font"));
    }
    if !CURRENCIES.contains(&p.currency.as_str()) {
        return Err(bad("Unsupported currency"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET preferences = $2 WHERE id = $1")
        .bind(ctx.user_id)
        .bind(json!(p))
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("auth", "preferences", "user", ctx.user_id).after(&p)).await?;
    tx.commit().await?;
    Ok(Json(p))
}

const FX_TTL: Duration = Duration::from_secs(60 * 60);
const FX_SOURCE: &str = "https://open.er-api.com/v6/latest/KES";

/// Rates from KES to each supported display currency. Cached for an hour; on a fetch failure the
/// last good rates are served (marked stale) so the app keeps working offline from the provider.
async fn fx(State(state): State<AppState>, _ctx: Ctx) -> AppResult<Json<Value>> {
    {
        let cache = state.fx.lock().await;
        if let Some((at, v)) = cache.as_ref() {
            if at.elapsed() < FX_TTL {
                return Ok(Json(v.clone()));
            }
        }
    }
    match fetch_rates(&state).await {
        Ok(v) => {
            *state.fx.lock().await = Some((Instant::now(), v.clone()));
            Ok(Json(v))
        }
        Err(e) => {
            tracing::warn!(error = %e, "exchange-rate refresh failed");
            match state.fx.lock().await.as_ref() {
                Some((_, v)) => {
                    let mut v = v.clone();
                    v["stale"] = json!(true);
                    Ok(Json(v))
                }
                None => Err(AppError::Upstream("Exchange rates are unavailable right now — amounts are shown in KES".into())),
            }
        }
    }
}

async fn fetch_rates(state: &AppState) -> Result<Value, String> {
    let body: Value = state
        .http
        .get(FX_SOURCE)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if body["result"] != "success" {
        return Err(format!("provider error: {}", body["error-type"]));
    }
    let mut rates = serde_json::Map::new();
    for c in CURRENCIES {
        let r = body["rates"][c].as_f64().filter(|r| *r > 0.0).ok_or_else(|| format!("missing rate for {c}"))?;
        rates.insert(c.to_string(), json!(r));
    }
    Ok(json!({
        "base": BASE_CURRENCY,
        "rates": rates,
        "updated_at": body["time_last_update_utc"],
        "source": "open.er-api.com (ExchangeRate-API)",
        "stale": false,
    }))
}
