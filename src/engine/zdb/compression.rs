use zstd::stream::{decode_all, encode_all};

/// Минимальный размер для попытки сжатия (например, 64 байта)
const MIN_COMPRESSION_SIZE: usize = 64;

/// Проверяет, стоит ли пытаться сжимать
pub fn should_compress(size: usize) -> bool {
    size >= MIN_COMPRESSION_SIZE
}

/// Сжимает блок байтов с помощью ZSTD
pub fn compress_block(data: &[u8]) -> std::io::Result<Vec<u8>> {
    // Уровень сжатия: 3 — баланс между скоростью и размером
    encode_all(data, 3)
}

/// Распаковывает блок ZSTD
pub fn decompress_block(data: &[u8]) -> std::io::Result<Vec<u8>> {
    decode_all(data)
}
