use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::info;

use zumic::{server::handle_connection, InMemoryStore, Settings, StorageEngine, StorageType};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let settings = Settings::load()?;
    info!("Loaded config: {:#?}", &settings);

    #[allow(clippy::arc_with_non_send_sync)]
    let engine = match settings.storage_type {
        StorageType::Memory => Arc::new(StorageEngine::Memory(InMemoryStore::new())),
        StorageType::Persistent => {
            unimplemented!("Persistent storage not yet implemented")
        }
        StorageType::Cluster => {
            unimplemented!("Cluster storage not yet implemented")
        }
    };

    let listener: TcpListener = TcpListener::bind(settings.listen_address).await?;
    info!("Listening on {}", settings.listen_address);

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("Client connected: {}", addr);

        let eng = engine.clone();
        if let Err(e) = handle_connection(socket, eng).await {
            tracing::error!("Connection error: {e:?}");
        }
    }
}
