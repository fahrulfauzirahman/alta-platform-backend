use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = alta_infrastructure::config::AppConfig::from_env().unwrap_or(
        alta_infrastructure::config::AppConfig {
            addr: "0.0.0.0:8080".to_string(),
            database_url: std::env::var("DATABASE_URL").unwrap_or_default(),
            log_filter: "info,alta=debug".to_string(),
        },
    );
    alta_infrastructure::observability::init_tracing(&cfg.log_filter);

    if cfg.database_url.is_empty() {
        tracing::error!("DATABASE_URL must be set for worker");
        std::process::exit(1);
    }
    let pool = alta_infrastructure::postgres::connect(&cfg.database_url).await?;
    tracing::info!("alta-worker connected to db, starting outbox loop");

    let publisher = alta_infrastructure::workers::OutboxPublisher::new(pool.clone());

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("worker shutdown requested");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                match publisher.publish_batch().await {
                    Ok(events) => {
                        if !events.is_empty() {
                            tracing::info!(count = events.len(), "published outbox batch");
                        }
                    }
                    Err(e) => tracing::error!(error = ?e, "outbox batch failed"),
                }
            }
        }
    }
    // Graceful: pool closes on drop.
    Ok(())
}
