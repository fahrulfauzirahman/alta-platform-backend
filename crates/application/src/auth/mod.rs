use alta_kernel::{ActorId, TenantId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub actor_id: ActorId,
    pub tenant_id: TenantId,
    pub display_name: String,
}

#[async_trait::async_trait]
pub trait SessionService: Send + Sync {
    async fn verify(&self, token: &str) -> Result<Session, alta_kernel::AppError>;
}

pub struct StaticSessionService {
    pub tenant_id: TenantId,
    pub actor_id: ActorId,
}

#[async_trait::async_trait]
impl SessionService for StaticSessionService {
    async fn verify(&self, _token: &str) -> Result<Session, alta_kernel::AppError> {
        Ok(Session {
            actor_id: self.actor_id,
            tenant_id: self.tenant_id,
            display_name: "Reference User".to_string(),
        })
    }
}
