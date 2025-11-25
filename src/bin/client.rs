use clap::Parser;
pub struct TcpClient {}

impl TcpClient {
    pub fn send() {}
}

#[derive(Parser, Debug, Clone)]
struct Command {
    /// Введите адрес на котором будет работать client. Пример: 127.0.0.1:8080,
    #[arg(short = 'S', long("server-addr"))]
    server_addr: String,
    /// Укажите udp port на котором хотели бы прослушивать стриминг данных.
    #[arg(short = 'U', long("udp-port"))]
    udp_port: String,
    /// Укажите путь до файла с катировками, для которых хотели бы получать данные.
    /// Формат данных в файле: path/to/tickers.txt => AAPL\nMSFT\nGOOGL\nAMZN => каждый фильтр с новой строки.
    #[arg(short = 'T', long("tickers-file"))]
    tickers_path: String,
}

// cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 34254 --tickers-file test_tickers.txt
fn main() -> std::io::Result<()> {
    let args = Command::parse();
    let data = std::fs::read_to_string(&args.tickers_path)?;
    let tickers = data
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    println!("Tickers: {:?}", tickers);
    println!("Joined: {}", tickers.join(","));
    Ok(())
}
