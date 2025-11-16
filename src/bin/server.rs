//! Сервер для поставки данных катировок.

#![warn(missing_docs)]
use std::time::{SystemTime, UNIX_EPOCH};

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

impl StockQuote {
    /// Преобразование данных из структуры [`StockQuote`] в строковое представление.
    pub fn to_string(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.ticker, self.price, self.volume, self.timestamp
        )
    }

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

    // pub fn generate_quote(&mut self, ticker: &str) -> Option<StockQuote> {
    //     // ... логика изменения цены ...

    //     let volume = match ticker {
    //         // Популярные акции имеют больший объём, потому умножаем на большее число -> 5000
    //         "AAPL" | "MSFT" | "TSLA" => 1000 + (rand::random::<f64>() * 5000.0) as u32,
    //         // Обычные акции - средний объём
    //         _ => 100 + (rand::random::<f64>() * 1000.0) as u32,
    //     };

    //     Some(StockQuote {
    //         ticker: ticker.to_string(),
    //         price: *last_price,
    //         volume,
    //         timestamp: SystemTime::now()
    //             .duration_since(UNIX_EPOCH)
    //             .unwrap()
    //             .as_millis() as u64,
    //     })
    // }
}

fn main() {}

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
