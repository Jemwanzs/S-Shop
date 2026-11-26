//! Roadmap 190: product photos managed from Settings → Website → Products → (product).
//!
//! The photos are the product's own (`product_photos`) — one source of truth for the catalogue, product pages, the till,
//! stock, the website and the ordering link. From the website editor, *Manage website product photos* may add, replace,
//! reorder, remove and choose the primary photo without any other product permission. A change is saved as one request:
//! new photos are **staged** (never shown anywhere), and the whole change is applied together —
//!
//! * at once when the person may publish the website and *Product edit* needs no approval for them;
//! * otherwise after approval through the workflow engine (*Product edit*'s chain, else the Tenant Administrator).
//!
//! The live website and catalogue therefore never change before that. A change is refused when the product's photos
//! changed after it was made (it is reviewed against what was there), over the photo limit, or with the same image twice.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::types::Json as DbJson;
use sqlx::PgConnection;
use uuid::Uuid;

use super::approvals::ApprovalRow;
use super::catalog::sniff_image;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, rule, AppError, AppResult};
use crate::settings;
use crate::state::AppState;
use crate::workflow;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/website/products/{id}/photos", get(current).put(save))
        .route("/website/products/{id}/photos/stage", post(stage).layer(DefaultBodyLimit::max(4 * 1024 * 1024)))
        .route("/website/photo-requests/{id}", get(request_detail))
        .route("/photos/staged/{id}", get(staged_photo))
}

/// Smallest side a product photo may have (smaller ones look blurred on the website and in the catalogue).
const MIN_SIDE: u32 = 200;

fn require(ctx: &Ctx) -> AppResult<()> {
    if !ctx.can("website.photos") {
        return Err(AppError::Forbidden("You do not have permission to manage website product photos".into()));
    }
    Ok(())
}

/// The product's photos as shown everywhere: primary first, then in order.
async fn photo_ids(conn: &mut PgConnection, tenant: Uuid, product: Uuid) -> AppResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar("SELECT id FROM product_photos WHERE product_id = $1 AND tenant_id = $2 ORDER BY is_primary DESC, sort_order, id")
        .bind(product)
        .bind(tenant)
        .fetch_all(&mut *conn)
        .await?)
}

async fn product_name(conn: &mut PgConnection, tenant: Uuid, product: Uuid) -> AppResult<String> {
    sqlx::query_scalar("SELECT name FROM products WHERE id = $1 AND tenant_id = $2")
        .bind(product)
        .bind(tenant)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Product"))
}

/// Whether this person's change is applied straight away (otherwise it is sent for approval).
async fn applies_directly(conn: &mut PgConnection, ctx: &Ctx) -> AppResult<bool> {
    Ok(ctx.can("website.publish") && !workflow::needs_approval(conn, ctx, "product.edit", workflow::Gate::default()).await?)
}

async fn awaiting(conn: &mut PgConnection, tenant: Uuid, product: Uuid) -> AppResult<Option<Value>> {
    let row: Option<(Uuid, Value, Option<Uuid>, Value, Option<String>, chrono::DateTime<chrono::Utc>, Option<Uuid>)> = sqlx::query_as(
        "SELECT q.id, q.proposed, q.primary_id, q.summary, u.name, q.requested_at, q.approval_id
         FROM product_photo_requests q LEFT JOIN users u ON u.id = q.requested_by
         WHERE q.product_id = $1 AND q.tenant_id = $2 AND q.status = 'awaiting'",
    )
    .bind(product)
    .bind(tenant)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(id, proposed, primary, summary, by, at, approval)| {
        json!({ "id": id, "photos": with_urls(&proposed), "primary_id": primary, "summary": summary, "requested_by": by, "requested_at": at, "approval_id": approval })
    }))
}

