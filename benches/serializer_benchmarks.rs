use std::{collections::HashSet, hint::black_box};

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::{
    network::zsp::protocol::{command::Response, serializer::serialize_response},
    Dict, QuickList, Sds, SkipList, SmartHash, Value,
};

/// Бенчмарк для сериализации Response::Ok
fn bench_serialize_ok(c: &mut Criterion) {
    c.bench_function("serialize Ok", |b| {
        b.iter(|| serialize_response(black_box(Response::Ok)));
    });
}

/// Бенчмарк для сериализации Response::Error
fn bench_serialize_error(c: &mut Criterion) {
    c.bench_function("serialize Error", |b| {
        b.iter(|| serialize_response(black_box(Response::Error("fail".into()))));
    });
}

/// Бенчмарк для сериализации Value::Str
fn bench_serialize_str(c: &mut Criterion) {
    let value = Value::Str(Sds::from_str("hello"));
    c.bench_function("serialize Str", |b| {
        b.iter(|| serialize_response(black_box(Response::Value(value.clone()))));
    });
}

/// Бенчмарк для сериализации Value::Int
fn bench_serialize_int(c: &mut Criterion) {
    let value = Value::Int(123);
    c.bench_function("serialize Int", |b| {
        b.iter(|| serialize_response(black_box(Response::Value(value.clone()))));
    });
}

/// Бенчмарк для сериализации Value::List
fn bench_serialize_list(c: &mut Criterion) {
    let mut list = QuickList::new(4);
    list.push_back(Sds::from_str("a"));
    list.push_back(Sds::from_str("b"));
    let value = Value::List(list);

    c.bench_function("serialize List", |b| {
        b.iter(|| serialize_response(black_box(Response::Value(value.clone()))));
    });
}

/// Бенчмарк для сериализации Value::Set
fn bench_serialize_set(c: &mut Criterion) {
    let mut set = HashSet::new();
    set.insert(Sds::from_str("x"));
    set.insert(Sds::from_str("y"));
    let value = Value::Set(set);

    c.bench_function("serialize Set", |b| {
        b.iter(|| serialize_response(black_box(Response::Value(value.clone()))));
    });
}

/// Бенчмарк для сериализации Value::Hash
fn bench_serialize_hash(c: &mut Criterion) {
    let mut sh = SmartHash::new();
    sh.insert(Sds::from_str("k1"), Sds::from_str("v1"));
    let value = Value::Hash(sh);

    c.bench_function("serialize Hash", |b| {
        b.iter(|| serialize_response(black_box(Response::Value(value.clone()))));
    });
}

/// Бенчмарк для сериализации Value::ZSet
fn bench_serialize_zset(c: &mut Criterion) {
    let mut dict = Dict::new();
    let mut sorted = SkipList::new();

    let key = Sds::from_str("one");
    let score = 1.0;
    dict.insert(key.clone(), score);
    sorted.insert(ordered_float::OrderedFloat(score), key);

    let value = Value::ZSet { dict, sorted };
    c.bench_function("serialize ZSet", |b| {
        b.iter(|| serialize_response(black_box(Response::Value(value.clone()))));
    });
}

criterion_group!(
    benches,
    bench_serialize_ok,
    bench_serialize_error,
    bench_serialize_str,
    bench_serialize_int,
    bench_serialize_list,
    bench_serialize_set,
    bench_serialize_hash,
    bench_serialize_zset
);

criterion_main!(benches);
