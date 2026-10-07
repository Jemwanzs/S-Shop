//! Duplicate-submission guard (roadmap 47) for every write request: a double click, a double tap or a client retry
//! of the *same* request must not run twice. Disabling buttons in the browser is not enough on its own.
//!
//! A request is identified by who sent it (the session token, else the client address), the method, the path with
//! its query, the branch header and the exact body. While one is being processed an identical one gets 409
//! *Already processing*; for a few seconds after it succeeded an identical one gets the same response again instead
//! of running twice. Failures are not remembered, so trying again after an error works as normal. Sign-in, webhooks
//! (their senders retry deliberately and are idempotent already) and the live event stream are left alone.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::{to_bytes, Body, Bytes};
use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use sha2::{Digest, Sha256};

/// How long a completed request is answered from memory.
const REPLAY: Duration = Duration::from_secs(3);
/// A request still marked in flight after this long is assumed lost (its client gave up).
const IN_FLIGHT_MAX: Duration = Duration::from_secs(120);
/// Larger bodies (photo uploads are ≤ 4 MB) are read in full to fingerprint them.
const MAX_BODY: usize = 8 * 1024 * 1024;
/// Responses up to this size are kept for replay.
const MAX_REPLAY_BODY: usize = 1024 * 1024;

enum Slot {
    InFlight(Instant),
    Done { at: Instant, status: StatusCode, content_type: Option<HeaderValue>, body: Bytes },
}

#[derive(Default)]
pub struct Dedupe {
    slots: Mutex<HashMap<[u8; 32], Slot>>,
}

enum Seen {
    New,
    Busy,
    Replay(StatusCode, Option<HeaderValue>, Bytes),
}

impl Dedupe {
    fn claim(&self, key: [u8; 32]) -> Seen {
        let Ok(mut slots) = self.slots.lock() else { return Seen::New };
        let now = Instant::now();
        // Forget anything old (cheap: the map only holds the last few seconds of writes).
        slots.retain(|_, s| match s {
            Slot::InFlight(at) => now.duration_since(*at) < IN_FLIGHT_MAX,
            Slot::Done { at, .. } => now.duration_since(*at) < REPLAY,
        });
        match slots.get(&key) {
            Some(Slot::InFlight(_)) => Seen::Busy,
            Some(Slot::Done { status, content_type, body, .. }) => Seen::Replay(*status, content_type.clone(), body.clone()),
            None => {
                slots.insert(key, Slot::InFlight(now));
                Seen::New
            }
        }
    }

    fn finish(&self, key: [u8; 32], done: Option<(StatusCode, Option<HeaderValue>, Bytes)>) {
        let Ok(mut slots) = self.slots.lock() else { return };
        match done {
            Some((status, content_type, body)) => {
                slots.insert(key, Slot::Done { at: Instant::now(), status, content_type, body });
            }
            None => {
                slots.remove(&key);
            }
        }
    }
}

fn exempt(path: &str) -> bool {
    let p = path.strip_prefix("/api").unwrap_or(path);
    p.starts_with("/auth/login") || p.starts_with("/webhooks/") || p.starts_with("/events")
}

/// Writes that are really status checks (is it paid yet? is this barcode free?): an identical request while one is
/// running is still refused, but a later one always runs — its answer can change, so it is never replayed.
fn never_replay(path: &str) -> bool {
    let p = path.strip_prefix("/api").unwrap_or(path);
    p.ends_with("/verify") || p.ends_with("/identify") || p.ends_with("/check-barcode")
}

pub async fn guard(axum::extract::State(state): axum::extract::State<crate::state::AppState>, req: Request, next: Next) -> Response {
    if !matches!(*req.method(), Method::POST | Method::PUT | Method::PATCH | Method::DELETE) || exempt(req.uri().path()) {
        return next.run(req).await;
    }
    let (parts, body) = req.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_BODY).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({ "error": { "code": "too_large", "message": "The upload is too large", "title": null } }))).into_response();
    };
    let who = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| crate::auth::client_meta(&parts.headers).0);
    let mut h = Sha256::new();
    for piece in [
        who.as_bytes(),
        parts.method.as_str().as_bytes(),
        parts.uri.path_and_query().map_or("", |p| p.as_str()).as_bytes(),
        parts.headers.get("x-branch-id").map_or(&b""[..], |v| v.as_bytes()),
    ] {
        h.update((piece.len() as u64).to_le_bytes());
        h.update(piece);
    }
    h.update(&bytes);
    let key: [u8; 32] = h.finalize().into();

    match state.dedupe.claim(key) {
        Seen::Busy => {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": { "code": "duplicate", "title": "Already processing", "message": "This action is already being processed — please wait" } })),
            )
                .into_response();
        }
        Seen::Replay(status, content_type, body) => {
            let mut res = Response::new(Body::from(body));
            *res.status_mut() = status;
            if let Some(ct) = content_type {
                res.headers_mut().insert(header::CONTENT_TYPE, ct);
            }
            res.headers_mut().insert("x-duplicate", HeaderValue::from_static("replayed"));
            return res;
        }
        Seen::New => {}
    }

    let replayable = !never_replay(parts.uri.path());
    let res = next.run(Request::from_parts(parts, Body::from(bytes))).await;
    if !res.status().is_success() || !replayable {
        state.dedupe.finish(key, None);
        return res;
    }
    let (parts, body) = res.into_parts();
    match to_bytes(body, MAX_REPLAY_BODY).await {
        Ok(b) => {
            state.dedupe.finish(key, Some((parts.status, parts.headers.get(header::CONTENT_TYPE).cloned(), b.clone())));
            Response::from_parts(parts, Body::from(b))
        }
        Err(_) => {
            // Too large to keep (never the case for JSON writes): stop guarding it rather than fail the response.
            state.dedupe.finish(key, None);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": { "code": "internal", "message": "Something went wrong. Please try again.", "title": null } }))).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_busy_then_replay_then_forget_failures() {
        let d = Dedupe::default();
        let k = [7u8; 32];
        assert!(matches!(d.claim(k), Seen::New));
        assert!(matches!(d.claim(k), Seen::Busy));
        d.finish(k, Some((StatusCode::OK, None, Bytes::from_static(b"{}"))));
        assert!(matches!(d.claim(k), Seen::Replay(StatusCode::OK, _, _)));
        let f = [8u8; 32];
        assert!(matches!(d.claim(f), Seen::New));
        d.finish(f, None);
        assert!(matches!(d.claim(f), Seen::New));
        assert!(exempt("/api/auth/login") && exempt("/webhooks/paystack") && !exempt("/api/sales"));
        assert!(never_replay("/billing/paystack/verify") && never_replay("/sales/check-barcode") && !never_replay("/sales"));
    }
}
