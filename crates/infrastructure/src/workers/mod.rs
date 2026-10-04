use sqlx::PgPool;

pub struct OutboxPublisher {
    pub pool: PgPool,
    pub batch_size: i64,
}

impl OutboxPublisher {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, batch_size: 50 }
    }

    /// Atomically claim a batch and mark dispatched.
    /// Single-statement CTE keeps SELECT FOR UPDATE SKIP LOCKED and UPDATE
    /// in one transaction, so concurrent workers cannot claim the same rows.
    /// Restart-safe: only rows with dispatched_at IS NULL are claimed.
    pub async fn publish_batch(&self) -> Result<Vec<serde_json::Value>, alta_kernel::AppError> {
        let rows = sqlx::query(
            "WITH claimed AS (SELECT id FROM platform_outbox WHERE dispatched_at IS NULL ORDER BY created_at ASC LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE platform_outbox o SET dispatched_at = now(), attempts = o.attempts + 1 FROM claimed WHERE o.id = claimed.id RETURNING o.payload",
        )
        .bind(self.batch_size)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;

        use sqlx::Row;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let payload: serde_json::Value = r.try_get("payload").unwrap_or(serde_json::json!({}));
            out.push(payload);
        }
        Ok(out)
    }
}

/// Claim one pending job with a lease. Single-statement UPDATE..SELECT with
/// SKIP LOCKED ensures exactly one worker wins under concurrency. Expired
/// leases (`leased_until < now()`) are reclaimable. Returns None when empty.
pub async fn claim_job(
    pool: &PgPool,
    lease_secs: i64,
) -> Result<Option<alta_application::jobs::JobRecord>, alta_kernel::AppError> {
    let row = sqlx::query(
        "UPDATE platform_job_executions SET status = 'leased', leased_until = now() + ($1 || ' seconds')::interval, attempts = attempts + 1, updated_at = now() WHERE job_id = (SELECT job_id FROM platform_job_executions WHERE status = 'pending' OR (status = 'leased' AND leased_until < now()) ORDER BY created_at ASC LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING job_id, job_type, payload, leased_until",
    )
    .bind(lease_secs.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;
    use sqlx::Row;
    Ok(row.map(|r| alta_application::jobs::JobRecord {
        job_id: r.try_get("job_id").unwrap_or_else(|_| uuid::Uuid::new_v4()),
        job_type: r.try_get("job_type").unwrap_or_default(),
        payload: r.try_get("payload").unwrap_or(serde_json::json!({})),
        leased_until: r.try_get("leased_until").ok(),
    }))
}

pub async fn complete_job(pool: &PgPool, job_id: uuid::Uuid) -> Result<(), alta_kernel::AppError> {
    sqlx::query("UPDATE platform_job_executions SET status = 'done', leased_until = NULL, updated_at = now() WHERE job_id = $1")
        .bind(job_id)
        .execute(pool)
        .await
        .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;
    Ok(())
}

pub async fn fail_job(pool: &PgPool, job_id: uuid::Uuid) -> Result<(), alta_kernel::AppError> {
    sqlx::query("UPDATE platform_job_executions SET status = 'failed', leased_until = NULL, updated_at = now() WHERE job_id = $1")
        .bind(job_id)
        .execute(pool)
        .await
        .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;
    Ok(())
}
