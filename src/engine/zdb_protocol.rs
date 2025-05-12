use std::{
    fs::File,
    io::{BufReader, BufWriter},
};

use super::{
    zdb::{read_value, write_value},
    InMemoryStore, Storage,
};
use crate::Value;

pub fn save_to_zdb(store: &InMemoryStore, path: &str) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    for (k, v) in store.iter() {
        // сначала key, потом само v.
        write_value(&mut file, &Value::Str(k.clone()))?;
        write_value(&mut file, &v)?;
    }

    Ok(())
}

pub fn load_from_zdb(store: &mut InMemoryStore, path: &str) -> std::io::Result<()> {
    let mut file = BufReader::new(File::open(path)?);

    loop {
        // Читаем ключ или выходим по EOF
        let key_val = match read_value(&mut file) {
            Ok(val) => val,
            Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };

        // Ожидаем, что ключ — именно Str
        let k = if let Value::Str(k) = key_val {
            k
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Expected Str variant for key",
            ));
        };

        // Читаем само значение
        let v = read_value(&mut file)?;

        // Сохраняем в хранилище
        store
            .set(&k, v)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::BufWriter;
    use std::io::Write;

    use byteorder::{BigEndian, WriteBytesExt};

    use super::*;
    use crate::{engine::TAG_INT, Sds};

    /// Проверяем, что сохранение и загрузка работают корректно для непустого хранилища.
    #[test]
    fn test_save_and_load_zdb() {
        let store: InMemoryStore = InMemoryStore::new();
        store.set(&Sds::from_str("k1"), Value::Int(123)).unwrap();
        store
            .set(&Sds::from_str("k2"), Value::Str(Sds::from_str("v2")))
            .unwrap();

        let path = std::env::temp_dir().join("dump.zdb");
        let path_str = path.to_str().unwrap();

        // сохраняем
        save_to_zdb(&store, path_str).expect("save_to_zdb failed");

        // загрузка в новый store
        let mut loaded: InMemoryStore = InMemoryStore::new();
        load_from_zdb(&mut loaded, path_str).expect("load_from_zdb failed");

        // проверка содержимого
        let got1 = loaded.get(&Sds::from_str("k1")).unwrap();
        assert_eq!(got1, Some(Value::Int(123)));

        let got2 = loaded.get(&Sds::from_str("k2")).unwrap();
        assert_eq!(got2, Some(Value::Str(Sds::from_str("v2"))));

        // чистим файл
        std::fs::remove_file(path).unwrap();
    }

    /// Проверяем, что при наличии не-Str тега в ключе возвращается ошибка InvalidData.
    #[test]
    fn test_load_invalid_key_type() {
        // Создаём файл, где первый записан TAG_IN вместо TAG_STR.
        let path = std::env::temp_dir().join("dump.zdb");
        let mut file = BufWriter::new(std::fs::File::create(&path).unwrap());

        // Запишем TAG_INT вместо Str для ключа
        file.write_u8(TAG_INT).unwrap();
        file.write_i64::<BigEndian>(42).unwrap();
        file.flush().unwrap();

        let mut store: InMemoryStore = InMemoryStore::new();
        let err = load_from_zdb(&mut store, path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

        std::fs::remove_file(path).unwrap();
    }

    /// Проверяем, что пустой файл не приводит к ошибке (просто ничего не загружается).
    #[test]
    fn test_load_empty_file() {
        let path = std::env::temp_dir().join("dump.rdb");
        std::fs::File::create(&path).unwrap(); // пустой файл

        let mut store: InMemoryStore = InMemoryStore::new();
        load_from_zdb(&mut store, path.to_str().unwrap()).expect("loading empty should not error");

        // Проверяем, что итератор сразу вернул None (нет элементов)
        assert!(store.iter().next().is_none());

        std::fs::remove_file(path).unwrap();
    }
}
