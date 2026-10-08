//! Public customer ordering portal: /order/{slug}.
//! Customers identify with their mobile number (optionally verified with a
//! WhatsApp one-time code), browse the catalogue, order, and track orders.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::like;
use super::orders::{announce_new, create_order, order_events, order_items, OrderLine, FLOW};
use crate::auth::{issue_token, PortalCustomer, PORTAL_TOKEN_DAYS};
use crate::error::{bad, rule, AppError, AppResult};
use crate::integrations::whatsapp;
use crate::settings::TenantSettings;
use crate::state::AppState;
use crate::util::normalize_mobile;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/portal/{slug}", get(business))
        .route("/portal/{slug}/identify", post(identify))
        .route("/portal/{slug}/session", post(session))
        .route("/portal/{slug}/me", get(me))
        .route("/portal/{slug}/catalogue", get(catalogue))
        .route("/portal/{slug}/products/{id}", get(product))
        .route("/portal/{slug}/orders", get(my_orders).post(place_order))
        .route("/portal/track/{token}", get(track))
}

/// A business taking orders (the ordering link, or its website — routes/site.rs).
pub(crate) struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub settings: TenantSettings,
}

/// The business without the ordering-link checks (the website decides for itself whether it takes orders).
pub(crate) async fn tenant_by_id(state: &AppState, id: Uuid) -> AppResult<Tenant> {
    let (name, raw): (String, Value) = sqlx::query_as("SELECT name, settings FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Business"))?;
    Ok(Tenant { id, name, settings: serde_json::from_value(raw).unwrap_or_default() })
}

async fn tenant(state: &AppState, slug: &str) -> AppResult<Tenant> {
    let row: Option<(Uuid, String, Value, String)> = sqlx::query_as("SELECT id, name, settings, status FROM tenants WHERE slug = $1")
        .bind(slug)
        .fetch_optional(&state.db)
        .await?;
    let (id, name, raw, status) = row.ok_or(AppError::NotFound("Business"))?;
    // A deactivated business's ordering link is switched off (its data is kept).
    if status != "active" {
        return Err(crate::error::refused("Ordering unavailable", "This shop is not taking orders at the moment"));
    }
    // The ordering link belongs to the Orders module, and stops while billing is suspended (roadmap 41–43).
    let access = {
        let mut conn = state.db.acquire().await?;
        crate::billing::access(&mut conn, id).await?
    };
    if access.suspended() || access.modules().is_some_and(|m| !m.iter().any(|x| x == "orders")) {
        return Err(crate::error::refused("Ordering unavailable", "This shop is not taking orders at the moment"));
    }
    let settings: TenantSettings = serde_json::from_value(raw).unwrap_or_default();
    if !settings.orders.portal_enabled {
        return Err(AppError::Forbidden("Online ordering is currently closed".into()));
    }
    Ok(Tenant { id, name, settings })
}

/// Branch fulfilling portal orders: configured default or the first active branch.
pub(crate) async fn portal_branch(state: &AppState, t: &Tenant) -> AppResult<Uuid> {
    if let Some(b) = t.settings.orders.default_branch_id {
        return Ok(b);
    }
    sqlx::query_scalar("SELECT id FROM branches WHERE tenant_id = $1 AND is_active ORDER BY created_at LIMIT 1")
        .bind(t.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| rule("This shop is not taking orders right now"))
}

pub(crate) fn otp_required(state: &AppState, t: &Tenant) -> bool {
    t.settings.orders.verify_with_otp && whatsapp::is_configured(state)
}

fn otp_hash(tenant: Uuid, mobile: &str, code: &str) -> String {
    hex::encode(Sha256::digest(format!("{tenant}:{mobile}:{code}").as_bytes()))
}

/// With "Show product prices" off, no price leaves the server for the ordering link (not just hidden in the page).
fn priced(st: &TenantSettings, mut v: Value) -> Value {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for k in ["price", "unit_price", "line_total", "total", "marked_price"] {
                    m.remove(k);
                }
                m.values_mut().for_each(strip);
            }
            Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    if !st.orders.show_prices {
        strip(&mut v);
    }
    v
}

