use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    ValidationFailed,
    IdempotencyConflict,
    RateLimited,
    Internal,
    ServiceUnavailable,
}

impl ErrorCode {
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict => 409,
            Self::ValidationFailed => 422,
            Self::IdempotencyConflict => 409,
            Self::RateLimited => 429,
            Self::Internal => 500,
            Self::ServiceUnavailable => 503,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::RateLimited => "RATE_LIMITED",
            Self::Internal => "INTERNAL",
            Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{code:?}: {message}")]
    Known { code: ErrorCode, message: String },
    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::Unauthorized, message: msg.into() }
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::Forbidden, message: msg.into() }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::NotFound, message: msg.into() }
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::ValidationFailed, message: msg.into() }
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::Conflict, message: msg.into() }
    }
    pub fn idempotency_conflict(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::IdempotencyConflict, message: msg.into() }
    }
    pub fn rate_limited(msg: impl Into<String>) -> Self {
        Self::Known { code: ErrorCode::RateLimited, message: msg.into() }
    }
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Known { code, .. } => *code,
            Self::Internal(_) => ErrorCode::Internal,
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::Known { message, .. } => message.clone(),
            Self::Internal(m) => m.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        assert_eq!(ErrorCode::Unauthorized.http_status(), 401);
        assert_eq!(ErrorCode::ValidationFailed.http_status(), 422);
        assert_eq!(ErrorCode::Internal.http_status(), 500);
    }

    #[test]
    fn message_sanitized_by_caller() {
        let e = AppError::validation("title must not be empty");
        assert_eq!(e.code(), ErrorCode::ValidationFailed);
    }
}
