use alta_application::reference_items::CreateReferenceItem;
use alta_infrastructure::{http, observability};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::{convert::Infallible, net::SocketAddr, time::Duration};
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    pool: Option<sqlx::PgPool>,
    events_tx: broadcast::Sender<serde_json::Value>,
    version: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = alta_infrastructure::config::AppConfig::from_env().unwrap_or(
        alta_infrastructure::config::AppConfig {
            addr: "0.0.0.0:8080".to_string(),
            database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
            log_filter: "info,alta=debug".to_string(),
        },
    );
    observability::init_tracing(&cfg.log_filter);

    let pool = if cfg.database_url.is_empty() {
        tracing::warn!("DATABASE_URL empty; running without DB (health degraded)");
        None
    } else {
        match alta_infrastructure::postgres::connect(&cfg.database_url).await {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::error!(error = %e, "db connect failed; running degraded");
                None
            }
        }
    };

    let (events_tx, _) = broadcast::channel::<serde_json::Value>(1024);
    let state = AppState { pool, events_tx, version: env!("CARGO_PKG_VERSION").to_string() };

    let app = router(state);
    let addr: SocketAddr = cfg.addr.parse().unwrap_or(([0, 0, 0, 0], 8080).into());
    tracing::info!(%addr, "alta-api listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/session", get(session))
        .route("/v1/reference-items", get(list_items).post(create_item))
        .route("/v1/events", get(events))
        .with_state(state)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

async fn healthz(State(s): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "version": s.version }))
}

async fn readyz(State(s): State<AppState>) -> impl IntoResponse {
    if let Some(pool) = s.pool {
        match sqlx::query("SELECT 1").fetch_one(&pool).await {
            Ok(_) => (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({ "status": "ready" })),
            )
                .into_response(),
            Err(e) => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "status": "not_ready", "error": e.to_string() })),
            )
                .into_response(),
        }
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "not_ready", "reason": "no database" })),
        )
            .into_response()
    }
}

async fn session(headers: HeaderMap) -> Response {
    let ctx = match http::request_context_from_headers(&headers) {
        Ok(c) => c,
        Err(e) => {
            let rid = alta_kernel::RequestId::new();
            return http::app_error_response(e, rid);
        }
    };
    let body = serde_json::json!({
        "actor_id": ctx.actor_id.map(|a| a.to_string()),
        "tenant_id": ctx.tenant_id.to_string(),
        "request_id": ctx.request_id.to_string(),
    });
    (axum::http::StatusCode::OK, Json(body)).into_response()
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn list_items(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Response {
    let ctx = match http::request_context_from_headers(&headers) {
        Ok(c) => c,
        Err(e) => return http::app_error_response(e, alta_kernel::RequestId::new()),
    };
    let pool = match s.pool {
        Some(p) => p,
        None => {
            return http::app_error_response(
                alta_kernel::AppError::Internal("database not configured".to_string()),
                ctx.request_id,
            )
        }
    };
    let page = alta_kernel::PageRequest { limit: q.limit.unwrap_or(50), cursor: q.cursor };
    match alta_infrastructure::postgres::list_reference_items(&pool, ctx.tenant_id, page).await {
        Ok(res) => (axum::http::StatusCode::OK, Json(res)).into_response(),
        Err(e) => http::app_error_response(e, ctx.request_id),
    }
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    title: String,
}

async fn create_item(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateBody>,
) -> Response {
    let ctx = match http::request_context_from_headers(&headers) {
        Ok(c) => c,
        Err(e) => return http::app_error_response(e, alta_kernel::RequestId::new()),
    };
    let pool = match s.pool {
        Some(p) => p,
        None => {
            return http::app_error_response(
                alta_kernel::AppError::Internal("database not configured".to_string()),
                ctx.request_id,
            )
        }
    };
    let idem = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let input = CreateReferenceItem { title: body.title };
    match alta_infrastructure::postgres::create_reference_item_atomic(&pool, ctx.tenant_id, input, idem).await {
        Ok((item, created)) => {
            let status = if created {
                axum::http::StatusCode::CREATED
            } else {
                axum::http::StatusCode::OK
            };
            let _ = s.events_tx.send(serde_json::json!({
                "event_type": "reference_item.created.v1",
                "tenant_id": item.tenant_id.to_string(),
                "payload": item,
            }));
            (status, Json(item)).into_response()
        }
        Err(e) => http::app_error_response(e, ctx.request_id),
    }
}

async fn events(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match http::request_context_from_headers(&headers) {
        Ok(c) => c,
        Err(e) => return http::app_error_response(e, alta_kernel::RequestId::new()),
    };
    let rx = s.events_tx.subscribe();
    let tenant = ctx.tenant_id.to_string();
    let stream = async_stream::stream! {
        let mut rx = rx;
        yield Ok::<_, Infallible>(Event::default().data("connected"));
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Ok(v) => {
                            let same = v.get("tenant_id").and_then(|t| t.as_str()).map(|t| t == tenant).unwrap_or(true);
                            if same {
                                yield Ok::<_, Infallible>(Event::default().event("reference_item.created.v1").data(v.to_string()));
                            }
                        }
                        Err(_) => {
                            yield Ok::<_, Infallible>(Event::default().data("heartbeat"));
                            tokio::time::sleep(Duration::from_secs(15)).await;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(15)) => {
                    yield Ok::<_, Infallible>(Event::default().data("heartbeat"));
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}
