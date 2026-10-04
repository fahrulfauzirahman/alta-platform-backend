use alta_kernel::{AppError, PageRequest, PageResult, TenantId};
use alta_application::reference_items::{CreateReferenceItem, ReferenceItem};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PgRepositories {
    pub pool: PgPool,
}

impl PgRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(database_url).await
}

pub async fn list_reference_items(
    pool: &PgPool,
    tenant: TenantId,
    page: PageRequest,
) -> Result<PageResult<ReferenceItem>, AppError> {
    let page = page.normalized();
    let rows = sqlx::query(
        "SELECT id, tenant_id, title, created_at FROM reference_items WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2",
    )
    .bind(tenant.0)
    .bind(page.limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let items = rows
        .into_iter()
        .map(|r| {
            use sqlx::Row;
            ReferenceItem {
                id: r.try_get("id").unwrap_or_else(|_| Uuid::new_v4()),
                tenant_id: tenant,
                title: r.try_get("title").unwrap_or_default(),
                created_at: r.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now()),
            }
        })
        .collect::<Vec<_>>();

    let next_cursor = None;
    Ok(PageResult { items, next_cursor })
}

pub async fn create_reference_item_atomic(
    pool: &PgPool,
    tenant: TenantId,
    input: CreateReferenceItem,
    idempotency_key: Option<String>,
) -> Result<(ReferenceItem, bool), AppError> {
    input.validate()?;
    let mut tx = pool.begin().await.map_err(|e| AppError::Internal(e.to_string()))?;

    // Idempotency: if key supplied, try to return existing completed response.
    if let Some(key) = idempotency_key.clone() {
        if !key.trim().is_empty() {
            let existing: Option<(Uuid,)> = sqlx::query_as(
                "SELECT resource_id FROM platform_idempotency WHERE tenant_id = $1 AND idempotency_key = $2",
            )
            .bind(tenant.0)
            .bind(&key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
            if let Some((rid,)) = existing {
                let row = sqlx::query("SELECT id, title, created_at FROM reference_items WHERE id = $1")
                    .bind(rid)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                if let Some(row) = row {
                    use sqlx::Row;
                    let item = ReferenceItem {
                        id: row.try_get("id").unwrap_or(rid),
                        tenant_id: tenant,
                        title: row.try_get("title").unwrap_or_default(),
                        created_at: row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now()),
                    };
                    tx.commit().await.map_err(|e| AppError::Internal(e.to_string()))?;
                    return Ok((item, false));
                }
            }
        }
    }

    let id = Uuid::new_v4();
    let row = sqlx::query("INSERT INTO reference_items (id, tenant_id, title) VALUES ($1, $2, $3) RETURNING created_at")
        .bind(id)
        .bind(tenant.0)
        .bind(input.title.trim())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    use sqlx::Row;
    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now());

    let item = ReferenceItem { id, tenant_id: tenant, title: input.title.trim().to_string(), created_at };

    // Transactional outbox: same tx writes resource + event.
    let envelope = alta_application::ReferenceItemService::build_created_event(&item);
    let payload = serde_json::to_value(&envelope).unwrap_or(serde_json::json!({}));
    sqlx::query(
        "INSERT INTO platform_outbox (event_id, tenant_id, event_type, payload, aggregate_type, aggregate_id) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(envelope.event_id)
    .bind(tenant.0)
    .bind(&envelope.event_type)
    .bind(&payload)
    .bind("reference_item")
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(key) = idempotency_key {
        if !key.trim().is_empty() {
            let _ = sqlx::query(
                "INSERT INTO platform_idempotency (tenant_id, idempotency_key, resource_id, status_code) VALUES ($1, $2, $3, 201) ON CONFLICT (tenant_id, idempotency_key) DO NOTHING",
            )
            .bind(tenant.0)
            .bind(&key)
            .bind(id)
            .execute(&mut *tx)
            .await;
        }
    }

    tx.commit().await.map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((item, true))
}
