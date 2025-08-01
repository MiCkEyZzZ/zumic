use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::network::zsp::{frame::zsp_types::ZspFrame, protocol::parser::parse_command};

fn bench_parse_set_inline_string(c: &mut Criterion) {
    let frame = ZspFrame::Array(vec![
        ZspFrame::InlineString("SET".into()),
        ZspFrame::InlineString("mykey".into()),
        ZspFrame::InlineString("myvalue".into()),
    ]);

    c.bench_function("parse_command - SET (InlineString)", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

fn bench_parse_set_binary_string(c: &mut Criterion) {
    let frame = ZspFrame::Array(vec![
        ZspFrame::BinaryString(Some(b"SET".to_vec())),
        ZspFrame::BinaryString(Some(b"mykey".to_vec())),
        ZspFrame::BinaryString(Some(b"myvalue".to_vec())),
    ]);

    c.bench_function("parse_command - SET (BinaryString)", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

fn bench_parse_get(c: &mut Criterion) {
    let frame = ZspFrame::Array(vec![
        ZspFrame::InlineString("GET".into()),
        ZspFrame::InlineString("key".into()),
    ]);

    c.bench_function("parse_command - GET", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

fn bench_parse_set_with_integer(c: &mut Criterion) {
    let frame = ZspFrame::Array(vec![
        ZspFrame::InlineString("SET".into()),
        ZspFrame::InlineString("num".into()),
        ZspFrame::Integer(12345),
    ]);

    c.bench_function("parse_command - SET (Int)", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_parse_set_inline_string,
    bench_parse_set_binary_string,
    bench_parse_get,
    bench_parse_set_with_integer,
);
criterion_main!(benches);
