# quotes_stream

## Сервер катировок

Запуск:

```bash
cargo run --bin server
```

## Клиентский сервер

```bash
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34254 --tickers-file test_tickers.txt
```

```bash
# прослушиватель
nc -u -l 34254
nc -u -l 34255
```

```bash
STREAM udp://127.0.0.1:34254 AAPL,TSLA
STREAM udp://127.0.0.1:34255 AAPL,PGR
```
