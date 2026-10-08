//! Authentication (JWT) and the per-request staff context.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use chrono::Utc;
use chrono_tz::Tz;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::util::parse_tz;

pub const STAFF_TOKEN_HOURS: i64 = 12;
pub const PORTAL_TOKEN_DAYS: i64 = 30;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub tid: Uuid,
    /// "staff" | "portal"
    pub typ: String,
    /// Set when a platform admin has opened another business: the admin's own (home) business.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Uuid>,
    pub exp: i64,
    pub iat: i64,
}

pub fn hash_pin(pin: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Other(anyhow::anyhow!("hash failure: {e}")))
}

pub fn verify_pin(pin: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(pin.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

pub fn validate_pin(pin: &str) -> AppResult<()> {
    if pin.len() < 4 || pin.len() > 12 {
        return Err(crate::error::bad("PIN must be 4–12 characters"));
    }
    Ok(())
}

pub fn issue_token(secret: &str, sub: Uuid, tid: Uuid, typ: &str, ttl: chrono::Duration) -> AppResult<String> {
    let now = Utc::now();
    let claims = Claims { sub, tid, typ: typ.into(), home: None, iat: now.timestamp(), exp: (now + ttl).timestamp() };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::Other(anyhow::anyhow!("token: {e}")))
}

/// Staff token for a platform admin working inside another business (`tid`) on behalf of the platform.
pub fn issue_acting_token(secret: &str, sub: Uuid, home: Uuid, tid: Uuid) -> AppResult<String> {
    let now = Utc::now();
    let claims = Claims {
        sub,
        tid,
        typ: "staff".into(),
        home: Some(home),
        iat: now.timestamp(),
        exp: (now + chrono::Duration::hours(STAFF_TOKEN_HOURS)).timestamp(),
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::Other(anyhow::anyhow!("token: {e}")))
}

pub fn read_token(secret: &str, token: &str, typ: &str) -> AppResult<Claims> {
    let data = decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default())
        .map_err(|_| AppError::Unauthorized)?;
    if data.claims.typ != typ {
        return Err(AppError::Unauthorized);
    }
    Ok(data.claims)
}

/// Bearer token from the Authorization header, or `access_token` query
/// parameter (EventSource and direct downloads cannot set headers).
pub fn bearer(parts: &Parts) -> Option<String> {
    if let Some(v) = parts.headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    parts.uri.query().and_then(|q| {
        q.split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "access_token")
            .map(|(_, v)| v.to_string())
    })
}

pub fn client_meta(headers: &HeaderMap) -> (String, String) {
    // The last X-Forwarded-For entry is the one added by our proxy (Railway); earlier entries come from the client
    // and could be forged, so they are never trusted for the audit trail or rate limits.
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .unwrap_or_default()
        .trim()
        .to_string();
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default();
    (ip, ua.chars().take(200).collect())
}

/// Authenticated staff member + the branch they are currently operating from.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub permissions: Vec<String>,
    pub branch_ids: Vec<Uuid>,
    /// Current Branch (X-Branch-Id header), validated against branch_ids.
    pub branch_id: Uuid,
    pub tz: Tz,
    /// Minutes after midnight that still belong to the previous business day at the Current Branch.
    pub day_shift: i32,
    /// Device position reported by the browser (X-Location), for geofencing and the audit trail.
    pub location: Option<crate::geo::Location>,
    pub ip: String,
    pub user_agent: String,
    /// The platform admin's own business when they have opened this one (full access, audited).
    pub acting_from: Option<Uuid>,
    /// Modules in the business's package (None = all) — roadmap 41.
    pub modules: Option<Vec<String>>,
}

impl Ctx {
    pub fn can(&self, perm: &str) -> bool {
        // "Manage all settings" implies every settings area.
        self.permissions.iter().any(|p| p == "*" || p == perm || (p == "settings.manage" && perm.starts_with("settings.")))
    }

    /// The business's package includes `module` (see `billing::MODULES`).
    pub fn has_module(&self, module: &str) -> bool {
        self.modules.as_ref().is_none_or(|m| m.iter().any(|x| x == module))
    }

    pub fn require_module(&self, module: &str) -> AppResult<()> {
        if self.has_module(module) {
            Ok(())
        } else {
            Err(module_refused(module))
        }
    }

    /// May see sales, figures and performance of employees other than themselves.
    pub fn sees_others(&self) -> bool {
        self.can("staff.view_others")
    }

    /// Today's business date at the Current Branch (a sale at 01:30 with a 02:00 close belongs to yesterday).
    pub fn today(&self) -> chrono::NaiveDate {
        crate::util::business_today(self.tz, self.day_shift)
    }

