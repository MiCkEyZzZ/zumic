#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct VersionCompatInput {
    data: Vec<u8>,
    // Тестируем, что данные можно декодировать в разных версиях
    test_v1: bool,
    test_v2: bool,
}

fuzz_target!(|input: VersionCompatInput| {});
