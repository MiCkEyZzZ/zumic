use tracing::{debug, info};

use zumic::{logging::init_logging, network::server::TcpServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    // Logging the server startup
    info!("Starting server on 127.0.0.1:6379");
    debug!("This is a debug message"); // For checking
    info!("This is an info message"); // Logging at info level

    let server = TcpServer::new("127.0.0.1:6379").await?;
    println!("Server started on 6379");
    info!("Server started on port 6379");

    server.run().await;
    Ok(())
}
