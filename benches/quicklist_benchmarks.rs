use std::{collections::VecDeque, hint::black_box};

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::QuickList;

/// Размеры для теста
const N_SMALL: usize = 1_000;
const N_LARGE: usize = 1_000_000;

/// Заполняет QuickList
fn bench_quicklist_push_back(n: usize) {
    let mut list = QuickList::new(128);
    for i in 0..n {
        list.push_back(black_box(i));
    }
}

/// Заполняет VecDeque
fn bench_vecdeque_push_back(n: usize) {
    let mut list = VecDeque::new();
    for i in 0..n {
        list.push_back(black_box(i));
    }
}

/// Итерирует по VecDeque
fn bench_vecdeque_iter(n: usize) {
    let mut list = VecDeque::new();
    for i in 0..n {
        list.push_back(i);
    }
    let mut sum = 0;
    for v in &list {
        sum += *v;
    }
    black_box(sum);
}

/// Смешанные push/pop с двух сторон
fn bench_quicklist_mixed(n: usize) {
    let mut list = QuickList::new(128);
    for i in 0..n {
        if i.is_multiple_of(2) {
            list.push_front(i);
        } else {
            list.push_back(i);
        }
    }
    for _ in 0..n {
        let _ = list.pop_front();
    }
}

/// Смешанные push/pop для VecDeque
fn bench_vecdeque_mixed(n: usize) {
    let mut list = VecDeque::new();
    for i in 0..n {
        if i.is_multiple_of(2) {
            list.push_front(i);
        } else {
            list.push_back(i);
        }
    }
    for _ in 0..n {
        let _ = list.pop_front();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("quicklist_vs_vecdeque");

    for &n in &[N_SMALL, N_LARGE] {
        group.bench_function(format!("quicklist_push_back_{n}"), |b| {
            b.iter(|| bench_quicklist_push_back(n))
        });

        group.bench_function(format!("vecdeque_push_back_{n}"), |b| {
            b.iter(|| bench_vecdeque_push_back(n))
        });

        group.bench_function(format!("vecdeque_iter_{n}"), |b| {
            b.iter(|| bench_vecdeque_iter(n))
        });

        group.bench_function(format!("quicklist_mixed_{n}"), |b| {
            b.iter(|| bench_quicklist_mixed(n))
        });

        group.bench_function(format!("vecdeque_mixed_{n}"), |b| {
            b.iter(|| bench_vecdeque_mixed(n))
        });
    }

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
