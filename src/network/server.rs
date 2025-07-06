use std::{io::ErrorKind, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

use crate::{Sds, StorageEngine, Value};

pub async fn handle_connection(
    socket: TcpStream,
    engine: Arc<StorageEngine>,
) -> anyhow::Result<()> {
    let mut lines = BufReader::new(socket).lines();

    loop {
        // Пробуем прочитать строку
        let line_opt = match lines.next_line().await {
            Ok(Some(line)) => Some(line),
            Ok(None) => {
                // Клиент закрыл соединение чисто - выходим
                break;
            }
            Err(e)
                if e.kind() == ErrorKind::InvalidData
                    || e.kind() == ErrorKind::UnexpectedEof
                    || e.kind() == ErrorKind::BrokenPipe =>
            {
                // Неправильный UTF-8 или разрыв канала — просто выходим без паники
                break;
            }
            Err(e) => {
                // Другая ошибка — пробрасываем
                return Err(e.into());
            }
        };

        let line = line_opt.unwrap();
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let response = match parts[0] {
            "SET" if parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let v = Value::Str(Sds::from(parts[2].as_bytes()));
                engine
                    .set(&k, v)
                    .map(|_| "+OK\r\n".to_string())
                    .unwrap_or("-ERR set failed\r\n".to_string())
            }
            "GET" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.get(&k) {
                    Ok(Some(Value::Str(s))) => String::from_utf8(s.to_vec())
                        .map(|s| format!("+{s}\r\n"))
                        .unwrap_or("-ERR utf8\r\n".to_string()),
                    Ok(Some(_)) => "-ERR unsupported type\r\n".to_string(),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(_) => "-ERR get failed\r\n".to_string(),
                }
            }
            "DEL" if parts.len() == 2 => {
                let k = Sds::from(parts[1].as_bytes());
                match engine.del(&k) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(_) => "-ERR del failed\r\n".to_string(),
                }
            }
            "MSET" if parts.len() > 1 => {
                let args = &parts[1..];
                if !args.len().is_multiple_of(2) {
                    "-ERR wrong number of arguments for MSET\r\n".to_string()
                } else {
                    let mut ok = true;
                    for chunk in args.chunks(2) {
                        let k = Sds::from(chunk[0].as_bytes());
                        let v = Value::Str(Sds::from(chunk[1].as_bytes()));
                        if engine.set(&k, v).is_err() {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        "+OK\r\n".to_string()
                    } else {
                        "-ERR mset failed\r\n".to_string()
                    }
                }
            }
            "MGET" if parts.len() > 1 => {
                let sds_keys: Vec<Sds> = parts[1..]
                    .iter()
                    .map(|&k| Sds::from(k.as_bytes()))
                    .collect();
                let refs: Vec<&Sds> = sds_keys.iter().collect();
                match engine.mget(&refs) {
                    Ok(vals) => {
                        let mut resp = format!("*{}\r\n", vals.len());
                        for opt in vals {
                            if let Some(Value::Str(s)) = opt {
                                let b = s.as_bytes();
                                resp += &format!("${}\r\n", b.len());
                                resp += &String::from_utf8_lossy(b);
                                resp += "\r\n";
                            } else {
                                resp += "$-1\r\n";
                            }
                        }
                        resp
                    }
                    Err(_) => "-ERR mget failed\r\n".to_string(),
                }
            }
            "GEOADD" if parts.len() == 5 => {
                let k = Sds::from(parts[1].as_bytes());
                let lon: f64 = parts[2].parse().unwrap_or(0.0);
                let lat: f64 = parts[3].parse().unwrap_or(0.0);
                let m = Sds::from(parts[4].as_bytes());
                match engine.geo_add(&k, lon, lat, &m) {
                    Ok(true) => ":1\r\n".to_string(),
                    Ok(false) => ":0\r\n".to_string(),
                    Err(_) => "-ERR geoadd failed\r\n".to_string(),
                }
            }
            "GEOPOS" if parts.len() == 3 => {
                let k = Sds::from(parts[1].as_bytes());
                let m = Sds::from(parts[2].as_bytes());
                match engine.geo_pos(&k, &m) {
                    Ok(Some(pt)) => format!(
                        "*2\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
                        pt.lon.to_string().len(),
                        pt.lon,
                        pt.lat.to_string().len(),
                        pt.lat
                    ),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(_) => "-ERR geopos failed\r\n".to_string(),
                }
            }
            "GEODIST" if parts.len() == 4 || parts.len() == 5 => {
                let k = Sds::from(parts[1].as_bytes());
                let m1 = Sds::from(parts[2].as_bytes());
                let m2 = Sds::from(parts[3].as_bytes());
                let unit = parts.get(4).copied().unwrap_or("m");
                match engine.geo_dist(&k, &m1, &m2, unit) {
                    Ok(Some(d)) => format!("+{d}\r\n"),
                    Ok(None) => "$-1\r\n".to_string(),
                    Err(_) => "-ERR geodist failed\r\n".to_string(),
                }
            }
            _ => "-ERR unknown command\r\n".to_string(),
        };

        let socket = lines.get_mut();
        socket.write_all(response.as_bytes()).await?;
    }

    Ok(())
}
