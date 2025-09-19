#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
use zumic::engine::{compress_block, read_value_with_version, FormatVersion, TAG_COMPRESSED};

fuzz_target!(|data: &[u8]| {
    // 1) Простейший проход: пытаемся декодировать содержимое как есть (V1 и V2).
    for &version in &[FormatVersion::V1, FormatVersion::V2] {
        let _ = std::panic::catch_unwind(|| {
            let mut cursor = Cursor::new(data);
            let _ = read_value_with_version(&mut cursor, version);
        });
    }

    // 2) Если это compressed-блок (первый байт TAG_COMPRESSED), делаем дополнительные проверки
    if !data.is_empty() && data[0] == TAG_COMPRESSED {
        // Создаём копию и МУТИРУЕМ её полностью
        let mut corrupted = data.to_vec();

        if corrupted.len() > 5 {
            corrupted[5] ^= 0xFF;
            let mid = corrupted.len() / 2;
            corrupted[mid] ^= 0xAA;
        } else if !corrupted.is_empty() {
            let last = corrupted.len() - 1;
            corrupted[last] ^= 0x11;
        }

        // Используем только immutable срезы внутри catch_unwind
        let corrupted_slice: &[u8] = &corrupted;
        for &version in &[FormatVersion::V1, FormatVersion::V2] {
            let _ = std::panic::catch_unwind(|| {
                let mut cursor = Cursor::new(corrupted_slice);
                let _ = read_value_with_version(&mut cursor, version);
            });
        }

        // 3) Попробуем пересажать "сырые" данные (после тега) и прочитать результат
        let raw_data: &[u8] = if data.len() > 1 { &data[1..] } else { &[] };
        if let Ok(recompressed) = compress_block(raw_data) {
            let mut buf = Vec::with_capacity(1 + 4 + recompressed.len());
            buf.push(TAG_COMPRESSED);
            buf.extend(&(recompressed.len() as u32).to_be_bytes());
            buf.extend(&recompressed);

            let buf_slice: &[u8] = &buf;
            for &version in &[FormatVersion::V1, FormatVersion::V2] {
                let _ = std::panic::catch_unwind(|| {
                    let mut cursor = Cursor::new(buf_slice);
                    let _ = read_value_with_version(&mut cursor, version);
                });
            }
        }
    }
});
