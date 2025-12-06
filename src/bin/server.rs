//! Сервер для поставки данных катировок.
#![warn(missing_docs)]
use quotes_stream::StockQuote;
use quotes_stream::{BASE_HOST, BASE_TCP_SERVER_PORT, DEFAULT_UDP_PORT, UDP_STREAM_TIMEOUT};
use quotes_stream::{ClientError, ServerError};
use rand::Rng;
use std::io;
use std::io::LineWriter;
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

    tracing::info!(
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

struct TcpHandler {}

impl TcpHandler {
    fn new() -> Self {
        Self {}
    }

    fn validate_client_url(&mut self, addr_str: &str) -> Result<(String, u16), ClientError> {
        let parsed_url = Url::parse(addr_str).map_err(|_| ClientError::InvalidAddress)?;
        if parsed_url.scheme() != "udp" {
            return Err(ClientError::InvalidAddress);
        }
        let host = parsed_url.host_str().ok_or(ClientError::InvalidAddress)?.to_string();
        let port = parsed_url.port().ok_or(ClientError::InvalidPort)?;
        Ok((host, port))
    }

    fn tcp_handler(&mut self, stream: TcpStream, quotes: StockMap, senders: SendersList) -> Result<(), ServerError> {
        let stream_clone = stream.try_clone()?;
        let mut writer = LineWriter::new(stream_clone);
        let mut reader = BufReader::new(stream);

        writeln!(writer, "Welcome!\n")?;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    break;
                }
                Ok(_) => {
                    let input = line.trim();
                    if input.is_empty() {
                        match writer.flush() {
                            Ok(_) => continue,
                            Err(e) => {
                                tracing::error!("ERROR: {}", e);
                                break;
                            }
                        }
                    }
                    let mut parts = input.split_whitespace();
                    match parts.next() {
                        Some("STREAM") => {
                            let addr_str = match parts.next() {
                                Some(s) => s,
                                None => {
                                    writeln!(writer, "ERROR: {}", ClientError::MissingAddress)?;
                                    continue;
                                }
                            };

                            let tickers_str = match parts.next() {
                                Some(s) => s,
                                None => {
                                    writeln!(writer, "ERROR: {}", ClientError::MissingTickers)?;
                                    continue;
                                }
                            };

                            if !addr_str.starts_with("udp://") {
                                writeln!(writer, "ERROR: {}", ClientError::MissingAddress)?;
                                continue;
                            }
                            let (host, port) = match self.validate_client_url(addr_str) {
                                Ok((host, port)) => (host, port),
                                Err(err) => {
                                    writeln!(writer, "ERROR: {}", err)?;
                                    continue;
                                }
                            };

                            let tickers: Vec<String> = tickers_str
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();

                            let map = match quotes.read() {
                                Ok(m) => m,
                                Err(e) => {
                                    writeln!(writer, "ERROR:{}", ServerError::PoisonedLock)?;
                                    tracing::error!("ERROR: can't reed StockQuotes[map]: {}", e);
                                    break;
                                }
                            };

                            if !tickers.iter().all(|ticker| map.contains_key(ticker)) {
                                writeln!(writer, "ERROR: {}", ClientError::InvalidTickers)?;
                                continue;
                            }

                            if tickers.is_empty() {
                                writeln!(writer, "ERROR: {}", ClientError::InvalidTickers)?;
                                continue;
                            }
                            let client_addr = format!("{}:{}", host, port);
                            let udp_socket = match UdpSocket::bind(format!("{}:{}", BASE_HOST, DEFAULT_UDP_PORT)) {
                                Ok(sock) => Arc::new(sock),
                                Err(e) => {
                                    writeln!(writer, "ERROR:{}", ServerError::BadGateway)?;
                                    tracing::error!("ERROR: failed to create UDP socket: {}", e);
                                    break;
                                }
                            };
                            let server_udp_addr: std::net::SocketAddr = match udp_socket.local_addr() {
                                Ok(addr) => addr,
                                Err(e) => {
                                    writeln!(writer, "ERROR:{}", ServerError::BadGateway)?;
                                    tracing::error!("ERROR: failed to get local addr: {}", e);
                                    break;
                                }
                            };
                            let (sender, receiver) = std::sync::mpsc::channel();
                            {
                                match senders.write() {
                                    Ok(mut res) => res.push(sender),
                                    Err(e) => {
                                        writeln!(writer, "ERROR: {}", ServerError::BadGateway)?;
                                        tracing::error!("ERROR: {}", e);
                                        break;
                                    }
                                }
                            }
                            match client_addr.parse::<std::net::SocketAddr>() {
                                Ok(client_socket_addr) => {
                                    writeln!(writer, "OK: send_PING_to {}", server_udp_addr)?;
                                    let quotes_clone = quotes.clone();
                                    std::thread::spawn(move || {
                                        udp_streamer(client_socket_addr, tickers, quotes_clone, udp_socket, receiver);
                                    });
                                }
                                Err(e) => {
                                    writeln!(writer, "ERROR:{}", ServerError::BadGateway)?;
                                    tracing::error!("ERROR: invalid socket address: {}", e);
                                    break;
                                }
                            }
                        }
                        _ => {
                            writeln!(writer, "ERROR: unknown command")?;
                        }
                    };
                }
                Err(e) => {
                    tracing::error!("ERROR: TCP read line from client connect: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }
}
/// Базовый обработчик для работы с [StockQuote]
pub struct QuoteHandler {}

impl QuoteHandler {
    /// Загрузчик default данных для stream катировок.
    pub fn base_load_quotes() -> io::Result<HashMap<String, StockQuote>> {
        let file = fs::File::open("tickers.txt")?;
        let reader = BufReader::new(file);
        let mut data = HashMap::new();
        for line in reader.lines() {
            let line = line?;
            let ticker = line.trim();
            if ticker.is_empty() {
                continue;
            }
            let mut quote = StockQuote::default();
            if let Some(quote) = quote.generate_quote(ticker) {
                data.insert(ticker.to_string(), quote);
            }
        }
        Ok(data)
    }

    /// Постоянное изменение цен([StockQuote::price]) в отдельном потоке.
    pub fn update_quotes(quotes: StockMap, senders: SendersList) {
        loop {
            {
                let mut map = match quotes.write() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        tracing::warn!("Quotes lock poisoned, recovering");
                        poisoned.into_inner()
                    }
                };
                for (ticker, stock) in map.iter_mut() {
                    stock.generate_quote(ticker);
                }
            }

            {
                let senders_guard = match senders.read() {
                    Ok(s) => s,
                    Err(poisoned) => {
                        tracing::warn!("Senders lock poisoned, recovering");
                        poisoned.into_inner()
                    }
                };
                let mut dead_senders = Vec::new();
                for (i, sender) in senders_guard.iter().enumerate() {
                    if sender.send(()).is_err() {
                        dead_senders.push(i);
                    }
                }

                if !dead_senders.is_empty() {
                    let mut list = match senders.write() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            tracing::warn!("Senders write lock poisoned, recovering");
                            poisoned.into_inner()
                        }
                    };
                    for &i in dead_senders.iter().rev() {
                        list.remove(i);
                    }
                }
            }
            let sleep_ms = rand::thread_rng().gen_range(100..=1000);
            thread::sleep(time::Duration::from_millis(sleep_ms));
        }
    }
}

