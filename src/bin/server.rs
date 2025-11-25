//! Сервер для поставки данных катировок.
#![warn(missing_docs)]
use clap::Error;
use crossbeam_channel::{Receiver, Sender};
use quotes_stream::shared::constants::BASE_SERVER_TCP_URL;
use rand::Rng;
use std::net::UdpSocket;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{fmt, time};
use url::Url;

#[derive(Debug, Clone)]
/// Структура для данных катировок.
pub struct StockQuote {
    /// Катировка -> тикер: уникальное имя ценной бумаги на рынке.
    pub ticker: String,
    /// Объём, количество акций.
    pub volume: u32,
    /// Цена за одну акцию.
    pub price: f64,
    /// Данные актуальны на момент времени -> timestamp.
    pub timestamp: u64,
}

impl Default for StockQuote {
    fn default() -> Self {
        StockQuote::new()
    }
}

impl fmt::Display for StockQuote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}|{}|{}", self.ticker, self.price, self.volume, self.timestamp)
    }
}

impl StockQuote {
    /// Конструктор.
    pub fn new() -> Self {
        Self {
            ticker: "DEFAULT".to_string(),
            volume: 100_u32,
            price: 100.0_f64,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        }
    }
    // Преобразование данных из структуры [`StockQuote`] в строковое представление.
    // pub fn to_string(&self) -> String {
    //     format!(
    //         "{}|{}|{}|{}",
    //         self.ticker, self.price, self.volume, self.timestamp
    //     )
    // }

    /// Преобразование данных из строкового представления в структуру [`StockQuote`].
    /// Пример данных с которыми работает данный метод: [`StockQuote::to_string`].
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('|').collect();
        if parts.len() == 4 {
            Some(StockQuote {
                ticker: parts[0].to_string(),
                price: parts[1].parse().ok()?,
                volume: parts[2].parse().ok()?,
                timestamp: parts[3].parse().ok()?,
            })
        } else {
            None
        }
    }

    /// Бинарная сериализация. Для поставки данных катировок в бинароном виде.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.ticker.as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.price.to_string().as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.volume.to_string().as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.timestamp.to_string().as_bytes());
        bytes
    }

    /// Генерирует произвольные рандомные данные для конкретной катировки.
    pub fn generate_quote(&mut self, ticker: &str) -> Option<StockQuote> {
        let last_price = &mut self.price;
        *last_price += (rand::random::<f64>() - 0.5) * 2.0; // небольшое изменение

        let volume = match ticker {
            // Популярные акции имеют больший объём, потому умножаем на большее число -> 5000
            "AAPL" | "MSFT" | "TSLA" => 1000 + (rand::random::<f64>() * 5000.0) as u32,
            // Обычные акции - средний объём
            _ => 100 + (rand::random::<f64>() * 1000.0) as u32,
        };

        Some(StockQuote {
            ticker: ticker.to_string(),
            price: *last_price,
            volume,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
        })
    }
}

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
                        let line = format!("{}: {}; value: {}; time: {}\n", ticker, quote.price, quote.volume, quote.timestamp);
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
                                let _ = writeln!(writer, "ERROR: missing UDP address. Expected udp://...");
                                continue;
                            }
                        };

                        let tickers_str = match parts.next() {
                            Some(s) => s,
                            None => {
                                let _ = writeln!(
                                    writer,
                                    "ERROR: missing ticker list[AAPL,TSLA]. Expected message -> STREAM udp://127.0.0.1:34254 AAPL,TSLA"
                                );
                                continue;
                            }
                        };

                        if !addr_str.starts_with("udp://") {
                            let _ = writeln!(writer, "ERROR: invalid protocol, expected messege -> STREAM udp://...");
                            continue;
                        }
                        let parsed_url = match Url::parse(addr_str) {
                            Ok(url) => url,
                            Err(_) => {
                                let _ = writeln!(writer, "ERROR: invalid URL format");
                                continue;
                            }
                        };
                        if parsed_url.scheme() != "udp" {
                            let _ = writeln!(
                                writer,
                                "ERROR: invalid protocol, expected message -> STREAM udp://127.0.0.1:34254 AAPL,TSLA"
                            );
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
                            let _ = writeln!(writer, "ERROR: no valid tickers provided");
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