/// Products whose price the business hides on its website (per-product "Hide price"). The same products never show a
/// price through the ordering link, an order confirmation, order history or tracking either — one rule everywhere.
pub(crate) async fn hidden_prices(state: &AppState, tenant_id: Uuid) -> AppResult<std::collections::HashSet<Uuid>> {
    let published: Option<sqlx::types::Json<crate::website::SiteConfig>> =
        sqlx::query_scalar("SELECT published FROM websites WHERE tenant_id = $1 AND status = 'active' AND published IS NOT NULL")
            .bind(tenant_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    Ok(published.map(|c| c.0.products.items.into_iter().filter(|p| p.price == "hide").map(|p| p.product_id).collect()).unwrap_or_default())
}

/// Removes every price from an order (summary or detail) that contains a product with a hidden price.
fn strip_order_prices(order: &mut Value, hidden: &std::collections::HashSet<Uuid>) {
    let has_hidden = order["items"].as_array().is_some_and(|items| {
        items.iter().any(|i| i["product_id"].as_str().and_then(|s| Uuid::parse_str(s).ok()).is_some_and(|id| hidden.contains(&id)))
    });
    if has_hidden {
        if let Some(o) = order.as_object_mut() {
            o.remove("total");
        }
        if let Some(items) = order["items"].as_array_mut() {
            for i in items {
                if let Some(m) = i.as_object_mut() {
                    m.remove("unit_price");
                    m.remove("line_total");
                }
            }
        }
    }
}

async fn business(State(state): State<AppState>, Path(slug): Path<String>) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    let (tagline, phone, currency, has_logo): (String, String, String, bool) =
        sqlx::query_as("SELECT tagline, phone, currency, logo IS NOT NULL FROM tenants WHERE id = $1")
            .bind(t.id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(json!({
        "name": t.name,
        "slug": slug,
        "tagline": if tagline.is_empty() { "Order directly from us. Your order will be attended to promptly by our team.".to_string() } else { tagline },
        "phone": phone,
        "currency": currency,
        "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")),
        "otp_required": otp_required(&state, &t),
        "show_loyalty": t.settings.loyalty.enabled && t.settings.loyalty.show_on_portal,
        "show_prices": t.settings.orders.show_prices,
    })))
}

#[derive(Deserialize)]
pub(crate) struct IdentifyBody {
    mobile: String,
}

async fn identify(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Json(b): Json<IdentifyBody>) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    identify_for(&state, &headers, &t, b).await.map(Json)
}

/// Mobile number → existing customer? (sends a WhatsApp code when verification is on). Shared with the website.
pub(crate) async fn identify_for(state: &AppState, headers: &HeaderMap, t: &Tenant, b: IdentifyBody) -> AppResult<Value> {
    state.limits.check(&crate::auth::client_meta(headers).0, "portal_auth", 30, std::time::Duration::from_secs(600))?;
    let mobile = normalize_mobile(&b.mobile)?;
    let existing: Option<(String, String)> = sqlx::query_as("SELECT first_name, nickname FROM customers WHERE tenant_id = $1 AND mobile = $2")
        .bind(t.id)
        .bind(&mobile)
        .fetch_optional(&state.db)
        .await?;
    let needs_otp = otp_required(state, t);

    if needs_otp {
        let recent: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM portal_otps WHERE tenant_id = $1 AND mobile = $2 AND created_at > now() - interval '10 minutes'",
        )
        .bind(t.id)
        .bind(&mobile)
        .fetch_one(&state.db)
        .await?;
        if recent >= 3 {
            return Err(rule("Too many codes requested. Please wait a few minutes."));
        }
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
        sqlx::query("INSERT INTO portal_otps (tenant_id, mobile, code_hash, expires_at) VALUES ($1,$2,$3,$4)")
            .bind(t.id)
            .bind(&mobile)
            .bind(otp_hash(t.id, &mobile, &code))
            .bind(Utc::now() + Duration::minutes(10))
            .execute(&state.db)
            .await?;
        whatsapp::send_notification(state, Some(t.id), &mobile, &format!("Your {} verification code is *{code}*. It expires in 10 minutes.", t.name))
            .await
            .map_err(|_| AppError::Upstream("We could not send your code on WhatsApp. Please try again.".into()))?;
    }

    // Names are only revealed before verification when verification is off.
    Ok(json!({
        "mobile": mobile,
        "exists": existing.is_some(),
        "first_name": if needs_otp { None } else { existing.as_ref().map(|e| e.0.clone()) },
        "nickname": if needs_otp { None } else { existing.as_ref().map(|e| e.1.clone()) },
        "otp_required": needs_otp,
    }))
}

#[derive(Deserialize)]
pub(crate) struct SessionBody {
    mobile: String,
    code: Option<String>,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    nickname: String,
}

async fn session(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Json(b): Json<SessionBody>) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    session_for(&state, &headers, &t, b).await.map(Json)
}

