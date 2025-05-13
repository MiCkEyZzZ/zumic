use std::collections::HashSet;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::{
    database::{QuickList, Value},
    network::zsp::frame::zsp_types::{
        convert_hashset, convert_quicklist, convert_sds_to_frame, convert_smart_hash, convert_zset,
        ZspFrame,
    },
    Dict, Sds, SmartHash,
};

fn bench_convert_sds_inline(c: &mut Criterion) {
    let sds = Sds::from_str("short inline");
    c.bench_function("convert_sds_to_frame (inline UTF-8)", |b| {
        b.iter(|| {
            let f = convert_sds_to_frame(black_box(sds.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_convert_sds_heap(c: &mut Criterion) {
    // > INLINE_CAP
    let long = "x".repeat(1024);
    let sds = Sds::from_str(&long);
    c.bench_function("convert_sds_to_frame (heap UTF-8)", |b| {
        b.iter(|| {
            let f = convert_sds_to_frame(black_box(sds.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_convert_quicklist(c: &mut Criterion) {
    let mut ql = QuickList::new(64);
    for i in 0..100 {
        ql.push_back(Sds::from_str(&format!("item{i}")));
    }
    c.bench_function("convert_quicklist (100 items)", |b| {
        b.iter(|| {
            let f = convert_quicklist(black_box(ql.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_convert_hashset(c: &mut Criterion) {
    let mut hs = HashSet::new();
    for i in 0..100 {
        hs.insert(Sds::from_str(&format!("key{i}")));
    }
    c.bench_function("convert_hashset (100 items)", |b| {
        b.iter(|| {
            let f = convert_hashset(black_box(hs.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_convert_smart_hash(c: &mut Criterion) {
    let mut sh = SmartHash::new();
    for i in 0..100 {
        sh.insert(
            Sds::from_str(&format!("hk{i}")),
            Sds::from_str(&format!("hv{i}")),
        );
    }
    c.bench_function("convert_smart_hash (100 entries)", |b| {
        b.iter(|| {
            let f = convert_smart_hash(black_box(sh.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_convert_zset(c: &mut Criterion) {
    let mut dict = Dict::new();
    for i in 0..100 {
        dict.insert(Sds::from_str(&format!("member{i}")), i as f64);
    }
    c.bench_function("convert_zset (100 entries)", |b| {
        b.iter(|| {
            let f = convert_zset(black_box(dict.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_tryfrom_value_str(c: &mut Criterion) {
    let sds = Sds::from_str("some inline");
    let v = Value::Str(sds);
    c.bench_function("ZspFrame::try_from(Value::Str)", |b| {
        b.iter(|| {
            let f = ZspFrame::try_from(black_box(v.clone())).unwrap();
            black_box(f);
        })
    });
}

fn bench_tryfrom_value_list(c: &mut Criterion) {
    let mut ql = QuickList::new(32);
    for i in 0..50 {
        ql.push_back(Sds::from_str(&format!("val{i}")));
    }
    let v = Value::List(ql);
    c.bench_function("ZspFrame::try_from(Value::List)", |b| {
        b.iter(|| {
            let f = ZspFrame::try_from(black_box(v.clone())).unwrap();
            black_box(f);
        })
    });
}

criterion_group!(
    benches,
    bench_convert_sds_inline,
    bench_convert_sds_heap,
    bench_convert_quicklist,
    bench_convert_hashset,
    bench_convert_smart_hash,
    bench_convert_zset,
    bench_tryfrom_value_str,
    bench_tryfrom_value_list,
);
criterion_main!(benches);
