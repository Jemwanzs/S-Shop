//! A business's own articles on its website (roadmap 158): written and reviewed by its people, published when they say so.
//!
//! Every query is limited to one business: its staff through their session, the public through the business the website
//! resolved to. Text is stored in a small safe format (headings, paragraphs, lists, bold, links) and turned into escaped HTML
//! here, so no article can carry a script. Publishing needs `website.publish`; `website.seo` alone can write and ask for review.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::site::{self, Resolved, Site};
use super::site_seo::esc;
use crate::audit::{self, Entry};
use crate::auth::Ctx;
use crate::error::{bad, refused, AppError, AppResult};
use crate::notify::{self, Note};
use crate::state::AppState;
use crate::util::slugify;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/website/articles", get(list).post(create))
        .route("/website/articles/{id}", get(one).put(update).delete(remove))
        .route("/website/articles/{id}/status", post(set_status))
        .route("/site/articles", get(public_list))
        .route("/site/articles/{slug}", get(public_one))
        .route("/site/articles/{slug}/view", post(public_view))
}

/// Live to the public: published, or scheduled and due. Reading it this way publishes on time even between job runs.
pub const LIVE: &str = "(a.status = 'published' OR (a.status = 'scheduled' AND a.scheduled_at <= now()))";
/// Above this, publishing asks the person to look at the article it resembles.
const SIMILAR: f64 = 0.6;

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct ArticleRow {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub seo_title: String,
    pub description: String,
    pub body: String,
    pub cover_media: Option<Uuid>,
    pub cover_alt: String,
    pub category: String,
    pub author_name: String,
    pub product_ids: Vec<Uuid>,
    pub status: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub ai_assisted: bool,
    pub reviewed_by: String,
    pub views: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const COLUMNS: &str = "a.id, a.slug, a.title, a.seo_title, a.description, a.body, a.cover_media, a.cover_alt, a.category, a.author_name, a.product_ids, a.status, a.scheduled_at,
    COALESCE(a.published_at, CASE WHEN a.status = 'scheduled' AND a.scheduled_at <= now() THEN a.scheduled_at END) AS published_at, a.ai_assisted, a.reviewed_by, a.views, a.created_at, a.updated_at";

// ── Text → safe HTML ────────────────────────────────────────────────────────────────────────────────────────────────

/// `**bold**` and `[text](link)` on already-escaped text. A link must be https, or a path on this website.
fn inline(line: &str, root: &str) -> String {
    let mut out = String::new();
    let mut rest = esc(line);
    while let Some(start) = rest.find('[') {
        let parsed = rest[start..].find("](").and_then(|mid| rest[start + mid..].find(')').map(|end| (mid, end)));
        let Some((mid, end)) = parsed else { break };
        let (text, href) = (&rest[start + 1..start + mid], &rest[start + mid + 2..start + mid + end]);
        out.push_str(&rest[..start]);
        if href.starts_with("https://") {
            out.push_str(&format!("<a href=\"{href}\" rel=\"noopener\">{text}</a>"));
        } else if href.starts_with('/') && !href.starts_with("//") {
            out.push_str(&format!("<a href=\"{root}{href}\">{text}</a>"));
        } else {
            out.push_str(text);
        }
        rest = rest[start + mid + end + 1..].to_string();
    }
    out.push_str(&rest);
    let mut bold = String::new();
    for (i, part) in out.split("**").enumerate() {
        if i % 2 == 1 && out.matches("**").count() % 2 == 0 {
            bold.push_str(&format!("<strong>{part}</strong>"));
        } else {
            if i % 2 == 1 {
                bold.push_str("**");
            }
            bold.push_str(part);
        }
    }
    bold
}

/// The article's text as HTML: `## ` and `### ` headings (the page's own title is the only h1), `- ` and `1. ` lists,
/// paragraphs. Everything is escaped first.
pub fn render_body(body: &str, root: &str) -> String {
    let mut html = String::new();
    let mut list: Option<&str> = None;
    let close = |html: &mut String, list: &mut Option<&str>| {
        if let Some(tag) = list.take() {
            html.push_str(&format!("</{tag}>"));
        }
    };
    let mut paragraph: Vec<&str> = Vec::new();
    let flush = |html: &mut String, p: &mut Vec<&str>| {
        if !p.is_empty() {
            html.push_str(&format!("<p>{}</p>", inline(&p.join(" "), root)));
            p.clear();
        }
    };
    for raw in body.lines() {
        let line = raw.trim();
        let numbered = line.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).map(|(_, t)| t);
        if let Some(t) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            flush(&mut html, &mut paragraph);
            if list != Some("ul") {
                close(&mut html, &mut list);
                html.push_str("<ul>");
                list = Some("ul");
            }
            html.push_str(&format!("<li>{}</li>", inline(t, root)));
        } else if let Some(t) = numbered {
            flush(&mut html, &mut paragraph);
            if list != Some("ol") {
                close(&mut html, &mut list);
                html.push_str("<ol>");
                list = Some("ol");
            }
            html.push_str(&format!("<li>{}</li>", inline(t, root)));
        } else if let Some(t) = line.strip_prefix("### ") {
            flush(&mut html, &mut paragraph);
            close(&mut html, &mut list);
            html.push_str(&format!("<h3>{}</h3>", inline(t, root)));
        } else if let Some(t) = line.strip_prefix("## ").or_else(|| line.strip_prefix("# ")) {
            flush(&mut html, &mut paragraph);
            close(&mut html, &mut list);
            html.push_str(&format!("<h2>{}</h2>", inline(t, root)));
        } else if line.is_empty() {
            flush(&mut html, &mut paragraph);
            close(&mut html, &mut list);
        } else {
            close(&mut html, &mut list);
            paragraph.push(line);
        }
    }
    flush(&mut html, &mut paragraph);
    close(&mut html, &mut list);
    html
}

