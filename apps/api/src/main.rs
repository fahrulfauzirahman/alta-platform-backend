use alta_application::reference_items::CreateReferenceItem;
use alta_infrastructure::{http, observability};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::HeaderMap,
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::{collections::{HashMap, HashSet}, convert::Infallible, net::SocketAddr, sync::{Arc, Mutex}, time::{Duration, Instant}};
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    pool: Option<sqlx::PgPool>,
    events_tx: broadcast::Sender<serde_json::Value>,
    version: String,
    auth_secret: Option<String>,
    rate_limiter: RateLimiter,
    rate_limit_per_min: u32,
}

#[derive(Clone, Default)]
struct RateLimiter {
    inner: Arc<Mutex<HashMap<uuid::Uuid, (Instant, u32)>>>,
}

impl RateLimiter {
    fn allow(&self, tenant: uuid::Uuid, limit_per_min: u32) -> bool {
        if limit_per_min == 0 {
            return true;
        }
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        match map.get_mut(&tenant) {
            Some((window_start, count)) => {
                if now.duration_since(*window_start) >= Duration::from_secs(60) {
                    *window_start = now;
                    *count = 1;
                    true
                } else if *count < limit_per_min {
                    *count += 1;
                    true
                } else {
                    false
                }
            }
            None => {
                map.insert(tenant, (now, 1));
                true
            }
        }
    }
}

fn ctx_from(headers: &HeaderMap, state: &AppState) -> Result<alta_kernel::RequestContext, alta_kernel::AppError> {
    http::request_context_from_headers_verified(headers, state.auth_secret.as_deref())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = alta_infrastructure::config::AppConfig::from_env().unwrap_or(
        alta_infrastructure::config::AppConfig {
            addr: std::env::var("API_ADDR").unwrap_or("0.0.0.0:8080".to_string()),
            database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
            log_filter: "info,alta=debug".to_string(),
            auth_hs256_secret: String::new(),
            rate_limit_per_min: 200,
            allowed_origins: String::new(),
        },
    );
    observability::init_tracing(&cfg.log_filter);
    if cfg.auth_secret_opt().is_none() {
        tracing::warn!("AUTH_HS256_SECRET empty; trusting X-Tenant-Id headers (local/proxy mode)");
    }

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
    let state = AppState {
        pool,
        events_tx,
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_secret: cfg.auth_secret_opt(),
        rate_limiter: RateLimiter::default(),
        rate_limit_per_min: cfg.rate_limit_per_min,
    };

    let app = router(state, &cfg.allowed_origins);
    let addr: SocketAddr = cfg.addr.parse().unwrap_or(([0, 0, 0, 0], 8080).into());
    tracing::info!(%addr, "alta-api listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn router(state: AppState, allowed_origins: &str) -> Router {
    use axum::http::{header, Method};
    use tower_http::{cors::CorsLayer, request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer}, trace::TraceLayer};

    let cors = if allowed_origins.trim().is_empty() {
        None
    } else {
        let origins: Vec<axum::http::HeaderValue> = allowed_origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if origins.is_empty() {
            None
        } else {
            let mut layer = CorsLayer::new()
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::HeaderName::from_static("x-tenant-id"), header::HeaderName::from_static("x-actor-id"), header::HeaderName::from_static("x-request-id"), header::HeaderName::from_static("idempotency-key"), header::HeaderName::from_static("last-event-id")]);
            for o in origins {
                layer = layer.allow_origin(o);
            }
            Some(layer)
        }
    };

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/session", get(session))
        .route("/v1/reference-items", get(list_items).post(create_item))
        .route("/v1/events", get(events))
        .with_state(state)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(tower::ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(TraceLayer::new_for_http())
            .layer(PropagateRequestIdLayer::x_request_id())
            .into_inner());
    if let Some(c) = cors {
        app = app.layer(c);
    }
    app
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
        tracing::info!("shutdown signal received");
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received");
    }
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
            Err(e) => {
                tracing::error!(error = %e, "readyz db check failed");
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({ "status": "not_ready" })),
                )
                    .into_response()
            }
        }
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "not_ready", "reason": "no database" })),
        )
            .into_response()
    }
}

async fn session(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match ctx_from(&headers, &s) {
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
    let ctx = match ctx_from(&headers, &s) {
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
    let ctx = match ctx_from(&headers, &s) {
        Ok(c) => c,
        Err(e) => return http::app_error_response(e, alta_kernel::RequestId::new()),
    };
    if !s.rate_limiter.allow(ctx.tenant_id.0, s.rate_limit_per_min) {
        return http::app_error_response(
            alta_kernel::AppError::rate_limited("rate limit exceeded, retry later"),
            ctx.request_id,
        );
    }
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
            let event_value = match sqlx::query("SELECT payload FROM platform_outbox WHERE aggregate_id = $1 ORDER BY created_at DESC LIMIT 1")
                .bind(item.id)
                .fetch_optional(&pool)
                .await
            {
                Ok(Some(row)) => {
                    use sqlx::Row;
                    row.try_get("payload").unwrap_or_else(|_| {
                        serde_json::to_value(&alta_application::ReferenceItemService::build_created_event(&item))
                            .unwrap_or(serde_json::json!({}))
                    })
                }
                _ => serde_json::to_value(&alta_application::ReferenceItemService::build_created_event(&item))
                    .unwrap_or(serde_json::json!({})),
            };
            let _ = s.events_tx.send(event_value);
            (status, Json(item)).into_response()
        }
        Err(e) => http::app_error_response(e, ctx.request_id),
    }
}

fn event_id_of(v: &serde_json::Value) -> Option<uuid::Uuid> {
    v.get("event_id").and_then(|t| t.as_str()).and_then(|s| uuid::Uuid::parse_str(s).ok())
}

