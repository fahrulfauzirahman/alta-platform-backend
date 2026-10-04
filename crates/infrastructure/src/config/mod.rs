use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_addr")]
    pub addr: String,
    pub database_url: String,
    #[serde(default = "default_log")]
    pub log_filter: String,
}

fn default_addr() -> String { "0.0.0.0:8080".to_string() }
fn default_log() -> String { "info,alta=debug".to_string() }

impl AppConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let cfg = config::Config::builder()
            .add_source(config::Environment::default())
            .set_default("addr", default_addr())?
            .set_default("log_filter", default_log())?
            .build()?;
        cfg.try_deserialize()
    }
}
