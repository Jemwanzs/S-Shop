//! Inbound webhooks: M-Pesa STK callbacks and the WhatsApp Cloud API.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::integrations::{mpesa, whatsapp};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks/mpesa/{token}", post(mpesa_callback))
        .route("/webhooks/whatsapp", get(whatsapp_verify).post(whatsapp_event))
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Daraja does not sign callbacks, so the URL carries a secret token.
async fn mpesa_callback(State(state): State<AppState>, Path(token): Path<String>, Json(body): Json<Value>) -> impl IntoResponse {
    let ack = Json(json!({ "ResultCode": 0, "ResultDesc": "Accepted" }));
    let Some(cfg) = &state.cfg.mpesa else { return (StatusCode::NOT_FOUND, ack) };
    if !constant_eq(&token, &cfg.callback_token) {
        tracing::warn!("mpesa callback with bad token");
        return (StatusCode::NOT_FOUND, ack);
    }
    let Some(cb) = mpesa::parse_callback(&body) else {
        tracing::warn!("unrecognised mpesa callback");
        return (StatusCode::OK, ack);
    };
    let status = match cb.result_code {
        0 => "success",
        1032 => "cancelled",
        _ => "failed",
    };
    let updated: Result<Option<(Uuid, Uuid)>, _> = sqlx::query_as(
        "UPDATE mpesa_requests SET status = $2, result_code = $3, result_desc = $4, mpesa_receipt = $5,
                amount = COALESCE($6, amount), raw_callback = $7, updated_at = now()
         WHERE checkout_request_id = $1 AND status IN ('pending','timeout') RETURNING id, tenant_id",
    )
    .bind(&cb.checkout_request_id)
    .bind(status)
    .bind(cb.result_code)
    .bind(&cb.result_desc)
    .bind(&cb.receipt)
    .bind(cb.amount)
    .bind(&body)
    .fetch_optional(&state.db)
    .await;
    match updated {
        Ok(Some((id, tenant))) => {
            state.emit(tenant, None, "mpesa", json!({ "id": id, "status": status, "receipt": cb.receipt, "message": cb.result_desc }));
        }
        Ok(None) => tracing::info!(checkout = %cb.checkout_request_id, "mpesa callback for unknown or settled request"),
        Err(e) => tracing::error!(error = %e, "mpesa callback update failed"),
    }
    (StatusCode::OK, ack)
}

#[derive(Deserialize)]
struct VerifyQuery {
    #[serde(rename = "hub.mode")]
    mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
}

async fn whatsapp_verify(State(state): State<AppState>, Query(q): Query<VerifyQuery>) -> impl IntoResponse {
    match (&state.cfg.whatsapp, q.mode.as_deref(), q.verify_token, q.challenge) {
        (Some(cfg), Some("subscribe"), Some(token), Some(challenge)) if constant_eq(&token, &cfg.verify_token) => {
            (StatusCode::OK, challenge)
        }
        _ => (StatusCode::FORBIDDEN, String::new()),
    }
}

async fn whatsapp_event(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let Some(cfg) = &state.cfg.whatsapp else { return StatusCode::NOT_FOUND };
    if let Some(secret) = &cfg.app_secret {
        let sig = headers.get("x-hub-signature-256").and_then(|v| v.to_str().ok());
        if !whatsapp::verify_signature(secret, sig, &body) {
            tracing::warn!("whatsapp webhook signature mismatch");
            return StatusCode::UNAUTHORIZED;
        }
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else { return StatusCode::OK };

    for entry in payload["entry"].as_array().into_iter().flatten() {
        for change in entry["changes"].as_array().into_iter().flatten() {
            let value = &change["value"];
            for st in value["statuses"].as_array().into_iter().flatten() {
                if let (Some(id), Some(status)) = (st["id"].as_str(), st["status"].as_str()) {
                    let _ = sqlx::query("UPDATE whatsapp_messages SET status = $2, updated_at = now() WHERE wa_message_id = $1")
                        .bind(id)
                        .bind(status)
                        .execute(&state.db)
                        .await;
                }
            }
            for msg in value["messages"].as_array().into_iter().flatten() {
                let from = msg["from"].as_str().unwrap_or_default().to_string();
                let text = msg["text"]["body"].as_str().unwrap_or_default().to_string();
                let _ = sqlx::query("INSERT INTO whatsapp_messages (direction, phone, body, wa_message_id, status) VALUES ('in',$1,$2,$3,'received')")
                    .bind(&from)
                    .bind(&text)
                    .bind(msg["id"].as_str())
                    .execute(&state.db)
                    .await;
                let state = state.clone();
                tokio::spawn(async move { auto_reply(&state, &from, &text).await });
            }
        }
    }
    StatusCode::OK
}

/// Customers can text an order number (or "status") to get their latest order status.
async fn auto_reply(state: &AppState, from: &str, text: &str) {
    let upper = text.to_uppercase();
    let order_no = upper.split_whitespace().find(|w| w.starts_with("ORD-")).map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-').to_string());
    let wants_status = order_no.is_some() || ["STATUS", "ORDER", "TRACK"].iter().any(|k| upper.contains(k));
    if !wants_status {
        return;
    }
    let row: Result<Option<(Uuid, String, String, Uuid, String)>, _> = sqlx::query_as(
        "SELECT o.tenant_id, o.order_no, o.status, o.track_token, t.name FROM orders o
         JOIN customers c ON c.id = o.customer_id JOIN tenants t ON t.id = o.tenant_id
         WHERE c.mobile = $1 AND ($2::text IS NULL OR o.order_no = $2) ORDER BY o.created_at DESC LIMIT 1",
    )
    .bind(from)
    .bind(&order_no)
    .fetch_optional(&state.db)
    .await;
    let reply = match row {
        Ok(Some((tenant, no, status, token, business))) => (
            Some(tenant),
            format!(
                "{business}: order {no} is *{}*.\nTrack it: {}/track/{token}",
                super::orders::status_label(&status),
                state.cfg.public_url
            ),
        ),
        Ok(None) => (None, "We couldn't find an order for this number. Reply with your order number, e.g. ORD-2026-000001.".to_string()),
        Err(e) => {
            tracing::warn!(error = %e, "whatsapp auto-reply lookup failed");
            return;
        }
    };
    if let Err(e) = whatsapp::send_text(state, reply.0, from, &reply.1).await {
        tracing::warn!(error = %e, "whatsapp auto-reply failed");
    }
}
