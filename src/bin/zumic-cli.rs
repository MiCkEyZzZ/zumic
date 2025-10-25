//! CLI клиент Zumic
//!
//! Клиент командной строки для взаимодействия с сервером Zumic.
//! Поддерживает интерактивный режим (REPL), одиночные команды,
//! получение информации о сервере, мониторинг и бенчмарки.

use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use tracing::debug;
use zumic::{
    client::{ClientConfig, ZumicClient},
    Value as ZumicValue,
};

/// Основная структура CLI аргументов
#[derive(Parser)]
#[command(name = "zumic-cli")]
#[command(author = "Zumic Contributors")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Zumic CLI - Command line client for Zumic server", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Хост сервера (IP или доменное имя)
    #[arg(
        short = 'H',
        long,
        default_value = "127.0.0.1",
        env = "ZUMIC_HOST",
        help = "Хост сервера для подключения"
    )]
    host: String,
    /// Порт сервера
    #[arg(
        short,
        long,
        default_value = "6174",
        env = "ZUMIC_PORT",
        help = "Порт сервера для подключения"
    )]
    port: u16,
    /// Пароль для аутентификации
    #[arg(
        short,
        long,
        env = "ZUMIC_PASSWORD",
        help = "Пароль для аутентификации (можно использовать переменную окружения ZUMIC_PASSWORD)"
    )]
    auth: Option<String>,
    /// Таймаут соединения в секундах
    #[arg(long, default_value = "5", help = "Таймаут соединения в секундах")]
    timeout: u64,
    /// Таймаут чтения данных в секундах
    #[arg(
        long,
        default_value = "30",
        help = "Таймаут ожидания ответа сервера в секундах"
    )]
    read_timeout: u64,
    /// Таймаут записи данных в секундах
    #[arg(
        long,
        default_value = "10",
        help = "Таймаут отправки команды серверу в секундах"
    )]
    write_timeout: u64,
    /// Включить подробный вывод (debug)
    #[arg(short, long, help = "Включить подробный вывод для отладки")]
    verbose: bool,
    /// Подавить большинство логов (только warn/error)
    #[arg(short = 'q', long, help = "Подавить логирование (только warn/error)")]
    quiet: bool,
    /// Формат вывода результатов
    #[arg(
        long,
        value_enum,
        default_value = "pretty",
        help = "Формат вывода ответа сервера"
    )]
    output: OutputFormat,
    /// Подкоманда для выполнения
    #[command(subcommand)]
    command: Option<Commands>,
    /// Прямое выполнение команды (например: GET key)
    #[arg(help = "Прямая команда для выполнения (например, 'GET key' или 'SET key value')")]
    args: Vec<String>,
}

/// Формат вывода CLI
#[derive(Clone, Debug, clap::ValueEnum)]
enum OutputFormat {
    /// Человекочитаемый формат
    Pretty,
    /// Сырой протокольный вывод
    Raw,
    /// JSON формат
    Json,
}

/// Подкоманды CLI
#[derive(Subcommand)]
enum Commands {
    /// Интерактивный режим (REPL)
    #[command(alias = "i")]
    Interactive {
        /// Путь к файлу истории команд
        #[arg(
            long,
            default_value = "~/.zumic_history",
            help = "Файл для сохранения истории команд"
        )]
        history: String,
    },
    /// Выполнить одну команду и выйти
    #[command(alias = "e")]
    Exec {
        /// Команда с аргументами
        #[arg(required = true, help = "Команда для выполнения (например, 'GET key')")]
        args: Vec<String>,
    },
    /// Проверка соединения с сервером
    Ping {
        /// Количество пингов
        #[arg(
            short = 'c',
            long,
            default_value = "1",
            help = "Количество пингов для отправки"
        )]
        count: u32,

        /// Интервал между пингами (мс)
        #[arg(
            short,
            long,
            default_value = "1000",
            help = "Интервал между пингами в миллисекундах"
        )]
        interval: u64,
    },
    /// Получить информацию о сервере
    #[command(alias = "status")]
    Info {
        /// Раздел информации (например: server, memory, stats)
        #[arg(help = "Название раздела информации (например, 'server', 'memory', 'stats')")]
        section: Option<String>,
    },
    /// Режим мониторинга команд (реaltime)
    Monitor,
    /// Запуск бенчмарка
    #[command(alias = "bench")]
    Benchmark {
        /// Количество запросов
        #[arg(
            short = 'n',
            long,
            default_value = "100000",
            help = "Количество запросов для теста"
        )]
        requests: usize,
        /// Количество параллельных клиентов
        #[arg(
            short = 'c',
            long,
            default_value = "50",
            help = "Количество параллельных клиентов"
        )]
        clients: usize,
        /// Тестируемые команды (например: SET,GET)
        #[arg(
            short = 't',
            long,
            default_value = "SET,GET",
            help = "Список команд для тестирования"
        )]
        tests: String,
    },
}

