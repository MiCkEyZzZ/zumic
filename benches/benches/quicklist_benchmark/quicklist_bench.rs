use std::{collections::VecDeque, hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zumic::QuickList;

const SIZES: [usize; 3] = [1_000usize, 10_000usize, 100_000usize];
const RANDOM_ACCESSES: usize = 1_000;

// Helper: fill containers with sequential integers 0..n-1
fn fill_vec(n: usize) -> Vec<i32> {
    (0..n as i32).collect()
}

fn fill_vecdeque(n: usize) -> VecDeque<i32> {
    (0..n as i32).collect::<Vec<_>>().into()
}

fn fill_quicklist(
    n: usize,
    seg_size: usize,
) -> QuickList<i32> {
    let mut q = QuickList::new(seg_size);
    for i in 0..n as i32 {
        q.push_back(i);
    }
    q
}

// Benchmark: bulk push_back
fn bench_push_back(c: &mut Criterion) {
    let mut g = c.benchmark_group("push_back_bulk");
    g.measurement_time(Duration::from_secs(5));
    for &size in SIZES.iter() {
        g.throughput(Throughput::Elements(size as u64));
        g.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut q = QuickList::new(256);
                for i in 0..n {
                    black_box(q.push_back(i as i32));
                }
                black_box(q);
            })
        });
        // Compare to VecDeque
        g.bench_with_input(BenchmarkId::new("VecDeque", size), &size, |b, &n| {
            b.iter(|| {
                let mut d = VecDeque::with_capacity(n);
                for i in 0..n {
                    black_box(d.push_back(i as i32));
                }
                black_box(d);
            })
        });
        // Compare to Vec
        g.bench_with_input(BenchmarkId::new("Vec", size), &size, |b, &n| {
            b.iter(|| {
                let mut v = Vec::with_capacity(n);
                for i in 0..n {
                    black_box(v.push(i as i32));
                }
                black_box(v);
            })
        });
    }
    g.finish();
}

// Benchmark: bulk push_front
fn bench_push_front(c: &mut Criterion) {
    let mut g = c.benchmark_group("push_front_bulk");
    g.measurement_time(Duration::from_secs(5));
    for &size in SIZES.iter() {
        g.throughput(Throughput::Elements(size as u64));
        g.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut q = QuickList::new(256);
                for i in 0..n {
                    black_box(q.push_front(i as i32));
                }
                black_box(q);
            })
        });

        g.bench_with_input(BenchmarkId::new("VecDeque", size), &size, |b, &n| {
            b.iter(|| {
                let mut d = VecDeque::with_capacity(n);
                for i in 0..n {
                    black_box(d.push_front(i as i32));
                }
                black_box(d);
            })
        });

        // Vec push_front is O(n) — include for reference but expensive
        g.bench_with_input(
            BenchmarkId::new("Vec_push_front_ref", size),
            &size,
            |b, &n| {
                b.iter(|| {
                    let mut v = Vec::with_capacity(n);
                    for i in 0..n {
                        v.insert(0, i as i32);
                    }
                    black_box(v);
                })
            },
        );
    }
    g.finish();
}

// Benchmark: pop_back / pop_front draining
fn bench_pop_drain(c: &mut Criterion) {
    let mut g = c.benchmark_group("pop_drain");
    g.measurement_time(Duration::from_secs(5));
    for &size in SIZES.iter() {
        g.throughput(Throughput::Elements(size as u64));
        // pop_back
        g.bench_with_input(
            BenchmarkId::new("QuickList_pop_back", size),
            &size,
            |b, &n| {
                b.iter(|| {
                    let mut q = fill_quicklist(n, 256);
                    while let Some(_) = q.pop_back() {}
                    black_box(q);
                })
            },
        );

        g.bench_with_input(
            BenchmarkId::new("VecDeque_pop_back", size),
            &size,
            |b, &n| {
                b.iter(|| {
                    let mut d = fill_vecdeque(n);
                    while d.pop_back().is_some() {}
                    black_box(d);
                })
            },
        );

        g.bench_with_input(BenchmarkId::new("Vec_pop_back", size), &size, |b, &n| {
            b.iter(|| {
                let mut v = fill_vec(n);
                while v.pop().is_some() {}
                black_box(v);
            })
        });

        // pop_front
        g.bench_with_input(
            BenchmarkId::new("QuickList_pop_front", size),
            &size,
            |b, &n| {
                b.iter(|| {
                    let mut q = fill_quicklist(n, 256);
                    while let Some(_) = q.pop_front() {}
                    black_box(q);
                })
            },
        );

        g.bench_with_input(
            BenchmarkId::new("VecDeque_pop_front", size),
            &size,
            |b, &n| {
                b.iter(|| {
                    let mut d = fill_vecdeque(n);
                    while d.pop_front().is_some() {}
                    black_box(d);
                })
            },
        );

        g.bench_with_input(
            BenchmarkId::new("Vec_pop_front_ref", size),
            &size,
            |b, &n| {
                b.iter(|| {
                    let mut v = fill_vec(n);
                    while !v.is_empty() {
                        v.remove(0);
                    }
                    black_box(v);
                })
            },
        );
    }
    g.finish();
}

