use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread,
    time::Duration,
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zumic::{database::ShardedSkipList, ConcurrentSkipList};

fn make_keys(
    n: usize,
    seed: u64,
) -> Vec<i64> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n).map(|_| rng.gen()).collect()
}

fn bench_concurrent_inserts(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_inserts");

    for &num_threads in &[1, 2, 4, 8, 16] {
        for &ops_per_thread in &[1000, 10_000] {
            let total_ops = num_threads * ops_per_thread;
            group.throughput(Throughput::Elements(total_ops as u64));

            // ConcurrentSkipList (global lock)
            group.bench_with_input(
                BenchmarkId::new(
                    "concurrent",
                    format!("{}t_{}ops", num_threads, ops_per_thread),
                ),
                &(num_threads, ops_per_thread),
                |b, &(num_threads, ops_per_thread)| {
                    b.iter(|| {
                        let list = ConcurrentSkipList::new();
                        let barrier = Arc::new(Barrier::new(num_threads));
                        let mut handles = vec![];

                        for tid in 0..num_threads {
                            let list = list.clone();
                            let barrier = Arc::clone(&barrier);
                            let keys = make_keys(ops_per_thread, (tid as u64) * 42);

                            handles.push(thread::spawn(move || {
                                barrier.wait();
                                for key in keys {
                                    list.insert(key, key);
                                }
                            }));
                        }

                        for h in handles {
                            h.join().unwrap();
                        }
                    })
                },
            );

            // ShardedSkipList
            group.bench_with_input(
                BenchmarkId::new("sharded", format!("{}t_{}ops", num_threads, ops_per_thread)),
                &(num_threads, ops_per_thread),
                |b, &(num_threads, ops_per_thread)| {
                    b.iter(|| {
                        let list = ShardedSkipList::with_shards(num_threads.max(16));
                        let barrier = Arc::new(Barrier::new(num_threads));
                        let mut handles = vec![];

                        for tid in 0..num_threads {
                            let list = list.clone();
                            let barrier = Arc::clone(&barrier);
                            let keys = make_keys(ops_per_thread, (tid as u64) * 42);

                            handles.push(thread::spawn(move || {
                                barrier.wait();
                                for key in keys {
                                    list.insert(key, key);
                                }
                            }));
                        }

                        for h in handles {
                            h.join().unwrap();
                        }
                    })
                },
            );
        }
    }

    group.finish();
}