/// Verifies the code (when required), creates or finds the customer and returns a customer session. Shared with the
/// website, so a customer is the same S'Shop customer whichever way they order.
pub(crate) async fn session_for(state: &AppState, headers: &HeaderMap, t: &Tenant, b: SessionBody) -> AppResult<Value> {
    state.limits.check(&crate::auth::client_meta(headers).0, "portal_auth", 30, std::time::Duration::from_secs(600))?;
    let mobile = normalize_mobile(&b.mobile)?;
    let mut tx = state.db.begin().await?;

    if otp_required(state, t) {
        let code = b.code.as_deref().map(str::trim).unwrap_or_default();
        let row: Option<(Uuid, String, i32)> = sqlx::query_as(
            "SELECT id, code_hash, attempts FROM portal_otps WHERE tenant_id = $1 AND mobile = $2 AND NOT used AND expires_at > now()
             ORDER BY created_at DESC LIMIT 1 FOR UPDATE",
        )
        .bind(t.id)
        .bind(&mobile)
        .fetch_optional(&mut *tx)
        .await?;
        let (otp_id, hash, attempts) = row.ok_or_else(|| rule("Your code has expired. Request a new one."))?;
        if attempts >= 5 {
            return Err(rule("Too many wrong attempts. Request a new code."));
        }
        if otp_hash(t.id, &mobile, code) != hash {
            sqlx::query("UPDATE portal_otps SET attempts = attempts + 1 WHERE id = $1").bind(otp_id).execute(&mut *tx).await?;
            tx.commit().await?;
            return Err(bad("That code is not correct"));
        }
        sqlx::query("UPDATE portal_otps SET used = true WHERE id = $1").bind(otp_id).execute(&mut *tx).await?;
    }

    let (customer_id, created) = super::customers::upsert_by_mobile(&mut tx, t.id, None, &mobile, &b.first_name, &b.nickname).await?;
    let (first_name, nickname): (String, String) = sqlx::query_as("SELECT first_name, nickname FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let token = issue_token(&state.cfg.jwt_secret, customer_id, t.id, "portal", Duration::days(PORTAL_TOKEN_DAYS))?;
    Ok(json!({
        "token": token,
        "customer": { "first_name": first_name, "nickname": nickname, "mobile": mobile },
        "created": created,
    }))
}

fn ensure_same_tenant(c: &PortalCustomer, t: &Tenant) -> AppResult<()> {
    if c.tenant_id != t.id {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

#[derive(Serialize)]
struct PortalOrder {
    id: Uuid,
    order_no: String,
    status: String,
    status_label: String,
    total: Decimal,
    created_at: DateTime<Utc>,
    track_token: Uuid,
    items: Vec<Value>,
    steps: Vec<Value>,
}

/// Progress tracker: Order received → Being prepared → On delivery → Delivered (→ Completed).
/// Steps the business has switched off are not shown to customers.
fn steps(st: &TenantSettings, status: &str) -> Vec<Value> {
    let visible = ["new", "preparing", "on_delivery", "delivered", "completed"];
    let current = FLOW.iter().position(|s| *s == status);
    visible
        .iter()
        .filter(|s| st.order_status_enabled(s))
        .map(|s| {
            let r = FLOW.iter().position(|f| f == s).unwrap_or(0);
            let done = current.is_some_and(|c| c >= r);
            json!({ "status": s, "label": st.order_label(s), "done": done, "current": Some(r) == current })
        })
        .collect()
}

async fn load_orders(state: &AppState, st: &TenantSettings, customer_id: Uuid, limit: i64) -> AppResult<Vec<PortalOrder>> {
    let rows: Vec<(Uuid, String, String, Decimal, DateTime<Utc>, Uuid)> = sqlx::query_as(
        "SELECT id, order_no, status, total, created_at, track_token FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(customer_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let mut conn = state.db.acquire().await?;
    let mut out = Vec::new();
    for (id, order_no, status, total, created_at, track_token) in rows {
        let items = order_items(&mut conn, id)
            .await?
            .into_iter()
            .map(|(pid, name, qty, price, line, photo)| json!({
                "product_id": pid, "name": name, "quantity": qty, "unit_price": price, "line_total": line,
                "photo_url": photo.map(|p| format!("/api/photos/{p}")),
            }))
            .collect();
        out.push(PortalOrder { id, steps: steps(st, &status), status_label: st.order_label(&status), order_no, status, total, created_at, track_token, items });
    }
    Ok(out)
}

async fn me(State(state): State<AppState>, Path(slug): Path<String>, c: PortalCustomer) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    ensure_same_tenant(&c, &t)?;
    let (first_name, nickname, mobile, points): (String, String, String, i64) =
        sqlx::query_as("SELECT first_name, nickname, mobile, points_available FROM customers WHERE id = $1")
            .bind(c.customer_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::Unauthorized)?;
    let total_orders: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE customer_id = $1")
        .bind(c.customer_id)
        .fetch_one(&state.db)
        .await?;
    let l = &t.settings.loyalty;
    let loyalty = (l.enabled && l.show_on_portal).then(|| {
        json!({
            "points": points,
            "value": l.show_value_on_portal.then(|| (Decimal::from(points) * l.point_value).round_dp(2)),
        })
    });
    Ok(Json(json!({
        "customer": { "first_name": first_name, "nickname": nickname, "mobile": mobile },
        "total_orders": total_orders,
        "loyalty": loyalty,
    })))
}

async fn my_orders(State(state): State<AppState>, Path(slug): Path<String>, c: PortalCustomer) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    ensure_same_tenant(&c, &t)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE customer_id = $1")
        .bind(c.customer_id)
        .fetch_one(&state.db)
        .await?;
    let mut orders = serde_json::to_value(load_orders(&state, &t.settings, c.customer_id, 3).await?).unwrap_or_default();
    let hidden = hidden_prices(&state, t.id).await?;
    if let Some(list) = orders.as_array_mut() {
        list.iter_mut().for_each(|o| strip_order_prices(o, &hidden));
    }
    Ok(Json(json!({ "total_orders": total, "orders": priced(&t.settings, orders) })))
}

#[derive(Deserialize)]
struct CatalogueQuery {
    q: Option<String>,
    category_id: Option<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
struct CatalogueItem {
    id: Uuid,
    name: String,
    description: String,
    category_id: Option<Uuid>,
    price: Decimal,
    available: i32,
    primary_photo_id: Option<Uuid>,
}

async fn catalogue(State(state): State<AppState>, Path(slug): Path<String>, Query(q): Query<CatalogueQuery>) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    let branch = portal_branch(&state, &t).await?;
    let items: Vec<CatalogueItem> = sqlx::query_as(
        "SELECT p.id, p.name, p.description, p.category_id, p.marked_price AS price,
                GREATEST(COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0), 0) AS available,
                (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1) AS primary_photo_id
         FROM products p LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $2
         WHERE p.tenant_id = $1 AND p.is_active AND p.available_for_orders
           AND (p.all_branches OR EXISTS (SELECT 1 FROM product_branches pb WHERE pb.product_id = p.id AND pb.branch_id = $2))
           AND ($3::text IS NULL OR p.name ILIKE $3 OR p.nickname ILIKE $3)
           AND ($4::uuid IS NULL OR p.category_id = $4)
           AND ($5 OR COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) > 0)
         ORDER BY (COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0) > 0) DESC, p.name",
    )
    .bind(t.id)
    .bind(branch)
    .bind(like(&q.q))
    .bind(q.category_id)
    .bind(t.settings.orders.show_out_of_stock)
    .fetch_all(&state.db)
    .await?;
    let categories: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT DISTINCT c.id, c.name FROM categories c JOIN products p ON p.category_id = c.id
         WHERE c.tenant_id = $1 AND c.is_active AND p.is_active AND p.available_for_orders ORDER BY c.name",
    )
    .bind(t.id)
    .fetch_all(&state.db)
    .await?;
    let hidden = hidden_prices(&state, t.id).await?;
    let mut items = serde_json::to_value(items).unwrap_or_default();
    if let Some(list) = items.as_array_mut() {
        for i in list.iter_mut() {
            if i["id"].as_str().and_then(|s| Uuid::parse_str(s).ok()).is_some_and(|id| hidden.contains(&id)) {
                i.as_object_mut().map(|m| m.remove("price"));
            }
        }
    }
    Ok(Json(priced(&t.settings, json!({
        "products": items,
        "categories": categories.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
    }))))
}

