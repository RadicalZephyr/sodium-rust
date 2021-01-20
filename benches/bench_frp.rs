use std::sync::{Arc, Mutex};

use criterion::{criterion_group, criterion_main, black_box, Benchmark, Criterion, ParameterizedBenchmark, Throughput};

use sodium_rust::{SodiumCtx, Cell, Stream};

fn sodium(c: &mut Criterion) {
    c.bench_function("stream sending", |b| b.iter_with_large_drop(|| {
        let ctx = SodiumCtx::new();
        let sink = ctx.new_stream_sink();
        let mut values: Vec<u8> = Vec::new();
         black_box(sink.stream().listen(move |v: &u8| values.push(*v)));
        for v in 0_u8..100 {
            sink.send(v);
        }
        (ctx, sink)
    }));
}

criterion_group!(benches, sodium);
criterion_main!(benches);
