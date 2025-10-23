//! CLI клиент Zumic
//!
//! Клиент командной строки для взаимодействия с сервером Zumic.
//! Поддерживает интерактивный режим (REPL), одиночные команды,
//! получение информации о сервере, мониторинг и бенчмарки.

use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{debug, info};

/// Основная структура CLI аргументов
///
/// Содержит параметры подключения к серверу, таймауты, формат вывода,
/// а также подкоманду или прямой набор аргументов.
#[derive(Parser)]
#[command(name = "zumic-cli")]
#[command(author = "Zumic Contributors")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "CLI клиент для Zumic сервера", long_about = None)]
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
///
/// Содержит серверный адрес, таймауты, пароль и формат вывода
#[derive(Debug, Clone)]
struct CliConfig {
    server_addr: SocketAddr,
    #[allow(dead_code)]
    auth: Option<String>,
    #[allow(dead_code)]
    timeout: Duration,
    #[allow(dead_code)]
    read_timeout: Duration,
    #[allow(dead_code)]
    write_timeout: Duration,
    #[allow(dead_code)]
    verbose: bool,
    #[allow(dead_code)]
    output_format: OutputFormat,
}

impl TryFrom<&Cli> for CliConfig {
    type Error = anyhow::Error;

    fn try_from(cli: &Cli) -> Result<Self> {
        let server_addr: SocketAddr = format!("{}:{}", cli.host, cli.port)
            .parse()
            .context("Неверный формат адреса сервера")?;

        Ok(Self {
            server_addr,
            auth: cli.auth.clone(),
            timeout: Duration::from_secs(cli.timeout),
            read_timeout: Duration::from_secs(cli.read_timeout),
            write_timeout: Duration::from_secs(cli.write_timeout),
            verbose: cli.verbose,
            output_format: cli.output.clone(),
        })
    }
}

/// Точка входа в CLI
///
/// Разбирает аргументы, инициализирует логирование, печатает баннер
/// и вызывает обработчик команды.
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Инициализация логирования
    init_logging(cli.verbose)?;

    // Баннер CLI
    print_banner();

    // Конфигурация CLI
    let config = CliConfig::try_from(&cli)?;

    debug!("Конфигурация CLI: {config:?}");
    info!("Подключение к {}...", config.server_addr);

    // Обработка команды
    match handle_command(&cli, &config).await {
        Ok(_) => {
            debug!("Команда выполнена успешно");
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

/// Обработчик выполнения команд
async fn handle_command(
    cli: &Cli,
    config: &CliConfig,
) -> Result<()> {
    match &cli.command {
        // Interactive mode
        Some(Commands::Interactive { history }) => {
            info!("Запуск интерактивного режима...");
            interactive_mode(config, history).await
        }

        // Execute single command from subcommand
        Some(Commands::Exec { args }) => {
            debug!("Выполнение команды: {args:?}");
            execute_command(config, args).await
        }

        // Ping command
        Some(Commands::Ping { count, interval }) => {
            ping_server(config, *count, Duration::from_millis(*interval)).await
        }

        // Info command
        Some(Commands::Info { section }) => get_server_info(config, section.as_deref()).await,

        // Monitor mode
        Some(Commands::Monitor) => monitor_mode(config).await,

        // Benchmark mode
        Some(Commands::Benchmark {
            requests,
            clients,
            tests,
        }) => run_benchmark(config, *requests, *clients, tests).await,

        // No subcommand - check if args provided
        None => {
            if cli.args.is_empty() {
                // Default to interactive mode
                info!("Команда не указана, запускается интерактивный режим...");
                interactive_mode(config, "~/.zumic_history").await
            } else {
                // Execute direct command
                debug!("Выполнение прямой команды: {:?}", cli.args);
                execute_command(config, &cli.args).await
            }
        }
    }
}

/// Инициализация логирования
fn init_logging(verbose: bool) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!("Ошибка инициализации логирования: {e}"))?;

    Ok(())
}

/// Баннер CLI
fn print_banner() {
    println!("┌─────────────────────────────────────────┐");
    println!(
        "│                   Zumic CLI v{}         │",
        env!("CARGO_PKG_VERSION")
    );
    println!("│   Интерфейс командной строки            │");
    println!("└─────────────────────────────────────────┘");
    println!();
}

/// Интерактивный режим (REPL)
async fn interactive_mode(
    config: &CliConfig,
    _history_path: &str,
) -> Result<()> {
    println!("🚧 Интерактивный режим - пока заглушка");
    println!("   Сервер: {}", config.server_addr);
    println!("   Введите 'help' для списка команд");
    println!("   Введите 'quit' или 'exit' для выхода");
    println!();
    println!("Пока используйте: zumic-cli exec <команда>");
    Ok(())
}

/// Выполнение одной команды
async fn execute_command(
    config: &CliConfig,
    args: &[String],
) -> Result<()> {
    println!("🚧 Режим выполнения команды - пока заглушка");
    println!("   Сервер: {}", config.server_addr);
    println!("   Команда: {}", args.join(" "));
    println!();
    println!("Следующие шаги:");
    println!("   1. Реализовать подключение к ZSP клиенту");
    println!("   2. Добавить выполнение команд");
    Ok(())
}

/// Ping сервера
async fn ping_server(
    config: &CliConfig,
    count: u32,
    interval: Duration,
) -> Result<()> {
    println!("🚧 Режим Ping - пока заглушка");
    println!("   Сервер: {}", config.server_addr);
    println!("   Количество пингов: {count}, Интервал: {interval:?}");
    println!();
    println!("Будет отправлен PING и измерена задержка");
    Ok(())
}

/// Информация о сервере
async fn get_server_info(
    config: &CliConfig,
    section: Option<&str>,
) -> Result<()> {
    println!("🚧 Режим информации - пока заглушка");
    println!("   Сервер: {}", config.server_addr);
    if let Some(sec) = section {
        println!("   Раздел: {sec}");
    }
    println!();
    println!("Будет получена статистика и конфигурация сервера");
    Ok(())
}

/// Режим мониторинга команд
async fn monitor_mode(config: &CliConfig) -> Result<()> {
    println!("🚧 Режим мониторинга - пока заглушка");
    println!("   Сервер: {}", config.server_addr);
    println!();
    println!("Будет отображаться поток команд в реальном времени");
    Ok(())
}

/// Запуск бенчмарка
async fn run_benchmark(
    config: &CliConfig,
    requests: usize,
    clients: usize,
    tests: &str,
) -> Result<()> {
    println!("🚧 Режим бенчмарка - пока заглушка");
    println!("   Сервер: {}", config.server_addr);
    println!("   Запросов: {requests}, Клиентов: {clients}");
    println!("   Тестируемые команды: {tests}");
    println!();
    println!("Будет выполнено тестирование производительности");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli_parsing() {
        let cli = Cli::parse_from(["zumic-cli", "--help"]);
        // Исправлено: используем is_none() вместо matches!(..., None) чтобы
        // удовлетворить clippy
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_config_from_cli() {
        let cli = Cli::parse_from(["zumic-cli", "-h", "localhost", "-p", "6174"]);
        let config = CliConfig::try_from(&cli).unwrap();
        assert_eq!(config.server_addr.port(), 6174);
    }
}
