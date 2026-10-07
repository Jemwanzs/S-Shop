mod audit;
mod auth;
mod billing;
mod bootstrap;
mod config;
mod demo;
mod error;
mod geo;
mod integrations;
mod inventory;
mod jobs;
mod legacy;
mod loyalty;
mod notify;
mod ratelimit;
mod perms;
mod routes;
mod settings;
mod state;
mod util;
mod workflow;

use std::net::SocketAddr;
use std::time::Duration;

use axum::http::{header, HeaderValue, Method};
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::{SetResponseHeader, SetResponseHeaderLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "S'Shop server

USAGE:
  sshop                                  Start the API + web server (default)
  sshop reset-pin <email> <new-pin>      Reset a user's PIN (recovery)
  sshop import-legacy <legacy-db-url> [--tenant <slug>]
                                         Import a Pablo Loyalty (Supabase) database
  sshop seed-demo [--reset]              Build the Pablo Niche demo business (demo-flagged; --reset rebuilds it)

ENVIRONMENT: see .env.example";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info")))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => serve().await,
        Some("reset-pin") => {
            let (Some(email), Some(pin)) = (args.get(1), args.get(2)) else { anyhow::bail!(USAGE) };
            let db = connect().await?;
            bootstrap::reset_pin(&db, email, pin).await
        }
        Some("import-legacy") => {
            let Some(source) = args.get(1) else { anyhow::bail!(USAGE) };
            let tenant = args.iter().position(|a| a == "--tenant").and_then(|i| args.get(i + 1)).cloned();
            let db = connect().await?;
            legacy::import(&db, source, tenant.as_deref()).await
        }
        Some("seed-demo") => {
            let db = connect().await?;
            let cfg = config::Config::from_env()?;
            let state = state::AppState::new(db, cfg);
            let report = demo::seed(&state, args.iter().any(|a| a == "--reset"), |step| println!("… {step}")).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Some(_) => {
            println!("{USAGE}");
            Ok(())
        }
    }
}

async fn connect() -> anyhow::Result<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    let db = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        // Recycle connections so none sits stale for long (a silently dropped socket can stall the health ping).
        .idle_timeout(Duration::from_secs(10 * 60))
        .max_lifetime(Duration::from_secs(30 * 60))
        .connect(&url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    Ok(db)
}

async fn healthz(axum::extract::State(state): axum::extract::State<state::AppState>) -> (axum::http::StatusCode, &'static str) {
    let ping = tokio::time::timeout(Duration::from_secs(3), sqlx::query("SELECT 1").execute(&state.db)).await;
    match ping {
        Ok(Ok(_)) => (axum::http::StatusCode::OK, "ok"),
        _ => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
    }
}

/// Browser hardening for every response. The CSP allows only this site's scripts (no inline script), Google Fonts,
/// and images from this site, data: and blob: (photo previews); camera and location only for this site.
fn security_headers(app: Router) -> Router {
    const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' data: https://fonts.gstatic.com; img-src 'self' data: blob:; connect-src 'self'; media-src 'self' blob:; worker-src 'self' blob:; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";
    let set = |name: &'static str, value: &'static str| {
        SetResponseHeaderLayer::if_not_present(header::HeaderName::from_static(name), HeaderValue::from_static(value))
    };
    app.layer(set("content-security-policy", CSP))
        .layer(set("strict-transport-security", "max-age=31536000; includeSubDomains"))
        .layer(set("x-content-type-options", "nosniff"))
        .layer(set("x-frame-options", "DENY"))
        .layer(set("referrer-policy", "strict-origin-when-cross-origin"))
        .layer(set("permissions-policy", "camera=(self), geolocation=(self), microphone=(), payment=()"))
}

async fn serve() -> anyhow::Result<()> {
    let cfg = config::Config::from_env()?;
    let db = connect().await?;
    bootstrap::ensure(&db, &cfg).await?;

    let port = cfg.port;
    let web_dir = cfg.web_dir.clone();
    let cors_origins = cfg.cors_origins.clone();
    tracing::info!(
        mpesa = cfg.mpesa.is_some(),
        whatsapp = cfg.whatsapp.is_some(),
        email = cfg.email.is_some(),
        platform_admins = cfg.platform_admins.len(),
        public_url = %cfg.public_url,
        "S'Shop starting"
    );

    let state = state::AppState::new(db, cfg);
    jobs::spawn(state.clone());

    // Single-page app: unknown paths fall back to index.html; hashed assets are cached forever.
    // index.html must be revalidated so browsers pick up a new release instead of asking for old chunks.
    let index = format!("{web_dir}/index.html");
    let spa = SetResponseHeader::overriding(
        ServeDir::new(&web_dir).fallback(ServeFile::new(&index)),
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    let assets = Router::new()
        .nest_service("/assets", ServeDir::new(format!("{web_dir}/assets")))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));

    let mut app = Router::new()
        .nest("/api", routes::api())
        // Healthy only when the database answers (Railway restarts the service otherwise).
        .route("/healthz", axum::routing::get(healthz))
        .merge(assets)
        .fallback_service(spa)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());
    app = security_headers(app);

    if !cors_origins.is_empty() {
        let origins: Vec<HeaderValue> = cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
        app = app.layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, "x-branch-id".parse().unwrap()]),
        );
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
