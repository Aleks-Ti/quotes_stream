//! Share модуль

#![warn(missing_docs)]
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Адрес для прослушивания [server.rs]
pub const BASE_SERVER_TCP_URL: &str = "127.0.0.1:8080";
/// Адрес общения с сервером [client.rs]
pub const UDP_STREAM_TIMEOUT: u64 = 5;

/// Ошибки клиенского запроса.
#[derive(Debug)]
pub enum ParseStreamError {
    /// Отсутствует указатель но адрес udp:// для стриминга данных.
    MissingAddress,
    /// Отсутствуют Tickers для стриминга.
    MissingTickers,
    /// Не корректный url адрес.
    InvalidUrl,
    /// Не корректный представленный адрес для стриминга.
    InvalidAddress,
    /// Не корректные Tickers.
    InvalidTickers,
    /// Не корректно указанный port для прослушивания.
    InvalidPort,
}

impl std::fmt::Display for ParseStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseStreamError::MissingAddress => write!(f, "missing UDP address. Expected udp://..."),
            ParseStreamError::MissingTickers => {
                write!(f, "missing ticker list. Expected message -> STREAM udp://127.0.0.1:34254 AAPL,TSLA")
            }
            ParseStreamError::InvalidUrl => write!(f, "invalid URL format"),
            ParseStreamError::InvalidAddress => write!(f, "invalid socket address"),
            ParseStreamError::InvalidTickers => write!(f, "no valid tickers provided"),
            ParseStreamError::InvalidPort => write!(f, "no valid port"),
        }
    }
}

#[derive(Debug, Clone)]
/// Структура для данных катировок.
pub struct StockQuote {
    /// Катировка -> тикер: уникальное имя ценной бумаги на рынке.
    pub ticker: String,
    /// Объём, количество акций.
    pub volume: u32,
    /// Цена за одну акцию.
    pub price: f64,
    /// Время сделки -> timestamp.
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
