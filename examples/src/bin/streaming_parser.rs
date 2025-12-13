use std::{fs::File, io};

use zumic::{
    engine::{
        write_stream, CallbackHandler, CollectHandler, CountHandler, FilterHandler,
        StreamingParser, TransformHandler,
    },
    Sds, Value,
};

fn main() -> io::Result<()> {
    println!("=== ZDB Streaming Parse Examples ===\n");

    create_test_dump("test_dump.zdb")?;

    example_1_collect()?;
    example_2_filter()?;
    example_3_count()?;
    example_4_callback()?;
    example_5_transform()?;

    // Cleanup
    std::fs::remove_file("test_dump.zdb").ok();
    std::fs::remove_file("filtered_dump.zdb").ok();

    println!("\n=== All examples completed successfully! ===");
    Ok(())
}

/// Пример 1: Сбор всех записей (аналог read_dump)
fn example_1_collect() -> io::Result<()> {
    println!("Example 1: Collect all entries");
    println!("-------------------------------");

    let file = File::open("test_dump.zdb")?;
    let mut parser = StreamingParser::new(file)?;
    let mut handler = CollectHandler::new();

    parser.parse(&mut handler)?;

    let items = handler.into_items();
    println!("Collected {} entries:", items.len());
    for (key, value) in items.iter().take(3) {
        println!("  {key} => {value:?}");
    }

    let stats = parser.stats();
    println!(
        "Stats: {} records, {} bytes read\n",
        stats.records_parsed, stats.bytes_read
    );

    Ok(())
}

/// Пример 2: Фильтрация по ключам
fn example_2_filter() -> io::Result<()> {
    println!("Example 2: Filter by key prefix");
    println!("--------------------------------");

    let file = File::open("test_dump.zdb")?;
    let mut parser = StreamingParser::new(file)?;

    // Фильтруем только ключи, начинающиеся с "user:"
    let mut handler = FilterHandler::new(|key| key.starts_with(b"user:"));

    parser.parse(&mut handler)?;

    let filtered = handler.into_items();
    println!("Filtered {} entries with prefix 'user:':", filtered.len());
    for (key, value) in filtered.iter() {
        println!("  {key} => {value:?}");
    }
    println!();

    Ok(())
}

/// Пример 3: Подсчет статистики без загрузки значений
fn example_3_count() -> io::Result<()> {
    println!("Example 3: Count statistics");
    println!("----------------------------");

    let file = File::open("test_dump.zdb")?;
    let mut parser = StreamingParser::new(file)?;
    let mut handler = CountHandler::new();

    parser.parse(&mut handler)?;

    println!("Total entries: {}", handler.total_entries());
    println!("Average key length: {:.2} bytes", handler.avg_key_length());
    println!("Dump version: {:?}\n", handler.version());

    Ok(())
}

/// Пример 4: Custom обработка через callback
fn example_4_callback() -> io::Result<()> {
    println!("Example 4: Custom callback processing");
    println!("--------------------------------------");

    let file = File::open("test_dump.zdb")?;
    let mut parser = StreamingParser::new(file)?;

    let mut sum = 0i64;
    let mut count = 0;

    let mut handler = CallbackHandler::new(|_key, value| {
        if let Value::Int(i) = value {
            sum += i;
            count += 1;
        }
        Ok(())
    });

    parser.parse(&mut handler)?;

    println!("Processed {count} integer values");
    if count > 0 {
        println!("Average value: {:.2}", sum as f64 / count as f64);
    }
    println!();

    Ok(())
}

/// Пример 5: Трансформация дампа на лету
fn example_5_transform() -> io::Result<()> {
    println!("Example 5: Transform and write to new dump");
    println!("-------------------------------------------");

    let input_file = File::open("test_dump.zdb")?;
    let output_file = File::create("filtered_dump.zdb")?;

    let mut parser = StreamingParser::new(input_file)?;

    // Трансформация: оставляем только пользователей и умножаем их ID на 10
    let mut handler = TransformHandler::new(output_file, |key, value| {
        if key.starts_with(b"user:") {
            if let Value::Int(id) = value {
                return Some((key.clone(), Value::Int(id * 10)));
            }
        }
        None
    });

    parser.parse(&mut handler)?;

    println!("Transformed {} entries", handler.count());
    println!("Output written to: filtered_dump.zdb\n");

    Ok(())
}

fn create_test_dump(path: &str) -> io::Result<()> {
    let items = vec![
        (Sds::from_str("user:1"), Value::Int(100)),
        (Sds::from_str("user:2"), Value::Int(200)),
        (
            Sds::from_str("post:1"),
            Value::Str(Sds::from_str("Hello World")),
        ),
        (Sds::from_str("user:3"), Value::Int(300)),
        (
            Sds::from_str("post:2"),
            Value::Str(Sds::from_str("Streaming Parser")),
        ),
        (Sds::from_str("counter"), Value::Int(42)),
        (Sds::from_str("user:4"), Value::Int(400)),
    ];

    let mut file = File::create(path)?;
    write_stream(&mut file, items.into_iter())?;

    Ok(())
}

/// Пример для документации: обработка огромного дампа
#[allow(dead_code)]
fn example_huge_dump() -> io::Result<()> {
    println!("Processing huge dump with constant memory usage...");

    let file = File::open("huge_dump.zdb")?;
    let mut parser = StreamingParser::new(file)?;

    // Обрабатываем записи по одной, без загрузки всего в память
    let mut processed = 0u64;
    let mut handler = CallbackHandler::new(|_key, _value| {
        // Здесь может быть любая логика обработки:
        // - запись в БД
        // - обновление индекса
        // - агрегация данных
        // и т.д.

        processed += 1;
        if processed % 1_000_000 == 0 {
            println!("Processed {} million records...", processed / 1_000_000);
        }

        Ok(())
    });

    parser.parse(&mut handler)?;

    println!("Total processed: {} records", processed);
    println!("Peak memory usage: constant (not proportional to dump size!)");

    Ok(())
}
