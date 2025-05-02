use tracing_subscriber::{fmt, EnvFilter};
use zumic::auth::ServerConfig;

fn main() {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let cfg = ServerConfig::load("zumic.conf");
    println!("{:?}", cfg);
}