type StockMap = Arc<RwLock<HashMap<String, StockQuote>>>;
type SendersList = Arc<RwLock<Vec<Sender<()>>>>;

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().with_target(false).with_level(true).init();
    tracing::info!(
        "Starting Quotes Stream Server at {}",
        format!("{}:{}", BASE_HOST, BASE_TCP_SERVER_PORT)
    );
    let base_quotes = match QuoteHandler::base_load_quotes() {
        Ok(q) => q,
        Err(e) => {
            tracing::error!("Failed to load tickers file: {}", e);
            return Err(e);
        }
    };
    let quotes = Arc::new(RwLock::new(base_quotes));
    let senders = Arc::new(RwLock::new(Vec::new()));

    let listener = TcpListener::bind(format!("{}:{}", BASE_HOST, BASE_TCP_SERVER_PORT))?;
    tracing::info!("TCP listener bound on {}", format!("{}:{}", BASE_HOST, BASE_TCP_SERVER_PORT));

    let quotes_for_updater = quotes.clone();
    let clone_senders = senders.clone();
    thread::spawn(move || QuoteHandler::update_quotes(quotes_for_updater, clone_senders));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let quotes_for_read = quotes.clone();
                let clone_senders = senders.clone();
                tracing::info!("New TCP connection from {:?}", stream.peer_addr());
                thread::spawn(move || {
                    let _ = TcpHandler::new().tcp_handler(stream, quotes_for_read, clone_senders);
                });
            }
            Err(e) => {
                tracing::error!("Failed to accept TCP connection: {}", e);
            }
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
