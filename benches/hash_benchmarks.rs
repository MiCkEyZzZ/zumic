// use criterion::{black_box, criterion_group, criterion_main, Criterion};
// use zumic::{
//     command::{CommandExecute, HDelCommand, HGetAllCommand, HGetCommand,
// HSetCommand},     engine::{engine::StorageEngine, memory::InMemoryStore},
//     SmartHash,
// };

// fn setup_store_with_hash() -> StorageEngine {
//     let mut store = StorageEngine::InMemory(InMemoryStore::new());

//     for i in 0..1000 {
//         for j in 0..10 {
//             let cmd = HSetCommand {
//                 key: format!("hash_{i}"),
//                 field: format!("field_{j}"),
//                 value: format!("value_{i}_{j}"),
//             };
//             let _ = cmd.execute(&mut store);
//         }
//     }

//     store
// }

// fn bench_hset(c: &mut Criterion) {
//     let mut store = StorageEngine::InMemory(InMemoryStore::new());

//     c.bench_function("HSet", |b| {
//         b.iter(|| {
//             let cmd = HSetCommand {
//                 key: "hash".into(),
//                 field: "field1".into(),
//                 value: "value1".into(),
//             };
//             let _ = cmd.execute(black_box(&mut store));
//         })
//     });
// }

// fn bench_hget(c: &mut Criterion) {
//     let mut store = setup_store_with_hash();
//     let cmd = HGetCommand {
//         key: "hash_0".into(),
//         field: "field_0".into(),
//     };

//     c.bench_function("HGet", |b| {
//         b.iter(|| {
//             let _ = cmd.execute(black_box(&mut store));
//         })
//     });
// }

// fn bench_hdel(c: &mut Criterion) {
//     let mut store = setup_store_with_hash();
//     let cmd = HDelCommand {
//         key: "hash_0".into(),
//         field: "field_0".into(),
//     };

//     c.bench_function("HDel", |b| {
//         b.iter(|| {
//             let _ = cmd.execute(black_box(&mut store));
//         })
//     });
// }

// fn bench_hgetall(c: &mut Criterion) {
//     let mut store = setup_store_with_hash();
//     let cmd = HGetAllCommand {
//         key: "hash_0".into(),
//     };

//     c.bench_function("HGetAll", |b| {
//         b.iter(|| {
//             let _ = cmd.execute(black_box(&mut store));
//         })
//     });
// }

// fn bench_hset_large_hash(c: &mut Criterion) {
//     let mut store = StorageEngine::InMemory(InMemoryStore::new());
//     let key = "big_hash".to_string();

//     for i in 0..200 {
//         let cmd = HSetCommand {
//             key: key.clone(),
//             field: format!("field_{i}"),
//             value: "val".to_string(),
//         };
//         let _ = cmd.execute(&mut store);
//     }

//     c.bench_function("HSet on large hash (Map)", |b| {
//         b.iter(|| {
//             let cmd = HSetCommand {
//                 key: key.clone(),
//                 field: "field_new".into(),
//                 value: "value_new".into(),
//             };
//             let _ = cmd.execute(black_box(&mut store));
//         })
//     });
// }

// fn bench_smart_hash_get(c: &mut Criterion) {
//     let mut hash = SmartHash::default();
//     for i in 0..100 {
//         hash.insert(format!("field_{i}").into(), "value".into());
//     }

//     c.bench_function("SmartHash get", |b| {
//         b.iter(|| {
//             black_box(hash.get(&"field_42".into()));
//         })
//     });
// }

// criterion_group!(
//     benches,
//     bench_hset,
//     bench_hget,
//     bench_hdel,
//     bench_hgetall,
//     bench_hset_large_hash,
//     bench_smart_hash_get
// );
// criterion_main!(benches);
