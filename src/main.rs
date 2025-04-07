use zumic::network::server::TcpServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = TcpServer::new("127.0.0.1:6379").await?;
    println!("Server started on 6379");
    server.run().await;
    Ok(())
}
