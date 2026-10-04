use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub Uuid);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActorId(pub Uuid);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub Uuid);

impl TenantId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn parse(s: &str) -> Result<Self, uuid::Error> { Ok(Self(Uuid::parse_str(s)?)) }
}
impl Default for TenantId {
    fn default() -> Self { Self::new() }
}
impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}
impl ActorId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}
impl Default for ActorId {
    fn default() -> Self { Self::new() }
}
impl std::fmt::Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}
impl RequestId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}
impl Default for RequestId {
    fn default() -> Self { Self::new() }
}
impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}
