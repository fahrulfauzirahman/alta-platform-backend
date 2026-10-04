use alta_kernel::{AppError, PageRequest, PageResult, TenantId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceItem {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReferenceItem {
    pub title: String,
}

impl CreateReferenceItem {
    pub fn validate(&self) -> Result<(), AppError> {
        let t = self.title.trim();
        if t.is_empty() {
            return Err(AppError::validation("title must not be empty"));
        }
        if t.len() > 200 {
            return Err(AppError::validation("title too long (max 200)"));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait ReferenceItemRepository: Send + Sync {
    async fn list(&self, tenant: TenantId, page: PageRequest) -> Result<PageResult<ReferenceItem>, AppError>;
    async fn create(
        &self,
        tenant: TenantId,
        input: CreateReferenceItem,
        idempotency_key: Option<String>,
    ) -> Result<(ReferenceItem, bool), AppError>;
}

pub struct ReferenceItemService;

impl ReferenceItemService {
    pub fn build_created_event(item: &ReferenceItem) -> crate::events::EventEnvelope {
        crate::events::EventEnvelope::new(
            item.tenant_id,
            "reference_item.created.v1",
            serde_json::json!({
                "id": item.id,
                "title": item.title,
                "created_at": item.created_at,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_title() {
        let r = CreateReferenceItem { title: "  ".to_string() }.validate();
        assert!(r.is_err());
    }

    #[test]
    fn builds_created_event_type() {
        let item = ReferenceItem {
            id: uuid::Uuid::new_v4(),
            tenant_id: alta_kernel::TenantId::new(),
            title: "hello".to_string(),
            created_at: chrono::Utc::now(),
        };
        let env = ReferenceItemService::build_created_event(&item);
        assert_eq!(env.event_type, "reference_item.created.v1");
    }
}
