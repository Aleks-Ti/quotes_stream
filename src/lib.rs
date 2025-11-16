//! Модуль для взаимодействия между клиентом и сервером

#![warn(missing_docs)]

/// Тестовая дефолтная функция.
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
