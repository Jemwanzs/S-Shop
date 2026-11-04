//! Each business website's own favicon (roadmap 161).
//!
//! The browser reads what the person chose (PNG, ICO, SVG, JPEG, WebP) and draws it at each size; only those PNGs reach the server,
//! which checks that each really is a PNG of the stated size. So an SVG, with whatever it may contain, is never stored or served.
//! A set never changes once stored, which lets its address be cached for good: a new favicon is a new address. The S'Shop app's
//! own icons are static files of the app and are not touched by anything here.

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde_json::{json, Value};
use uuid::Uuid;

use super::site::Site;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/website/favicon", post(upload).layer(DefaultBodyLimit::max(3 * 1024 * 1024)))
        .route("/site/favicon/{id}/{size}", get(image))
}

const SIZES: [u32; 6] = [16, 32, 48, 180, 192, 512];

/// The width and height a PNG's own header states; None when it is not a complete PNG.
pub fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() < 45 || data[..8] != SIGNATURE || &data[12..16] != b"IHDR" || &data[data.len() - 8..data.len() - 4] != b"IEND" {
        return None;
    }
    Some((u32::from_be_bytes(data[16..20].try_into().ok()?), u32::from_be_bytes(data[20..24].try_into().ok()?)))
}

async fn upload(State(state): State<AppState>, ctx: Ctx, mut mp: Multipart) -> AppResult<Json<Value>> {
    ctx.require("website.design")?;
    let on: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM websites WHERE tenant_id = $1 AND status = 'active')").bind(ctx.tenant_id).fetch_one(&state.db).await?;
    if !on {
        return Err(refused("Website not active", "A favicon needs the website service to be active"));
    }
    let unreadable = || bad("The favicon could not be read. Try again.");
    let mut name = String::new();
    let mut files: [Option<Vec<u8>>; 6] = Default::default();
    while let Some(field) = mp.next_field().await.map_err(|_| unreadable())? {
        let key = field.name().unwrap_or_default().to_string();
        if key == "name" {
            name = field.text().await.map_err(|_| unreadable())?.trim().chars().take(120).collect();
        } else if let Some(i) = SIZES.iter().position(|s| key == format!("s{s}")) {
            files[i] = Some(field.bytes().await.map_err(|_| unreadable())?.to_vec());
        }
    }
    for (i, size) in SIZES.iter().enumerate() {
        let data = files[i].as_deref().ok_or_else(|| bad(format!("The {size} × {size} image is missing")))?;
        if png_size(data) != Some((*size, *size)) {
            return Err(refused("Cannot use this image", format!("The {size} × {size} image is not a valid PNG of that size")));
        }
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO website_favicons (tenant_id, png16, png32, png48, png180, png192, png512, source_name, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id")
        .bind(ctx.tenant_id)
        .bind(files[0].as_deref())
        .bind(files[1].as_deref())
        .bind(files[2].as_deref())
        .bind(files[3].as_deref())
        .bind(files[4].as_deref())
        .bind(files[5].as_deref())
        .bind(&name)
        .bind(ctx.user_id)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "favicon_uploaded", "website", ctx.tenant_id).after(json!({ "favicon": id, "file": name }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

/// One image of a set. Public, as any picture on a website is; the id changes with the favicon, so it can be cached for good.
async fn image(State(state): State<AppState>, Path((id, size)): Path<(Uuid, String)>) -> AppResult<axum::response::Response> {
    let size: u32 = size.trim_end_matches(".png").parse().map_err(|_| AppError::NotFound("Image"))?;
    let data = png_of(&state, id, size).await?.ok_or(AppError::NotFound("Image"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        data,
    )
        .into_response())
}

async fn png_of(state: &AppState, id: Uuid, size: u32) -> AppResult<Option<Vec<u8>>> {
    let column = match size {
        16 => "png16",
        32 => "png32",
        48 => "png48",
        180 => "png180",
        192 => "png192",
        512 => "png512",
        _ => return Ok(None),
    };
    Ok(sqlx::query_scalar(&format!("SELECT {column} FROM website_favicons WHERE id = $1")).bind(id).fetch_optional(&state.db).await?)
}

/// A set this business owns, for checking a draft that names one.
pub async fn owned(conn: &mut sqlx::PgConnection, tenant: Uuid, id: Uuid) -> AppResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_favicons WHERE id = $1 AND tenant_id = $2)").bind(id).bind(tenant).fetch_one(&mut *conn).await?)
}

fn url(id: Uuid, size: u32) -> String {
    format!("/api/site/favicon/{id}/{size}.png")
}

/// The first letter of the business on its own colour: what a website with neither a favicon nor a logo shows. Never S'Shop's icon.
pub fn letter_icon(brand: &str, colour: &str) -> String {
    let letter: String = brand.chars().find(|c| c.is_alphanumeric()).map_or_else(|| "•".to_string(), |c| c.to_uppercase().collect());
    let fill = if colour.len() == 7 && colour.starts_with('#') && colour[1..].chars().all(|c| c.is_ascii_hexdigit()) { colour } else { "#444444" };
    let on = crate::website::on_color(fill);
    let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"{fill}\"/><text x=\"32\" y=\"45\" font-family=\"sans-serif\" font-size=\"40\" font-weight=\"700\" text-anchor=\"middle\" fill=\"{on}\">{letter}</text></svg>");
    format!("data:image/svg+xml;base64,{}", base64::engine::general_purpose::STANDARD.encode(svg))
}

