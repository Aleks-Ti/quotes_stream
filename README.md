# quotes_stream

## Сервер катировок

Запуск:

```bash
cargo run --bin server
```

## Клиентский сервер

```bash
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34254 --tickers-file tickers.txt
```
