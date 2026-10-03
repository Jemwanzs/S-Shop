use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::PgPool;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::config::Config;

/// Real-time event pushed to connected staff clients over SSE.
#[derive(Clone, Debug, Serialize)]
pub struct LiveEvent {
    #[serde(skip)]
    pub tenant_id: Uuid,
    /// None = broadcast to every connected user of the tenant.
    #[serde(skip)]
    pub user_id: Option<Uuid>,
    /// "notification" | "order" | "stock" | "mpesa" | "approval"
    pub topic: &'static str,
    pub data: serde_json::Value,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
    pub events: broadcast::Sender<LiveEvent>,
    pub mpesa_token: Arc<Mutex<Option<(String, Instant)>>>,
}

impl AppState {
    pub fn new(db: PgPool, cfg: Config) -> Self {
        let (events, _) = broadcast::channel(512);
        Self {
            db,
            cfg: Arc::new(cfg),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("S'Shop/1.0")
                .build()
                .expect("http client"),
            events,
            mpesa_token: Arc::new(Mutex::new(None)),
        }
    }

    pub fn emit(&self, tenant_id: Uuid, user_id: Option<Uuid>, topic: &'static str, data: serde_json::Value) {
        // No subscribers is fine — nobody is connected.
        let _ = self.events.send(LiveEvent { tenant_id, user_id, topic, data });
    }
}
