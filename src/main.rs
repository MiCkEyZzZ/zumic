use std::sync::Arc;

use tokio::net::TcpListener;

use zumic::{server::handle_connection, InMemoryStore, StorageEngine};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    #[allow(clippy::arc_with_non_send_sync)]
    let engine = Arc::new(StorageEngine::Memory(InMemoryStore::new()));
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    println!("Listening on 127.0.0.1:6379");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Client: {addr}");
        // Обрабатываем соединение **последовательно** в текущем потоке
        if let Err(e) = handle_connection(socket, engine.clone()).await {
            eprintln!("Connection error: {e:?}");
        }
    }
}
