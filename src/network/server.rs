use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub struct TcpServer {
    listener: TcpListener,
}

impl TcpServer {
    pub fn new(address: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        Ok(Self { listener })
    }

    pub fn run(&self) -> io::Result<()> {
        println!("Server is running...");

        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        if let Err(e) = handle_client(stream) {
                            eprintln!("Error handling client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e)
                }
            }
        }
        Ok(())
    }
}

fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    let mut buffer = [0; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                stream.write_all(&buffer[0..n])?;
            }
            Err(e) => {
                eprintln!("Error reading from client: {}", e);
                break;
            }
        }
    }
    println!("Connection closed.");
    Ok(())
}