/// Конфигурация CLI после разбора аргументов
#[derive(Debug, Clone)]
struct CliConfig {
    server_addr: SocketAddr,
    client_config: ClientConfig,
    output_format: OutputFormat,
}

impl TryFrom<&Cli> for CliConfig {
    type Error = anyhow::Error;

    fn try_from(cli: &Cli) -> Result<Self> {
        let server_addr: SocketAddr = format!("{}:{}", cli.host, cli.port)
            .parse()
            .context("Неверный формат адреса сервера")?;

        let client_config = ClientConfig {
            connect_timeout: Duration::from_secs(cli.timeout),
            read_timeout: Duration::from_secs(cli.read_timeout),
            write_timeout: Duration::from_secs(cli.write_timeout),
            password: cli.auth.clone(),
            username: None,
        };

        Ok(Self {
            server_addr,
            client_config,
            output_format: cli.output.clone(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logging(cli.verbose, cli.quiet)?;

    let should_print_banner = matches!(cli.command, Some(Commands::Interactive { .. }));
    if should_print_banner {
        print_banner();
    }

    let config = CliConfig::try_from(&cli)?;
    debug!("Конфигурация CLI: {config:?}");

    match handle_command(&cli, &config).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

async fn handle_command(
    cli: &Cli,
    config: &CliConfig,
) -> Result<()> {
    match &cli.command {
        Some(Commands::Interactive { history }) => interactive_mode(config, history).await,
        Some(Commands::Exec { args }) => execute_command(config, args).await,
        Some(Commands::Ping { count, interval }) => {
            ping_server(config, *count, Duration::from_millis(*interval)).await
        }
        Some(Commands::Info { section }) => get_server_info(config, section.as_deref()).await,
        Some(Commands::Monitor) => monitor_mode(config).await,
        Some(Commands::Benchmark {
            requests,
            clients,
            tests,
        }) => run_benchmark(config, *requests, *clients, tests).await,
        None => {
            if cli.args.is_empty() {
                interactive_mode(config, "~/.zumic_history").await
            } else {
                execute_command(config, &cli.args).await
            }
        }
    }
}

fn init_logging(
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};
    let level = if quiet {
        "off"
    } else if verbose {
        "debug"
    } else {
        "error"
    };
    let filter = EnvFilter::new(level);

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_level(true)
        .try_init()
        .map_err(|e| anyhow::anyhow!("Ошибка инициализации логирования: {e}"))?;

    Ok(())
}

fn print_banner() {
    println!("zumic-cli {}", env!("CARGO_PKG_VERSION"));
}

async fn interactive_mode(
    config: &CliConfig,
    _history_path: &str,
) -> Result<()> {
    println!("Интерактивный режим - заглушка");
    println!("Сервер: {}", config.server_addr);
    println!("Пока используйте: zumic-cli exec <команда>");
    Ok(())
}

fn format_value_for_cli(v: &ZumicValue) -> String {
    match v {
        ZumicValue::Str(s) => String::from_utf8_lossy(s).to_string(),
        ZumicValue::Int(i) => i.to_string(),
        ZumicValue::Float(f) => f.to_string(),
        ZumicValue::Bool(b) => b.to_string(),
        ZumicValue::Null => "(nil)".to_string(),
        ZumicValue::Array(arr) => {
            let inner: Vec<String> = arr.iter().map(format_value_for_cli).collect();
            format!("[{}]", inner.join(", "))
        }
        ZumicValue::List(list) => {
            let items: Vec<String> = list
                .iter()
                .map(|s| String::from_utf8_lossy(s).to_string())
                .collect();
            format!("[{}]", items.join(", "))
        }
        ZumicValue::Set(set) => {
            let mut items: Vec<String> = set
                .iter()
                .map(|s| String::from_utf8_lossy(s).to_string())
                .collect();
            items.sort();
            format!("[{}]", items.join(", "))
        }
        ZumicValue::Bitmap(bmp) => String::from_utf8_lossy(bmp.as_bytes()).to_string(),
        ZumicValue::Hash(_) => "(hash)".to_string(),
        ZumicValue::ZSet { .. } => "(zset)".to_string(),
        ZumicValue::HyperLogLog(_) => "(hll)".to_string(),
        ZumicValue::SStream(_) => "(stream)".to_string(),
    }
}

/// Форматирует значение как wire-level ZSP/RESP3-подобный фрейм.
/// Возвращает строку содержащую CRLF где нужно.
fn format_value_raw(v: &ZumicValue) -> String {
    match v {
        ZumicValue::Str(s) => {
            let s_text = String::from_utf8_lossy(s);
            format!("+{}\r\n", s_text)
        }
        ZumicValue::Int(i) => format!(":{}\r\n", i),
        ZumicValue::Float(f) => format!(",{}\r\n", f),
        ZumicValue::Bool(b) => format!("#{}\r\n", if *b { "t" } else { "f" }),
        ZumicValue::Null => "$-1\r\n".to_string(),
        ZumicValue::Array(arr) => {
            let mut out = format!("*{}\r\n", arr.len());
            for item in arr {
                match item {
                    ZumicValue::Str(s) => {
                        let s_text = String::from_utf8_lossy(s);
                        out.push_str(&format!("${}\r\n{}\r\n", s_text.len(), s_text));
                    }
                    ZumicValue::Int(i) => out.push_str(&format!(":{}\r\n", i)),
                    ZumicValue::Float(f) => out.push_str(&format!(",{}\r\n", f)),
                    ZumicValue::Null => out.push_str("$-1\r\n"),
                    other => {
                        let pretty = format_value_for_cli(other);
                        out.push_str(&format!("${}\r\n{}\r\n", pretty.len(), pretty));
                    }
                }
            }
            out
        }
        ZumicValue::List(list) => {
            let mut out = format!("*{}\r\n", list.len());
            for item in list.iter() {
                let s = String::from_utf8_lossy(item);
                out.push_str(&format!("${}\r\n{}\r\n", s.len(), s));
            }
            out
        }
        ZumicValue::Set(set) => {
            let mut items: Vec<String> = set
                .iter()
                .map(|b| String::from_utf8_lossy(b).to_string())
                .collect();
            items.sort();
            let mut out = format!("*{}\r\n", items.len());
            for s in items {
                out.push_str(&format!("${}\r\n{}\r\n", s.len(), s));
            }
            out
        }
        ZumicValue::Bitmap(bmp) => {
            let bytes = bmp.as_bytes();
            format!("${}\r\n{}\r\n", bytes.len(), String::from_utf8_lossy(bytes))
        }
        ZumicValue::Hash(_)
        | ZumicValue::ZSet { .. }
        | ZumicValue::HyperLogLog(_)
        | ZumicValue::SStream(_) => {
            let pretty = format_value_for_cli(v);
            format!("${}\r\n{}\r\n", pretty.len(), pretty)
        }
    }
}

/// Конвертирует ZumicValue в serde_json::Value для --output json
fn to_json_value(v: &ZumicValue) -> serde_json::Value {
    match v {
        ZumicValue::Str(s) => json!(String::from_utf8_lossy(s).to_string()),
        ZumicValue::Int(i) => json!(i),
        ZumicValue::Float(f) => json!(f),
        ZumicValue::Bool(b) => json!(b),
        ZumicValue::Null => serde_json::Value::Null,
        ZumicValue::Array(arr) => serde_json::Value::Array(arr.iter().map(to_json_value).collect()),
        ZumicValue::List(list) => serde_json::Value::Array(
            list.iter()
                .map(|b| json!(String::from_utf8_lossy(b).to_string()))
                .collect(),
        ),
        ZumicValue::Set(set) => {
            let mut v: Vec<serde_json::Value> = set
                .iter()
                .map(|b| json!(String::from_utf8_lossy(b).to_string()))
                .collect();
            v.sort_by_key(|a| a.to_string());
            serde_json::Value::Array(v)
        }
        ZumicValue::Bitmap(bmp) => json!(String::from_utf8_lossy(bmp.as_bytes()).to_string()),
        ZumicValue::Hash(_)
        | ZumicValue::ZSet { .. }
        | ZumicValue::HyperLogLog(_)
        | ZumicValue::SStream(_) => {
            json!(format_value_for_cli(v))
        }
    }
}

async fn execute_command(
    config: &CliConfig,
    args: &[String],
) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("Не указана команда");
    }

    let mut client = ZumicClient::connect(config.server_addr, config.client_config.clone())
        .await
        .context("Не удалось подключиться к серверу")?;

    debug!("✓ Подключено к {}", config.server_addr);

    let cmd = args[0].to_uppercase();

    match cmd.as_str() {
        "PING" => {
            let result = client.ping().await?;
            match config.output_format {
                OutputFormat::Pretty => {
                    if result {
                        println!("PONG");
                    } else {
                        println!("(nil)");
                    }
                }
                OutputFormat::Raw => {
                    if result {
                        print!("+PONG\r\n");
                    } else {
                        print!("$-1\r\n");
                    }
                }
                OutputFormat::Json => {
                    if result {
                        println!("{}", serde_json::to_string(&json!("PONG"))?);
                    } else {
                        println!("null");
                    }
                }
            }
        }
        "GET" => {
            if args.len() != 2 {
                anyhow::bail!("Использование: GET <ключ>");
            }
            match client.get(&args[1]).await? {
                Some(value) => match config.output_format {
                    OutputFormat::Pretty => println!("{}", format_value_for_cli(&value)),
                    OutputFormat::Raw => print!("{}", format_value_raw(&value)),
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string(&to_json_value(&value))?)
                    }
                },
                None => match config.output_format {
                    OutputFormat::Pretty => println!("(nil)"),
                    OutputFormat::Raw => print!("$-1\r\n"),
                    OutputFormat::Json => println!("null"),
                },
            }
        }
        "SET" => {
            if args.len() != 3 {
                anyhow::bail!("Использование: SET <ключ> <значение>");
            }
            let value = zumic::Value::Str(zumic::Sds::from_str(&args[2]));
            client.set(&args[1], value).await?;
            match config.output_format {
                OutputFormat::Pretty => println!("OK"),
                OutputFormat::Raw => print!("+OK\r\n"),
                OutputFormat::Json => println!("{}", serde_json::to_string(&json!("OK"))?),
            }
        }
        "DEL" => {
            if args.len() != 2 {
                anyhow::bail!("Использование: DEL <ключ>");
            }
            let deleted = client.del(&args[1]).await?;
            let n = if deleted { 1 } else { 0 };
            match config.output_format {
                OutputFormat::Pretty => println!("{n}"),
                OutputFormat::Raw => print!(":{n}\r\n"),
                OutputFormat::Json => println!("{}", serde_json::to_string(&json!(n))?),
            }
        }
        _ => {
            anyhow::bail!("Неизвестная команда: {cmd}. Поддерживаются: PING, GET, SET, DEL");
        }
    }

