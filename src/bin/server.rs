//! Сервер для поставки данных катировок.
#![warn(missing_docs)]
use clap::Error;
use quotes_stream::ParseStreamError;
use quotes_stream::StockQuote;
use quotes_stream::{BASE_SERVER_TCP_URL, UDP_STREAM_TIMEOUT};
use rand::Rng;
use std::io::BufWriter;
use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time;
use std::time::{Duration, Instant};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
};
use url::Url;

fn udp_streamer(
    client_udp_addr: std::net::SocketAddr,
    tickers: Vec<String>,
    quotes: StockMap,
    server_client_udp_socket: Arc<UdpSocket>,
    receiver: Receiver<()>,
) {
    server_client_udp_socket.set_nonblocking(true).expect("nonblocking");

    let mut last_ping = Instant::now();
    let ping_timeout = Duration::from_secs(UDP_STREAM_TIMEOUT);
    let mut buf = [0; 64];

    println!(
        "UDP streamer for {} using local port {}",
        client_udp_addr,
        server_client_udp_socket.local_addr().unwrap().port()
    );
    loop {
        if last_ping.elapsed() > ping_timeout {
            break;
        }

        match server_client_udp_socket.recv_from(&mut buf) {
            Ok((size, src)) => {
                if src != client_udp_addr {
                    continue;
                }
                let msg = std::str::from_utf8(&buf[..size]).unwrap_or("").trim();
                if msg == "PING" {
                    last_ping = Instant::now();
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("UDP recv error: {}", e);
                break;
            }
        }

        match receiver.recv() {
            Ok(()) => {
                let map = match quotes.read() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let mut lines = Vec::new();
                for ticker in &tickers {
                    if let Some(quote) = map.get(ticker) {
                        let line = quote.to_string();
                        lines.push(line);
                    }
                }
                if !lines.is_empty() {
                    let msg = lines.join("\n") + "\n";
                    if let Err(e) = server_client_udp_socket.send_to(msg.as_bytes(), client_udp_addr) {
                        eprintln!("UDP send error to {}: {}", client_udp_addr, e);
                        break;
                    }
                }
            }
            Err(_) => {
                break;
            }
        }
    }
}

fn tcp_handler(stream: TcpStream, quotes: StockMap, senders: SendersList) {
    let stream_clone = stream.try_clone().expect("failed to clone stream");
    let mut writer = BufWriter::new(stream_clone);
    let mut reader = BufReader::new(stream);

    let _ = writer.write_all(b"Welcome to the Quotes stream server!\n");
    let _ = writer.flush();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                continue;
            }
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() {
                    let _ = writer.flush();
                    continue;
                }

                let mut parts = input.split_whitespace();
                match parts.next() {
                    Some("STREAM") => {
                        println!("STREAM");
                        let addr_str = match parts.next() {
                            Some(s) => s,
                            None => {
                                let _ = writeln!(writer, "ERROR: {}", ParseStreamError::MissingAddress);
                                writer.flush().unwrap();
                                continue;
                            }
                        };

                        let tickers_str = match parts.next() {
                            Some(s) => s,
                            None => {
                                let _ = writeln!(writer, "ERROR: {}", ParseStreamError::MissingTickers);
                                writer.flush().unwrap();
                                continue;
                            }
                        };

                        if !addr_str.starts_with("udp://") {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::MissingAddress);
                            writer.flush().unwrap();
                            continue;
                        }
                        let parsed_url = match Url::parse(addr_str) {
                            Ok(url) => url,
                            Err(_) => {
                                let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidUrl);
                                writer.flush().unwrap();
                                continue;
                            }
                        };
                        if parsed_url.scheme() != "udp" {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidAddress);
                            writer.flush().unwrap();
                            continue;
                        }
                        let host = parsed_url.host_str().unwrap_or("127.0.0.1");
                        let port = parsed_url.port().unwrap_or(0);
                        if port == 0 {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidPort);
                            writer.flush().unwrap();
                            continue;
                        }
                        let tickers: Vec<String> = tickers_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        let map = match quotes.read() {
                            Ok(m) => m,
                            Err(_) => return,
                        };

                        if !tickers.iter().all(|ticker| map.contains_key(ticker)) {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidTickers);
                            writer.flush().unwrap();
                            continue;
                        }

                        if tickers.is_empty() {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidTickers);
                            writer.flush().unwrap();
                            continue;
                        }
                        let addr = format!("{}:{}", host, port);
                        let client_udp_socket = match UdpSocket::bind("127.0.0.1:0") {
                            Ok(sock) => Arc::new(sock),
                            Err(e) => {
                                let _ = writeln!(writer, "ERROR: failed to create UDP socket: {}", e);
                                writer.flush().unwrap();
                                return;
                            }
                        };
                        let server_udp_addr: std::net::SocketAddr = match client_udp_socket.local_addr() {
                            Ok(addr) => addr,
                            Err(e) => {
                                let _ = writeln!(writer, "ERROR: failed to get local addr: {}", e);
                                writer.flush().unwrap();
                                return;
                            }
                        };
                        let (sender, receiver) = std::sync::mpsc::channel();
                        {
                            senders.write().unwrap().push(sender);
                        }
                        match addr.parse::<std::net::SocketAddr>() {
                            Ok(client_socket_addr) => {
                                let _ = writeln!(writer, "OK: send_PING_to {}", server_udp_addr);
                                writer.flush().unwrap();
                                let quotes_clone = quotes.clone();
                                std::thread::spawn(move || {
                                    udp_streamer(client_socket_addr, tickers, quotes_clone, client_udp_socket, receiver);
                                });
                            }
                            Err(_) => {
                                let _ = writeln!(writer, "ERROR: invalid socket address");
                                return;
                            }
                        }
                    }

                    Some("PING") => {
                        println!("PING");
                        let _ = writeln!(writer, "PONG\n");
                        writer.flush().unwrap();
                    }
                    _ => {
                        let _ = writeln!(writer, "ERROR: unknown command");
                        writer.flush().unwrap();
                    }
                };
            }
            Err(_) => {
                return;
            }
        }
    }
}

