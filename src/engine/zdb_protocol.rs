use std::{fs::File, io::BufWriter};

use zumic_error::{ResultExt, StatusCode, ZumicResult};

use super::{write_stream, CallbackHandler, InMemoryStore, Storage, StreamingParser};

/// Сохраняет все ключи и значения из хранилища в файл ZDB.
/// Ключи и значения записываются попарно: сначала ключ, затем значение.
pub fn save_to_zdb(
    store: &InMemoryStore,
    path: &str,
) -> ZumicResult<()> {
    let file = File::create(path).context("Failed to create ZDB file")?;
    let mut writer = BufWriter::new(file);
    let items = store.iter().map(|(k, v)| (k.clone(), v.clone()));
    write_stream(&mut writer, items)
}

/// Загружает ключи и значения из файла ZDB в указанное хранилище.
/// Ожидается, что каждая пара состоит из строки-ключа и произвольного значения.
pub fn load_from_zdb(
    store: &mut InMemoryStore,
    path: &str,
) -> ZumicResult<()> {
    let file = File::open(path).context("Failed to open ZDB file")?;
    let mut parser = StreamingParser::new(file)?;

    let mut handler = CallbackHandler::new(|key, value| {
        store.set(&key, value).map_err(|e| {
            // временная конверсия старой ошибки -> StackError
            zumic_error::StackError::new(SimpleError(format!("store.set failed: {e}")))
        })
    });

    parser.parse(&mut handler)
}

// ВРЕМЕННАЯ локальная обёртка — можно жить с ней до полной миграции ошибок
#[derive(Debug)]
struct SimpleError(String);

impl std::fmt::Display for SimpleError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for SimpleError {}

impl zumic_error::ErrorExt for SimpleError {
    fn status_code(&self) -> StatusCode {
        StatusCode::Internal
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn client_message(&self) -> String {
        "Internal server error".to_string()
    }
    fn log_message(&self) -> String {
        self.0.clone()
    }
    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        vec![
            ("error_type", "simple".to_string()),
            ("status_code", self.status_code().to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::{Sds, Value};

    #[test]
    fn test_zdb_save_and_load_roundtrip() {
        // уникальное имя файла для теста (timestamp)
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let test_path = format!("test_zdb_roundtrip_{ts}.zdb");

        // 1. Создаём тестовое хранилище и наполняем его
        let store = InMemoryStore::default();
        store.set(&Sds::from_str("key1"), Value::Int(123)).unwrap();
        store
            .set(&Sds::from_str("key2"), Value::Bool(true))
            .unwrap();
        store
            .set(&Sds::from_str("key3"), Value::Str(Sds::from_str("hello")))
            .unwrap();

        // 2. Сохраняем его в файл
        save_to_zdb(&store, &test_path).unwrap();

        // 3. Загружаем в другое хранилище
        let mut loaded = InMemoryStore::default();
        load_from_zdb(&mut loaded, &test_path).unwrap();

        // 4. Проверяем, что количество ключей совпадает
        let store_count = store.iter().count();
        let loaded_count = loaded.iter().count();
        assert_eq!(
            store_count, loaded_count,
            "Key count mismatch: {store_count} vs {loaded_count}"
        );

        // 5. Проверяем, что все пары ключ->значение совпадают
        for (k, v) in store.iter() {
            let loaded_val = loaded.get(&k).unwrap();
            assert!(loaded_val.is_some(), "Key {k:?} not found after loading");
            assert_eq!(
                loaded_val,
                Some(v.clone()),
                "Value mismatch for key {k:?}: expected {v:?}, got {loaded_val:?}"
            );
        }

        // cleanup
        let _ = fs::remove_file(&test_path);
    }

    #[test]
    fn test_empty_store_roundtrip() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let test_path = format!("test_zdb_empty_{ts}.zdb");

        let store = InMemoryStore::default();
        save_to_zdb(&store, &test_path).unwrap();

        let mut loaded = InMemoryStore::default();
        load_from_zdb(&mut loaded, &test_path).unwrap();

        assert_eq!(store.iter().count(), 0);
        assert_eq!(loaded.iter().count(), 0);

        let _ = std::fs::remove_file(&test_path);
    }

    #[test]
    fn test_large_value_roundtrip() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let test_path = format!("test_zdb_large_{ts}.zdb");

        let store = InMemoryStore::default();
        let big = "x".repeat(200_000); // 200 KB, можно увеличить
        store
            .set(
                &crate::Sds::from_str("big"),
                crate::Value::Str(crate::Sds::from_str(&big)),
            )
            .unwrap();

        save_to_zdb(&store, &test_path).unwrap();

        let mut loaded = InMemoryStore::default();
        load_from_zdb(&mut loaded, &test_path).unwrap();

        assert_eq!(store.iter().count(), loaded.iter().count());
        assert_eq!(
            loaded.get(&crate::Sds::from_str("big")).unwrap(),
            Some(crate::Value::Str(crate::Sds::from_str(&big)))
        );

        let _ = std::fs::remove_file(&test_path);
    }

    #[test]
    fn test_many_items_roundtrip() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let test_path = format!("test_zdb_many_{ts}.zdb");

        let store = InMemoryStore::default();
        let n = 1_000; // подними до 10_000 для интеграционного теста
        for i in 0..n {
            let key = format!("k{i}");
            store
                .set(&crate::Sds::from_str(&key), crate::Value::Int(i as i64))
                .unwrap();
        }

        save_to_zdb(&store, &test_path).unwrap();

        let mut loaded = InMemoryStore::default();
        load_from_zdb(&mut loaded, &test_path).unwrap();

        assert_eq!(store.iter().count(), loaded.iter().count());
        for i in 0..n {
            let key = crate::Sds::from_str(&format!("k{i}"));
            let v = loaded.get(&key).unwrap();
            assert_eq!(v, Some(crate::Value::Int(i as i64)));
        }

        let _ = std::fs::remove_file(&test_path);
    }

    #[test]
    fn test_truncated_file_fails() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let test_path = format!("test_zdb_trunc_{ts}.zdb");

        // prepare a normal file
        let store = InMemoryStore::default();
        store
            .set(&crate::Sds::from_str("k"), crate::Value::Int(1))
            .unwrap();
        save_to_zdb(&store, &test_path).unwrap();

        // truncate last byte(s)
        let meta = std::fs::metadata(&test_path).unwrap();
        let len = meta.len();
        assert!(len > 0);
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(&test_path)
            .unwrap();
        // отрезаем последний байт (если файл маленький, отрезаем 1)
        let new_len = len.saturating_sub(1);
        f.set_len(new_len).unwrap();
        drop(f);

        // load should return Err (Unexpected EOF / InvalidData)
        let mut loaded = InMemoryStore::default();
        let res = load_from_zdb(&mut loaded, &test_path);
        assert!(
            res.is_err(),
            "Expected error when loading truncated file, got Ok"
        );

        let _ = std::fs::remove_file(&test_path);
    }
}
