use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use zumic::network::zsp::frame::decoder::ZSPDecoder;

fn bench_simple_string(c: &mut Criterion) {
    let input = b"+OK\r\n";
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_simple_string")
        .throughput(Throughput::Bytes(input.len() as u64))
        .bench_function("simple_string", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

fn bench_error_string(c: &mut Criterion) {
    let input = b"-ERR something went wrong\r\n";
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_error_string")
        .throughput(Throughput::Bytes(input.len() as u64))
        .bench_function("error_string", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

fn bench_integer(c: &mut Criterion) {
    let input = b":1234567890\r\n";
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_integer")
        .throughput(Throughput::Bytes(input.len() as u64))
        .bench_function("integer", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

fn bench_bulk_small(c: &mut Criterion) {
    let input = b"$3\r\nfoo\r\n";
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_bulk_small")
        .throughput(Throughput::Bytes(input.len() as u64))
        .bench_function("bulk_small", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

fn bench_bulk_large(c: &mut Criterion) {
    // 1KB payload
    let payload = vec![b'x'; 1024];
    let mut input = format!("${}\r\n", payload.len()).into_bytes();
    input.extend_from_slice(&payload);
    input.extend_from_slice(b"\r\n");
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_bulk_large_1KB")
        .throughput(Throughput::Bytes(input.len() as u64))
        .bench_function("bulk_large_1KB", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

fn bench_array(c: &mut Criterion) {
    // Array of 100 integers
    let mut input = b"*100\r\n".to_vec();
    for i in 0..100 {
        input.extend_from_slice(format!(":{}\r\n", i).as_bytes());
    }
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_array_100_ints")
        .throughput(Throughput::Elements(100))
        .bench_function("array_100_ints", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

fn bench_dictionary(c: &mut Criterion) {
    // Dictionary of 50 inline string pairs
    let mut input = b"%50\r\n".to_vec();
    for i in 0..50 {
        input.extend_from_slice(format!("+key{}\r\n", i).as_bytes());
        input.extend_from_slice(format!("+val{}\r\n", i).as_bytes());
    }
    let mut decoder = ZSPDecoder::new();

    c.benchmark_group("decode_dictionary_50")
        .throughput(Throughput::Elements(50))
        .bench_function("dictionary_50", |b| {
            b.iter(|| {
                let mut slice = &input[..];
                black_box(decoder.decode(&mut slice).unwrap().unwrap());
            })
        });
}

criterion_group!(
    decoder_benches,
    bench_simple_string,
    bench_error_string,
    bench_integer,
    bench_bulk_small,
    bench_bulk_large,
    bench_array,
    bench_dictionary,
);
criterion_main!(decoder_benches);
