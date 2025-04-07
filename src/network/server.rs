use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub struct TcpServer {
    listener: TcpListener,
}

impl TcpServer {
    pub async fn new(address: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(address).await?;
        Ok(Self { listener })
    }

    pub async fn run(&self) {
        loop {
            let (stream, _) = self.listener.accept().await.unwrap();
            tokio::spawn(Self::handle_connection(stream));
        }
    }

    async fn handle_connection(mut stream: TcpStream) {
        let mut buffer = [0; 1024];

        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = stream.write_all(&buffer[..n]).await {
                        eprintln!("Write error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    break;
                }
            }
        }
    }
}