/// [{id, staged}] → [{id, staged, url}] (staged photos are served from their own address).
fn with_urls(proposed: &Value) -> Vec<Value> {
    proposed
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    let id = p["id"].as_str().unwrap_or_default();
                    let staged = p["staged"].as_bool().unwrap_or(false);
                    json!({ "id": id, "staged": staged, "url": if staged { format!("/api/photos/staged/{id}") } else { format!("/api/photos/{id}") } })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The product's photos, the limit, any change waiting for approval and whether saving applies at once.
async fn current(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require(&ctx)?;
    let mut conn = state.db.acquire().await?;
    let name = product_name(&mut conn, ctx.tenant_id, id).await?;
    let ids = photo_ids(&mut conn, ctx.tenant_id, id).await?;
    let max = settings::load(&mut conn, ctx.tenant_id).await?.product.max_photos;
    let pending = awaiting(&mut conn, ctx.tenant_id, id).await?;
    let direct = applies_directly(&mut conn, &ctx).await?;
    Ok(Json(json!({
        "product": { "id": id, "name": name },
        "photos": ids.iter().enumerate().map(|(i, p)| json!({ "id": p, "is_primary": i == 0, "url": format!("/api/photos/{p}") })).collect::<Vec<_>>(),
        "max": max, "pending": pending, "applies_directly": direct,
    })))
}

/// Stages one new photo (checked, stored, shown nowhere until its change is applied). `upload_ref` makes retries safe.
async fn stage(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, mut mp: Multipart) -> AppResult<Json<Value>> {
    require(&ctx)?;
    let mut upload_ref: Option<Uuid> = None;
    let mut data: Option<Bytes> = None;
    let mut thumb: Option<Bytes> = None;
    while let Some(field) = mp.next_field().await.map_err(|_| bad("The photo could not be read. Try again."))? {
        match field.name() {
            Some("upload_ref") => upload_ref = field.text().await.ok().and_then(|t| Uuid::parse_str(t.trim()).ok()),
            Some("file") => data = Some(field.bytes().await.map_err(|_| bad("The photo could not be read. Try again."))?),
            Some("thumb") => thumb = Some(field.bytes().await.map_err(|_| bad("The photo could not be read. Try again."))?),
            _ => {}
        }
    }
    let data = data.ok_or_else(|| bad("No photo uploaded"))?;
    let mime = sniff_image(&data).ok_or_else(|| refused("Not a photo", "Only JPEG, PNG or WebP images can be added as product photos."))?;
    if data.len() > 3 * 1024 * 1024 {
        return Err(refused("Photo too large", "Each photo must be under 3 MB."));
    }
    if let Some((w, h)) = super::profile::image_size(&data, mime) {
        if w.min(h) < MIN_SIDE {
            return Err(refused("Photo too small", format!("This photo is {w}×{h} pixels — use one at least {MIN_SIDE} pixels on each side so it looks sharp.")));
        }
    }
    let thumb = thumb.filter(|t| t.len() <= 400 * 1024).and_then(|t| sniff_image(&t).map(|m| (t.to_vec(), m)));
    let hash = hex::encode(Sha256::digest(&data));

    let mut tx = state.db.begin().await?;
    product_name(&mut tx, ctx.tenant_id, id).await?;
    if let Some(r) = upload_ref {
        let already: Option<Uuid> = sqlx::query_scalar("SELECT id FROM product_photo_staging WHERE product_id = $1 AND upload_ref = $2 AND tenant_id = $3")
            .bind(id)
            .bind(r)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(sid) = already {
            return Ok(Json(json!({ "id": sid, "url": format!("/api/photos/staged/{sid}"), "staged": true, "duplicate": true })));
        }
    }
    // The same image twice on one product is refused (it would show twice everywhere).
    let on_product: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM product_photos WHERE product_id = $1 AND hash = $2)")
        .bind(id)
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await?;
    if on_product {
        return Err(refused("Already a photo of this product", "This exact photo is already on the product."));
    }
    // Staged photos never used are cleared after two days.
    sqlx::query(
        "DELETE FROM product_photo_staging s WHERE s.tenant_id = $1 AND s.created_at < now() - interval '2 days'
           AND NOT EXISTS (SELECT 1 FROM product_photo_requests q WHERE q.status = 'awaiting' AND q.proposed @> jsonb_build_array(jsonb_build_object('id', s.id::text)))",
    )
    .bind(ctx.tenant_id)
    .execute(&mut *tx)
    .await?;
    let sid: Uuid = sqlx::query_scalar(
        "INSERT INTO product_photo_staging (tenant_id, product_id, data, mime, thumb, thumb_mime, hash, upload_ref, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(data.to_vec())
    .bind(mime)
    .bind(thumb.as_ref().map(|t| t.0.clone()))
    .bind(thumb.as_ref().map(|t| t.1))
    .bind(&hash)
    .bind(upload_ref)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": sid, "url": format!("/api/photos/staged/{sid}"), "staged": true })))
}

#[derive(Deserialize)]
struct SizeQuery {
    size: Option<String>,
}

/// A staged photo (unguessable id, like product photos): for the editor's preview and the approver's review.
async fn staged_photo(State(state): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<SizeQuery>) -> AppResult<impl IntoResponse> {
    let thumb = q.size.as_deref() == Some("thumb");
    let (data, mime): (Vec<u8>, String) = sqlx::query_as(
        "SELECT CASE WHEN $2 AND thumb IS NOT NULL THEN thumb ELSE data END, CASE WHEN $2 AND thumb IS NOT NULL THEN thumb_mime ELSE mime END
         FROM product_photo_staging WHERE id = $1",
    )
    .bind(id)
    .bind(thumb)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Photo"))?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=86400".into())], data))
}

