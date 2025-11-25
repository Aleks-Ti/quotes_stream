use quotes_stream::BASE_SERVER_TCP_URL;
use std::net::TcpListener;

pub struct TcpClient {}

impl TcpClient {
    pub fn send() {}
}

fn main() {
    let _ = TcpListener::bind(BASE_SERVER_TCP_URL);
}
