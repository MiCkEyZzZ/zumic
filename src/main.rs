use zumic::network::server::TcpServer;

fn main() {
    let server = TcpServer::new("127.0.0.1:8080").expect("Failed to start server");
    server.run().expect("Server encountered an error");
}
