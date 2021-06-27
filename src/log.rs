pub fn init() {
    use tracing_subscriber::{EnvFilter, FmtSubscriber};
    let env_filter = EnvFilter::new(std::env::var("RUST_LOG").as_deref().unwrap_or("info"));
    FmtSubscriber::builder().with_env_filter(env_filter).init();
}
