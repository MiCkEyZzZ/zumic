#![no_main]

use std::io::Cursor;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use zumic::{
    engine::{read_value_with_version, FormatVersion},
    Value,
};

#[derive(Debug, Arbitrary)]
enum FuzzVersion {
    V1,
    V2,
}

#[derive(Debug, Arbitrary)]
struct FuzzInput {
    data: Vec<u8>,
    version: FuzzVersion,
}

impl From<FuzzVersion> for FormatVersion {
    fn from(v: FuzzVersion) -> Self {
        match v {
            FuzzVersion::V1 => FormatVersion::V1,
            FuzzVersion::V2 => FormatVersion::V2,
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    let mut cursor = Cursor::new(&input.data);
    let version = input.version.into();

    // Основной тест - decoder не должен паниковать ни на каких данных.
    let result = std::panic::catch_unwind(|| read_value_with_version(&mut cursor, version));

    // Если паника произошла, то это ошибка (логично :-))
    if result.is_err() {
        panic!("Decoder panicked on input: {input:?}");
    }

    // Если декодирование успешно, проверяем, что результат валидный
    if let Ok(Ok(value)) = result {
        // Базовая проверка, что результат корректный
        validate_decoded_value(&value);

        // Если получилось декодировать, попробуем энкодировать обратно
        // Это действие так же не должно вызвать панику.
        let encode_result = std::panic::carch_unwind(|| {
            let mut buf = Vec::new();
            write_value_with_version(&mut buf, &value, version).expected("Encoding failed");
        });

        if encode_result.is_err() {
            panic!("Encoder panicked on decoded value: {value:?}");
        }
    }
});

/// Проверяем, что декодированное Value имеет валидную структуру.
fn validate_decoded_value(value: &Value) {
    match value {
        Value::Str(s) => {}
        Value::Int(i) => {}
        Value::Float(f) => {}
        Value::Bool(b) => {}
        Value::Hash(h) => {}
        Value::Set(set) => {}
        Value::ZSet { dict, sorted } => {}
        Value::Array(arr) => {}
        Value::List(list) => {}
        Value::Bitmap(data) => {}
        Value::HyperLogLog(data) => {}
        Value::SStream(entries) => {}
        Value::Null => {}
    }
}
