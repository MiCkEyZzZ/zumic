use bytes::BytesMut;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::database::sds::Sds;

const INLINE_STR: &str = "short_str"; // длина < 22
const HEAP_STR: &str = "this string is definitely longer than inline cap";

fn bench_sds_inline_push(c: &mut Criterion) {
    c.bench_function("Sds (inline) push", |b| {
        b.iter(|| {
            let mut s = Sds::from_str(INLINE_STR);
            s.push(b'x');
            black_box(s);
        })
    });
}

fn bench_sds_heap_push(c: &mut Criterion) {
    c.bench_function("Sds (heap) push", |b| {
        b.iter(|| {
            let mut s = Sds::from_str(HEAP_STR);
            s.push(b'x');
            black_box(s);
        })
    });
}

fn bench_vec_push(c: &mut Criterion) {
    c.bench_function("Vec<u8>::push", |b| {
        b.iter(|| {
            let mut v = Vec::from(INLINE_STR.as_bytes());
            v.push(b'x');
            black_box(v);
        })
    });
}

fn bench_bytesmut_push(c: &mut Criterion) {
    c.bench_function("BytesMut::push", |b| {
        b.iter(|| {
            let mut bm = BytesMut::from(INLINE_STR);
            bm.extend_from_slice(b"x");
            black_box(bm);
        })
    });
}

fn bench_string_push(c: &mut Criterion) {
    c.bench_function("String::push", |b| {
        b.iter(|| {
            let mut s = String::from(INLINE_STR);
            s.push('x');
            black_box(s);
        })
    });
}

fn bench_sds_append_inline(c: &mut Criterion) {
    c.bench_function("Sds (inline) append", |b| {
        b.iter(|| {
            let mut s = Sds::from_str(INLINE_STR);
            s.append(b"more");
            black_box(s);
        })
    });
}

fn bench_sds_append_heap(c: &mut Criterion) {
    c.bench_function("Sds (heap) append", |b| {
        b.iter(|| {
            let mut s = Sds::from_str(HEAP_STR);
            s.append(b"even more data");
            black_box(s);
        })
    });
}

fn bench_sds_slice_range(c: &mut Criterion) {
    c.bench_function("Sds slice_range", |b| {
        b.iter(|| {
            let s = Sds::from_str(HEAP_STR);
            let slice = s.slice_range(5, 15);
            black_box(slice);
        })
    });
}

fn bench_sds_reserve(c: &mut Criterion) {
    c.bench_function("Sds reserve (from inline)", |b| {
        b.iter(|| {
            let mut s = Sds::from_str("a");
            s.reserve(100);
            black_box(s);
        })
    });
}

fn bench_sds_clear(c: &mut Criterion) {
    c.bench_function("Sds clear", |b| {
        b.iter(|| {
            let mut s = Sds::from_str(HEAP_STR);
            s.clear();
            black_box(s);
        })
    });
}

criterion_group!(
    benches,
    bench_sds_inline_push,
    bench_sds_heap_push,
    bench_sds_append_inline,
    bench_sds_append_heap,
    bench_sds_slice_range,
    bench_sds_reserve,
    bench_sds_clear,
    bench_vec_push,
    bench_bytesmut_push,
    bench_string_push,
);
criterion_main!(benches);
