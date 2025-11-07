//! Тесты для проверки memory usage при парсинге больших дампов.
//!
//! Запуск:
//! ```bash
//! cargo test --test memory_usage --release -- --ignored --nocapture
//! ```

#[cfg(target_os = "linux")]
use std::fs;
use std::{
    fs::File,
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use tempfile::TempDir;
use zumic::{
    engine::zdb::{
        streaming::{CountHandler, StreamingParser},
        write_stream,
    },
    Sds, Value,
};

#[cfg(target_os = "linux")]
fn get_current_rss_kb() -> Option<usize> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse().ok();
            }
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn get_current_rss_kb() -> Option<usize> {
    None
}

fn start_rss_monitor(interval_ms: u64) -> (Arc<AtomicBool>, thread::JoinHandle<usize>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    let handle = thread::spawn(move || {
        let mut peak: usize = 0;
        while !stop_clone.load(Ordering::Relaxed) {
            if let Some(rss) = get_current_rss_kb() {
                if rss > peak {
                    peak = rss;
                }
            }
            thread::sleep(Duration::from_millis(interval_ms));
        }
        if let Some(rss) = get_current_rss_kb() {
            if rss > peak {
                peak = rss;
            }
        }
        peak
    });

    (stop, handle)
}

fn generate_large_dump(
    path: &std::path::Path,
    entries: usize,
) -> std::io::Result<u64> {
    let iter = (0..entries).map(|i| {
        let key = Sds::from_str(&format!("key_{i:08}"));
        let value = Value::Str(Sds::from_str(&format!(
            "This is a relatively long string value for entry number {}. \
             It contains some padding to make the dump larger. \
             Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
            i
        )));
        (key, value)
    });

    let mut file = File::create(path)?;
    write_stream(&mut file, iter)?;
    file.flush()?;

    let size = std::fs::metadata(path)?.len();
    Ok(size)
}

#[test]
#[ignore]
fn test_constant_memory_1mb_dump() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("test_1mb.zdb");

    let entries = 5_000;
    let file_size = generate_large_dump(&dump_path, entries).unwrap();

    println!("\n=== Memory Test: 1MB Dump ===");
    println!("Entries: {entries}");
    println!("File size: {:.2} MB", file_size as f64 / (1024.0 * 1024.0));

    let rss_before_parse = get_current_rss_kb();
    if let Some(rss) = rss_before_parse {
        println!("RSS before parse: {rss} KB");
    }

    let (stop_flag, monitor_handle) = start_rss_monitor(100);

    let file = File::open(&dump_path).unwrap();
    let mut parser = StreamingParser::new(file).unwrap();
    let mut handler = CountHandler::new();
    parser.parse(&mut handler).unwrap();

    stop_flag.store(true, Ordering::Relaxed);
    let peak_rss_kb = monitor_handle.join().unwrap_or(0);

    println!("Peak RSS during parsing: {peak_rss_kb} KB");
    println!("File size: {} KB", file_size / 1024);

    if let Some(before) = rss_before_parse {
        let delta_kb = peak_rss_kb.saturating_sub(before);
        println!("Delta RSS during parse: {delta_kb} KB");

        let file_kb = (file_size / 1024) as usize;
        let max_allowed_kb = std::cmp::max(file_kb / 5, 5_120);

        println!("Max allowed delta: {max_allowed_kb} KB");

        assert!(
            delta_kb < max_allowed_kb,
            "Peak memory delta too high: {delta_kb} KB (should be < {max_allowed_kb} KB)",
        );
    }

    assert_eq!(handler.total_entries(), entries as u64);
}

