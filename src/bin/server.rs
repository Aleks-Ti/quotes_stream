//! Сервер для поставки данных катировок.
#![warn(missing_docs)]
use std::sync::{Arc, RwLock};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{fmt, time};

use clap::Error;

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
        write!(
            f,
            "{}|{}|{}|{}",
            self.ticker, self.price, self.volume, self.timestamp
        )
    }
}

impl StockQuote {
    /// Конструктор.
    pub fn new() -> Self {
        Self {
            ticker: "DEFAULT".to_string(),
            volume: 100_u32,
            price: 100.0_f64,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
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
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        })
    }
}

fn tcp_handler(stream: TcpStream, _: Arc<RwLock<HashMap<String, StockQuote>>>) {
    let mut writer = stream.try_clone().expect("failed to clone stream");
    let mut reader = BufReader::new(stream);

    let _ = writer.write_all(b"Welcome to the Qutes stream server!\n");
    let _ = writer.flush();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                return;
            }
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() {
                    let _ = writer.flush();
                    continue;
                }
                // let mut parts = input.split_whitespace();
                // let response = match parts.next() {};
                // NOTE:сделать реакцию на правильный запрос и создать отдельный поток для клиента со стримом запрошенных катировок
            }
            Err(_) => {
                return;
            }
        }
    }
}

fn base_load_quotes() -> Result<HashMap<String, StockQuote>, Error> {
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
    // for (ticker, quote) in &data {
    //     println!("{:?}: {:?}", ticker, quote);
    // }
    Ok(data)
}

/// Постоянное изменение цен([StockQuote::price]) в отдельном потоке.
fn price_changes(quotes: Arc<RwLock<HashMap<String, StockQuote>>>) {
    loop {
        {
            let mut map = quotes.write().unwrap();
            let mut count = 0;
            for (ticker, stock) in map.iter_mut() {
                stock.generate_quote(ticker);
                count += 1;
            }
            println!("{}", count) // for test[я в ахуе какой быстрый код] -> its blaizing fast!
        }
        println!("update");
        thread::sleep(time::Duration::from_millis(100));
    }
}

fn main() -> std::io::Result<()> {
    let quotes = Arc::new(RwLock::new(base_load_quotes().unwrap()));
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    let quotes_for_updater = quotes.clone();
    thread::spawn(move || price_changes(quotes_for_updater));
    for stream in listener.incoming() {
        let clone_quotes = quotes.clone();
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    tcp_handler(stream, clone_quotes);
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
