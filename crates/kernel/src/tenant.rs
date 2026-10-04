use crate::{ActorId, TenantId};

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub actor_id: Option<ActorId>,
}

impl TenantContext {
    pub fn new(tenant_id: TenantId, actor_id: Option<ActorId>) -> Self {
        Self { tenant_id, actor_id }
    }

    pub fn require_actor(&self) -> Result<ActorId, crate::AppError> {
        self.actor_id.ok_or_else(|| crate::AppError::unauthorized("missing actor")) 
    }
}
