use crate::{ActorId, RequestId, TenantId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub request_id: RequestId,
    pub tenant_id: TenantId,
    pub actor_id: Option<ActorId>,
}
