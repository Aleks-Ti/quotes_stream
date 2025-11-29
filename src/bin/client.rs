//! Клиент для получения стриминговых данных о котировках акций по TCP и UDP.

#![warn(missing_docs)]
use clap::Parser;
use quotes_stream::StockQuote;
use std::io::{BufRead, Write};
use std::{
    io::BufReader,
    net::{TcpStream, UdpSocket},
    sync::Arc,
};
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

/// TCP клиент для общения с сервером.
pub struct TcpClient {}

impl TcpClient {
    fn new() -> Self {
        Self {}
    }
    fn send_cmd_and_read(&self, writer: &mut dyn Write, reader: &mut dyn BufRead, cmd: &str) -> std::io::Result<String> {
        writer.write_all(cmd.as_bytes())?;
        writer.flush()?;

        let mut line = String::new();
        reader.read_line(&mut line)?;
        Ok(line)
    }

    fn ping(&self, reader: &mut dyn BufRead, writer: &mut dyn Write) -> std::io::Result<()> {
        let response = self.send_cmd_and_read(writer, reader, "PING\n")?;
        match response.trim() {
            "PONG" => {
                println!("Received PONG");
                Ok(())
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unexpected PONG")),
        }
    }

    fn handler(&self, address: String, tickers: Vec<String>, udp_port: String) -> Result<(), std::io::Error> {
        let tcp_stream = self.create_tcp_connect(&address)?;
        let mut writer = tcp_stream.try_clone()?;
        let mut line = String::new();
        let mut reader = BufReader::new(tcp_stream);
        let udp_client = Arc::new(UpdClient::new());
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    continue;
                }
                Ok(_) => {
                    if let Err(e) = self.ping(&mut reader, &mut writer) {
                        eprintln!("PING failed: {}", e);
                        continue;
                    }
                    let stream = udp_client.create_upd_socket(&udp_port)?;
                    let udp_stream: Arc<UdpSocket> = Arc::new(stream);
                    let local_addr = udp_stream.local_addr()?;

                    let cmd: String = format!("STREAM udp://{} {}\n", local_addr, tickers.join(","));
                    let response = self.send_cmd_and_read(&mut writer, &mut reader, &cmd)?;

                    let clean = if response.trim().is_empty() {
                        let mut second = String::new();
                        reader.read_line(&mut second)?;
                        second.trim().to_string()
                    } else {
                        response.trim().to_string()
                    };

                    let target_addr = match clean.strip_prefix("OK: send_PING_to ") {
                        Some(addr) => addr.to_string(),
                        None => {
                            eprintln!("Invalid STREAM response: {}", clean);
                            continue;
                        }
                    };

                    let udp_client_clone = udp_client.clone();
                    let udp_stream_clone = udp_stream.clone();
                    let thread = std::thread::spawn(move || udp_client_clone.listener(udp_stream_clone));

                    let udp_client_clone2 = udp_client.clone();
                    let udp_stream_clone2 = udp_stream.clone();
                    let _ = std::thread::spawn(move || udp_client_clone2.keep_alive(udp_stream_clone2, target_addr));

                    thread.join().unwrap();
                }
                Err(e) => {
                    eprintln!("TCP read error: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    fn create_tcp_connect(&self, address: &String) -> Result<TcpStream, std::io::Error> {
        let tcp_stream = TcpStream::connect(address)?;
        Ok(tcp_stream)
    }
}

struct UpdClient {}

impl UpdClient {
    fn new() -> Self {
        Self {}
    }

    fn listener(&self, udp_socket: Arc<UdpSocket>) {
        let mut buf = [0; 65536]; // макс. размер UDP-датаграммы
        loop {
            match udp_socket.recv(&mut buf) {
                Ok(n) => match std::str::from_utf8(&buf[..n]) {
                    Ok(msg) => {
                        for line in msg.lines() {
                            let stock = StockQuote::from_string(line).unwrap();
                            println!("{}", stock.to_json());
                        }
                    }
                    Err(e) => eprintln!("Received invalid UTF-8 data: {:?}", e),
                },
                Err(e) => {
                    eprintln!("UDP receive error: {}", e);
                    break;
                }
            }
        }
    }

    fn keep_alive(&self, udp_socket: Arc<UdpSocket>, target_addr: String) {
        udp_socket.connect(&target_addr).expect("Failed to connect UDP socket");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let Err(e) = udp_socket.send(b"PING") {
                eprintln!("Failed to send KEEPALIVE: {}", e);
                break;
            }
        }
    }
    fn create_upd_socket(&self, port: &String) -> Result<UdpSocket, std::io::Error> {
        let client_udp_socket = UdpSocket::bind(format!("127.0.0.1:{}", port))?;
        Ok(client_udp_socket)
    }
}

fn main() -> std::io::Result<()> {
    let args = Command::parse();
    let data = std::fs::read_to_string(&args.tickers_path)?;
    let tickers = data
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let tcp_client = TcpClient::new();
    let thread_handle = std::thread::spawn(move || tcp_client.handler(args.server_addr, tickers, args.udp_port));
    let _ = thread_handle.join().unwrap();
    Ok(())
}
