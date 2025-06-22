use std::{
    fs::File,
    io::{BufReader, BufWriter},
};

use super::{
    zdb::{read_value, write_value},
    InMemoryStore, Storage,
};
use crate::Value;

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
) -> std::io::Result<()> {
    let mut file = BufReader::new(File::open(path)?);

    loop {
        // Читаем ключ или выходим при достижении конца файла
        let key_val = match read_value(&mut file) {
            Ok(val) => val,
            Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };

        // Проверяем, что ключ — это строка
        let k = if let Value::Str(k) = key_val {
            k
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Expected Str variant for key",
            ));
        };

        // Читаем связанное значение
        let v = read_value(&mut file)?;

        // Сохраняем пару ключ-значение в хранилище
        store
            .set(&k, v)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::Sds;

    use super::*;

    #[test]
    fn test_zdb_save_and_load_roundtrip() {
        // уникальное имя файла для теста
        let test_path = "test_zdb_roundtrip.zdb";

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
        save_to_zdb(&store, test_path).unwrap();

        // 3. Загружаем в другое хранилище
        let mut loaded = InMemoryStore::default();
        load_from_zdb(&mut loaded, test_path).unwrap();

        // 4. Проверяем, что количество ключей совпадает
        let store_count = store.iter().count();
        let loaded_count = loaded.iter().count();
        assert_eq!(
            store_count, loaded_count,
            "Key count mismatch: {} vs {}",
            store_count, loaded_count
        );

        // 5. Проверяем, что все пары ключ->значение совпадают
        for (k, v) in store.iter() {
            // вызов .get() возвращает Result<Option<Value>, _>, unwrap() гарантирует panic при Err
            let loaded_val = loaded.get(&k).unwrap();
            assert!(loaded_val.is_some(), "Key {:?} not found after loading", k);
            // сравниваем значение: клонируем v, чтобы получить Value, и оборачиваем в Some
            assert_eq!(
                loaded_val,
                Some(v.clone()),
                "Value mismatch for key {:?}: expected {:?}, got {:?}",
                k,
                v,
                loaded_val
            );
        }
    }
}
