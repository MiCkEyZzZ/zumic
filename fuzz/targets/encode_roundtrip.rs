#![no_main]
use std::{io::Cursor, panic};

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use zumic::{
    engine::{read_value_with_version, write_value_versioned, FormatVersion},
    Value,
};

/// Простая генерация произвольного Value из `Unstructed`.
/// Ограничиваем рекурсию глубиной.
fn arb_value(
    u: &mut Unstructed<'_>,
    depth: usize,
) -> arbitrary::Result<Value> {
    if depth == 0 {
        // только примитивы на глубине 0
        let leaf = u.int_in_range::<u8>(0..=3)?;
        return match leaf {
            0 => Ok(Value::Null),
            1 => {
                let b = u.arbitrary::<bool>()?;
                Ok(Value::Bool(b))
            }
            2 => {
                let i = u.arbitrary::<i64>()?;
                Ok(Value::Int(i))
            }
            _ => {
                let s: String = u.arbitrary()?;
                Ok(Value::Str(s.into()))
            }
        };
    }

    let choice = u.int_in_range::<u8>(0..=10)?;
    match choice {
        0 => Ok(Value::Null),
        1 => {
            let b = u.arbitrary::<bool>()?;
            Ok(Value::Bool(b))
        }
        2 => {
            let i = u.arbitrary::<i64>()?;
            Ok(Value::Int(i))
        }
        3 => {
            let f = u.arbitrary::<f64>()?;
            Ok(Value::Float(f))
        }
        4 => {
            let s = u.arbitrary()?;
            Ok(Value::Str(s.into()))
        }
        5 => {
            let len = u.int_in_range::<u8>(0..=6)? as usize;
            let mut v = Vec::with_capacity(len);
            for _ in 0..len {
                if let Ok(it) = arb_value(u, depth - 1) {
                    v.push(it);
                }
            }
            Ok(Value::Array(v))
        }
        6 => {
            // simple List (vector of sds-like strings)
            let len = u.int_in_range::<u8>(0..=6)? as usize;
            let mut list = Vec::new();
            for _ in 0..len {
                let s: String = u.arbitrary()?;
                list.push(s.into());
            }
            Ok(Value::List(list))
        }
        7 => {
            // Set - use Vec then convert
            let len = u.int_in_range::<u8>(0..=6)? as usize;
            let mut set = std::collections::HashSet::new();
            for _ in 0..len {
                let s: String = u.arbitrary()?;
                set.insert(s.into());
            }
            Ok(Value::Set(set))
        }
        8 => {
            // Hash: small map with string values only (to match read expectations)
            let len = u.int_in_range::<u8>(0..=6)? as usize;
            let mut map = crate::SmartHash::new();
            for _ in 0..len {
                let k: String = u.arbitrary()?;
                let v: String = u.arbitrary()?;
                map.insert(k.into(), v.into());
            }
            Ok(Value::Hash(map))
        }
        9 => {
            // ZSet: a few elements with f64 scores
            let len = u.int_in_range::<u8>(0..=6)? as usize;
            let mut dict = crate::Dict::new();
            let mut sorted = crate::SkipList::new();
            for _ in 0..len {
                let k: String = u.arbitrary()?;
                let score = u.arbitrary::<f64>()?;
                let sds = k.into();
                dict.insert(sds.clone(), score);
                sorted.insert(ordered_float::OrderedFloat(score), sds);
            }
            Ok(Value::ZSet { dict, sorted })
        }
        _ => {
            // Bitmap / HLL / SStream are harder; fallback to Str
            let s: String = u.arbitrary()?;
            Ok(Value::Str(s.into()))
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // пытаемся построить value; если не получается — скипаем
    if let Ok(v) = arb_value(&mut u, 3) {
        // 1) Roundtrip для каждой версии (включая compress-ветку)
        for &version in &[FormatVersion::V1, FormatVersion::V2, FormatVersion::V3] {
            let _ = panic::catch_unwind(|| {
                // сериализуем (включая авто-сжатие в write_value_versioned)
                let mut buf = Vec::new();
                let _ = write_value_versioned(&mut buf, &v, version).ok();

                // читаем назад
                let mut cursor = Cursor::new(&buf);
                let _ = read_value_with_version(&mut cursor, version, None, 0).ok();
            });
        }

        // 2) force-compress path: если писатель не сжал (мало данных), попробуем
        // создать большой строковый blob, чтобы гарантированно попасть в compress
        // ветку
        if let Value::Str(_) = &v {
            // создаём больший буфер, повторяя строку
            let mut big = Vec::new();
            for _ in 0..512 {
                // повтор
                if let Value::Str(s) = &v {
                    big.extend_from_slice(s.as_bytes());
                }
            }
            let big_s = String::from_utf8_lossy(&big).to_string();
            let v_big = Value::Str(big_s.into());

            for &version in &[FormatVersion::V1, FormatVersion::V2] {
                let _ = panic::catch_unwind(|| {
                    let mut buf = Vec::new();
                    let _ = write_value_versioned(&mut buf, &v_big, version).ok();
                    let mut cursor = Cursor::new(&buf);
                    let _ = read_value_with_version(&mut cursor, version, None, 0).ok();
                });
            }
        }
    }
});