#[test]
#[ignore]
fn test_constant_memory_100mb_dump() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("test_100mb.zdb");

    let entries = 500_000;
    let file_size = generate_large_dump(&dump_path, entries).unwrap();

    println!("\n=== Memory Test: 100MB Dump ===");
    println!("Entries: {entries}");
    println!("File size: {:.2} MB", file_size as f64 / (1024.0 * 1024.0));

    let rss_before_parse = get_current_rss_kb();
    if let Some(rss) = rss_before_parse {
        println!("RSS before parse: {:.2} MB", rss as f64 / 1024.0);
    }

    let (stop_flag, monitor_handle) = start_rss_monitor(100);

    let file = File::open(&dump_path).unwrap();
    let mut parser = StreamingParser::new(file).unwrap();
    let mut handler = CountHandler::new();
    parser.parse(&mut handler).unwrap();

    stop_flag.store(true, Ordering::Relaxed);
    let peak_rss_kb = monitor_handle.join().unwrap_or(0);

    if let Some(before) = rss_before_parse {
        let delta_kb = peak_rss_kb.saturating_sub(before);
        let used_mb = delta_kb as f64 / 1024.0;
        println!("Memory delta during parse: {used_mb:.2} MB");

        let max_allowed_mb = 10.0;
        println!("Max allowed delta: {max_allowed_mb:.2} MB");

        assert!(
            used_mb < max_allowed_mb,
            "Memory usage too high: {used_mb:.2} MB (should be < {max_allowed_mb:.2} MB)",
        );
    }

    assert_eq!(handler.total_entries(), entries as u64);
}

#[test]
#[ignore]
fn test_constant_memory_1gb_dump() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("test_1gb.zdb");

    let entries = 5_000_000;
    println!("\n=== Memory Test: 1GB Dump ===");
    println!("Generating {entries} entries...");

    let file_size = generate_large_dump(&dump_path, entries).unwrap();
    println!("File size: {:.2} MB", file_size as f64 / (1024.0 * 1024.0));

    let rss_before_parse = get_current_rss_kb();
    if let Some(rss) = rss_before_parse {
        println!("RSS before parse: {:.2} MB", rss as f64 / 1024.0);
    }

    let (stop_flag, monitor_handle) = start_rss_monitor(100);

    println!("Parsing...");
    let file = File::open(&dump_path).unwrap();
    let mut parser = StreamingParser::new(file).unwrap();
    let mut handler = CountHandler::new();
    parser.parse(&mut handler).unwrap();

    stop_flag.store(true, Ordering::Relaxed);
    let peak_rss_kb = monitor_handle.join().unwrap_or(0);

    if let Some(before) = rss_before_parse {
        let delta_kb = peak_rss_kb.saturating_sub(before);
        let used_mb = delta_kb as f64 / 1024.0;
        println!("Peak delta RSS during parsing: {used_mb:.2} MB");

        let max_allowed_mb = 50.0;
        println!("Max allowed delta: {max_allowed_mb:.2} MB");

        assert!(
            used_mb < max_allowed_mb,
            "Memory usage too high: {used_mb:.2} MB (should be < {max_allowed_mb:.2} MB)",
        );

        println!("✅ SUCCESS: Constant memory usage confirmed!");
        println!(
            "   File: {:.2} MB, Memory delta: {:.2} MB (ratio: {:.1}x)",
            file_size as f64 / (1024.0 * 1024.0),
            used_mb,
            (file_size as f64 / (1024.0 * 1024.0)) / used_mb.max(1.0)
        );
    }

    assert_eq!(handler.total_entries(), entries as u64);
}

#[test]
fn test_peak_memory_during_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("test_peak.zdb");

    let entries = 10_000;
    generate_large_dump(&dump_path, entries).unwrap();

    println!("\n=== Peak Memory Test ===");

    let rss_before_parse = get_current_rss_kb();
    if let Some(rss) = rss_before_parse {
        println!("RSS before parse: {rss} KB");
    }

    let (stop_flag, monitor_handle) = start_rss_monitor(100);

    let file = File::open(&dump_path).unwrap();
    let mut parser = StreamingParser::new(file).unwrap();
    let mut handler = CountHandler::new();
    parser.parse(&mut handler).unwrap();

    stop_flag.store(true, Ordering::Relaxed);
    let peak_rss_kb = monitor_handle.join().unwrap_or(0);

    let rss_after = get_current_rss_kb().unwrap_or(0);

    println!("RSS sample after: {rss_after} KB");
    println!("Peak RSS observed during parse: {peak_rss_kb} KB");

    if let Some(before) = rss_before_parse {
        let delta = peak_rss_kb.saturating_sub(before);
        println!("Delta during parse: {delta} KB");
        assert!(delta < 5000, "Memory delta too large: {delta} KB");
    }
}