/// How much two texts overlap, 0–1 (shared runs of three words). Used to catch repetition before anything is published.
pub fn similarity(a: &str, b: &str) -> f64 {
    let runs = |s: &str| -> std::collections::HashSet<String> {
        let words: Vec<String> = s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
        words.windows(3).map(|w| w.join(" ")).collect()
    };
    let (x, y) = (runs(a), runs(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    x.intersection(&y).count() as f64 / x.union(&y).count() as f64
}

// ── Staff ───────────────────────────────────────────────────────────────────────────────────────────────────────────

fn may_read(ctx: &Ctx) -> AppResult<()> {
    ctx.require_any(&["website.seo", "website.publish", "website.view"])
}

/// Articles belong to a website the platform has switched on for this business.
async fn require_website(conn: &mut PgConnection, tenant: Uuid) -> AppResult<()> {
    let on: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM websites WHERE tenant_id = $1 AND status = 'active')").bind(tenant).fetch_one(&mut *conn).await?;
    if on { Ok(()) } else { Err(refused("Website not active", "Articles need the website service to be active")) }
}

async fn fetch(conn: &mut PgConnection, tenant: Uuid, id: Uuid) -> AppResult<ArticleRow> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM website_articles a WHERE a.id = $1 AND a.tenant_id = $2"))
        .bind(id)
        .bind(tenant)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AppError::NotFound("Article"))
}

#[derive(Deserialize, Default)]
struct ListQuery {
    status: Option<String>,
}

async fn list(State(state): State<AppState>, ctx: Ctx, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    may_read(&ctx)?;
    let rows: Vec<ArticleRow> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM website_articles a WHERE a.tenant_id = $1 AND ($2::text IS NULL OR a.status = $2) ORDER BY COALESCE(a.published_at, a.scheduled_at, a.updated_at) DESC LIMIT 200"
    ))
    .bind(ctx.tenant_id)
    .bind(&q.status)
    .fetch_all(&state.db)
    .await?;
    let counts: Vec<(String, i64)> = sqlx::query_as("SELECT status, COUNT(*) FROM website_articles WHERE tenant_id = $1 GROUP BY status").bind(ctx.tenant_id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "items": rows, "counts": counts.into_iter().collect::<std::collections::HashMap<_, _>>() })))
}

