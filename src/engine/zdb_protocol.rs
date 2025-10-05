use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read},
};

use super::{
    zdb::{read_value, write_value},
    InMemoryStore, Storage,
};
use crate::{engine::read_value_with_version, Value};

/// Сохраняет все ключи и значения из хранилища в файл ZDB.
/// Ключи и значения записываются попарно: сначала ключ, затем значение.
pub fn save_to_zdb(
    store: &InMemoryStore,
    path: &str,
) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    for (k, v) in store.iter() {
        // Сначала сохраняем ключ как Value::Str
        write_value(&mut file, &Value::Str(k.clone()))?;
        // Затем сохраняем соответствующее значение
        write_value(&mut file, &v)?;
    }

    Ok(())
}

/// Загружает ключи и значения из файла ZDB в указанное хранилище.
/// Ожидается, что каждая пара состоит из строки-ключа и произвольного значения.
pub fn load_from_zdb(
    store: &mut InMemoryStore,
    path: &str,
) -> io::Result<()> {
    let mut file = BufReader::new(File::open(path)?);
    // По умолчанию используем текущую версию reader-а (read_value)
    load_from_reader(store, &mut file)
}

/// Новая функция — читает пары <key, value> из произвольного `Read`.
/// Удобна для unit-тестов и для fuzzing (можно подавать Cursor).
///
/// Использует `read_value_with_version` если он доступен (чтобы фаззить разные
/// версии), но по умолчанию вызывает `read_value` (текущее поведение).
pub fn load_from_reader<R: Read>(
    store: &mut InMemoryStore,
    r: &mut R,
) -> io::Result<()> {
    loop {
        // Читаем ключ или выходим при EOF
        // Сначала пробуем версионный reader (если экспортирован), иначе fallback на
        // read_value.
        let key_val = match read_value_with_version(r, crate::engine::zdb::file::FormatVersion::V1)
            .or_else(|_| read_value(r))
        {
            Ok(v) => v,
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };

        // Проверяем, что ключ — это строка
        let k = if let Value::Str(k) = key_val {
            k
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected Str variant for key",
            ));
        };

        // Читаем связанное значение
        let v = match read_value_with_version(r, crate::engine::zdb::file::FormatVersion::V1)
            .or_else(|_| read_value(r))
        {
            Ok(val) => val,
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Unexpected EOF while reading value for key",
                ));
            }
            Err(e) => return Err(e),
        };

        // Сохраняем пару ключ-значение в хранилище
        store
            .set(&k, v)
            .map_err(|e| io::Error::other(format!("{e:?}")))?
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::Sds;

    #[test]
    fn test_zdb_save_and_load_roundtrip() {
        // уникальное имя файла для теста (timestamp)
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let test_path = format!("test_zdb_roundtrip_{}.zdb", ts);

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
}
