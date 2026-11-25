//! Roadmap 185: each person's own profile — name and profile photo. Email, role and branches are shown read-only
//! (administrators change those in Settings → Users). Applies to the person's sign-in identity and their linked access
//! to the tenant's other businesses (roadmap 72). Audited without the photo itself; never from a support session.

use axum::body::Bytes;
use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/profile", get(show).put(update))
        .route("/auth/profile/avatar", post(upload).delete(remove))
        .route("/avatars/{key}", get(avatar))
}

pub const MAX_AVATAR_BYTES: usize = 1024 * 1024;

/// The person's sign-in identity (a linked row's identity).
async fn identity(state: &AppState, ctx: &Ctx) -> AppResult<Uuid> {
    if ctx.support.is_some() || ctx.acting_from.is_some() {
        return Err(AppError::Forbidden("Your profile is changed from your own business".into()));
    }
    Ok(sqlx::query_scalar("SELECT COALESCE(login_user_id, id) FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(ctx.user_id)
        .bind(ctx.tenant_id)
        .fetch_one(&state.db)
        .await?)
}

async fn show(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let (name, email, role, all_branches, avatar_url): (String, String, String, bool, Option<String>) = sqlx::query_as(
        "SELECT u.name, u.email, r.name, u.all_branches OR '*' = ANY(r.permissions), user_avatar_url(u.id)
         FROM users u JOIN roles r ON r.id = u.role_id WHERE u.id = $1 AND u.tenant_id = $2",
    )
    .bind(ctx.user_id)
    .bind(ctx.tenant_id)
    .fetch_one(&state.db)
    .await?;
    let branches: Vec<String> = if all_branches {
        vec![]
    } else {
        sqlx::query_scalar("SELECT b.name FROM user_branches ub JOIN branches b ON b.id = ub.branch_id WHERE ub.user_id = $1 AND b.is_active ORDER BY b.created_at")
            .bind(ctx.user_id)
            .fetch_all(&state.db)
            .await?
    };
    Ok(Json(json!({ "name": name, "email": email, "role": role, "all_branches": all_branches, "branches": branches, "avatar_url": avatar_url })))
}

#[derive(Deserialize)]
struct ProfileBody {
    name: String,
}

pub fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = name.chars().count();
    if !(2..=80).contains(&n) {
        return Err(bad("Enter your full name (2–80 characters)"));
    }
    if name.chars().any(|c| c.is_control() || matches!(c, '<' | '>')) {
        return Err(bad("Your name contains characters that are not allowed"));
    }
    Ok(name)
}

/// The name only — email, role and branches are never changed here, whatever is sent.
async fn update(State(state): State<AppState>, ctx: Ctx, Json(b): Json<ProfileBody>) -> AppResult<Json<Value>> {
    let me = identity(&state, &ctx).await?;
    let name = clean_name(&b.name)?;
    let mut tx = state.db.begin().await?;
    let before: String = sqlx::query_scalar("SELECT name FROM users WHERE id = $1 FOR UPDATE").bind(me).fetch_one(&mut *tx).await?;
    if before == name {
        return Ok(Json(json!({ "ok": true, "name": name })));
    }
    sqlx::query("UPDATE users SET name = $2 WHERE id = $1 OR login_user_id = $1").bind(me).bind(&name).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("users", "profile_updated", "user", ctx.user_id).before(json!({ "name": before })).after(json!({ "name": name }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "name": name })))
}

/// Width and height from the image's own header (PNG IHDR, JPEG start-of-frame, WebP VP8/VP8L/VP8X) — a file that
/// only looks like an image is refused.
pub fn image_size(data: &[u8], mime: &str) -> Option<(u32, u32)> {
    let be16 = |i: usize| data.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32);
    let be32 = |i: usize| data.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le16 = |i: usize| data.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32);
    let le24 = |i: usize| data.get(i..i + 3).map(|b| b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16);
    match mime {
        "image/png" => (data.get(12..16)? == b"IHDR").then_some(())
            .and_then(|_| Some((be32(16)?, be32(20)?))),
        "image/jpeg" => {
            let mut i = 2;
            while i + 9 < data.len() {
                if data[i] != 0xFF {
                    return None;
                }
                let marker = data[i + 1];
                if marker == 0xFF {
                    i += 1;
                    continue;
                }
                let len = be16(i + 2)? as usize;
                if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                    return Some((be16(i + 7)?, be16(i + 5)?));
                }
                i += 2 + len;
            }
            None
        }
        "image/webp" => match data.get(12..16)? {
            b"VP8 " => (data.get(23..26)? == [0x9d, 0x01, 0x2a]).then_some(())
                .and_then(|_| Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff))),
            b"VP8L" => {
                let b = data.get(21..25)?;
                let bits = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                (data.get(20) == Some(&0x2f)).then_some((1 + (bits & 0x3fff), 1 + ((bits >> 14) & 0x3fff)))
            }
            b"VP8X" => Some((1 + le24(24)?, 1 + le24(27)?)),
            _ => None,
        },
        _ => None,
    }
}

