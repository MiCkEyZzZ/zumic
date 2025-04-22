use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::network::zsp::{frame::zsp_types::ZSPFrame, protocol::parser::parse_command};

fn bench_parse_set_inline_string(c: &mut Criterion) {
    let frame = ZSPFrame::Array(Some(vec![
        ZSPFrame::InlineString("SET".to_string()),
        ZSPFrame::InlineString("mykey".to_string()),
        ZSPFrame::InlineString("myvalue".to_string()),
    ]));

    c.bench_function("parse_command - SET (InlineString)", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

fn bench_parse_set_binary_string(c: &mut Criterion) {
    let frame = ZSPFrame::Array(Some(vec![
        ZSPFrame::BinaryString(Some(b"SET".to_vec())),
        ZSPFrame::BinaryString(Some(b"mykey".to_vec())),
        ZSPFrame::BinaryString(Some(b"myvalue".to_vec())),
    ]));

    c.bench_function("parse_command - SET (BinaryString)", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

fn bench_parse_get(c: &mut Criterion) {
    let frame = ZSPFrame::Array(Some(vec![
        ZSPFrame::InlineString("GET".to_string()),
        ZSPFrame::InlineString("key".to_string()),
    ]));

    c.bench_function("parse_command - GET", |b| {
        b.iter(|| {
            let _ = parse_command(black_box(frame.clone())).unwrap();
        });
    });
}

fn bench_parse_set_with_integer(c: &mut Criterion) {
    let frame = ZSPFrame::Array(Some(vec![
        ZSPFrame::InlineString("SET".to_string()),
        ZSPFrame::InlineString("num".to_string()),
        ZSPFrame::Integer(12345),
    ]));

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
