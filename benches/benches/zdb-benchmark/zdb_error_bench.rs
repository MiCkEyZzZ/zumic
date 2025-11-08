use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use zumic_error::{CompressionOp, ErrorExt, ZdbError, ZdbVersionError};

fn make_corrupted() -> ZdbError {
    ZdbError::CorruptedData {
        reason: "something went wrong".to_string(),
        offset: None,
        key: None,
        expected: Some("expected".to_string()),
        got: Some("got".to_string()),
    }
}

fn make_compression() -> ZdbError {
    ZdbError::CompressionError {
        operation: CompressionOp::Decompress,
        reason: "zstd failed".to_string(),
        offset: None,
        key: None,
        compressed_size: Some(12345),
    }
}

fn make_version() -> ZdbError {
    ZdbError::Version(ZdbVersionError::UnsupportedVersion {
        found: 5,
        supported: vec![1, 2, 3],
        offset: None,
        key: None,
    })
}

pub fn bench_create_clone(c: &mut Criterion) {
    c.bench_function("create_corrupted", |b| {
        b.iter(|| {
            let e = ZdbError::CorruptedData {
                reason: "oops".into(),
                offset: None,
                key: None,
                expected: None,
                got: None,
            };
            black_box(e);
        })
    });

    let base = make_corrupted();
    c.bench_function("clone_corrupted", |b| {
        b.iter(|| {
            black_box(base.clone());
        })
    });
}

pub fn bench_with_consuming_methods(c: &mut Criterion) {
    let base = make_corrupted();
    c.bench_function("with_offset_consuming", |b| {
        b.iter(|| {
            // .with_offset consumes self, so clone before calling to preserve base
            black_box(base.clone().with_offset(0xDEADBEEF));
        })
    });

    c.bench_function("with_key_consuming", |b| {
        b.iter(|| {
            black_box(base.clone().with_key("some-key"));
        })
    });
}

pub fn bench_inplace_methods(c: &mut Criterion) {
    c.bench_function("set_offset_inplace", |b| {
        b.iter(|| {
            let mut e = make_corrupted();
            e.set_offset(0x1234);
            black_box(e);
        })
    });

    c.bench_function("set_key_inplace", |b| {
        b.iter(|| {
            let mut e = make_corrupted();
            e.set_key("k");
            black_box(e);
        })
    });
}

pub fn bench_display_and_metrics(c: &mut Criterion) {
    let compress = make_compression();
    c.bench_function("display_compression", |b| {
        b.iter(|| {
            black_box(format!("{}", compress));
        })
    });

    let ver = make_version();
    c.bench_function("metrics_tags_version", |b| {
        b.iter(|| {
            black_box(ver.metrics_tags());
        })
    });

    c.bench_function("into_std_io_error", |b| {
        b.iter(|| {
            let e = make_corrupted();
            let io_err: std::io::Error = e.into();
            black_box(io_err);
        })
    });
}

criterion_group!(
    benches,
    bench_create_clone,
    bench_with_consuming_methods,
    bench_inplace_methods,
    bench_display_and_metrics
);
criterion_main!(benches);
