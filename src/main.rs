use zumic::auth::ServerConfig;

fn main() {
    let cfg = ServerConfig::load("zumic.conf");
    println!("{:?}", cfg)
}