/// Базовый обработчик для работы со [StockQuote]
pub struct QuoteHandler {}

impl QuoteHandler {
    /// Загрузчик default данных для stream катировок.
    pub fn base_load_quotes() -> Result<HashMap<String, StockQuote>, Error> {
        let mut data = HashMap::new();
        let file_with_all_quotes = fs::File::open("tickers.txt");
        let file_reader = BufReader::new(file_with_all_quotes.unwrap());
        for line in file_reader.lines() {
            let line_string = line.expect("Ошибка чтения строки из файла");
            let mut default_quotes = StockQuote::new();
            if let Some(quote) = default_quotes.generate_quote(&line_string) {
                data.insert(line_string, quote);
            }
        }
        Ok(data)
    }

    /// Постоянное изменение цен([StockQuote::price]) в отдельном потоке.
    pub fn update_quotes(quotes: StockMap, senders: SendersList) {
        loop {
            {
                let mut map = quotes.write().unwrap();
                for (ticker, stock) in map.iter_mut() {
                    stock.generate_quote(ticker);
                }
                let mut dead_senders = Vec::new();
                for (i, sender) in senders.read().unwrap().iter().enumerate() {
                    if sender.send(()).is_err() {
                        dead_senders.push(i);
                    }
                }

                if !dead_senders.is_empty() {
                    let mut list = senders.write().unwrap();
                    for &i in dead_senders.iter().rev() {
                        list.remove(i);
                    }
                }
            }
            let sleep_ms = rand::thread_rng().gen_range(10..=100);
            thread::sleep(time::Duration::from_millis(sleep_ms));
        }
    }
}

type StockMap = Arc<RwLock<HashMap<String, StockQuote>>>;
type SendersList = Arc<RwLock<Vec<Sender<()>>>>;

fn main() -> std::io::Result<()> {
    let quotes = Arc::new(RwLock::new(QuoteHandler::base_load_quotes().unwrap()));
    let senders = Arc::new(RwLock::new(Vec::new()));
    let listener = TcpListener::bind(BASE_SERVER_TCP_URL)?;
    let quotes_for_updater = quotes.clone();
    let clone_senders = senders.clone();
    thread::spawn(move || QuoteHandler::update_quotes(quotes_for_updater, clone_senders));
    for stream in listener.incoming() {
        let clone_senders = senders.clone();
        let quotes_for_read = quotes.clone();
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    tcp_handler(stream, quotes_for_read, clone_senders);
                });
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        let quotes_struct = StockQuote {
            ticker: "AAPL".to_string(),
            volume: 325_u32,
            price: 0.123_f64,
            timestamp: 3486970,
        };
        let str_result = quotes_struct.to_string();
        assert_eq!(str_result, "AAPL|0.123|325|3486970".to_string());
    }
}