fn bench_concurrent_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_reads");

    for &num_threads in &[1, 2, 4, 8, 16] {
        let ops_per_thread = 10_000;
        let total_ops = num_threads * ops_per_thread;
        group.throughput(Throughput::Elements(total_ops as u64));

        // Подготовка: заполняем список
        let list = ConcurrentSkipList::new();
        let keys = make_keys(10_000, 123);
        for key in &keys {
            list.insert(*key, *key);
        }

        group.bench_with_input(
            BenchmarkId::new("concurrent", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];

                    for _ in 0..num_threads {
                        let list = list.clone();
                        let barrier = Arc::clone(&barrier);
                        let keys = keys.clone();

                        handles.push(thread::spawn(move || {
                            barrier.wait();
                            for key in keys {
                                std::hint::black_box(list.search(&key));
                            }
                        }));
                    }

                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );

        // ShardedSkipList reads
        let sharded_list = ShardedSkipList::with_shards(num_threads.max(16));
        for key in &keys {
            sharded_list.insert(*key, *key);
        }

        group.bench_with_input(
            BenchmarkId::new("sharded", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];

                    for _ in 0..num_threads {
                        let list = sharded_list.clone();
                        let barrier = Arc::clone(&barrier);
                        let keys = keys.clone();

                        handles.push(thread::spawn(move || {
                            barrier.wait();
                            for key in keys {
                                std::hint::black_box(list.search(&key));
                            }
                        }));
                    }

                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");

    let num_threads = 8;
    let duration = Duration::from_millis(500);

    for &read_percentage in &[50, 80, 95, 99] {
        group.bench_with_input(
            BenchmarkId::new("concurrent", format!("{}%reads", read_percentage)),
            &read_percentage,
            |b, &read_pct| {
                b.iter(|| {
                    let list = ConcurrentSkipList::new();

                    // Предварительно заполняем
                    for i in 0..1000 {
                        list.insert(i, i);
                    }

                    let stop = Arc::new(AtomicBool::new(false));
                    let total_ops = Arc::new(AtomicUsize::new(0));
                    let mut handles = vec![];

                    for _ in 0..num_threads {
                        let list = list.clone();
                        let stop = Arc::clone(&stop);
                        let ops = Arc::clone(&total_ops);

                        handles.push(thread::spawn(move || {
                            let mut rng = rand::thread_rng();
                            while !stop.load(Ordering::Relaxed) {
                                let key = rng.gen_range(0..1000);

                                if rng.gen_range(0..100) < read_pct {
                                    // Read
                                    std::hint::black_box(list.search(&key));
                                } else {
                                    // Write
                                    list.insert(key, key);
                                }

                                ops.fetch_add(1, Ordering::Relaxed);
                            }
                        }));
                    }

                    thread::sleep(duration);
                    stop.store(true, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    total_ops.load(Ordering::Relaxed)
                })
            },
        );

        // ShardedSkipList
        group.bench_with_input(
            BenchmarkId::new("sharded", format!("{}%reads", read_percentage)),
            &read_percentage,
            |b, &read_pct| {
                b.iter(|| {
                    let list = ShardedSkipList::with_shards(16);

                    for i in 0..1000 {
                        list.insert(i, i);
                    }

                    let stop = Arc::new(AtomicBool::new(false));
                    let total_ops = Arc::new(AtomicUsize::new(0));
                    let mut handles = vec![];

                    for _ in 0..num_threads {
                        let list = list.clone();
                        let stop = Arc::clone(&stop);
                        let ops = Arc::clone(&total_ops);

                        handles.push(thread::spawn(move || {
                            let mut rng = rand::thread_rng();
                            while !stop.load(Ordering::Relaxed) {
                                let key = rng.gen_range(0..1000);

                                if rng.gen_range(0..100) < read_pct {
                                    std::hint::black_box(list.search(&key));
                                } else {
                                    list.insert(key, key);
                                }

                                ops.fetch_add(1, Ordering::Relaxed);
                            }
                        }));
                    }

                    thread::sleep(duration);
                    stop.store(true, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    total_ops.load(Ordering::Relaxed)
                })
            },
        );
    }

    group.finish();
}

fn bench_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");

    let total_ops = 100_000;

    for &num_threads in &[1, 2, 4, 8, 16, 32] {
        let ops_per_thread = total_ops / num_threads;

        // ConcurrentSkipList
        group.bench_with_input(
            BenchmarkId::new("concurrent", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let list = ConcurrentSkipList::new();
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];

                    for tid in 0..num_threads {
                        let list = list.clone();
                        let barrier = Arc::clone(&barrier);
                        let keys = make_keys(ops_per_thread, (tid as u64) * 123);

                        handles.push(thread::spawn(move || {
                            barrier.wait();
                            for key in keys {
                                list.insert(key, key);
                            }
                        }));
                    }

                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );

        // ShardedSkipList
        group.bench_with_input(
            BenchmarkId::new("sharded", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let list = ShardedSkipList::with_shards(num_threads.max(16));
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];

                    for tid in 0..num_threads {
                        let list = list.clone();
                        let barrier = Arc::clone(&barrier);
                        let keys = make_keys(ops_per_thread, (tid as u64) * 123);

                        handles.push(thread::spawn(move || {
                            barrier.wait();
                            for key in keys {
                                list.insert(key, key);
                            }
                        }));
                    }

                    for h in handles {
                        h.join().unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("lock_contention");

    let num_threads = 16;
    let duration = Duration::from_millis(200);

    // Hot keys (все threads работают с одними ключами)
    group.bench_function("hot_keys_concurrent", |b| {
        b.iter(|| {
            let list = ConcurrentSkipList::new();

            // 10 hot keys
            for i in 0..10 {
                list.insert(i, i);
            }

            let stop = Arc::new(AtomicBool::new(false));
            let mut handles = vec![];

            for _ in 0..num_threads {
                let list = list.clone();
                let stop = Arc::clone(&stop);

                handles.push(thread::spawn(move || {
                    let mut rng = rand::thread_rng();
                    let mut count = 0;

                    while !stop.load(Ordering::Relaxed) {
                        let key = rng.gen_range(0..10);
                        list.insert(key, key);
                        count += 1;
                    }

                    count
                }));
            }

            thread::sleep(duration);
            stop.store(true, Ordering::Relaxed);

            let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
            total
        })
    });

    group.bench_function("hot_keys_sharded", |b| {
        b.iter(|| {
            let list = ShardedSkipList::with_shards(16);

            for i in 0..10 {
                list.insert(i, i);
            }

            let stop = Arc::new(AtomicBool::new(false));
            let mut handles = vec![];

            for _ in 0..num_threads {
                let list = list.clone();
                let stop = Arc::clone(&stop);

                handles.push(thread::spawn(move || {
                    let mut rng = rand::thread_rng();
                    let mut count = 0;

                    while !stop.load(Ordering::Relaxed) {
                        let key = rng.gen_range(0..10);
                        list.insert(key, key);
                        count += 1;
                    }

                    count
                }));
            }

            thread::sleep(duration);
            stop.store(true, Ordering::Relaxed);

            let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
            total
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_concurrent_inserts,
    bench_concurrent_reads,
    bench_mixed_workload,
    bench_scalability,
    bench_contention
);

criterion_main!(benches);
