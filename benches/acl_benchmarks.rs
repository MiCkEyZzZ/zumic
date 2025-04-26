use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::auth::acl::Acl;

fn bench_check_permission(c: &mut Criterion) {
    let mut group = c.benchmark_group("check_permission");
    let acl = Acl::default();
    let rules = vec!["on", "+@read", "+get", "-del"];
    acl.acl_setuser("anton", &rules).unwrap();
    let user = acl.acl_getuser("anton").unwrap();

    group.bench_function("check_permission(get)", |b| {
        b.iter(|| {
            black_box(user.check_permission("read", "get"));
        });
    });

    group.bench_function("check_permission(del)", |b| {
        b.iter(|| {
            black_box(user.check_permission("read", "del"));
        });
    });

    group.finish();
}

fn bench_check_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("check_key");
    let acl = Acl::default();
    let rules = vec!["on", "~data:*"];
    acl.acl_setuser("anton", &rules).unwrap();
    let mut user = acl.acl_getuser("anton").unwrap();

    group.bench_function("check_key(data:123)", |b| {
        b.iter(|| {
            black_box(user.check_key("data:123"));
        });
    });

    group.bench_function("check_key(other:456)", |b| {
        b.iter(|| {
            black_box(user.check_key("other:456"));
        });
    });

    group.finish();
}

fn bench_check_channel(c: &mut Criterion) {
    let mut group = c.benchmark_group("check_channel");
    let acl = Acl::default();
    let rules = vec!["on", "&chan?"];
    acl.acl_setuser("anton", &rules).unwrap();
    let mut user = acl.acl_getuser("anton").unwrap();

    group.bench_function("check_channel(chan1)", |b| {
        b.iter(|| {
            black_box(user.check_channel("chan1"));
        });
    });

    group.bench_function("check_channel(channelX)", |b| {
        b.iter(|| {
            black_box(user.check_channel("channelX"));
        });
    });

    group.finish();
}

fn bench_acl_setuser(c: &mut Criterion) {
    let mut group = c.benchmark_group("acl_setuser");
    let acl = Acl::default();

    group.bench_function("acl_setuser complex rules", |b| {
        b.iter(|| {
            let _ = acl.acl_setuser(
                "anton",
                &[
                    "on",
                    "+@read",
                    "+@write",
                    "+get",
                    "-flushall",
                    "~data:*",
                    "&chan*",
                    ">somehash1",
                    ">somehash2",
                ],
            );
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_check_permission,
    bench_check_key,
    bench_check_channel,
    bench_acl_setuser
);
criterion_main!(benches);
