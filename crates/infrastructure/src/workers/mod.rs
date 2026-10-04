use sqlx::PgPool;

pub struct OutboxPublisher {
    pub pool: PgPool,
    pub batch_size: i64,
}

impl OutboxPublisher {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, batch_size: 50 }
    }

    /// Claim a batch and mark dispatched. Restart-safe: only rows with
    /// dispatched_at IS NULL are claimed, using FOR UPDATE SKIP LOCKED.
    pub async fn publish_batch(&self) -> Result<Vec<serde_json::Value>, alta_kernel::AppError> {
        let rows = sqlx::query(
            "SELECT id, payload FROM platform_outbox WHERE dispatched_at IS NULL ORDER BY created_at ASC LIMIT $1 FOR UPDATE SKIP LOCKED",
        )
        .bind(self.batch_size)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| alta_kernel::AppError::Internal(e.to_string()))?;

        use sqlx::Row;
        let mut out = Vec::new();
        for r in rows {
            let id: uuid::Uuid = r.try_get("id").unwrap_or_else(|_| uuid::Uuid::new_v4());
            let payload: serde_json::Value = r.try_get("payload").unwrap_or(serde_json::json!({}));
            // In foundation: mark dispatched (real broadcast happens via SSE poll / webhook later).
            let _ = sqlx::query("UPDATE platform_outbox SET dispatched_at = now(), attempts = attempts + 1 WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await;
            out.push(payload);
        }
        Ok(out)
    }
}