fn sse_event_for(tenant_str: &str, v: &serde_json::Value) -> Option<Event> {
    let same = v.get("tenant_id").and_then(|t| t.as_str()).map(|t| t == tenant_str).unwrap_or(false);
    if !same {
        return None;
    }
    let et = v.get("event_type").and_then(|t| t.as_str()).unwrap_or("reference_item.created.v1").to_string();
    let eid = v.get("event_id").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let mut ev = Event::default().event(et).data(v.to_string());
    if !eid.is_empty() {
        ev = ev.id(eid);
    }
    Some(ev)
}

async fn events(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match ctx_from(&headers, &s) {
        Ok(c) => c,
        Err(e) => return http::app_error_response(e, alta_kernel::RequestId::new()),
    };
    let pool = s.pool.clone();
    let rx = s.events_tx.subscribe();
    let tenant = ctx.tenant_id;
    let tenant_str = tenant.to_string();
    let last_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| uuid::Uuid::parse_str(v).ok());
    let missed: Vec<serde_json::Value> = if let (Some(p), Some(lid)) = (pool.clone(), last_id) {
        match replay_missed(&p, tenant, lid).await {
            Ok(v) => v,
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let anchor: Option<uuid::Uuid> = if last_id.is_some() {
        missed.iter().filter_map(event_id_of).last().or(last_id)
    } else if let Some(p) = pool.clone() {
        latest_event(&p, tenant).await.unwrap_or(None)
    } else {
        None
    };
    let stream = async_stream::stream! {
        let mut rx = rx;
        let mut emitted: HashSet<String> = HashSet::new();
        let mut last_seen = anchor;
        yield Ok::<_, Infallible>(Event::default().data("connected"));
        for v in missed {
            if let Some(ev) = sse_event_for(&tenant_str, &v) {
                if let Some(eid) = event_id_of(&v) {
                    emitted.insert(eid.to_string());
                    last_seen = Some(eid);
                }
                yield Ok::<_, Infallible>(ev);
            }
        }
        let mut poll = tokio::time::interval(Duration::from_secs(2));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Ok(v) => {
                            let eid_str = v.get("event_id").and_then(|t| t.as_str()).unwrap_or("").to_string();
                            if !eid_str.is_empty() && emitted.contains(&eid_str) {
                                continue;
                            }
                            if let Some(ev) = sse_event_for(&tenant_str, &v) {
                                if !eid_str.is_empty() {
                                    emitted.insert(eid_str.clone());
                                    if let Ok(eid) = uuid::Uuid::parse_str(&eid_str) {
                                        last_seen = Some(eid);
                                    }
                                }
                                yield Ok::<_, Infallible>(ev);
                            }
                        }
                        Err(_) => {
                            yield Ok::<_, Infallible>(Event::default().data("heartbeat"));
                        }
                    }
                }
                _ = poll.tick() => {
                    if let Some(p) = pool.clone() {
                        let since = last_seen;
                        match fetch_since(&p, tenant, since).await {
                            Ok(rows) => {
                                for v in rows {
                                    let eid_str = v.get("event_id").and_then(|t| t.as_str()).unwrap_or("").to_string();
                                    if !eid_str.is_empty() && emitted.contains(&eid_str) {
                                        continue;
                                    }
                                    if let Some(ev) = sse_event_for(&tenant_str, &v) {
                                        if !eid_str.is_empty() {
                                            emitted.insert(eid_str.clone());
                                            if let Ok(eid) = uuid::Uuid::parse_str(&eid_str) {
                                                last_seen = Some(eid);
                                            }
                                        }
                                        yield Ok::<_, Infallible>(ev);
                                    }
                                }
                            }
                            Err(_) => {}
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

async fn latest_event(
    pool: &sqlx::PgPool,
    tenant: alta_kernel::TenantId,
) -> Result<Option<uuid::Uuid>, alta_kernel::AppError> {
    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT event_id FROM platform_outbox WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(tenant.0)
    .fetch_optional(pool)
    .await
    .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;
    Ok(row.map(|r| r.0))
}

async fn fetch_since(
    pool: &sqlx::PgPool,
    tenant: alta_kernel::TenantId,
    after_event: Option<uuid::Uuid>,
) -> Result<Vec<serde_json::Value>, alta_kernel::AppError> {
    let Some(after) = after_event else {
        return Ok(Vec::new());
    };
    replay_missed(pool, tenant, after).await
}

async fn replay_missed(
    pool: &sqlx::PgPool,
    tenant: alta_kernel::TenantId,
    after_event: uuid::Uuid,
) -> Result<Vec<serde_json::Value>, alta_kernel::AppError> {
    let anchor: Option<(chrono::DateTime<chrono::Utc>, uuid::Uuid,)> = sqlx::query_as(
        "SELECT created_at, id FROM platform_outbox WHERE tenant_id = $1 AND event_id = $2",
    )
    .bind(tenant.0)
    .bind(after_event)
    .fetch_optional(pool)
    .await
    .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;
    let Some((after_ts, after_id)) = anchor else {
        return Ok(Vec::new());
    };
    let rows = sqlx::query(
        "SELECT payload FROM platform_outbox WHERE tenant_id = $1 AND (created_at, id) > ($2, $3) ORDER BY created_at ASC, id ASC LIMIT 100",
    )
    .bind(tenant.0)
    .bind(after_ts)
    .bind(after_id)
    .fetch_all(pool)
    .await
    .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;
    use sqlx::Row;
    Ok(rows.into_iter().map(|r| r.try_get("payload").unwrap_or(serde_json::json!({}))).collect())
}
