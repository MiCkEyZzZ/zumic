//! CLI клиент Zumic
//!
//! Клиент командной строки для взаимодействия с сервером Zumic.
//! Поддерживает интерактивный режим (REPL), одиночные команды,
//! получение информации о сервере, мониторинг и бенчмарки.

use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::debug;
use zumic::{
    client::{ClientConfig, ZumicClient},
    Value as ZumicValue,
};

/// Основная структура CLI аргументов
///
/// Содержит параметры подключения к серверу, таймауты, формат вывода,
/// а также подкоманду или прямой набор аргументов.
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
///
/// Содержит серверный адрес, таймауты, пароль и формат вывода
#[derive(Debug, Clone)]
struct CliConfig {
    server_addr: SocketAddr,
    client_config: ClientConfig,
    #[allow(dead_code)]
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

/// Точка входа в CLI
///
/// Разбирает аргументы, инициализирует логирование, печатает баннер
/// и вызывает обработчик команды.
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Инициализация логирования (учитываем quiet)
    init_logging(cli.verbose, cli.quiet)?;

    // Баннер CLI — печатаем только в интерактивном режиме (как redis-cli)
    // или если явно запрошен вывод баннера через флаг в будущем.
    let should_print_banner = matches!(cli.command, Some(Commands::Interactive { .. }));
    if should_print_banner {
        print_banner();
    }

    // Конфигурация CLI
    let config = CliConfig::try_from(&cli)?;

    debug!("Конфигурация CLI: {config:?}");
    // debug!("Подключение к {}...", config.server_addr);

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
        // Интерактивный режим
        Some(Commands::Interactive { history }) => {
            debug!("Запуск интерактивного режима...");
            interactive_mode(config, history).await
        }

        // Выполнить одиночную команду из подкоманды
        Some(Commands::Exec { args }) => {
            debug!("Выполнение команды: {args:?}");
            execute_command(config, args).await
        }

        // Команда Ping
        Some(Commands::Ping { count, interval }) => {
            ping_server(config, *count, Duration::from_millis(*interval)).await
        }

        // Информационная команда
        Some(Commands::Info { section }) => get_server_info(config, section.as_deref()).await,

        // Режим мониторинга
        Some(Commands::Monitor) => monitor_mode(config).await,

        // Контрольный режим
        Some(Commands::Benchmark {
            requests,
            clients,
            tests,
        }) => run_benchmark(config, *requests, *clients, tests).await,

        // Нет подкоманды - проверьте, указаны ли аргументы
        None => {
            if cli.args.is_empty() {
                // По умолчанию используется интерактивный режим
                debug!("Команда не указана, запускается интерактивный режим...");
                interactive_mode(config, "~/.zumic_history").await
            } else {
                // Выполнить прямую команду
                debug!("Выполнение прямой команды: {:?}", cli.args);
                execute_command(config, &cli.args).await
            }
        }
    }
}

/// Инициализация логирования
fn init_logging(
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    // quiet имеет приоритет — если задан, отключаем почти всё (ERROR -> повторно
    // quiet => OFF) Поведение:
    // - quiet == true  -> ERROR (только ошибки). Можно поставить "off" если хочешь
    //   полностью молчать.
    // - verbose == true -> DEBUG
    // - по умолчанию -> ERROR (убираем INFO)
    let level = if quiet {
        // Полностью молчать: "off"
        "off"
    } else if verbose {
        "debug"
    } else {
        // по умолчанию показываем только ошибки — INFO скрыты
        "error"
    };

    let filter = EnvFilter::new(level);

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        // не печатаем цветной префикс уровня (чтобы вывод был очень чистым)
        .with_level(true)
        .try_init()
        .map_err(|e| anyhow::anyhow!("Ошибка инициализации логирования: {e}"))?;

    Ok(())
}

/// Баннер CLI
fn print_banner() {
    println!("zumic-cli {}", env!("CARGO_PKG_VERSION"));
}

/// Интерактивный режим (REPL)
async fn interactive_mode(
    config: &CliConfig,
    _history_path: &str,
) -> Result<()> {
    println!("🚧 Интерактивный режим - Coming in Issue #4");
    println!("   Сервер: {}", config.server_addr);
    println!();
    println!("Пока используйте: zumic-cli exec <команда>");
    Ok(())
}

/// Небольшая утилита для удобного отображения Value в CLI
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
        // Для сложных/неподдерживаемых типов — краткая метка
        ZumicValue::Hash(_) => "(hash)".to_string(),
        ZumicValue::ZSet { .. } => "(zset)".to_string(),
        ZumicValue::HyperLogLog(_) => "(hll)".to_string(),
        ZumicValue::SStream(_) => "(stream)".to_string(),
    }
}

/// Выполнение одной команды
async fn execute_command(
    config: &CliConfig,
    args: &[String],
) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("Не указана команда");
    }

    // Подключаемся к серверу
    let mut client = ZumicClient::connect(config.server_addr, config.client_config.clone())
        .await
        .context("Не удалось подключиться к серверу")?;

    // Перекладываем служебный вывод в debug, чтобы он не мешал результату при
    // обычном запуске.
    debug!("✓ Подключено к {}", config.server_addr);

    // Парсим и выполняем команду
    let cmd = args[0].to_uppercase();

    match cmd.as_str() {
        "PING" => {
            let result = client.ping().await?;
            if result {
                println!("PONG");
            }
        }
        "GET" => {
            if args.len() != 2 {
                anyhow::bail!("Использование: GET <ключ>");
            }
            match client.get(&args[1]).await? {
                Some(value) => {
                    println!("{}", format_value_for_cli(&value));
                }
                None => {
                    println!("(nil)");
                }
            }
        }
        "SET" => {
            if args.len() != 3 {
                anyhow::bail!("Использование: SET <ключ> <значение>");
            }
            let value = zumic::Value::Str(zumic::Sds::from_str(&args[2]));
            client.set(&args[1], value).await?;
            println!("OK");
        }
        "DEL" => {
            if args.len() != 2 {
                anyhow::bail!("Использование: DEL <ключ>");
            }
            let deleted = client.del(&args[1]).await?;
            println!("{}", if deleted { "1" } else { "0" });
        }
        _ => {
            anyhow::bail!(
                "Неизвестная команда: {}. Поддерживаются: PING, GET, SET, DEL",
                cmd
            );
        }
    }

    client.close().await?;
    Ok(())
}

/// Ping сервера
async fn ping_server(
    config: &CliConfig,
    count: u32,
    interval: Duration,
) -> Result<()> {
    println!("🔄 PING {}", config.server_addr);
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
                println!(
                    "#{}: PONG - время={:.2}ms",
                    i,
                    elapsed.as_secs_f64() * 1000.0
                );
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