async fn one(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<ArticleRow>> {
    may_read(&ctx)?;
    let mut conn = state.db.acquire().await?;
    Ok(Json(fetch(&mut conn, ctx.tenant_id, id).await?))
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ArticleBody {
    pub slug: String,
    pub title: String,
    pub seo_title: String,
    pub description: String,
    pub body: String,
    pub cover_media: Option<Uuid>,
    pub cover_alt: String,
    pub category: String,
    pub author_name: String,
    pub product_ids: Vec<Uuid>,
    /// `updated_at` the editor loaded: a colleague's newer save is never overwritten.
    pub base_updated_at: Option<DateTime<Utc>>,
}

/// Checks and normalises what was typed. Media and products must be this business's own; others are refused or left out.
pub async fn clean(conn: &mut PgConnection, tenant: Uuid, b: &mut ArticleBody) -> AppResult<()> {
    b.title = b.title.trim().to_string();
    if b.title.is_empty() {
        return Err(bad("Give the article a title"));
    }
    for (v, n, what) in [(&b.title, 120, "Title"), (&b.seo_title, 70, "Search title"), (&b.description, 170, "Description"), (&b.cover_alt, 200, "Cover image description"), (&b.category, 80, "Category"), (&b.author_name, 120, "Author"), (&b.body, 60_000, "Article text")] {
        if v.chars().count() > n {
            return Err(bad(format!("{what}: up to {n} characters")));
        }
    }
    b.slug = slugify(if b.slug.trim().is_empty() { &b.title } else { &b.slug }).chars().take(90).collect::<String>().trim_end_matches('-').to_string();
    if b.slug.is_empty() {
        return Err(bad("The address needs letters or numbers"));
    }
    if let Some(m) = b.cover_media {
        let own: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_media WHERE id = $1 AND tenant_id = $2)").bind(m).bind(tenant).fetch_one(&mut *conn).await?;
        if !own {
            return Err(bad("That cover image is not in your media"));
        }
    }
    b.product_ids.truncate(20);
    b.product_ids = sqlx::query_scalar("SELECT id FROM products WHERE tenant_id = $1 AND id = ANY($2)").bind(tenant).bind(&b.product_ids).fetch_all(&mut *conn).await?;
    Ok(())
}

async fn slug_free(conn: &mut PgConnection, tenant: Uuid, slug: &str, except: Option<Uuid>) -> AppResult<()> {
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM website_articles WHERE tenant_id = $1 AND (slug = $2 OR $2 = ANY(previous_slugs)) AND id IS DISTINCT FROM $3)")
        .bind(tenant)
        .bind(slug)
        .bind(except)
        .fetch_one(&mut *conn)
        .await?;
    if taken { Err(bad("Another article already uses this address")) } else { Ok(()) }
}

async fn create(State(state): State<AppState>, ctx: Ctx, Json(mut b): Json<ArticleBody>) -> AppResult<Json<ArticleRow>> {
    ctx.require("website.seo")?;
    let mut tx = state.db.begin().await?;
    require_website(&mut tx, ctx.tenant_id).await?;
    clean(&mut tx, ctx.tenant_id, &mut b).await?;
    slug_free(&mut tx, ctx.tenant_id, &b.slug, None).await?;
    let id = insert(&mut tx, ctx.tenant_id, Some(ctx.user_id), &b, false, "draft").await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "article_created", "article", id).after(json!({ "title": b.title }))).await?;
    let row = fetch(&mut tx, ctx.tenant_id, id).await?;
    tx.commit().await?;
    Ok(Json(row))
}

pub async fn insert(conn: &mut PgConnection, tenant: Uuid, by: Option<Uuid>, b: &ArticleBody, ai: bool, status: &str) -> AppResult<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO website_articles (tenant_id, slug, title, seo_title, description, body, cover_media, cover_alt, category, author_name, product_ids, created_by, ai_assisted, status)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING id",
    )
    .bind(tenant)
    .bind(&b.slug)
    .bind(&b.title)
    .bind(&b.seo_title)
    .bind(&b.description)
    .bind(&b.body)
    .bind(b.cover_media)
    .bind(&b.cover_alt)
    .bind(&b.category)
    .bind(&b.author_name)
    .bind(&b.product_ids)
    .bind(by)
    .bind(ai)
    .bind(status)
    .fetch_one(&mut *conn)
    .await?)
}

