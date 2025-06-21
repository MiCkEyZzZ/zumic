use once_cell::sync::Lazy;

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::auth::password::{hash_password, verify_password};

const PEPPER: Option<&str> = Some("super_pepper");

// Подготовим заранее зафиксированный пароль
static PASSWORD: Lazy<String> = Lazy::new(|| {
    // Просто фиксированный пароль
    "very_secure_password".to_string()
});

// Подготовим заранее сгенерированный хеш
static PASSWORD_HASH: Lazy<String> =
    Lazy::new(|| hash_password(&PASSWORD, PEPPER).expect("Failed to hash password"));

fn bench_hash_password(c: &mut Criterion) {
    c.bench_function("hash_password", |b| {
        b.iter(|| {
            let _ = hash_password(&PASSWORD, PEPPER).unwrap();
        })
    });
}

fn bench_verify_password(c: &mut Criterion) {
    c.bench_function("verify_password (correct password)", |b| {
        b.iter(|| {
            let ok = verify_password(&PASSWORD_HASH, &PASSWORD, PEPPER).unwrap();
            assert!(ok);
        })
    });
}

fn bench_verify_wrong_password(c: &mut Criterion) {
    c.bench_function("verify_password (wrong password)", |b| {
        b.iter(|| {
            let wrong_password = "wrong_password";
            let ok = verify_password(&PASSWORD_HASH, wrong_password, PEPPER).unwrap();
            assert!(!ok);
        })
    });
}

criterion_group!(
    benches,
    bench_hash_password,
    bench_verify_password,
    bench_verify_wrong_password
);
criterion_main!(benches);