// Benchmark: random_get (1000 random reads)
fn bench_random_get(c: &mut Criterion) {
    let mut g = c.benchmark_group("random_get_1k");
    g.measurement_time(Duration::from_secs(5));
    let mut rng = StdRng::seed_from_u64(0xDEADBEEF);

    for &size in SIZES.iter() {
        let indices: Vec<usize> = (0..RANDOM_ACCESSES)
            .map(|_| rng.gen_range(0..size))
            .collect();

        // Prepare containers
        let v = fill_vec(size);
        let d = fill_vecdeque(size);
        let q = fill_quicklist(size, 256);

        g.throughput(Throughput::Elements(RANDOM_ACCESSES as u64));

        g.bench_with_input(
            BenchmarkId::new("Vec_random_get", size),
            &indices,
            |b, idxs| {
                b.iter(|| {
                    for &i in idxs.iter() {
                        black_box(black_box(&v).get(i));
                    }
                })
            },
        );

        g.bench_with_input(
            BenchmarkId::new("VecDeque_random_get", size),
            &indices,
            |b, idxs| {
                b.iter(|| {
                    for &i in idxs.iter() {
                        black_box(black_box(&d).get(i));
                    }
                })
            },
        );

        g.bench_with_input(
            BenchmarkId::new("QuickList_random_get", size),
            &indices,
            |b, idxs| {
                // QuickList::get mutates internal cache; we clone to avoid cache interference
                // across iterations
                b.iter(|| {
                    let mut q_clone = q.clone();
                    for &i in idxs.iter() {
                        black_box(q_clone.get(i));
                    }
                    black_box(q_clone);
                })
            },
        );
    }
    g.finish();
}

// Benchmark: sequential iterator performance
fn bench_sequential_iter(c: &mut Criterion) {
    let mut g = c.benchmark_group("sequential_iter");
    g.measurement_time(Duration::from_secs(5));

    for &size in SIZES.iter() {
        let v = fill_vec(size);
        let d = fill_vecdeque(size);
        let q = fill_quicklist(size, 256);

        g.throughput(Throughput::Elements(size as u64));

        g.bench_with_input(BenchmarkId::new("Vec_iter", size), &v, |b, v| {
            b.iter(|| {
                for x in v.iter() {
                    black_box(x);
                }
            })
        });

        g.bench_with_input(BenchmarkId::new("VecDeque_iter", size), &d, |b, d| {
            b.iter(|| {
                for x in d.iter() {
                    black_box(x);
                }
            })
        });

        g.bench_with_input(BenchmarkId::new("QuickList_iter", size), &q, |b, q| {
            b.iter(|| {
                for x in q.iter() {
                    black_box(x);
                }
            })
        });
    }
    g.finish();
}

// Benchmark: into_vecdeque / flatten
fn bench_into_vecdeque(c: &mut Criterion) {
    let mut g = c.benchmark_group("into_vecdeque_flatten");
    g.measurement_time(Duration::from_secs(5));
    for &size in SIZES.iter() {
        g.throughput(Throughput::Elements(size as u64));

        let q = fill_quicklist(size, 128);
        g.bench_with_input(
            BenchmarkId::new("QuickList_into_vecdeque", size),
            &q,
            |b, q| {
                b.iter_batched(
                    || q.clone(),
                    |q_clone| {
                        let out = q_clone.into_vecdeque();
                        black_box(out);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );

        // flatten Vec<Vec<T>> as reference
        let vec_of_chunks: Vec<Vec<i32>> = (0..(size / 128 + 1))
            .map(|chunk| ((chunk * 128) as i32..((chunk + 1) * 128) as i32).collect())
            .collect();

        g.bench_with_input(
            BenchmarkId::new("Vec_chunks_flatten", size),
            &vec_of_chunks,
            |b, chunks| {
                b.iter(|| {
                    let mut out = Vec::with_capacity(size);
                    for ch in chunks.iter() {
                        out.extend(ch.iter().cloned());
                    }
                    black_box(out);
                });
            },
        );
    }
    g.finish();
}

pub fn criterion_benchmark(c: &mut Criterion) {
    bench_push_back(c);
    bench_push_front(c);
    bench_pop_drain(c);
    bench_random_get(c);
    bench_sequential_iter(c);
    bench_into_vecdeque(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(100);
    targets = criterion_benchmark
}
criterion_main!(benches);
