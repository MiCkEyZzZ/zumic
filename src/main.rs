mod config;
mod network;

use std::sync::Arc;
use tokio::net::TcpListener;

use zumic::{config::config::Settings, server::handle_connection, StorageEngine};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // 1) Загружаем конфиг и получаем StorageConfig
    let (settings, storage_cfg) = Settings::load()?;
    println!(
        "Config: listen={} storage={:?}",
        settings.listen_address, storage_cfg.storage_type
    );

    // 2) Инициализируем StorageEngine
    let engine = StorageEngine::initialize(&storage_cfg).expect("failed to init storage");
    let engine = Arc::new(engine);

    // 3) Поднимаем TCP-listener
    let listener = TcpListener::bind(&settings.listen_address).await?;
    println!("Zumic listening on {}", &settings.listen_address);

    // 4) Последовательно обслуживаем клиентов
    loop {
        let (socket, peer) = listener.accept().await?;
        println!("Client connected: {}", peer);
        handle_connection(socket, engine.clone()).await?;
    }
}
