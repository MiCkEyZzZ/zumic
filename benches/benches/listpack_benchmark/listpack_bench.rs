use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::ListPack;

fn bench_push_back_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("listpack_push_back_small");

    for &size in &[8usize, 16, 32, 64] {
        let value = vec![42u8; size];

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &value, |b, v| {
            b.iter(|| {
                let mut lp = ListPack::new();
                for _ in 0..10_000 {
                    lp.push_back(black_box(v));
                }
                black_box(lp);
            });
        });
    }

    group.finish();
}

fn bench_push_front_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("listpack_push_front_small");

    let value = vec![1u8; 16];
    group.bench_function("push_front_10k", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for _ in 0..10_000 {
                lp.push_front(black_box(&value));
            }
            black_box(lp);
        });
    });

    group.finish();
}

fn bench_push_back_large(c: &mut Criterion) {
    let value = vec![7u8; 4096];

    c.bench_function("listpack_push_back_large_4k", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for _ in 0..1_000 {
                lp.push_back(black_box(&value));
            }
            black_box(lp);
        });
    });
}

fn bench_pop_front_fifo(c: &mut Criterion) {
    c.bench_function("listpack_pop_front_fifo", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for i in 0..20_000u32 {
                lp.push_back(&i.to_le_bytes());
            }

            while lp.pop_front().is_some() {}
            black_box(lp);
        });
    });
}

fn bench_pop_back_lifo(c: &mut Criterion) {
    c.bench_function("listpack_pop_back_lifo", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for i in 0..20_000u32 {
                lp.push_back(&i.to_le_bytes());
            }

            while lp.pop_back().is_some() {}
            black_box(lp);
        });
    });
}

fn bench_mixed_workload(c: &mut Criterion) {
    c.bench_function("listpack_mixed_workload", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0..10_000u32 {
                if i & 1 == 0 {
                    lp.push_back(&i.to_le_bytes());
                } else {
                    lp.push_front(&i.to_le_bytes());
                }
            }

            for _ in 0..5_000 {
                lp.pop_front();
                lp.pop_back();
            }

            black_box(lp);
        });
    });
}

fn bench_iteration(c: &mut Criterion) {
    c.bench_function("listpack_iter_10k", |b| {
        let mut lp = ListPack::new();
        for i in 0..10_000u32 {
            lp.push_back(&i.to_le_bytes());
        }

        b.iter(|| {
            let mut sum = 0u64;
            for v in lp.iter() {
                sum += v.len() as u64;
            }
            black_box(sum);
        });
    });
}

fn bench_remove_middle(c: &mut Criterion) {
    c.bench_function("listpack_remove_middle", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for i in 0u32..5_000 {
                lp.push_back(&i.to_le_bytes());
            }

            lp.remove(black_box(2_500));
            black_box(lp);
        });
    });
}

criterion_group!(
    listpack,
    bench_push_back_small,
    bench_push_front_small,
    bench_push_back_large,
    bench_pop_front_fifo,
    bench_pop_back_lifo,
    bench_mixed_workload,
    bench_iteration,
    bench_remove_middle,
);

criterion_main!(listpack);
