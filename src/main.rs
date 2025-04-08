use zumic::{logging::init_logging, network::server::TcpServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    // Логируем запуск сервера
    tracing::info!("Starting server on 127.0.0.1:6379");
    tracing::debug!("This is a debug message"); // Для проверки
    tracing::info!("This is an info message"); // Логирование на уровне info

    let server = TcpServer::new("127.0.0.1:6379").await?;
    println!("Server started on 6379");
    tracing::info!("Server started on port 6379");

    server.run().await;
    Ok(())
}
