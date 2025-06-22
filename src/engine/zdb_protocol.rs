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
pub fn save_to_zdb(store: &InMemoryStore, path: &str) -> std::io::Result<()> {
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
pub fn load_from_zdb(store: &mut InMemoryStore, path: &str) -> std::io::Result<()> {
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