#[derive(Deserialize, Clone)]
struct ProposedPhoto {
    id: Uuid,
    #[serde(default)]
    staged: bool,
}

#[derive(Deserialize)]
struct SaveBody {
    /// The photos in order; the first is the primary photo.
    photos: Vec<ProposedPhoto>,
    /// The photos as the person saw them (in order): refused if they changed meanwhile.
    base: Vec<Uuid>,
    #[serde(default)]
    comments: String,
}

/// What a change does, for the request, the approver and the audit trail.
fn summarize(base: &[Uuid], proposed: &[ProposedPhoto]) -> Value {
    let kept: Vec<Uuid> = proposed.iter().filter(|p| !p.staged).map(|p| p.id).collect();
    let added = proposed.iter().filter(|p| p.staged).count();
    let removed = base.iter().filter(|b| !kept.contains(b)).count();
    let primary_changed = proposed.first().map(|p| p.id) != base.first().copied();
    let remaining: Vec<Uuid> = base.iter().copied().filter(|b| kept.contains(b)).collect();
    let reordered = remaining != kept;
    json!({ "added": added, "removed": removed, "reordered": reordered, "primary_changed": primary_changed, "before": base.len(), "after": proposed.len() })
}

fn describe(s: &Value) -> String {
    let mut parts = Vec::new();
    let n = |k: &str| s[k].as_u64().unwrap_or(0);
    if n("added") > 0 {
        parts.push(format!("{} added", n("added")));
    }
    if n("removed") > 0 {
        parts.push(format!("{} removed", n("removed")));
    }
    if s["primary_changed"].as_bool() == Some(true) {
        parts.push("new primary photo".into());
    }
    if s["reordered"].as_bool() == Some(true) {
        parts.push("reordered".into());
    }
    format!("{} ({} → {} photos)", parts.join(", "), n("before"), n("after"))
}

