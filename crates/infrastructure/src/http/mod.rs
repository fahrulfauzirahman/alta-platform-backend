use alta_kernel::{ActorId, AppError, RequestContext, RequestId, TenantId};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

pub fn request_context_from_headers(headers: &HeaderMap) -> Result<RequestContext, AppError> {
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(RequestId)
        .unwrap_or_else(RequestId::new);

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
    // Never leak secrets: strip anything resembling a token.
    msg.replace("postgres://", "postgres://[redacted]@")
}
