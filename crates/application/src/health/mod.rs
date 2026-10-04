use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: String,
    pub version: String,
}

#[async_trait::async_trait]
pub trait HealthService: Send + Sync {
    async fn liveness(&self) -> HealthReport;
    async fn readiness(&self) -> HealthReport;
}