/// A new (or replaced) profile photo: PNG, JPEG or WebP, at most 1 MB, a real image 64–2048 px a side, roughly square
/// (the app crops it). Stored on the person's identity under a new key.
async fn upload(State(state): State<AppState>, ctx: Ctx, mut mp: Multipart) -> AppResult<Json<Value>> {
    let me = identity(&state, &ctx).await?;
    let field = mp.next_field().await.map_err(|_| bad("The photo could not be read. Try again."))?.ok_or_else(|| bad("Choose a photo"))?;
    let data: Bytes = field.bytes().await.map_err(|_| bad("The photo could not be read. Try again."))?;
    if data.is_empty() {
        return Err(bad("Choose a photo"));
    }
    if data.len() > MAX_AVATAR_BYTES {
        return Err(bad("The photo must be under 1 MB"));
    }
    let mime = super::catalog::sniff_image(&data).ok_or_else(|| bad("The photo must be a PNG, JPEG or WebP image"))?;
    let (w, h) = image_size(&data, mime).ok_or_else(|| bad("This file is not a readable image"))?;
    if !(64..=2048).contains(&w) || !(64..=2048).contains(&h) || w.max(h) > w.min(h) * 5 / 4 {
        return Err(bad("The photo must be square, between 64 and 2048 pixels"));
    }
    let key = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    let had: bool = sqlx::query_scalar("SELECT avatar_key IS NOT NULL FROM users WHERE id = $1 FOR UPDATE").bind(me).fetch_one(&mut *tx).await?;
    sqlx::query("UPDATE users SET avatar = $2, avatar_mime = $3, avatar_key = $4 WHERE id = $1").bind(me).bind(data.to_vec()).bind(mime).bind(key).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &ctx,
        Entry::new("users", if had { "profile_photo_replaced" } else { "profile_photo_added" }, "user", ctx.user_id)
            .after(json!({ "type": mime, "bytes": data.len(), "size": format!("{w}×{h}") })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "avatar_url": format!("/api/avatars/{key}") })))
}

/// Back to the initials.
async fn remove(State(state): State<AppState>, ctx: Ctx) -> AppResult<Json<Value>> {
    let me = identity(&state, &ctx).await?;
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("UPDATE users SET avatar = NULL, avatar_mime = NULL, avatar_key = NULL WHERE id = $1 AND avatar_key IS NOT NULL")
        .bind(me)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n > 0 {
        audit::record(&mut tx, &ctx, Entry::new("users", "profile_photo_removed", "user", ctx.user_id)).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "avatar_url": null })))
}

/// The photo by its unguessable key (it changes with every new photo, so it is cached for good); `nosniff` and a
/// sandboxing policy so it can only ever be shown as an image.
async fn avatar(State(state): State<AppState>, Path(key): Path<Uuid>) -> AppResult<impl IntoResponse> {
    let (data, mime): (Vec<u8>, String) = sqlx::query_as("SELECT avatar, avatar_mime FROM users WHERE avatar_key = $1 AND avatar IS NOT NULL")
        .bind(key)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Photo"))?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, max-age=31536000, immutable".into()),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox".into()),
        ],
        data,
    ))
}

#[cfg(test)]
mod tests {
    use super::{clean_name, image_size};

    #[test]
    fn names_are_tidied_and_bounded() {
        assert_eq!(clean_name("  Pablo   Oindo ").unwrap(), "Pablo Oindo");
        assert!(clean_name("P").is_err());
        assert!(clean_name("<script>").is_err());
        assert!(clean_name(&"a".repeat(81)).is_err());
    }

    #[test]
    fn image_sizes_from_headers() {
        let mut png = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&512u32.to_be_bytes());
        png.extend_from_slice(&500u32.to_be_bytes());
        assert_eq!(image_size(&png, "image/png"), Some((512, 500)));
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0x00, 0x02, 0x00, 0x03];
        assert_eq!(image_size(&jpeg, "image/jpeg"), Some((512, 256)));
        assert_eq!(image_size(b"\xFF\xD8\xFFnot really a jpeg at all", "image/jpeg"), None);
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
        webp.extend_from_slice(&[0; 8]);
        webp.extend_from_slice(&[0xFF, 0x01, 0x00, 0xFF, 0x01, 0x00]);
        assert_eq!(image_size(&webp, "image/webp"), Some((512, 512)));
    }
}