async fn update(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(mut b): Json<ArticleBody>) -> AppResult<Json<ArticleRow>> {
    ctx.require("website.seo")?;
    let mut tx = state.db.begin().await?;
    let before = fetch(&mut tx, ctx.tenant_id, id).await?;
    if b.base_updated_at.is_some_and(|at| at != before.updated_at) {
        return Err(refused("Changed by someone else", "Someone saved this article after you opened it — open it again so their changes are not lost"));
    }
    // A live article is changed only by someone who may publish.
    if matches!(before.status.as_str(), "published" | "scheduled") {
        ctx.require("website.publish")?;
    }
    clean(&mut tx, ctx.tenant_id, &mut b).await?;
    slug_free(&mut tx, ctx.tenant_id, &b.slug, Some(id)).await?;
    // A published address keeps working: it forwards to the new one.
    let keep_old = b.slug != before.slug && before.published_at.is_some();
    sqlx::query(
        "UPDATE website_articles SET slug=$3, title=$4, seo_title=$5, description=$6, body=$7, cover_media=$8, cover_alt=$9, category=$10, author_name=$11, product_ids=$12,
                previous_slugs = CASE WHEN $13 THEN array_append(previous_slugs, $14) ELSE previous_slugs END, updated_at = now()
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(&b.slug)
    .bind(&b.title)
    .bind(&b.seo_title)
    .bind(&b.description)
    .bind(&b.body)
    .bind(b.cover_media)
    .bind(&b.cover_alt)
    .bind(&b.category)
    .bind(&b.author_name)
    .bind(&b.product_ids)
    .bind(keep_old)
    .bind(&before.slug)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "article_edited", "article", id).after(json!({ "title": b.title, "status": before.status }))).await?;
    let row = fetch(&mut tx, ctx.tenant_id, id).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn remove(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    ctx.require("website.seo")?;
    let mut tx = state.db.begin().await?;
    let a = fetch(&mut tx, ctx.tenant_id, id).await?;
    if a.published_at.is_some() {
        return Err(refused("Archive it instead", "This article has been public: archive it so its history is kept"));
    }
    if a.status != "draft" && a.status != "pending_review" {
        return Err(refused("Archive it instead", "Only unpublished drafts can be deleted"));
    }
    sqlx::query("DELETE FROM website_articles WHERE id = $1 AND tenant_id = $2").bind(id).bind(ctx.tenant_id).execute(&mut *tx).await?;
    audit::record(&mut tx, &ctx, Entry::new("website", "article_deleted", "article", id).before(json!({ "title": a.title }))).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct StatusBody {
    /// submit | draft | publish | schedule | archive
    action: String,
    scheduled_at: Option<DateTime<Utc>>,
    /// The person looked at the similar article and still wants to publish this one.
    #[serde(default)]
    confirm_similar: bool,
}

/// What an article needs before the public may read it.
fn ready(a: &ArticleRow) -> AppResult<()> {
    if a.description.trim().is_empty() {
        return Err(refused("Not ready to publish", "Add a description: it is what search results and shared links show"));
    }
    if a.body.trim().chars().count() < 200 {
        return Err(refused("Not ready to publish", "The article is too short to be useful: write at least a couple of paragraphs"));
    }
    Ok(())
}

/// The most similar other article of this business that is live or waiting, if any is close.
pub async fn too_similar(conn: &mut PgConnection, tenant: Uuid, id: Option<Uuid>, body: &str) -> AppResult<Option<(String, f64)>> {
    let others: Vec<(String, String)> = sqlx::query_as("SELECT title, body FROM website_articles WHERE tenant_id = $1 AND id IS DISTINCT FROM $2 AND status <> 'archived'").bind(tenant).bind(id).fetch_all(&mut *conn).await?;
    Ok(others.into_iter().map(|(t, b)| { let s = similarity(body, &b); (t, s) }).filter(|x| x.1 >= SIMILAR).max_by(|a, b| a.1.total_cmp(&b.1)))
}

async fn set_status(State(state): State<AppState>, ctx: Ctx, Path(id): Path<Uuid>, Json(b): Json<StatusBody>) -> AppResult<Json<ArticleRow>> {
    let mut tx = state.db.begin().await?;
    let a = fetch(&mut tx, ctx.tenant_id, id).await?;
    let (status, at): (&str, Option<DateTime<Utc>>) = match b.action.as_str() {
        "submit" => {
            ctx.require("website.seo")?;
            ready(&a)?;
            ("pending_review", None)
        }
        "draft" => {
            ctx.require_any(&["website.seo", "website.publish"])?;
            if matches!(a.status.as_str(), "published" | "scheduled") {
                ctx.require("website.publish")?;
            }
            ("draft", None)
        }
        "archive" => {
            ctx.require("website.publish")?;
            ("archived", None)
        }
        "publish" | "schedule" => {
            ctx.require("website.publish")?;
            ready(&a)?;
            if !b.confirm_similar {
                if let Some((title, _)) = too_similar(&mut tx, ctx.tenant_id, Some(id), &a.body).await? {
                    return Err(refused("Very similar article", format!("This reads very like “{title}”. Repeating yourself does not help readers: change it, or confirm you want to publish it anyway")));
                }
            }
            if b.action == "schedule" {
                let when = b.scheduled_at.ok_or_else(|| bad("Choose when to publish"))?;
                if when <= Utc::now() {
                    return Err(bad("Choose a time in the future"));
                }
                ("scheduled", Some(when))
            } else {
                ("published", None)
            }
        }
        _ => return Err(bad("Unknown action")),
    };
    sqlx::query(
        "UPDATE website_articles SET status = $3, scheduled_at = $4, updated_at = now(),
                published_at = CASE WHEN $3 = 'published' THEN COALESCE(published_at, now()) WHEN $3 = 'scheduled' THEN COALESCE(published_at, $4) ELSE published_at END,
                reviewed_by = CASE WHEN $3 IN ('published', 'scheduled') THEN $5 ELSE reviewed_by END
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(status)
    .bind(at)
    .bind(&ctx.name)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &ctx, Entry::new("website", &format!("article_{}", b.action), "article", id).before(json!({ "status": a.status })).after(json!({ "status": status, "scheduled_at": at, "title": a.title }))).await?;
    let row = fetch(&mut tx, ctx.tenant_id, id).await?;
    tx.commit().await?;
    if status == "pending_review" {
        notify::to_permission(&state, ctx.tenant_id, None, "website.publish", Note::new("article_review", format!("Article ready for review: {}", a.title), format!("From {}", ctx.name), "/settings/website?tab=seo")).await;
    }
    Ok(Json(row))
}

/// Scheduled articles whose time has come become published (the public already sees them; this records it). Every 15 minutes.
pub async fn publish_due(state: &AppState) -> anyhow::Result<()> {
    sqlx::query("UPDATE website_articles SET status = 'published', updated_at = now(), published_at = COALESCE(published_at, scheduled_at) WHERE status = 'scheduled' AND scheduled_at <= now()").execute(&state.db).await?;
    Ok(())
}

// ── Public ──────────────────────────────────────────────────────────────────────────────────────────────────────────

/// One business's live articles, newest first.
pub async fn live_articles(state: &AppState, tenant: Uuid, limit: i64, offset: i64) -> AppResult<(Vec<ArticleRow>, i64)> {
    let rows: Vec<ArticleRow> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM website_articles a WHERE a.tenant_id = $1 AND {LIVE} ORDER BY COALESCE(a.published_at, a.scheduled_at) DESC LIMIT $2 OFFSET $3"))
        .bind(tenant)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM website_articles a WHERE a.tenant_id = $1 AND {LIVE}")).bind(tenant).fetch_one(&state.db).await?;
    Ok((rows, total))
}

/// A live article by its address or an earlier one. Staff drafts are never reachable here.
pub async fn live_article(state: &AppState, tenant: Uuid, slug: &str) -> AppResult<Option<ArticleRow>> {
    Ok(sqlx::query_as(&format!("SELECT {COLUMNS} FROM website_articles a WHERE a.tenant_id = $1 AND {LIVE} AND (a.slug = $2 OR $2 = ANY(a.previous_slugs)) LIMIT 1"))
        .bind(tenant)
        .bind(slug)
        .fetch_optional(&state.db)
        .await?)
}

/// What the public may read of an article (no status, staff names beyond the byline, or counters).
pub fn public_json(a: &ArticleRow, root: &str) -> Value {
    json!({
        "slug": a.slug, "title": a.title, "description": a.description, "cover": a.cover_media.map(site::media_url), "cover_alt": a.cover_alt, "category": a.category,
        "author": a.author_name, "published_at": a.published_at, "updated_at": a.updated_at, "ai_assisted": a.ai_assisted, "reviewed_by": a.reviewed_by,
        "html": render_body(&a.body, root),
    })
}

#[derive(Deserialize, Default)]
struct PublicQuery {
    slug: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

async fn public_site(state: &AppState, headers: &HeaderMap, slug: Option<&str>) -> AppResult<Site> {
    match site::resolve(state, headers, slug, false).await? {
        Resolved::Live(s) => Ok(*s),
        Resolved::Unavailable { .. } => Err(AppError::NotFound("Article")),
    }
}

async fn public_list(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<PublicQuery>) -> AppResult<Json<Value>> {
    let s = public_site(&state, &headers, q.slug.as_deref()).await?;
    let (rows, total) = live_articles(&state, s.tenant, q.limit.unwrap_or(12).clamp(1, 50), q.offset.unwrap_or(0).max(0)).await?;
    let root = site::site_root(&state, &s);
    Ok(Json(json!({ "total": total, "items": rows.iter().map(|a| { let mut v = public_json(a, &root); v.as_object_mut().map(|o| o.remove("html")); v }).collect::<Vec<_>>() })))
}

async fn public_one(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Query(q): Query<PublicQuery>) -> AppResult<Json<Value>> {
    let s = public_site(&state, &headers, q.slug.as_deref()).await?;
    let a = live_article(&state, s.tenant, &slug).await?.ok_or(AppError::NotFound("Article"))?;
    let root = site::site_root(&state, &s);
    let items = site::published(&state, &s).await?;
    let products: Vec<&site::PubProduct> = a.product_ids.iter().filter_map(|id| items.iter().find(|p| p.id == *id)).collect();
    let (recent, _) = live_articles(&state, s.tenant, 4, 0).await?;
    let mut related: Vec<&ArticleRow> = recent.iter().filter(|x| x.id != a.id).collect();
    related.sort_by_key(|x| x.category != a.category);
    related.truncate(3);
    Ok(Json(json!({
        "article": public_json(&a, &root),
        "products": products,
        "related": related.iter().map(|x| json!({ "slug": x.slug, "title": x.title, "description": x.description })).collect::<Vec<_>>(),
    })))
}

async fn public_view(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Query(q): Query<PublicQuery>) -> AppResult<Json<Value>> {
    state.limits.check(&crate::auth::client_meta(&headers).0, "site_event", 600, std::time::Duration::from_secs(600))?;
    let s = public_site(&state, &headers, q.slug.as_deref()).await?;
    sqlx::query(&format!("UPDATE website_articles a SET views = views + 1 WHERE a.tenant_id = $1 AND a.slug = $2 AND {LIVE}")).bind(s.tenant).bind(&slug).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_text_can_never_carry_html_or_scripts() {
        let html = render_body("Hello <script>alert(1)</script>\n\n[bad](javascript:alert(1)) [ok](https://x.example/a) [own](/products/a)", "https://shop.example");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(html.contains("<a href=\"https://x.example/a\""));
        assert!(html.contains("<a href=\"https://shop.example/products/a\">own</a>"));
        assert!(!render_body("[x](//evil.example)", "https://s").contains("href"));
        assert!(!render_body("[x](https://a\" onclick=\"b)", "https://s").contains("onclick=\""));
    }

    #[test]
    fn article_structure() {
        let html = render_body("# Top\n## Sub\n### Small\nOne\nline\n\n- a\n- **b**\n\n1. first\n2. second", "");
        assert_eq!(html, "<h2>Top</h2><h2>Sub</h2><h3>Small</h3><p>One line</p><ul><li>a</li><li><strong>b</strong></li></ul><ol><li>first</li><li>second</li></ol>");
        assert_eq!(render_body("an ** odd star", ""), "<p>an ** odd star</p>");
    }

    #[test]
    fn similarity_tells_repeats_from_different_text() {
        let a = "How to choose the right size for your home and keep it working for many years without trouble";
        assert!(similarity(a, a) > 0.99);
        assert!(similarity(a, "Opening hours over the holidays and how to reach our team when the shop is closed for stock taking") < 0.1);
        assert_eq!(similarity("", a), 0.0);
    }
}
