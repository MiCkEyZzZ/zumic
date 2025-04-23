use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::{borrow::Cow, collections::HashMap};
use zumic::network::zsp::frame::{encoder::ZSPEncoder, zsp_types::ZSPFrame};

fn bench_inline_string(c: &mut Criterion) {
    let frame = ZSPFrame::InlineString("hello".into());
    c.bench_function("encode_inline_string", |b| {
        b.iter(|| ZSPEncoder::encode(black_box(&frame)).unwrap())
    });
}

fn bench_binary_string(c: &mut Criterion) {
    let data = vec![0u8; 1024];
    let frame = ZSPFrame::BinaryString(Some(data));
    c.bench_function("encode_binary_string_1KB", |b| {
        b.iter(|| ZSPEncoder::encode(black_box(&frame)).unwrap())
    });
}

fn bench_integer(c: &mut Criterion) {
    let frame = ZSPFrame::Integer(123456789);
    c.bench_function("encode_integer", |b| {
        b.iter(|| ZSPEncoder::encode(black_box(&frame)).unwrap())
    });
}

fn bench_array(c: &mut Criterion) {
    let frame = ZSPFrame::Array(vec![
        ZSPFrame::InlineString(Cow::Owned("a".repeat(10))),
        ZSPFrame::Integer(123),
        ZSPFrame::Float(3.1415),
    ]);
    c.bench_function("encode_small_array", |b| {
        b.iter(|| ZSPEncoder::encode(black_box(&frame)).unwrap())
    });
}

fn bench_dictionary(c: &mut Criterion) {
    let mut map: HashMap<Cow<'_, str>, ZSPFrame<'_>> = HashMap::new();
    for i in 0..10 {
        map.insert(
            Cow::Owned(format!("key{}", i)),
            ZSPFrame::InlineString(Cow::Owned(format!("val{}", i))),
        );
    }

    let frame = ZSPFrame::Dictionary(map);
    c.bench_function("encode_dictionary_10_items", |b| {
        b.iter(|| ZSPEncoder::encode(black_box(&frame)).unwrap())
    });
}

fn bench_zset(c: &mut Criterion) {
    let mut zset = Vec::new();
    for i in 0..50 {
        zset.push((format!("member{}", i), i as f64));
    }
    let frame = ZSPFrame::ZSet(zset);
    c.bench_function("encode_zset_50_entries", |b| {
        b.iter(|| ZSPEncoder::encode(black_box(&frame)).unwrap())
    });
}

criterion_group!(
    benches,
    bench_inline_string,
    bench_binary_string,
    bench_integer,
    bench_array,
    bench_dictionary,
    bench_zset,
);
criterion_main!(benches);