/// The page's icon tags: the favicon set, else the business logo, else the letter. `base` is this address's own path to the website
/// ("" on its domain, `/s/{slug}` on the S'Shop host), so the installed-app file is always same-origin.
pub fn head_tags(favicon: Option<Uuid>, logo: Option<&str>, brand: &str, colour: &str, base: &str) -> String {
    use super::site_seo::esc;
    let mut head = String::new();
    match (favicon, logo) {
        (Some(id), _) => {
            for s in [16, 32, 48, 192] {
                head.push_str(&format!("<link rel=\"icon\" type=\"image/png\" sizes=\"{s}x{s}\" href=\"{}\" />\n", url(id, s)));
            }
            head.push_str(&format!("<link rel=\"apple-touch-icon\" sizes=\"180x180\" href=\"{}\" />\n", url(id, 180)));
        }
        (None, Some(logo)) => head.push_str(&format!("<link rel=\"icon\" href=\"{0}\" />\n<link rel=\"apple-touch-icon\" href=\"{0}\" />\n", esc(logo))),
        (None, None) => head.push_str(&format!("<link rel=\"icon\" type=\"image/svg+xml\" href=\"{}\" />\n", letter_icon(brand, colour))),
    }
    if favicon.is_some() || logo.is_some() {
        head.push_str(&format!("<link rel=\"manifest\" href=\"{base}/site.webmanifest\" />\n<meta name=\"apple-mobile-web-app-title\" content=\"{}\" />\n", esc(brand)));
    }
    head
}

/// The address a browser or crawler tries by convention on a website's own domain: this business's icon or nothing, never S'Shop's.
pub async fn conventional(state: &AppState, s: &Site, size: u32) -> AppResult<axum::response::Response> {
    if let Some(id) = s.config.brand.favicon {
        if let Some(data) = png_of(state, id, size).await? {
            return Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=3600")], data).into_response());
        }
    }
    if let Some(logo) = s.config.brand.logo.map(super::site::media_url).or_else(|| s.has_logo.then(|| format!("/api/public/{}/logo", s.slug))) {
        return Ok(axum::response::Redirect::temporary(&logo).into_response());
    }
    Ok(axum::http::StatusCode::NOT_FOUND.into_response())
}

/// The website as an installable app: this business's name and icons. The S'Shop app keeps its own manifest.
pub fn manifest(s: &Site, brand: &str, base: &str) -> axum::response::Response {
    let start = if base.is_empty() { "/".to_string() } else { format!("{base}/") };
    let icons: Vec<Value> = match (s.config.brand.favicon, s.config.brand.logo.map(super::site::media_url).or_else(|| s.has_logo.then(|| format!("/api/public/{}/logo", s.slug)))) {
        (Some(id), _) => [192, 512].iter().map(|n| json!({ "src": url(id, *n), "sizes": format!("{n}x{n}"), "type": "image/png", "purpose": "any" })).collect(),
        (None, Some(logo)) => vec![json!({ "src": logo, "sizes": "any", "purpose": "any" })],
        (None, None) => Vec::new(),
    };
    let short: String = brand.chars().take(12).collect();
    let body = json!({
        "id": start, "name": brand, "short_name": short, "start_url": start, "scope": start, "display": "standalone",
        "background_color": s.config.theme.light.background, "theme_color": s.config.theme.light.primary, "icons": icons,
    });
    ([(header::CONTENT_TYPE, "application/manifest+json"), (header::CACHE_CONTROL, "no-cache")], body.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut d = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        d.extend_from_slice(b"IHDR");
        d.extend_from_slice(&w.to_be_bytes());
        d.extend_from_slice(&h.to_be_bytes());
        d.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        d.extend_from_slice(&[0, 0, 0, 0]);
        d.extend_from_slice(b"IEND");
        d.extend_from_slice(&[0, 0, 0, 0]);
        d
    }

    #[test]
    fn only_a_real_png_of_the_stated_size_passes() {
        assert_eq!(png_size(&png(32, 32)), Some((32, 32)));
        assert_eq!(png_size(&png(32, 16)), Some((32, 16)));
        assert_eq!(png_size(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>"), None);
        let mut cut = png(32, 32);
        cut.truncate(30);
        assert_eq!(png_size(&cut), None);
    }

    #[test]
    fn the_letter_icon_is_the_business_not_s_shop() {
        let icon = letter_icon("Sheryl's Niche", "#112233");
        let svg = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(icon.trim_start_matches("data:image/svg+xml;base64,")).unwrap()).unwrap();
        assert!(svg.contains(">S</text>") && svg.contains("#112233"));
        let odd = letter_icon("<script>", "red\" onload=\"x");
        let svg = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(odd.trim_start_matches("data:image/svg+xml;base64,")).unwrap()).unwrap();
        assert!(!svg.contains("onload") && !svg.contains("<script"), "{svg}");
    }

    #[test]
    fn the_tags_prefer_the_favicon_then_the_logo_then_the_letter() {
        let id = Uuid::nil();
        let t = head_tags(Some(id), Some("/logo"), "B", "#000000", "/s/b");
        assert!(t.contains("sizes=\"16x16\"") && t.contains("sizes=\"180x180\"") && !t.contains("/logo") && t.contains("/s/b/site.webmanifest"));
        assert!(head_tags(None, Some("/logo"), "B", "#000000", "").contains("href=\"/logo\""));
        let t = head_tags(None, None, "B", "#000000", "");
        assert!(t.contains("data:image/svg+xml") && !t.contains("manifest") && !t.contains("/favicon"));
    }
}
