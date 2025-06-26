use std::{hint::black_box, io::Cursor};

use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{
    engine::zdb::{
        decode::{read_dump, read_value, StreamReader},
        encode::{write_dump, write_stream, write_value},
    },
    Sds, SmartHash, Value,
};

fn encode_value(v: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    write_value(&mut buf, v).unwrap();
    buf
}

fn bench_read_value_variants(c: &mut Criterion) {
    // Int
    let buf_int = encode_value(&Value::Int(42));
    c.bench_function("read_value Int", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_int));
            read_value(&mut cur).unwrap()
        })
    });

    // Float
    let buf_f = encode_value(&Value::Float(std::f64::consts::PI));
    c.bench_function("read_value Float", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_f));
            read_value(&mut cur).unwrap()
        })
    });

    // Bool
    let buf_b = encode_value(&Value::Bool(true));
    c.bench_function("read_value Bool", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_b));
            read_value(&mut cur).unwrap()
        })
    });

    // Null
    let buf_n = encode_value(&Value::Null);
    c.bench_function("read_value Null", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_n));
            read_value(&mut cur).unwrap()
        })
    });

    // Short Str
    let buf_s = encode_value(&Value::Str(Sds::from_str("short")));
    c.bench_function("read_value short Str", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_s));
            read_value(&mut cur).unwrap()
        })
    });

    // Long Str
    let raw = "x".repeat(256);
    let buf_ls = encode_value(&Value::Str(Sds::from_str(&raw)));
    c.bench_function("read_value long Str", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_ls));
            read_value(&mut cur).unwrap()
        })
    });

    // Hash
    let mut hmap = SmartHash::new();
    hmap.insert(Sds::from_str("a"), Sds::from_str("1"));
    hmap.insert(Sds::from_str("b"), Sds::from_str("2"));
    let buf_hash = encode_value(&Value::Hash(hmap));
    c.bench_function("read_value Hash", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf_hash));
            read_value(&mut cur).unwrap()
        })
    });
}

fn bench_read_dump(c: &mut Criterion) {
    let items: Vec<(Sds, Value)> = (0..1000)
        .map(|i| (Sds::from_str(&format!("key{i}")), Value::Int(i)))
        .collect();
    let mut buf = Vec::new();
    write_dump(&mut buf, items.clone().into_iter()).unwrap();

    c.bench_function("read_dump 1000 ints", |b| {
        b.iter(|| {
            let mut cur = Cursor::new(black_box(&buf));
            read_dump(&mut cur).unwrap()
        })
    });
}

fn bench_stream_reader(c: &mut Criterion) {
    let items: Vec<(Sds, Value)> = (0..1000)
        .map(|i| (Sds::from_str(&format!("key{i}")), Value::Int(i)))
        .collect();
    let mut buf = Vec::new();
    write_stream(&mut buf, items.clone().into_iter()).unwrap();

    c.bench_function("stream_reader 1000 ints", |b| {
        b.iter(|| {
            let reader = StreamReader::new(Cursor::new(black_box(&buf))).unwrap();
            reader.for_each(|res| {
                let _ = black_box(res.unwrap());
            });
        })
    });
}

criterion_group!(
    decode_benches,
    bench_read_value_variants,
    bench_read_dump,
    bench_stream_reader
);
criterion_main!(decode_benches);
