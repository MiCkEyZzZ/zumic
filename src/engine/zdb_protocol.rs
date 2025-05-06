use std::{
    fs::File,
    io::{BufReader, BufWriter},
};

use crate::Value;

use super::{
    zdb::{decode::read_value, encode::write_value},
    InMemoryStore, Storage,
};

pub fn save_to_zdb(store: &InMemoryStore, path: &str) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    for (k, v) in store.iter() {
        // сначада key, потом само v.
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

        // Сохраняем в стор
        store
            .set(&k, v)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))?;
    }

    Ok(())
}