/// Saves a change: applied at once, or sent for approval (staged photos stay invisible until then).
async fn save(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<SaveBody>) -> AppResult<Json<Value>> {
    require(&ctx)?;
    let comments: String = b.comments.trim().chars().take(500).collect();
    let mut tx = state.db.begin().await?;
    let name = product_name(&mut tx, ctx.tenant_id, id).await?;
    sqlx::query("SELECT id FROM products WHERE id = $1 FOR UPDATE").bind(id).execute(&mut *tx).await?;
    let current = photo_ids(&mut tx, ctx.tenant_id, id).await?;
    if current != b.base {
        return Err(refused("Changed meanwhile", "This product's photos were changed after you opened them — reopen the product to see the latest photos"));
    }
    if awaiting(&mut tx, ctx.tenant_id, id).await?.is_some() {
        return Err(refused("Already waiting", "A photo change for this product is already waiting for approval — wait for its decision (or withdraw it in Approvals)"));
    }
    validate(&mut tx, ctx.tenant_id, id, &current, &b.photos).await?;
    let summary = summarize(&current, &b.photos);
    let proposed = json!(b.photos.iter().map(|p| json!({ "id": p.id, "staged": p.staged })).collect::<Vec<_>>());
    let request: Uuid = sqlx::query_scalar(
        "INSERT INTO product_photo_requests (tenant_id, product_id, base, proposed, primary_id, summary, requested_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(id)
    .bind(json!(current))
    .bind(&proposed)
    .bind(b.photos.first().map(|p| p.id))
    .bind(&summary)
    .bind(ctx.user_id)
    .fetch_one(&mut *tx)
    .await?;
    if applies_directly(&mut tx, &ctx).await? {
        apply(&mut tx, &ctx, request, None).await?;
        tx.commit().await?;
        state.emit(ctx.tenant_id, None, "products", json!({ "product_id": id }));
        return Ok(Json(json!({ "applied": true, "request_id": request })));
    }
    let custom = workflow::rule_for(&mut tx, ctx.tenant_id, "product.edit").await?.is_some_and(|r| r.enabled && !r.levels.is_empty());
    let approval = workflow::submit(
        &mut tx,
        &ctx,
        workflow::Request {
            action: "product.edit",
            entity_type: "product",
            entity_id: id,
            branch_id: None,
            summary: format!("Photos of {name} — {}", describe(&summary)),
            amount: None,
            payload: json!({ "photos_request": request, "product_name": name, "summary": summary, "comments": comments }),
        },
    )
    .await?;
    if !custom {
        sqlx::query("UPDATE approvals SET steps = $2 WHERE id = $1").bind(approval).bind(DbJson(vec![workflow::Level::single("admin")])).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE product_photo_requests SET approval_id = $2 WHERE id = $1").bind(request).bind(approval).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("products", "photos_change_requested", "product", id).after(json!({ "request": request, "summary": summary })).approval(Some(approval)).comments(&comments),
    )
    .await?;
    tx.commit().await?;
    super::approvals::notify_approvers(&state, &ctx, approval).await;
    Ok(Json(json!({ "applied": false, "pending_approval": true, "approval_id": approval, "request_id": request })))
}

/// The proposal uses only this product's photos and its own staged photos, each once, within the limit, and changes something.
async fn validate(conn: &mut PgConnection, tenant: Uuid, product: Uuid, current: &[Uuid], photos: &[ProposedPhoto]) -> AppResult<()> {
    let max = settings::load(&mut *conn, tenant).await?.product.max_photos as usize;
    if photos.len() > max {
        return Err(refused("Photo limit reached", format!("A product can have at most {max} photos.")));
    }
    let mut seen: Vec<Uuid> = Vec::new();
    let mut hashes: Vec<String> = Vec::new();
    for p in photos {
        if seen.contains(&p.id) {
            return Err(bad("A photo appears twice"));
        }
        seen.push(p.id);
        if p.staged {
            let hash: Option<String> = sqlx::query_scalar("SELECT hash FROM product_photo_staging WHERE id = $1 AND product_id = $2 AND tenant_id = $3")
                .bind(p.id)
                .bind(product)
                .bind(tenant)
                .fetch_optional(&mut *conn)
                .await?;
            let hash = hash.ok_or_else(|| refused("Photo missing", "A new photo was not uploaded (or expired) — add it again"))?;
            if hashes.contains(&hash) {
                return Err(refused("Same photo twice", "The same photo was added twice — remove one"));
            }
            hashes.push(hash);
        } else if !current.contains(&p.id) {
            return Err(refused("Photo missing", "A photo is no longer on this product — reopen the product"));
        }
    }
    let unchanged = photos.iter().all(|p| !p.staged) && photos.iter().map(|p| p.id).collect::<Vec<_>>() == current;
    if unchanged {
        return Err(rule("Nothing to save — the photos are unchanged"));
    }
    Ok(())
}

/// Applies a request in the caller's transaction: removed photos deleted, staged photos added, order and primary set.
async fn apply(conn: &mut PgConnection, ctx: &Ctx, request: Uuid, approval: Option<Uuid>) -> AppResult<()> {
    let (product, base, proposed, status, summary): (Uuid, Value, Value, String, Value) =
        sqlx::query_as("SELECT product_id, base, proposed, status, summary FROM product_photo_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
            .bind(request)
            .bind(ctx.tenant_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(AppError::NotFound("Photo change"))?;
    if status != "awaiting" {
        return Err(rule(format!("This photo change is already {status}")));
    }
    sqlx::query("SELECT id FROM products WHERE id = $1 FOR UPDATE").bind(product).execute(&mut *conn).await?;
    let base: Vec<Uuid> = serde_json::from_value(base).map_err(|_| bad("Stored photo change is invalid"))?;
    let photos: Vec<ProposedPhoto> = serde_json::from_value::<Vec<Value>>(proposed)
        .map_err(|_| bad("Stored photo change is invalid"))?
        .into_iter()
        .map(|p| ProposedPhoto { id: p["id"].as_str().and_then(|s| s.parse().ok()).unwrap_or_default(), staged: p["staged"].as_bool().unwrap_or(false) })
        .collect();
    let current = photo_ids(conn, ctx.tenant_id, product).await?;
    if current != base {
        return Err(refused("Outdated request", "This product's photos were changed after the request was made — return it so it can be redone on the current photos"));
    }
    validate(conn, ctx.tenant_id, product, &current, &photos).await?;
    let kept: Vec<Uuid> = photos.iter().filter(|p| !p.staged).map(|p| p.id).collect();
    for gone in current.iter().filter(|c| !kept.contains(c)) {
        sqlx::query("DELETE FROM product_photos WHERE id = $1 AND product_id = $2 AND tenant_id = $3").bind(gone).bind(product).bind(ctx.tenant_id).execute(&mut *conn).await?;
    }
    sqlx::query("UPDATE product_photos SET is_primary = false WHERE product_id = $1 AND tenant_id = $2").bind(product).bind(ctx.tenant_id).execute(&mut *conn).await?;
    let mut applied: Vec<Uuid> = Vec::new();
    for (i, p) in photos.iter().enumerate() {
        let id: Uuid = if p.staged {
            sqlx::query_scalar(
                "INSERT INTO product_photos (tenant_id, product_id, data, mime, is_primary, sort_order, thumb, thumb_mime, hash)
                 SELECT tenant_id, product_id, data, mime, $3, $4, thumb, thumb_mime, hash FROM product_photo_staging WHERE id = $1 AND tenant_id = $2
                 RETURNING id",
            )
            .bind(p.id)
            .bind(ctx.tenant_id)
            .bind(i == 0)
            .bind(i as i32)
            .fetch_one(&mut *conn)
            .await?
        } else {
            sqlx::query("UPDATE product_photos SET sort_order = $3, is_primary = $4 WHERE id = $1 AND tenant_id = $2")
                .bind(p.id)
                .bind(ctx.tenant_id)
                .bind(i as i32)
                .bind(i == 0)
                .execute(&mut *conn)
                .await?;
            p.id
        };
        applied.push(id);
    }
    let staged: Vec<Uuid> = photos.iter().filter(|p| p.staged).map(|p| p.id).collect();
    sqlx::query("DELETE FROM product_photo_staging WHERE id = ANY($1) AND tenant_id = $2").bind(&staged).bind(ctx.tenant_id).execute(&mut *conn).await?;
    sqlx::query("UPDATE products SET updated_at = now() WHERE id = $1 AND tenant_id = $2").bind(product).bind(ctx.tenant_id).execute(&mut *conn).await?;
    sqlx::query("UPDATE product_photo_requests SET status = 'applied', decided_by = $2, decided_at = now() WHERE id = $1")
        .bind(request)
        .bind(ctx.user_id)
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        ctx,
        Entry::new("products", "photos_updated", "product", product)
            .before(json!({ "photos": base }))
            .after(json!({ "photos": applied, "summary": summary, "request": request, "from": "website" }))
            .approval(approval),
    )
    .await
}

/// Final approval of a photo change: applied by someone who may change the website or the product.
pub async fn on_approved(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow) -> AppResult<()> {
    if !ctx.can("website.publish") && !ctx.can("products.edit") {
        return Err(AppError::Forbidden("Approving this changes the product's photos everywhere — only someone who may publish the website or edit products can give the final approval".into()));
    }
    let request: Uuid = a.payload["photos_request"].as_str().and_then(|s| s.parse().ok()).ok_or_else(|| bad("Stored photo change is invalid"))?;
    apply(conn, ctx, request, Some(a.id)).await
}

/// Returned, rejected or withdrawn: nothing changes; the staged photos are discarded.
pub async fn on_closed(conn: &mut PgConnection, ctx: &Ctx, a: &ApprovalRow, outcome: &str, comments: &str) -> AppResult<()> {
    let Some(request) = a.payload["photos_request"].as_str().and_then(|s| s.parse::<Uuid>().ok()) else { return Ok(()) };
    let status = match outcome {
        "returned" => "returned",
        "withdrawn" | "cancelled" => "withdrawn",
        _ => "rejected",
    };
    let proposed: Option<Value> = sqlx::query_scalar(
        "UPDATE product_photo_requests SET status = $3, decided_by = $4, decided_at = now(), decision_comments = $5
         WHERE id = $1 AND tenant_id = $2 AND status = 'awaiting' RETURNING proposed",
    )
    .bind(request)
    .bind(ctx.tenant_id)
    .bind(status)
    .bind(ctx.user_id)
    .bind(comments)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(p) = proposed {
        let staged: Vec<Uuid> = p.as_array().map(|a| a.iter().filter(|x| x["staged"].as_bool() == Some(true)).filter_map(|x| x["id"].as_str()?.parse().ok()).collect()).unwrap_or_default();
        sqlx::query("DELETE FROM product_photo_staging WHERE id = ANY($1) AND tenant_id = $2").bind(&staged).bind(ctx.tenant_id).execute(&mut *conn).await?;
    }
    audit::record(conn, ctx, Entry::new("products", &format!("photos_change_{status}"), "product", a.entity_id).after(json!({ "request": request })).approval(Some(a.id)).comments(comments)).await
}

/// One photo change (the approver's review: the photos before and as proposed).
async fn request_detail(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    if !ctx.can("website.photos") && !ctx.can("approvals.approve") && !ctx.can("audit.view") {
        return Err(AppError::Forbidden("You cannot see photo changes".into()));
    }
    let row: Option<(Uuid, Value, Value, Value, String, Option<String>)> = sqlx::query_as(
        "SELECT q.product_id, q.base, q.proposed, q.summary, q.status, p.name FROM product_photo_requests q JOIN products p ON p.id = q.product_id
         WHERE q.id = $1 AND q.tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&state.db)
    .await?;
    let (product, base, proposed, summary, status, name) = row.ok_or(AppError::NotFound("Photo change"))?;
    let before: Vec<Value> = base.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| json!({ "id": x, "url": format!("/api/photos/{x}") })).collect()).unwrap_or_default();
    Ok(Json(json!({ "id": id, "product_id": product, "product_name": name, "before": before, "proposed": with_urls(&proposed), "summary": summary, "status": status })))
}
