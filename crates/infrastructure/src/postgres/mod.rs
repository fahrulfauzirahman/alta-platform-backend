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
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .idle_timeout(std::time::Duration::from_secs(60))
        .max_lifetime(std::time::Duration::from_secs(1800))
        .connect(database_url)
        .await
}

pub fn request_hash_for_title(title: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(title.trim().as_bytes());
    hex::encode(h.finalize())
}

pub async fn list_reference_items(
    pool: &PgPool,
    tenant: TenantId,
    page: PageRequest,
) -> Result<PageResult<ReferenceItem>, AppError> {
    let page = page.normalized();
    let fetch = page.limit.saturating_add(1);
    let rows = if let Some(cursor) = page.cursor.clone() {
        let (ts, cid) = alta_kernel::pagination::decode_cursor(&cursor)
            .ok_or_else(|| AppError::validation("invalid cursor"))?;
        sqlx::query(
            "SELECT id, tenant_id, title, created_at FROM reference_items WHERE tenant_id = $1 AND (created_at, id) < ($2, $3) ORDER BY created_at DESC, id DESC LIMIT $4",
        )
        .bind(tenant.0)
        .bind(ts)
        .bind(cid)
        .bind(fetch)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        sqlx::query(
            "SELECT id, tenant_id, title, created_at FROM reference_items WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2",
        )
        .bind(tenant.0)
        .bind(fetch)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    };

    let mut items = rows
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

    let next_cursor = if items.len() as i64 > page.limit {
        items.truncate(page.limit as usize);
        items.last().map(|last| alta_kernel::pagination::encode_cursor(last.created_at, last.id))
    } else {
        None
    };
    Ok(PageResult { items, next_cursor })
}

pub async fn create_reference_item_atomic(
    pool: &PgPool,
    tenant: TenantId,
    input: CreateReferenceItem,
    idempotency_key: Option<String>,
) -> Result<(ReferenceItem, bool), AppError> {
    input.validate()?;
    let key = match idempotency_key {
        Some(k) if !k.trim().is_empty() => {
            if !alta_application::idempotency::idempotency_key_valid(&k) {
                return Err(AppError::validation("invalid idempotency key"));
            }
            Some(k)
        }
        _ => None,
    };
    let req_hash = request_hash_for_title(&input.title);

    let mut tx = pool.begin().await.map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(k) = key.clone() {
        let reserved = Uuid::new_v4();
        let res = sqlx::query(
            "INSERT INTO platform_idempotency (tenant_id, idempotency_key, resource_id, status_code, request_hash) VALUES ($1, $2, $3, 201, $4) ON CONFLICT (tenant_id, idempotency_key) DO NOTHING",
        )
        .bind(tenant.0)
        .bind(&k)
        .bind(reserved)
        .bind(&req_hash)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        if res.rows_affected() == 0 {
            let existing: Option<(Uuid, Option<String>)> = sqlx::query_as(
                "SELECT resource_id, request_hash FROM platform_idempotency WHERE tenant_id = $1 AND idempotency_key = $2",
            )
            .bind(tenant.0)
            .bind(&k)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
            if let Some((rid, stored_hash)) = existing {
                if let Some(stored) = stored_hash {
                    if stored != req_hash {
                        tx.rollback().await.ok();
                        return Err(AppError::idempotency_conflict(
                            "idempotency key already used with different payload",
                        ));
                    }
                }
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
            tx.rollback().await.ok();
            return Err(AppError::Internal("idempotency mapping without resource".to_string()));
        }
        let row = sqlx::query("INSERT INTO reference_items (id, tenant_id, title) VALUES ($1, $2, $3) RETURNING created_at")
            .bind(reserved)
            .bind(tenant.0)
            .bind(input.title.trim())
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        use sqlx::Row;
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now());
        let item = ReferenceItem { id: reserved, tenant_id: tenant, title: input.title.trim().to_string(), created_at };
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
        .bind(reserved)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        tx.commit().await.map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok((item, true));
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

    tx.commit().await.map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((item, true))
}

#[async_trait::async_trait]
impl alta_application::reference_items::ReferenceItemRepository for PgRepositories {
    async fn list(
        &self,
        tenant: TenantId,
        page: PageRequest,
    ) -> Result<PageResult<ReferenceItem>, AppError> {
        list_reference_items(&self.pool, tenant, page).await
    }
    async fn create(
        &self,
        tenant: TenantId,
        input: CreateReferenceItem,
        idempotency_key: Option<String>,
    ) -> Result<(ReferenceItem, bool), AppError> {
        create_reference_item_atomic(&self.pool, tenant, input, idempotency_key).await
    }
}

#[async_trait::async_trait]
impl alta_application::events::OutboxRepository for PgRepositories {
    async fn count_pending(&self) -> Result<i64, AppError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM platform_outbox WHERE dispatched_at IS NULL")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(row.0)
    }
}

#[async_trait::async_trait]
impl alta_application::idempotency::IdempotencyRepository for PgRepositories {
    async fn probe(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1 FROM platform_idempotency LIMIT 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl alta_application::jobs::JobLeaseRepository for PgRepositories {
    async fn pending_count(&self) -> Result<i64, AppError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM platform_job_executions WHERE status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(row.0)
    }
}
