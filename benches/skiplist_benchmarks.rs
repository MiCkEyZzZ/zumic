use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::SkipList;

fn bench_insert(c: &mut Criterion) {
    c.bench_function("insert 1000 elements", |b| {
        b.iter(|| {
            let mut list = SkipList::new();
            for i in 0..1000 {
                list.insert(i, i);
            }
        })
    });
}

fn bench_search(c: &mut Criterion) {
    c.bench_function("search 1000 elements", |b| {
        let mut list = SkipList::new();
        for i in 0..1000 {
            list.insert(i, i);
        }
        b.iter(|| {
            black_box(list.search(&500));
        })
    });
}

fn bench_remove(c: &mut Criterion) {
    c.bench_function("remove 500 elements", |b| {
        let mut list = SkipList::new();
        for i in 0..1000 {
            list.insert(i, i);
        }
        b.iter(|| {
            black_box(list.remove(&500));
        })
    });
}

fn bench_len(c: &mut Criterion) {
    c.bench_function("len of 1000 elements", |b| {
        let mut list = SkipList::new();
        for i in 0..1000 {
            list.insert(i, i);
        }
        b.iter(|| {
            black_box(list.len());
        })
    });
}

criterion_group!(benches, bench_insert, bench_search, bench_remove, bench_len);

criterion_main!(benches);
