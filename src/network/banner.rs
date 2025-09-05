use atty::Stream;
use chrono::{DateTime, Local};
use num_cpus;
use owo_colors::OwoColorize;
use std::env;
use sysinfo::System;

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

pub const ASCII_COMPACT: &str = r#"
Zumic DB {version} — {mode} — {listen}:{port} — PID {pid}
"#;

/// Formatted and aligned banner output
pub fn print_banner(
    listen: &str,
    port: u16,
    storage: &str,
) {
    // choose mode
    let forced = env::var("ZUMIC_BANNER").ok();
    let full = match forced.as_deref() {
        Some("full") => true,
        Some("compact") => false,
        _ => cfg!(debug_assertions), // debug => full, release => compact
    };

    // metadata
    let version = env!("CARGO_PKG_VERSION");
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

    // correct memory calculation
    let mem_total_kb = sys.total_memory(); // KB
    let mem_total_gb = mem_total_kb as f64 / 1024.0 / 1024.0; // KB -> GB

    let (mem_value, mem_unit) = if mem_total_gb >= 1.0 {
        (mem_total_gb, "GB")
    } else {
        (mem_total_kb as f64 / 1024.0, "MB")
    };

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
        // prepare substitutions
        let mut s = ASCII_FULL.to_string();
        s = s
            .replace("{version}", version)
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
                } else if line.trim_start().starts_with("Режим:") {
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
            .replace("{version}", version)
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
}