async fn product(State(state): State<AppState>, Path((slug, id)): Path<(String, Uuid)>) -> AppResult<Json<Value>> {
    let t = tenant(&state, &slug).await?;
    let branch = portal_branch(&state, &t).await?;
    let item: CatalogueItem = sqlx::query_as(
        "SELECT p.id, p.name, p.description, p.category_id, p.marked_price AS price,
                GREATEST(COALESCE(sl.on_hand,0) - COALESCE(sl.reserved,0), 0) AS available,
                (SELECT ph.id FROM product_photos ph WHERE ph.product_id = p.id ORDER BY ph.is_primary DESC, ph.sort_order LIMIT 1) AS primary_photo_id
         FROM products p LEFT JOIN stock_levels sl ON sl.product_id = p.id AND sl.branch_id = $3
         WHERE p.id = $1 AND p.tenant_id = $2 AND p.is_active AND p.available_for_orders",
    )
    .bind(id)
    .bind(t.id)
    .bind(branch)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Product"))?;
    let photos: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM product_photos WHERE product_id = $1 ORDER BY is_primary DESC, sort_order")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    let mut item = serde_json::to_value(item).unwrap_or_default();
    if hidden_prices(&state, t.id).await?.contains(&id) {
        item.as_object_mut().map(|m| m.remove("price"));
    }
    Ok(Json(priced(&t.settings, json!({ "product": item, "photos": photos.into_iter().map(|p| format!("/api/photos/{p}")).collect::<Vec<_>>() }))))
}

