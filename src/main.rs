use std::{sync::Arc, time::Duration};

use tracing::{error, info, warn};
use zumic::{
    banner,
    engine::{InClusterStore, PersistentStoreConfig},
    logging,
    network::connection::ConnectionConfig,
    server::{Server, ServerConfig},
    InMemoryStore, InPersistentStore, Settings, Storage, StorageEngine, StorageType,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let local = tokio::task::LocalSet::new();

    local
        .run_until(async {
            if let Err(e) = run_server().await {
                error!("Server failed: {e}");
                Err(e)
            } else {
                Ok(())
            }
        })
        .await
}

async fn run_server() -> anyhow::Result<()> {
    let settings = Settings::load()?;

    let logging_handle = logging::init_logging(settings.logging.clone())
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {e}"))?
        .with_flush_timeout(Duration::from_secs(10));

    info!("Loaded config: {:#?}", &settings);

    let port = if settings.listen_address.port() == 0 {
        6174
    } else {
        settings.listen_address.port()
    };

    let storage_type_str = match settings.storage_type {
        StorageType::Memory => "in-memory",
        StorageType::Persistent => "persistent",
        StorageType::Cluster => "cluster",
    }
    .to_string();

    banner::print_banner(
        &settings.listen_address.to_string(),
        port,
        &storage_type_str,
    );
    banner::print_startup_log();

    #[allow(clippy::arc_with_non_send_sync)]
    let engine = match settings.storage_type {
        StorageType::Memory => {
            info!("Initializing in-memory storage");
            Arc::new(StorageEngine::Memory(InMemoryStore::new()))
        }
        StorageType::Persistent => {
            info!("Initializing persistent storage");
            let config = PersistentStoreConfig::default();
            let store =
                InPersistentStore::new("zumic.aof", config).map_err(|e| anyhow::anyhow!("{e}"))?;
            Arc::new(StorageEngine::Persistent(store))
        }
        StorageType::Cluster => {
            info!("Initializing cluster storage");
            let shards: Vec<Arc<dyn Storage>> = (0..3)
                .map(|_| Arc::new(InMemoryStore::new()) as Arc<dyn Storage>)
                .collect();
            let cluster_store = InClusterStore::new(shards);
            Arc::new(StorageEngine::Cluster(cluster_store))
        }
    };

    let server_config = ServerConfig {
        listen_address: settings.listen_address,
        connection_config: ConnectionConfig {
            max_connections: settings.max_connections as usize,
            max_connections_per_ip: settings.max_connections_per_ip.unwrap_or(100),
            idle_timeout: Duration::from_secs(settings.connection_timeout.unwrap_or(300)),
            read_timeout: Duration::from_secs(settings.read_timeout.unwrap_or(30)),
            write_timeout: Duration::from_secs(settings.write_timeout.unwrap_or(10)),
            read_buffer_size: settings.read_buffer_size.unwrap_or(8192),
        },
        shutdown_timeout: Duration::from_secs(settings.shutdown_timeout.unwrap_or(30)),
    };

    let mut server = Server::new(server_config, engine);

    match server.start().await {
        Ok(_) => {
            info!("Server started successfully");
            setup_signal_handlers(&mut server, logging_handle).await?;
        }
        Err(e) => {
            error!("Failed to start server: {e}");
            logging_handle.shutdown();
            return Err(e);
        }
    }

    Ok(())
}

/// Настройка обработчиков сигналов для graceful shutdown
async fn setup_signal_handlers(
    server: &mut Server,
    logging_handle: logging::LoggingHandle,
) -> Result<(), anyhow::Error> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).map_err(|e| anyhow::anyhow!(e))?;
        let mut sigint = signal(SignalKind::interrupt()).map_err(|e| anyhow::anyhow!(e))?;

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown...");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
            }
        }
    }

    #[cfg(windows)]
    {
        use tokio::signal;

        let ctrl_c = signal::ctrl_c();
        tokio::select! {
            _ = ctrl_c => {
                info!("Received Ctrl+C, initiating graceful shutdown...");
            }
        }
    }

    // Получаем статистику перед shutdown
    let stats = logging_handle.get_metrics();
    if stats.dropped_messages > 0 {
        warn!(
            dropped_messages = stats.dropped_messages,
            "Some log messages were dropped during execution"
        );
    }

    info!("Shutting down server...");
    if let Err(e) = server.shutdown().await {
        error!("Error during server shutdown: {e}");
    }

    info!("Server shutdown completed, flushing logs...");

    // Graceful logging shutdown с таймаутом
    logging_handle.shutdown_async(Duration::from_secs(5)).await;

    Ok(())
}
