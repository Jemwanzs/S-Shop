mod audit;
mod auth;
mod bootstrap;
mod config;
mod error;
mod integrations;
mod inventory;
mod jobs;
mod legacy;
mod loyalty;
mod notify;
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
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "S'Shop server

USAGE:
  sshop                                  Start the API + web server (default)
  sshop reset-pin <email> <new-pin>      Reset a user's PIN (recovery)
  sshop import-legacy <legacy-db-url> [--tenant <slug>]
                                         Import a Pablo Loyalty (Supabase) database

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
        .connect(&url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    Ok(db)
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
        public_url = %cfg.public_url,
        "S'Shop starting"
    );

    let state = state::AppState::new(db, cfg);
    jobs::spawn(state.clone());

    // Single-page app: unknown paths fall back to index.html; hashed assets are cached forever.
    let index = format!("{web_dir}/index.html");
    let spa = ServeDir::new(&web_dir).fallback(ServeFile::new(&index));
    let assets = Router::new()
        .nest_service("/assets", ServeDir::new(format!("{web_dir}/assets")))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));

    let mut app = Router::new()
        .nest("/api", routes::api())
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .merge(assets)
        .fallback_service(spa)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

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