#[derive(Deserialize)]
struct PlaceBody {
    items: Vec<OrderLine>,
    #[serde(default)]
    delivery_location: String,
    #[serde(default)]
    notes: String,
}

async fn place_order(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, c: PortalCustomer, Json(b): Json<PlaceBody>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "portal_order", 20, std::time::Duration::from_secs(600))?;
    let t = tenant(&state, &slug).await?;
    ensure_same_tenant(&c, &t)?;
    if b.delivery_location.trim().is_empty() {
        return Err(bad("Tell us where to deliver"));
    }
    let branch = portal_branch(&state, &t).await?;
    let mut tx = state.db.begin().await?;
    let (id, order_no, track_token, total) =
        create_order(&mut tx, t.id, branch, c.customer_id, &b.items, &b.delivery_location, &b.notes, "portal", None).await?;
    let (name, mobile): (String, String) = sqlx::query_as("SELECT first_name, mobile FROM customers WHERE id = $1")
        .bind(c.customer_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let hidden = hidden_prices(&state, t.id).await?;
    let price_hidden = b.items.iter().any(|l| hidden.contains(&l.product_id));

    announce_new(&state, t.id, branch, id, &order_no, &name, total).await;
    if t.settings.orders.notify_customer_whatsapp {
        crate::notify::whatsapp(
            &state,
            t.id,
            mobile,
            format!(
                "Hi {name}! 🎉 We've received your {} order {order_no}. We are now processing it.\nTrack it here: {}/track/{track_token}",
                t.name, state.cfg.public_url
            ),
        );
    }
    let mut out = json!({ "id": id, "order_no": order_no, "track_token": track_token, "total": total });
    if price_hidden {
        out.as_object_mut().map(|m| m.remove("total"));
    }
    Ok(Json(priced(&t.settings, out)))
}

/// Public tracking by unguessable token — no account needed.
async fn track(State(state): State<AppState>, headers: HeaderMap, Path(token): Path<Uuid>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "portal_track", 120, std::time::Duration::from_secs(600))?;
    let row: Option<(Uuid, String, String, Decimal, DateTime<Utc>, String, String, String, bool, Value, Uuid)> = sqlx::query_as(
        "SELECT o.id, o.order_no, o.status, o.total, o.created_at, o.delivery_location, t.name, t.slug, t.logo IS NOT NULL, t.settings, t.id
         FROM orders o JOIN tenants t ON t.id = o.tenant_id WHERE o.track_token = $1",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?;
    let (id, order_no, status, total, created_at, location, business, slug, has_logo, raw, tenant_id) = row.ok_or(AppError::NotFound("Order"))?;
    let mut st: TenantSettings = serde_json::from_value(raw).unwrap_or_default();
    let mut conn = state.db.acquire().await?;
    let lines = order_items(&mut conn, id).await?;
    let hidden = hidden_prices(&state, tenant_id).await?;
    if lines.iter().any(|l| hidden.contains(&l.0)) {
        st.orders.show_prices = false;
    }
    let items: Vec<Value> = lines
        .into_iter()
        .map(|(_, name, qty, price, line, _)| json!({ "name": name, "quantity": qty, "unit_price": price, "line_total": line }))
        .collect();
    let events = order_events(&mut conn, &st, id).await?;
    Ok(Json(priced(&st, json!({
        "business": { "name": business, "slug": slug, "logo_url": has_logo.then(|| format!("/api/public/{slug}/logo")), "show_prices": st.orders.show_prices },
        "order": {
            "order_no": order_no, "status": status, "status_label": st.order_label(&status), "total": total,
            "created_at": created_at, "delivery_location": location,
            "terminal": (["cancelled", "rejected", "returned"].contains(&status.as_str())),
        },
        "steps": steps(&st, &status),
        "items": items,
        "events": events.into_iter().map(|mut e| { e.as_object_mut().map(|o| o.remove("user_name")); e }).collect::<Vec<_>>(),
    }))))
}
