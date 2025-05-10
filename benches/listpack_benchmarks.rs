use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::database::ListPack;

fn bench_push_back(c: &mut Criterion) {
    c.bench_function("push_back 1000 small elements", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for _ in 0..1000 {
                lp.push_back(black_box(b"abc"));
            }
        })
    });
}

fn bench_push_front(c: &mut Criterion) {
    c.bench_function("push_front 1000 small elements", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for _ in 0..1000 {
                lp.push_front(black_box(b"abc"));
            }
        })
    });
}

fn bench_iterate(c: &mut Criterion) {
    let mut lp = ListPack::new();
    for _ in 0..1000 {
        lp.push_back(b"abc");
    }

    c.bench_function("iterate over 1000 elements", |b| {
        b.iter(|| {
            for item in lp.iter() {
                black_box(item);
            }
        })
    });
}

fn bench_get_random(c: &mut Criterion) {
    let mut lp = ListPack::new();
    for _ in 0..1000 {
        lp.push_back(b"abc");
    }

    c.bench_function("get 100 random elements", |b| {
        b.iter(|| {
            for i in (0..100).map(|x| x * 10) {
                black_box(lp.get(i));
            }
        })
    });
}

fn bench_remove(c: &mut Criterion) {
    c.bench_function("remove 100 elements from middle", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            for _ in 0..1000 {
                lp.push_back(b"abc");
            }
            for _ in 0..100 {
                lp.remove(black_box(500)); // удаляем из середины
            }
        })
    });
}

criterion_group!(
    benches,
    bench_push_back,
    bench_push_front,
    bench_iterate,
    bench_get_random,
    bench_remove
);
criterion_main!(benches);