    client.close().await?;
    Ok(())
}

async fn ping_server(
    config: &CliConfig,
    count: u32,
    interval: Duration,
) -> Result<()> {
    println!("PING {}", config.server_addr);
    println!();

    let mut client = ZumicClient::connect(config.server_addr, config.client_config.clone())
        .await
        .context("Не удалось подключиться к серверу")?;

    let mut successful = 0;
    let mut total_time = Duration::ZERO;

    for i in 1..=count {
        let start = std::time::Instant::now();

        match client.ping().await {
            Ok(true) => {
                let elapsed = start.elapsed();
                total_time += elapsed;
                successful += 1;
                println!("#{i}: PONG - время={:.2}ms", elapsed.as_secs_f64() * 1000.0);
            }
            Ok(false) => {
                println!("#{i}: Неожиданный ответ");
            }
            Err(e) => {
                println!("#{i}: Ошибка - {e}");
            }
        }

        if i < count {
            tokio::time::sleep(interval).await;
        }
    }

    println!();
    println!("--- Статистика ---");
    println!("Отправлено: {}", count);
    println!("Успешно: {}", successful);
    println!("Потеряно: {}", count - successful);
    if successful > 0 {
        let avg_ms = (total_time.as_secs_f64() * 1000.0) / successful as f64;
        println!("Среднее время: {avg_ms:.2}ms");
    }

    client.close().await?;
    Ok(())
}

async fn get_server_info(
    config: &CliConfig,
    section: Option<&str>,
) -> Result<()> {
    println!("Режим информации - заглушка");
    println!("Сервер: {}", config.server_addr);
    if let Some(sec) = section {
        println!("Раздел: {sec}");
    }
    println!();
    Ok(())
}

async fn monitor_mode(config: &CliConfig) -> Result<()> {
    println!("Режим мониторинга - заглушка");
    println!("Сервер: {}", config.server_addr);
    println!();
    Ok(())
}

async fn run_benchmark(
    config: &CliConfig,
    requests: usize,
    clients: usize,
    tests: &str,
) -> Result<()> {
    println!("Режим бенчмарка - заглушка");
    println!("Сервер: {}", config.server_addr);
    println!("Запросов: {requests}, Клиентов: {clients}");
    println!("Тестируемые команды: {tests}");
    println!();
    Ok(())
}
