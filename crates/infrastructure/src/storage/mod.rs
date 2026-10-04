#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub endpoint: Option<String>,
    pub bucket: String,
}

#[async_trait::async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn health(&self) -> Result<(), alta_kernel::AppError> {
        Ok(())
    }
}