    pub fn is_admin(&self) -> bool {
        self.permissions.iter().any(|p| p == "*")
    }

    pub fn require(&self, perm: &str) -> AppResult<()> {
        if self.can(perm) {
            Ok(())
        } else {
            Err(AppError::Forbidden("You do not have permission for this action".into()))
        }
    }

    pub fn require_any(&self, perms: &[&str]) -> AppResult<()> {
        if perms.iter().any(|p| self.can(p)) {
            Ok(())
        } else {
            Err(AppError::Forbidden("You do not have permission for this action".into()))
        }
    }

    pub fn has_branch(&self, branch_id: Uuid) -> bool {
        self.branch_ids.contains(&branch_id)
    }

    pub fn ensure_branch(&self, branch_id: Uuid) -> AppResult<()> {
        if self.has_branch(branch_id) {
            Ok(())
        } else {
            Err(AppError::Forbidden("You are not assigned to that branch".into()))
        }
    }

    /// Resolve an optional branch filter: an explicit branch the user may see, else the Current Branch.
    pub fn branch_or_current(&self, branch: Option<Uuid>) -> AppResult<Uuid> {
        match branch {
            Some(b) => {
                self.ensure_branch(b)?;
                Ok(b)
            }
            None => Ok(self.branch_id),
        }
    }

    /// Branches for a report/list filter: explicit branch, or all the user may see.
    pub fn branch_scope(&self, branch: Option<Uuid>) -> AppResult<Vec<Uuid>> {
        match branch {
            Some(b) => {
                self.ensure_branch(b)?;
                Ok(vec![b])
            }
            None => Ok(self.branch_ids.clone()),
        }
    }
}

pub fn module_refused(module: &str) -> AppError {
    crate::error::refused(
        "Not in your package",
        format!("{} is not part of this business's S'Shop package — contact S'Shop to add it", crate::billing::module_label(module)),
    )
}

/// While billing is suspended a business can still sign in, see its profile and notifications, and pay.
fn allowed_while_suspended(path: &str) -> bool {
    let p = path.strip_prefix("/api").unwrap_or(path);
    ["/auth/", "/billing", "/notifications", "/fx", "/events"].iter().any(|pre| p.starts_with(pre))
}

#[derive(sqlx::FromRow)]
struct CtxRow {
    name: String,
    is_active: bool,
    all_branches: bool,
    permissions: Vec<String>,
    timezone: String,
    email: String,
    tenant_status: String,
    sessions_valid_after: Option<chrono::DateTime<Utc>>,
    user_sessions_valid_after: Option<chrono::DateTime<Utc>>,
    must_change_pin: bool,
}

/// Paths are matched as the API router sees them (inside /api).
fn request_path(parts: &Parts) -> String {
    parts
        .extensions
        .get::<axum::extract::OriginalUri>()
        .map_or_else(|| parts.uri.path().to_string(), |u| u.0.path().to_string())
}

impl FromRequestParts<AppState> for Ctx {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;
        let claims = read_token(&state.cfg.jwt_secret, &token, "staff")?;

        // A platform admin who opened another business is still checked against their own account on
        // every request: losing platform-admin status or the admin role ends the session immediately.
        let home_tenant = claims.home.unwrap_or(claims.tid);
        let row: Option<CtxRow> = sqlx::query_as(
            "SELECT u.name, u.is_active, u.all_branches, r.permissions || u.extra_permissions AS permissions, t.timezone, lower(u.email) AS email,
                    t.status AS tenant_status, t.sessions_valid_after, u.sessions_valid_after AS user_sessions_valid_after, u.must_change_pin
             FROM users u JOIN roles r ON r.id = u.role_id JOIN tenants t ON t.id = $3
             WHERE u.id = $1 AND u.tenant_id = $2",
        )
        .bind(claims.sub)
        .bind(home_tenant)
        .bind(claims.tid)
        .fetch_optional(&state.db)
        .await?;
        let mut row = row.ok_or(AppError::Unauthorized)?;
        if !row.is_active {
            return Err(AppError::Unauthorized);
        }
        // A PIN change / reset ends the person's older sessions (roadmap 60).
        if row.user_sessions_valid_after.is_some_and(|t| claims.iat < t.timestamp()) {
            return Err(AppError::Unauthorized);
        }
        // Signed in with a one-time PIN: nothing but replacing it (and reading the profile) until it is replaced.
        if row.must_change_pin && !["/auth/", "/fx"].iter().any(|p| request_path(parts).trim_start_matches("/api").starts_with(p)) {
            return Err(crate::error::refused("Set your own PIN", "Replace your one-time PIN with your own PIN to continue"));
        }
        if claims.home.is_some() {
            if !crate::routes::access::is_platform_admin(state, &row.email, &row.permissions) {
                return Err(AppError::Unauthorized);
            }
            row.permissions = vec!["*".into()];
            row.all_branches = true;
            // The platform owner may look inside a deactivated business but not change anything in it.
            if row.tenant_status != "active" && parts.method != axum::http::Method::GET {
                return Err(crate::error::refused("Business deactivated", "This business is deactivated — reactivate it before making changes"));
            }
        } else {
            // Deactivation ends every session of the business; sessions issued before it never come back.
            if row.tenant_status != "active" {
                return Err(AppError::Unauthorized);
            }
            if row.sessions_valid_after.is_some_and(|t| claims.iat < t.timestamp()) {
                return Err(AppError::Unauthorized);
            }
        }

