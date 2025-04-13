use std::io::Cursor;

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::zsp::frame::decoder::ZSPDecoder;
use crate::network::zsp::frame::zsp_types::ZSPFrame;

const BUFFER_CAPACITY: usize = 4096;

pub async fn run_tcp_server(addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    println!("Listening on {}", addr);

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Accepted connection from {}", addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}

async fn handle_connection(mut socket: TcpStream) -> anyhow::Result<()> {
    let mut decoder = ZSPDecoder::new();
    let mut buffer = BytesMut::with_capacity(BUFFER_CAPACITY);

    loop {
        let mut temp_buf = [0u8; BUFFER_CAPACITY];
        let n = socket.read(&mut temp_buf).await?;

        if n == 0 {
            println!("Connection closed");
            return Ok(());
        }

        buffer.extend_from_slice(&temp_buf[..n]);

        let mut cursor = Cursor::new(&buffer[..]);

        while let Ok(Some(frame)) = decoder.decode(&mut cursor) {
            println!("Received frame: {:?}", frame);

            let response = match &frame {
                ZSPFrame::SimpleString(s) => format!("+{}\r\n", s),
                ZSPFrame::Integer(i) => format!(":{}\r\n", i),
                ZSPFrame::FrameError(e) => format!("-{}\r\n", e),
                _ => "+OK\r\n".to_string(),
            };

            socket.write_all(response.as_bytes()).await?;
        }

        let pos = cursor.position() as usize;
        buffer.advance(pos);
    }
}
