use alta_kernel::TenantId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    pub event_version: u32,
    pub tenant_id: TenantId,
    pub occurred_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

impl EventEnvelope {
    pub fn new(tenant_id: TenantId, event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            event_type: event_type.into(),
            event_version: 1,
            tenant_id,
            occurred_at: Utc::now(),
            payload,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewEvent {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub envelope: EventEnvelope,
}

#[async_trait::async_trait]
pub trait OutboxRepository: Send + Sync {
    async fn count_pending(&self) -> Result<i64, alta_kernel::AppError>;
}
