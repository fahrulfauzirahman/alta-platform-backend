use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IdempotencyStatus {
    InProgress,
    Completed { response_code: u16, response_body: serde_json::Value },
}

#[async_trait::async_trait]
pub trait IdempotencyRepository: Send + Sync {
    async fn probe(&self) -> Result<(), alta_kernel::AppError> {
        Ok(())
    }
}

pub fn idempotency_key_valid(key: &str) -> bool {
    let k = key.trim();
    !k.is_empty() && k.len() <= 128
}
