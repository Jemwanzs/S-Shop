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
    /// The support session that admitted a platform admin to another business (roadmap 71). Re-checked on every
    /// request: ended, expired or revoked sessions stop working at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sid: Option<Uuid>,
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
    let claims = Claims { sub, tid, typ: typ.into(), home: None, sid: None, iat: now.timestamp(), exp: (now + ttl).timestamp() };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::Other(anyhow::anyhow!("token: {e}")))
}

/// Staff token for a platform admin working inside another business (`tid`) through support session `sid`, valid
/// until the session ends (roadmap 71).
pub fn issue_acting_token(secret: &str, sub: Uuid, home: Uuid, tid: Uuid, sid: Uuid, until: chrono::DateTime<Utc>) -> AppResult<String> {
    let now = Utc::now();
    let claims = Claims { sub, tid, typ: "staff".into(), home: Some(home), sid: Some(sid), iat: now.timestamp(), exp: until.timestamp() };
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataScope {
    Own,
    Branches,
    All,
}

/// What a list / report may show: these branches, and only this owner's records when `owner` is set.
#[derive(Debug, Clone)]
pub struct Visibility {
    pub scope: DataScope,
    pub branches: Vec<Uuid>,
    pub owner: Option<Uuid>,
}

/// A platform support session inside a business (roadmap 71).
#[derive(Clone, Debug)]
pub struct Support {
    pub id: Uuid,
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
    /// The platform admin's own business when they have opened this one (support session, audited).
    pub acting_from: Option<Uuid>,
    /// The support session in force (roadmap 71).
    pub support: Option<Support>,
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

    /// Data-visibility scope for an area (roadmap 64): the role's `scope.<area>.*` (a user's own scope replaces it —
    /// see `effective_permissions`), else derived from the older "view other employees" permission.
    pub fn scope(&self, area: &str) -> DataScope {
        if self.permissions.iter().any(|p| p == "*") {
            return DataScope::All;
        }
        let prefix = format!("scope.{area}.");
        match self.permissions.iter().find_map(|p| p.strip_prefix(&prefix)) {
            Some("all") => DataScope::All,
            Some("branches") => DataScope::Branches,
            Some("own") => DataScope::Own,
            _ if self.sees_others() => DataScope::Branches,
            _ => DataScope::Own,
        }
    }

    /// May this user open one record of `area` at `branch`? `mine`: the record is credited to / created by them.
    pub fn may_view(&self, area: &str, branch: Uuid, mine: bool) -> bool {
        match self.scope(area) {
            DataScope::All => true,
            DataScope::Branches => mine || self.has_branch(branch),
            DataScope::Own => mine,
        }
    }

    /// Branches and owner filter for an area, from an optional requested branch and user. Own: only the user's own
    /// records (another user's figures are refused); Assigned branches: the user's branches; All: every branch of the
    /// business. Never another business.
    pub async fn visibility(&self, conn: &mut sqlx::PgConnection, area: &str, branch: Option<Uuid>, user: Option<Uuid>) -> AppResult<Visibility> {
        let scope = self.scope(area);
        let branches = match scope {
            DataScope::Branches => self.branch_scope(branch)?,
            DataScope::All | DataScope::Own => {
                let all: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM branches WHERE tenant_id = $1 ORDER BY created_at")
                    .bind(self.tenant_id)
                    .fetch_all(&mut *conn)
                    .await?;
                match branch {
                    Some(b) if all.contains(&b) => vec![b],
                    Some(_) => return Err(AppError::Forbidden("You are not assigned to that branch".into())),
                    None => all,
                }
            }
        };
        let owner = match scope {
            DataScope::Own => {
                if user.is_some_and(|u| u != self.user_id) {
                    return Err(AppError::Forbidden("You can only see your own records".into()));
                }
                Some(self.user_id)
            }
            _ => user,
        };
        Ok(Visibility { scope, branches, owner })
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
    /// Linked access to another business of the same tenant (roadmap 72): the sign-in identity must still be active,
    /// in the same tenant, and its PIN changes end these sessions too.
    linked: bool,
    identity_ok: bool,
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
            "SELECT u.name, u.is_active, u.all_branches, effective_permissions(r.permissions, u.extra_permissions) AS permissions, t.timezone, lower(u.email) AS email,
                    t.status AS tenant_status, t.sessions_valid_after,
                    GREATEST(u.sessions_valid_after, li.sessions_valid_after) AS user_sessions_valid_after,
                    u.must_change_pin OR COALESCE(li.must_change_pin, false) AS must_change_pin,
                    u.login_user_id IS NOT NULL AS linked,
                    (u.login_user_id IS NULL OR (li.is_active AND lt.account_id = t.account_id AND lt.status = 'active')) AS identity_ok
             FROM users u JOIN roles r ON r.id = u.role_id JOIN tenants t ON t.id = $3
             LEFT JOIN users li ON li.id = u.login_user_id LEFT JOIN tenants lt ON lt.id = li.tenant_id
             WHERE u.id = $1 AND u.tenant_id = $2",
        )
        .bind(claims.sub)
        .bind(home_tenant)
        .bind(claims.tid)
        .fetch_optional(&state.db)
        .await?;
        let mut row = row.ok_or(AppError::Unauthorized)?;
        if !row.is_active || !row.identity_ok || (row.linked && claims.home.is_some()) {
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
        let mut support = None;
        if claims.home.is_some() {
            if !crate::routes::access::is_platform_admin(state, &row.email, &row.permissions) {
                return Err(AppError::Unauthorized);
            }
            // Roadmap 71: inside another business only through a live support session (never a bare acting token).
            let sid = claims.sid.ok_or(AppError::Unauthorized)?;
            let s: Option<(String, chrono::DateTime<Utc>)> = sqlx::query_as(
                "UPDATE support_sessions SET status = CASE WHEN expires_at <= now() THEN 'expired' ELSE status END,
                        ended_at = CASE WHEN expires_at <= now() THEN expires_at ELSE ended_at END
                 WHERE id = $1 AND user_id = $2 AND tenant_id = $3 AND home_tenant_id = $4 AND status = 'active'
                 RETURNING (CASE WHEN status = 'active' THEN scope END), expires_at",
            )
            .bind(sid)
            .bind(claims.sub)
            .bind(claims.tid)
            .bind(home_tenant)
            .fetch_optional(&state.db)
            .await?
            .and_then(|(scope, exp): (Option<String>, chrono::DateTime<Utc>)| scope.map(|s| (s, exp)));
            let (scope, _expires_at) = s.ok_or_else(|| crate::error::refused("Support session ended", "This support session has ended — open the business again from Platform → Tenants"))?;
            let path = request_path(parts);
            let p = path.trim_start_matches("/api");
            if scope == "view" && parts.method != axum::http::Method::GET && !p.starts_with("/platform/support") && p != "/auth/logout" {
                return Err(crate::error::refused("View-only support access", "This support session is view-only — nothing can be changed in it"));
            }
            support = Some(Support { id: sid });
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
            support,
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
