#![no_main]

use std::io::Cursor;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use zumic::{
    engine::{read_value_with_version, FormatVersion, TAG_COMPRESSED},
    Value,
};

#[derive(Debug, Arbitrary)]
enum FuzzVersion {
    V1,
    V2,
}

#[derive(Debug, Arbitrary)]
struct CompressedFuzzInput {
    /// Повреждённые ZSTD данные
    corrupt_zstd_data: Vec<u8>,
    /// Валидный заголовок или тоже повреждённый
    header_data: Vec<u8>,
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

fuzz_target!(|input: CompressedFuzzInput| {
    // Создаём буфер с TAG_COMPRESSED + повреждёнными данными
    let mut test_data = Vec::new();

    // Добавляем TAG_COMPRESSED
    test_data.push(TAG_COMPRESSED);

    // Добавляем произвольный header данные
    test_data.extend_from_slice(&imput.corrupt_zstd_data);

    let mut cursor = Cursor::new(&test_data);
    let version = input.version.into();

    // Тестируем, что decoder не паникует на повреждённых compressed данных
    let result = std::panic::catch_unwind(|| {
        read_value_with_version(&mut cursor, version);
    });

    match result {
        Ok(decode_result) => {
            match decode_result {
                Ok(_value) => {
                    // Если somehow декодирование прошло успешно, это нормально
                    // Главное что не было panic
                }
                Err(_err) => {
                    // Ошибки декодирования ожидаемы для corrupt данных
                    // Главное что это не panic
                }
            }
        }
        Err(_panic) => {
            panic!(
                "Decoder panicked on compressed data: header={:?}, zstd_data={:?}",
                input.header_data, input.corrupt_zstd_data
            );
        }
    }
});
