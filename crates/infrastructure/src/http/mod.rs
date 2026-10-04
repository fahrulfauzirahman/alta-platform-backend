use alta_kernel::{ActorId, AppError, RequestContext, RequestId, TenantId};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

pub fn request_context_from_headers(headers: &HeaderMap) -> Result<RequestContext, AppError> {
    request_context_from_headers_verified(headers, None)
}

/// When `secret` is Some, require `Authorization: Bearer <HS256 JWT>` with
/// `tid` (tenant) claim and optional `aid` (actor) claim. If `X-Tenant-Id`
/// is also sent it must match the token tenant, otherwise 403. When
/// `secret` is None, fall back to trusted `X-Tenant-Id` headers (local/test
/// and authenticating-proxy deployments; see docs/security.md).
pub fn request_context_from_headers_verified(
    headers: &HeaderMap,
    secret: Option<&str>,
) -> Result<RequestContext, AppError> {
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(RequestId)
        .unwrap_or_else(RequestId::new);

    if let Some(sec) = secret {
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::unauthorized("missing bearer token"))?;
        let (tenant_uuid, actor_uuid) = crate::auth::verify_bearer(token, sec)?;
        let tenant_id = TenantId(tenant_uuid);
        let actor_id = actor_uuid.map(ActorId);
        if let Some(hdr) = headers.get("x-tenant-id").and_then(|v| v.to_str().ok()) {
            let claimed = TenantId::parse(hdr).map_err(|_| AppError::validation("invalid X-Tenant-Id"))?;
            if claimed != tenant_id {
                return Err(AppError::forbidden("tenant mismatch between token and header"));
            }
        }
        if let Some(hdr) = headers.get("x-actor-id").and_then(|v| v.to_str().ok()) {
            if let Ok(claimed) = Uuid::parse_str(hdr) {
                if Some(claimed) != actor_uuid && actor_uuid.is_some() {
                    return Err(AppError::forbidden("actor mismatch between token and header"));
                }
            }
        }
        return Ok(RequestContext { request_id, tenant_id, actor_id });
    }

    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| TenantId::parse(s).ok())
        .ok_or_else(|| AppError::validation("missing or invalid X-Tenant-Id"))?;

    let actor_id = headers
        .get("x-actor-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(ActorId);

    Ok(RequestContext { request_id, tenant_id, actor_id })
}

pub fn app_error_response(err: AppError, request_id: RequestId) -> Response {
    let code = err.code();
    let status = StatusCode::from_u16(code.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({
        "error": { "code": code.as_str(), "message": sanitize_message(err.message()) },
        "request_id": request_id.to_string(),
    });
    (status, axum::Json(body)).into_response()
}

fn sanitize_message(msg: String) -> String {
    msg.replace("postgres://", "postgres://[redacted]@")
}
