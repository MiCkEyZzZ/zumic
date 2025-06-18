use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::{Sds, StorageEngine, Value};

pub async fn handle_connection(
    socket: TcpStream,
    engine: Arc<StorageEngine>,
) -> anyhow::Result<()> {
    let mut lines = BufReader::new(socket).lines();

    while let Some(line) = lines.next_line().await? {
        let parts: Vec<&str> = line.trim_end().splitn(3, ' ').collect();
        let response = match parts.as_slice() {
            ["SET", key, val] => {
                let k = Sds::from(key.as_bytes());
                let v = Value::Str(Sds::from(val.as_bytes()));
                engine
                    .set(&k, v)
                    .map(|_| "+OK\r\n".to_string())
                    .unwrap_or("-ERR set failed\r\n".to_string())
            }
            ["GET", key] => {
                let k = Sds::from(key.as_bytes());
                match engine.get(&k) {
                    Ok(Some(Value::Str(s))) => String::from_utf8(s.to_vec())
                        .map(|s| format!("+{}\r\n", s))
                        .unwrap_or("-ERR utf8\r\n".to_string()),
                    Ok(Some(_)) => "-ERR unsupported type\r\n".to_string(),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(_) => "-ERR get failed\r\n".to_string(),
                }
            }
            ["DEL", key] => {
                let k = Sds::from(key.as_bytes());
                match engine.del(&k) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(_) => "-ERR del failed\r\n".to_string(),
                }
            }
            _ => "-ERR unknown command\r\n".to_string(),
        };

        let socket = lines.get_mut();
        socket.write_all(response.as_bytes()).await?;
    }

    Ok(())
}
