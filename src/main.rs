use zumic::network::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::run_tcp_server("127.0.0.1:6379").await
}
