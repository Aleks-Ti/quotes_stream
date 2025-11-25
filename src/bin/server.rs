//! Сервер для поставки данных катировок.
#![warn(missing_docs)]
use clap::Error;
use crossbeam_channel::{Receiver, Sender};
use quotes_stream::BASE_SERVER_TCP_URL;
use quotes_stream::ParseStreamError;
use quotes_stream::StockQuote;
use rand::Rng;
use std::net::UdpSocket;
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
    tick_recv: Receiver<()>,
) {
    server_client_udp_socket.set_nonblocking(true).expect("nonblocking");

    let mut last_ping = Instant::now();
    let ping_timeout = Duration::from_secs(5);
    let mut buf = [0; 64];

    println!(
        "UDP streamer for {} using local port {}",
        client_udp_addr,
        server_client_udp_socket.local_addr().unwrap().port()
    );
    loop {
        if last_ping.elapsed() > ping_timeout {
            println!("Client {} timed out (no PING)", client_udp_addr);
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

        match tick_recv.try_recv() {
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
        thread::sleep(Duration::from_millis(10));
    }
}

fn tcp_handler(stream: TcpStream, quotes: StockMap, tick_recv: Receiver<()>) {
    let mut writer = stream.try_clone().expect("failed to clone stream");
    let mut reader = BufReader::new(stream);

    let _ = writer.write_all(b"Welcome to the Qutes stream server!\n");
    let _ = writer.flush();
    let mut line = String::new();
    loop {
        line.clear();
        let tick_recv_clone = tick_recv.clone();
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
                        let addr_str = match parts.next() {
                            Some(s) => s,
                            None => {
                                let _ = writeln!(writer, "ERROR: {}", ParseStreamError::MissingAddress);
                                continue;
                            }
                        };

                        let tickers_str = match parts.next() {
                            Some(s) => s,
                            None => {
                                let _ = writeln!(writer, "ERROR: {}", ParseStreamError::MissingTickers);
                                continue;
                            }
                        };

                        if !addr_str.starts_with("udp://") {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::MissingAddress);
                            continue;
                        }
                        let parsed_url = match Url::parse(addr_str) {
                            Ok(url) => url,
                            Err(_) => {
                                let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidUrl);
                                continue;
                            }
                        };
                        if parsed_url.scheme() != "udp" {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidAddress,);
                        }
                        let host = parsed_url.host_str().unwrap_or("127.0.0.1");
                        let port = parsed_url.port().unwrap_or(0);
                        if port == 0 {
                            let _ = writeln!(writer, "ERROR: port is required");
                            continue;
                        }
                        let tickers: Vec<String> = tickers_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        if tickers.is_empty() {
                            let _ = writeln!(writer, "ERROR: {}", ParseStreamError::InvalidTickers);
                            continue;
                        }
                        let addr = format!("{}:{}", host, port);
                        let client_udp_socket = match UdpSocket::bind("0.0.0.0:0") {
                            Ok(sock) => Arc::new(sock),
                            Err(e) => {
                                let _ = writeln!(writer, "ERROR: failed to create UDP socket: {}", e);
                                continue;
                            }
                        };
                        let server_udp_addr = match client_udp_socket.local_addr() {
                            Ok(addr) => addr,
                            Err(e) => {
                                let _ = writeln!(writer, "ERROR: failed to get local addr: {}", e);
                                continue;
                            }
                        };
                        match addr.parse::<std::net::SocketAddr>() {
                            Ok(client_socket_addr) => {
                                let _ = writeln!(writer, "OK: streaming_to: {}, send_PING_to {}", client_socket_addr, server_udp_addr);
                                let quotes_clone = quotes.clone();
                                std::thread::spawn(move || {
                                    udp_streamer(client_socket_addr, tickers, quotes_clone, client_udp_socket, tick_recv_clone);
                                });
                            }
                            Err(_) => {
                                let _ = writeln!(writer, "ERROR: invalid socket address");
                                continue;
                            }
                        }
                    }

                    Some("PING") => {
                        let _ = writeln!(writer, "PONG\n");
                    }
                    _ => {
                        let _ = writeln!(writer, "ERROR: unknown command");
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
    pub fn update_quotes(quotes: StockMap, tick_sender: Sender<()>) {
        loop {
            {
                let mut map = quotes.write().unwrap();
                for (ticker, stock) in map.iter_mut() {
                    stock.generate_quote(ticker);
                }
                // ОТправляем сигнал по каналу, о том что катировки изменились.
                let _ = tick_sender.try_send(());
            }
            println!("update");
            let sleep_ms = rand::thread_rng().gen_range(100..=1000);
            thread::sleep(time::Duration::from_millis(sleep_ms));
        }
    }
}

type StockMap = Arc<RwLock<HashMap<String, StockQuote>>>;

fn main() -> std::io::Result<()> {
    let quotes = Arc::new(RwLock::new(QuoteHandler::base_load_quotes().unwrap()));
    let listener = TcpListener::bind(BASE_SERVER_TCP_URL)?;
    let quotes_for_updater = quotes.clone();
    let (tick_s, tick_r) = crossbeam_channel::bounded::<()>(100);
    let tick_sender = tick_s.clone();
    thread::spawn(move || QuoteHandler::update_quotes(quotes_for_updater, tick_sender));
    for stream in listener.incoming() {
        let tick_recv_clone = tick_r.clone();
        let quotes_for_read = quotes.clone();
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    tcp_handler(stream, quotes_for_read, tick_recv_clone);
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
