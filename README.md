# quotes_stream

## Сервер катировок

Запуск:

```bash
cargo run --bin server
```

TCP клиент для проверки из терминала без клиента:

```bash
nc 127.0.0.1 8080
>> STREAM udp://127.0.0.1:34254 AAPL,TSLA
```

UDP прослушиватель для проверки:

```bash
nc -u -l 34254
nc -u -l 34255
nc -u -l 34256
```

## Клиентский сервер

Запуск клиента:

Вывести примеры команд:

```bash
cargo run --bin client -- --help
```

Запуск множества клиентов:

```bash
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34254 --tickers-file test_tickers_1.txt
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34255 --tickers-file test_tickers_2.txt
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34256 --tickers-file test_tickers_3.txt
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34257 --tickers-file test_tickers_4.txt
```