        // Roadmap 41–43: the package decides which modules this business may use, and a billing suspension leaves
        // only sign-in, notifications and Billing (so the business can pay). The platform owner acting inside a
        // business is not restricted.
        let access: crate::billing::AccessRow = {
            let mut conn = state.db.acquire().await?;
            crate::billing::access(&mut conn, claims.tid).await?
        };
        let path = request_path(parts);
        let modules = if claims.home.is_some() { None } else { access.modules() };
        if claims.home.is_none() {
            if access.suspended() && !allowed_while_suspended(&path) {
                return Err(crate::error::refused(
                    "Billing suspended",
                    "This business's S'Shop access is suspended for an overdue invoice — an administrator can pay it in Settings → Billing",
                ));
            }
            if let (Some(m), Some(allowed)) = (crate::billing::module_for_path(&path), modules.as_ref()) {
                if !allowed.iter().any(|x| x == m) {
                    return Err(module_refused(m));
                }
            }
        }

        let all_branches = row.all_branches || row.permissions.iter().any(|p| p == "*");
        let branches: Vec<(Uuid, i32)> = if all_branches {
            sqlx::query_as("SELECT id, day_shift_minutes FROM branches WHERE tenant_id = $1 AND is_active ORDER BY created_at")
                .bind(claims.tid)
                .fetch_all(&state.db)
                .await?
        } else {
            sqlx::query_as(
                "SELECT b.id, b.day_shift_minutes FROM user_branches ub JOIN branches b ON b.id = ub.branch_id
                 WHERE ub.user_id = $1 AND b.is_active ORDER BY b.created_at",
            )
            .bind(claims.sub)
            .fetch_all(&state.db)
            .await?
        };
        let branch_ids: Vec<Uuid> = branches.iter().map(|b| b.0).collect();

        let requested = parts
            .headers
            .get("x-branch-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| Uuid::parse_str(v).ok());
        let branch_id = match requested {
            Some(b) if branch_ids.contains(&b) => b,
            Some(_) => return Err(AppError::Forbidden("You are not assigned to that branch".into())),
            None => *branch_ids
                .first()
                .ok_or_else(|| AppError::Forbidden("Your account is not assigned to any branch".into()))?,
        };

        let (ip, user_agent) = client_meta(&parts.headers);
        Ok(Ctx {
            user_id: claims.sub,
            tenant_id: claims.tid,
            name: row.name,
            permissions: row.permissions,
            branch_ids,
            branch_id,
            tz: parse_tz(&row.timezone),
            day_shift: branches.iter().find(|b| b.0 == branch_id).map_or(0, |b| b.1),
            location: parts.headers.get("x-location").and_then(|v| v.to_str().ok()).and_then(crate::geo::Location::parse),
            ip,
            user_agent,
            acting_from: claims.home,
            modules,
        })
    }
}

/// Customer identity on the public ordering portal.
#[derive(Clone, Debug)]
pub struct PortalCustomer {
    pub customer_id: Uuid,
    pub tenant_id: Uuid,
}

impl FromRequestParts<AppState> for PortalCustomer {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;
        let claims = read_token(&state.cfg.jwt_secret, &token, "portal")?;
        Ok(PortalCustomer { customer_id: claims.sub, tenant_id: claims.tid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_round_trip() {
        let h = hash_pin("1234").unwrap();
        assert!(verify_pin("1234", &h));
        assert!(!verify_pin("4321", &h));
    }

    #[test]
    fn token_type_is_enforced() {
        let secret = "x".repeat(40);
        let t = issue_token(&secret, Uuid::new_v4(), Uuid::new_v4(), "portal", chrono::Duration::hours(1)).unwrap();
        assert!(read_token(&secret, &t, "staff").is_err());
        assert!(read_token(&secret, &t, "portal").is_ok());
    }
}
