use std::env;

use atty::Stream;
use chrono::{DateTime, Local};
use num_cpus;
use owo_colors::OwoColorize;
use sysinfo::System;

/// Полный баннер с информацией о сервере.
pub const ASCII_FULL: &str = r#"
    Zumic {version}
    ----------------------------------------------
    Mode:             {mode}
    Listening:        {listen}
    Port:             {port}
    Storage:          {storage}
    PID:              {pid}
    Host:             {host}
    OS/Arch:          {os}/{arch}
    CPU(s):           {cpus}
    Memory:           {mem_value} {mem_unit}
    Git:              {git}
    Build:            {git} ({build_time})
"#;

/// Компактный баннер для вывода.
pub const ASCII_COMPACT: &str = r#"
Zumic {version} — {mode} — {listen}:{port} — PID {pid}
"#;

/// Вывод баннера сервера с информацией о конфигурации
///
/// # Параметры
/// - `listen`: адрес, на котором слушает сервер
/// - `port`: порт сервера
/// - `storage`: тип хранилища (память, постоянное, кластер)
pub fn print_banner(
    listen: &str,
    port: u16,
    storage: &str,
) {
    // Определяем режим отображения баннера: полный или компактный
    let forced = env::var("ZUMIC_BANNER").ok();
    let full = match forced.as_deref() {
        Some("full") => true,
        Some("compact") => false,
        _ => cfg!(debug_assertions), // debug => полный, release => компактный
    };

    // Получение метаданных
    let version = env!("CARGO_PKG_VERSION");
    // добавляем разрядность (32/64-bit) к версии
    let bits = std::mem::size_of::<usize>() * 8;
    let version_with_bits = format!("{version} ({bits}-bit)");

    let mode = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let pid = std::process::id();
    let mut sys = System::new_all();
    sys.refresh_memory();

    let host = System::host_name().unwrap_or_else(|| "unknown".into());
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let cpus = num_cpus::get();

    // Расчет памяти
    let mem_total_kb = sys.total_memory(); // KB
    let mem_total_mb = mem_total_kb as f64 / 1024.0;
    let mem_total_gb = mem_total_mb / 1024.0;

    let (mem_value, mem_unit) = if mem_total_gb >= 1.0 {
        (mem_total_gb, "GB")
    } else if mem_total_mb >= 1.0 {
        (mem_total_mb, "MB")
    } else {
        (mem_total_kb as f64, "KB")
    };

    // Git и время сборки
    let git = option_env!("GIT_COMMIT").unwrap_or("unknown");
    let build_time_raw = option_env!("BUILD_TIME").unwrap_or("unknown");
    let build_time_fmt = if let Ok(dt) = DateTime::parse_from_rfc3339(build_time_raw) {
        dt.with_timezone(&Local)
            .format("%d.%m.%Y %H:%M:%S")
            .to_string()
    } else {
        build_time_raw.to_string()
    };

    let color = atty::is(Stream::Stdout);

    if full {
        // Подстановка значений в шаблон
        let mut s = ASCII_FULL.to_string();
        s = s
            .replace("{version}", &version_with_bits)
            .replace("{mode}", mode)
            .replace("{listen}", listen)
            .replace("{port}", &port.to_string())
            .replace("{storage}", storage)
            .replace("{pid}", &pid.to_string())
            .replace("{host}", &host)
            .replace("{os}", os)
            .replace("{arch}", arch)
            .replace("{cpus}", &cpus.to_string())
            .replace("{mem_value}", &format!("{mem_value:.1}"))
            .replace("{mem_unit}", mem_unit)
            .replace("{git}", git)
            .replace("{build_time}", &build_time_fmt);

        if color {
            for (i, line) in s.lines().enumerate() {
                if i == 1 {
                    // заголовок "Зумик БД..."
                    println!("{}", line.bold().bright_blue());
                } else if line.trim_start().starts_with("Mode:") {
                    println!("{}", line.replace(mode, &mode.cyan().to_string()));
                } else if line.trim_start().starts_with("Port:")
                    || line.trim_start().starts_with("PID:")
                {
                    println!(
                        "{}",
                        line.replace(&port.to_string(), &port.to_string().magenta().to_string())
                            .replace(&pid.to_string(), &pid.to_string().magenta().to_string())
                    );
                } else if line.trim_start().starts_with("Git:")
                    || line.trim_start().starts_with("Build:")
                {
                    println!("{}", line.dimmed());
                } else {
                    println!("{line}");
                }
            }
        } else {
            println!("{s}");
        }
    } else {
        let mut s = ASCII_COMPACT.to_string();
        s = s
            .replace("{version}", &version_with_bits)
            .replace("{mode}", mode)
            .replace("{listen}", listen)
            .replace("{port}", &port.to_string())
            .replace("{pid}", &pid.to_string());

        if color {
            println!("{}", s.bold().green());
        } else {
            println!("{s}");
        }
    }
    println!();
}

/// Лог запуска сервера с точностью до миллисекунд
///
/// Показывает PID, временную метку, статус запуска и готовность принимать
/// соединения
pub fn print_startup_log() {
    let pid = std::process::id();
    let now = Local::now();
    let ts = now.format("%d %b %Y %H:%M:%S%.3f");

    if atty::is(Stream::Stdout) {
        println!(
            "[{}] {} {} {}",
            pid.to_string().red(),
            ts.to_string().white(),
            "# Server started, Zumic version".dimmed().bold(),
            env!("CARGO_PKG_VERSION").dimmed().bold()
        );
        println!(
            "[{}] {} {}",
            pid.to_string().red(),
            ts.to_string().white(),
            "* Ready to accept connections".green()
        );
    } else {
        println!(
            "[{}] {} # Server started, Zumic version {}",
            pid,
            ts,
            env!("CARGO_PKG_VERSION")
        );
        println!("[{pid}] {ts} * Ready to accept connections");
    }
}
