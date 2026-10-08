//! Receipt endpoints (roadmap 65–67): a sale's receipts, the secure share link, the public receipt page data, logo
//! assets, and emailing a receipt with its PDF attached. Rendering (on-screen, PDF, image, print) happens in the app
//! from the same stored snapshot, so every channel shows the same receipt.

use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::{client_meta, Ctx};
use crate::error::{bad, AppError, AppResult};
use crate::mailer::{self, button, esc, layout, p, Mail};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sales/{id}/receipts", get(of_sale))
        .route("/receipts/{id}/link", post(link))
        .route("/receipts/{id}/email", post(email).layer(DefaultBodyLimit::max(4 * 1024 * 1024)))
        .route("/r/{token}", get(public))
        .route("/receipt-assets/{hash}", get(asset))
}

/// Receipt ids belong to a sale the user may see (same rule as the sale itself).
async fn visible_receipt(state: &AppState, ctx: &Ctx, id: Uuid) -> AppResult<(Uuid, String, String, Value, Uuid)> {
    let row: Option<(Uuid, String, String, Value, Uuid)> =
        sqlx::query_as("SELECT sale_id, number, kind, snapshot, share_token FROM receipts WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(ctx.tenant_id)
            .fetch_optional(&state.db)
            .await?;
    let row = row.ok_or(AppError::NotFound("Receipt"))?;
    super::sales::ensure_visible(state, ctx, row.0).await?;
    Ok(row)
}

async fn of_sale(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require_any(&["sales.view", "sales.create"])?;
    super::sales::ensure_visible(&state, &ctx, id).await?;
    let mut conn = state.db.acquire().await?;
    let items = crate::receipts::of_sale(&mut conn, ctx.tenant_id, id, Some(ctx.user_id)).await?;
    let customer: Option<(String, String)> = sqlx::query_as(
        "SELECT c.mobile, c.email FROM sales s JOIN customers c ON c.id = s.customer_id WHERE s.id = $1 AND s.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Json(json!({
        "items": items,
        "customer": customer.map(|(mobile, email)| json!({ "mobile": mobile, "email": email })),
    })))
}

/// The secure share link of a receipt (anyone with the link can view and download that one receipt).
async fn link(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("sales.print")?;
    let (_, number, _, _, token) = visible_receipt(&state, &ctx, id).await?;
    Ok(Json(json!({ "url": format!("{}/r/{token}", state.cfg.public_url), "number": number })))
}

/// Public receipt by link (no sign-in). Rate-limited; the token is unguessable.
async fn public(State(state): State<AppState>, headers: HeaderMap, Path(token): Path<Uuid>) -> AppResult<Json<Value>> {
    state.limits.check(&client_meta(&headers).0, "receipt_link", 120, std::time::Duration::from_secs(600))?;
    let row: Option<(Value, String)> = sqlx::query_as("SELECT r.snapshot, r.kind FROM receipts r WHERE r.share_token = $1")
        .bind(token)
        .fetch_optional(&state.db)
        .await?;
    let (snapshot, kind) = row.ok_or(AppError::NotFound("Receipt"))?;
    Ok(Json(json!({ "snapshot": snapshot, "kind": kind })))
}

/// A logo as it was when a receipt was issued (content-addressed, so it never changes).
async fn asset(State(state): State<AppState>, Path(hash): Path<String>) -> AppResult<impl IntoResponse> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::NotFound("Image"));
    }
    let row: Option<(String, Vec<u8>)> = sqlx::query_as("SELECT mime, data FROM receipt_assets WHERE hash = $1").bind(&hash).fetch_optional(&state.db).await?;
    let (mime, data) = row.ok_or(AppError::NotFound("Image"))?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string())], data))
}

#[derive(Deserialize)]
struct EmailBody {
    to: String,
    /// The receipt PDF, generated in the app from this receipt's snapshot (base64).
    pdf: String,
}

/// Emails a receipt to the customer with its PDF attached, from the business's name; tracked like every email.
async fn email(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<EmailBody>) -> AppResult<Json<Value>> {
    ctx.require("sales.print")?;
    state.limits.check(&ctx.user_id.to_string(), "receipt_email", 20, std::time::Duration::from_secs(600))?;
    let to = b.to.trim().to_lowercase();
    if !to.contains('@') || !to.contains('.') || to.contains(' ') || to.len() > 160 {
        return Err(bad("Enter a valid email address"));
    }
    let pdf = base64::engine::general_purpose::STANDARD.decode(b.pdf.trim()).map_err(|_| bad("The receipt file could not be read"))?;
    if !pdf.starts_with(b"%PDF") || pdf.len() > 2 * 1024 * 1024 {
        return Err(bad("The receipt file could not be read"));
    }
    let (sale_id, number, kind, snap, token) = visible_receipt(&state, &ctx, id).await?;
    let business = snap["business"]["name"].as_str().unwrap_or("Receipt").to_string();
    let customer = snap["customer"].as_str().map(|n| n.split_whitespace().next().unwrap_or(n).to_string());
    let link = format!("{}/r/{token}", state.cfg.public_url);
    let hello = customer.as_deref().map_or("Hello,".to_string(), |n| format!("Hello {},", esc(n)));
    let what = if kind == "adjustment" { "your updated receipt" } else { "your receipt" };
    let mut body = p(&hello);
    body.push_str(&p(&format!("Thank you for shopping with <b>{}</b>. Attached is {what} <b>{}</b>.", esc(&business), esc(&number))));
    body.push_str(&button("View receipt", &link));
    body.push_str(&p("<span style=\"color:#78716c;font-size:13px\">We appreciate your business!</span>"));
    let mail = Mail {
        kind: "receipt",
        to: vec![to.clone()],
        subject: format!("{business} — receipt {number}"),
        html: layout(&format!("{}{} {number}", what[..1].to_uppercase(), &what[1..]), &format!("Receipt {number}"), &body),
        text: format!(
            "{}\n\nThank you for shopping with {business}. Attached is {what} {number}.\nView it online: {link}\n\nWe appreciate your business!",
            customer.as_deref().map_or("Hello,".to_string(), |n| format!("Hello {n},"))
        ),
        tenant_id: Some(ctx.tenant_id),
        access_request_id: None,
        user_id: None,
        created_by: Some(ctx.user_id),
        retry_of: None,
    };
    let filename = format!("{}.pdf", number.replace(['/', '\\'], "-"));
    let out = mailer::send_ext(&state, mail, Some(&business), &[(filename, b.pdf.trim().to_string())]).await;
    let mut tx = state.db.begin().await?;
    audit::record(&mut tx, &ctx, Entry::new("sales", "receipt_email", "sale", sale_id).after(json!({ "receipt": number, "to": to, "status": out.status }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "email_status": out })))
}
