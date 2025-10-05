use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;
use zumic::auth::AuthManager;

fn bench_auth_manager(c: &mut Criterion) {
    // Создаем runtime
    let rt = Runtime::new().unwrap();

    c.bench_function("create_user", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                let _ = manager
                    .create_user("bench", "pass", &[">somehash", "+get", "~key:*"])
                    .await;
            });
        });
    });

    c.bench_function("authenticate_success", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                manager
                    .create_user("bench", "pass", &["+get"])
                    .await
                    .unwrap();
                let _ = manager.authenticate("bench", "pass").await;
            });
        });
    });

    c.bench_function("authenticate_fail", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                manager
                    .create_user("bench", "pass", &["+get"])
                    .await
                    .unwrap();
                let _ = manager.authenticate("bench", "wrong").await;
            });
        });
    });

    c.bench_function("authorize_command (allowed)", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                manager
                    .create_user("bench", "pass", &["+get", "+@read"])
                    .await
                    .unwrap();
                manager.authenticate("bench", "pass").await.unwrap();
                let _ = manager.authorize_command("bench", "read", "get").await;
            });
        });
    });

    c.bench_function("authorize_command (denied)", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                manager
                    .create_user("bench", "pass", &["+get"])
                    .await
                    .unwrap();
                manager.authenticate("bench", "pass").await.unwrap();
                let _ = manager.authorize_command("bench", "write", "set").await;
            });
        });
    });

    c.bench_function("authorize_key (allowed)", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                manager
                    .create_user("bench", "pass", &["~data:*"])
                    .await
                    .unwrap();
                manager.authenticate("bench", "pass").await.unwrap();
                let _ = manager.authorize_key("bench", "data:123").await;
            });
        });
    });

    c.bench_function("authorize_key (denied)", |b| {
        b.iter(|| {
            rt.block_on(async {
                let manager = AuthManager::new();
                manager
                    .create_user("bench", "pass", &["~data:*"])
                    .await
                    .unwrap();
                manager.authenticate("bench", "pass").await.unwrap();
                let _ = manager.authorize_key("bench", "other:456").await;
            });
        });
    });
}

criterion_group!(benches, bench_auth_manager);
criterion_main!(benches);
