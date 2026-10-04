use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub job_id: Uuid,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub leased_until: Option<DateTime<Utc>>,
}

#[async_trait::async_trait]
pub trait JobLeaseRepository: Send + Sync {
    async fn pending_count(&self) -> Result<i64, alta_kernel::AppError> {
        Ok(0)
    }
}
