pub fn init_tracing(filter: &str) {
    use tracing_subscriber::{fmt, EnvFilter};
    let env = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(env)
        .json()
        .try_init();
}
