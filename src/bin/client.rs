use std::{
    io::Write,
    net::{TcpStream, UdpSocket},
};

use clap::Parser;

#[derive(Parser, Debug, Clone)]
struct Command {
    /// Введите адрес на котором будет работать client. Пример: 127.0.0.1:8001,
    #[arg(short = 'S', long("server-addr"))]
    server_addr: String,
    /// Укажите udp port на котором хотели бы прослушивать стриминг данных.
    #[arg(short = 'U', long("udp-port"))]
    udp_port: String,
    /// Укажите путь до файла с катировками, для которых хотели бы получать данные.
    /// Формат данных в файле: path/to/tickers.txt => AAPL\nMSFT\nGOOGL\nAMZN => каждый фильтр с новой строки.
    #[arg(short = 'T', long("tickers-file"))]
    tickers_path: String,
}

pub struct Client {}

impl Client {
    fn handler(&self, address: String, tickers: Vec<String>, udp_port: String) -> Result<(), std::io::Error> {
        let mut tcp_stream = self.create_tcp_connect(&address)?;
        tcp_stream.write_all(b"PING\n")?;
        tcp_stream.flush()?;
        // let mut buf = [0; 1024];
        // let n = tcp_stream.read(&mut buf)?;
        // let response = std::str::from_utf8(&buf[..n])?;
        // if response != "PONG" { /* ошибка */ }
        let udp_stream: UdpSocket = self.create_upd_socket(&udp_port)?;
        let local_addr = udp_stream.local_addr()?;
        println!("Sending command: STREAM udp://{} {}", local_addr, tickers.join(","));
        let thread = std::thread::spawn(move || Self {}.udp_listener(udp_stream));
        tcp_stream.write_all(format!("STREAM udp://{} {}\n", local_addr, tickers.join(",")).as_bytes())?;
        thread.join().unwrap();
        Ok(())
    }
    fn create_upd_socket(&self, port: &String) -> Result<UdpSocket, std::io::Error> {
        let client_udp_socket = UdpSocket::bind(format!("127.0.0.1:{}", port))?;
        Ok(client_udp_socket)
    }
    fn create_tcp_connect(&self, address: &String) -> Result<TcpStream, std::io::Error> {
        let tcp_stream = TcpStream::connect(address)?;
        Ok(tcp_stream)
    }
    fn udp_listener(&self, udp_socket: UdpSocket) {
        let mut buf = [0; 65536]; // макс. размер UDP-датаграммы
        loop {
            match udp_socket.recv(&mut buf) {
                Ok(n) => match std::str::from_utf8(&buf[..n]) {
                    Ok(msg) => println!("{}", msg),
                    Err(e) => eprintln!("Received invalid UTF-8 data: {:?}", e),
                },
                Err(e) => {
                    eprintln!("UDP receive error: {}", e);
                    break;
                }
            }
        }
    }
}

// cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34254 --tickers-file test_tickers.txt
fn main() -> std::io::Result<()> {
    let args = Command::parse();
    let data = std::fs::read_to_string(&args.tickers_path)?;
    let tickers = data
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let thread_handle = std::thread::spawn(move || Client {}.handler(args.server_addr, tickers, args.udp_port));
    let _ = thread_handle.join().unwrap();
    Ok(())
}
