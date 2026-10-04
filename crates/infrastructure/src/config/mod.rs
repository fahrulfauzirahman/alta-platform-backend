use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_addr")]
    pub addr: String,
    #[serde(default)]
    pub database_url: String,
    #[serde(default = "default_log")]
    pub log_filter: String,
    #[serde(default)]
    pub auth_hs256_secret: String,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_min: u32,
    #[serde(default)]
    pub allowed_origins: String,
}

fn default_addr() -> String { "0.0.0.0:8080".to_string() }
fn default_log() -> String { "info,alta=debug".to_string() }
fn default_rate_limit() -> u32 { 200 }

impl AppConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let cfg = config::Config::builder()
            .add_source(config::Environment::default())
            .set_default("addr", default_addr())?
            .set_default("log_filter", default_log())?
            .set_default("auth_hs256_secret", String::new())?
            .set_default("rate_limit_per_min", default_rate_limit() as i64)?
            .set_default("allowed_origins", String::new())?
            .build()?;
        let mut out: Self = cfg.try_deserialize()?;
        if let Ok(v) = std::env::var("API_ADDR") {
            if !v.trim().is_empty() {
                out.addr = v;
            }
        }
        if let Ok(v) = std::env::var("AUTH_HS256_SECRET") {
            out.auth_hs256_secret = v;
        }
        if let Ok(v) = std::env::var("ALLOWED_ORIGINS") {
            out.allowed_origins = v;
        }
        if let Ok(v) = std::env::var("RATE_LIMIT_PER_MIN") {
            if let Ok(n) = v.trim().parse::<u32>() {
                out.rate_limit_per_min = n.clamp(1, 10000);
            }
        }
        Ok(out)
    }

    pub fn auth_secret_opt(&self) -> Option<String> {
        let s = self.auth_hs256_secret.trim();
        if s.is_empty() { None } else { Some(s.to_string()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_addr_alias_overrides() {
        let prev = std::env::var("API_ADDR").ok();
        std::env::set_var("API_ADDR", "127.0.0.1:18081");
        std::env::set_var("DATABASE_URL", "postgres://localhost/test");
        let cfg = AppConfig::from_env().expect("config");
        assert_eq!(cfg.addr, "127.0.0.1:18081");
        if let Some(v) = prev {
            std::env::set_var("API_ADDR", v);
        } else {
            std::env::remove_var("API_ADDR");
        }
    }
}
