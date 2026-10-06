use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("Please sign in to continue")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    /// Business-rule violation the user can act on (insufficient stock, discount too high …)
    #[error("{0}")]
    Rule(String),
    /// A rule violation with a short title for the error popup ("Barcode mismatch") and the explanation.
    #[error("{message}")]
    Refused { title: String, message: String },
    #[error("{0}")]
    Upstream(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub fn bad(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}

pub fn rule(msg: impl Into<String>) -> AppError {
    AppError::Rule(msg.into())
}

pub fn refused(title: impl Into<String>, message: impl Into<String>) -> AppError {
    AppError::Refused { title: title.into(), message: message.into() }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Rule(_) | AppError::Refused { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "rule_violation"),
            AppError::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream_error"),
            AppError::Db(sqlx::Error::RowNotFound) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Db(sqlx::Error::Database(e)) if e.is_unique_violation() => (StatusCode::CONFLICT, "conflict"),
            AppError::Db(sqlx::Error::Database(e)) if e.is_check_violation() => (StatusCode::UNPROCESSABLE_ENTITY, "rule_violation"),
            AppError::Db(_) | AppError::Other(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };

        let message = match &self {
            AppError::Db(sqlx::Error::RowNotFound) => "Record not found".to_string(),
            AppError::Db(sqlx::Error::Database(e)) if e.is_unique_violation() => unique_message(e.constraint()),
            AppError::Db(sqlx::Error::Database(e)) if e.is_check_violation() => {
                "The change would break a data-integrity rule".to_string()
            }
            AppError::Db(e) => {
                tracing::error!(error = %e, "database error");
                "Something went wrong. Please try again.".to_string()
            }
            AppError::Other(e) => {
                tracing::error!(error = ?e, "internal error");
                "Something went wrong. Please try again.".to_string()
            }
            other => other.to_string(),
        };

        let title = match &self {
            AppError::Refused { title, .. } => Some(title.clone()),
            _ => None,
        };
        (status, Json(json!({ "error": { "code": code, "message": message, "title": title } }))).into_response()
    }
}

/// Human messages for unique constraints so the UI can show them directly.
fn unique_message(constraint: Option<&str>) -> String {
    match constraint.unwrap_or_default() {
        "customers_tenant_id_mobile_key" => "A customer with this mobile number already exists",
        "users_email_uq" => "A user with this email already exists",
        "products_tenant_id_code_key" => "A product with this code already exists",
        "stock_items_active_barcode_uq" => "This barcode is already assigned to an active item",
        "branches_tenant_id_code_key" => "A branch with this code already exists",
        "referrals_referred_uq" => "This customer already has a referrer",
        "roles_tenant_id_name_key" => "A role with this name already exists",
        "categories_name_uq" | "expense_categories_name_uq" => "A category with this name already exists",
        "suppliers_name_uq" => "A supplier with this name already exists",
        "award_periods_one_open" => "Close the current award period before opening a new one",
        "customer_fields_tenant_id_key_key" => "A field with this name already exists",
        "tenants_slug_key" => "This business link is already taken",
        "payments_mpesa_ref_uq" => "This M-Pesa code has already been used for another payment",
        "sales_client_ref_uq" => "This sale was already recorded",
        _ => "This record already exists",
    }
    .to_string()
}
