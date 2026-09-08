// Companion to `benches/sodium.rs`: that file measures event propagation
// through `Stream`/`Cell` chains, this one covers the rest of the combinator
// surface (lifting, state accumulation, switching, routing) plus graph
// construction and teardown.
//
// The benches are dev-only and cannot be built on the MSRV regardless: the
// criterion dependency tree reaches edition-2024 manifests that 1.71's cargo
// cannot parse. rust-version governs what consumers of the published crate
// need, so let this target use newer APIs. The lint still applies to src/.
#![allow(clippy::incompatible_msrv)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use sodium_rust::{Cell, SodiumCtx};

/// Lifting: combine several cells into one derived cell.
fn lift(c: &mut Criterion) {
    let mut lift = c.benchmark_group("Cell::lift");
    lift.bench_function("lift2", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let ca = ctx.new_cell_sink(0_u16);
            let cb = ctx.new_cell_sink(1_u16);
            let sum = ca.cell().lift2(&cb.cell(), |a: &u16, b: &u16| *a + *b);

            let mut values: Vec<u16> = Vec::new();
            let _listener = sum.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                if v % 2 == 0 {
                    ca.send(black_box(v));
                } else {
                    cb.send(black_box(v));
                }
            }
        })
    });
    lift.bench_function("lift3", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let ca = ctx.new_cell_sink(0_u16);
            let cb = ctx.new_cell_sink(1_u16);
            let cc = ctx.new_cell_sink(2_u16);
            let sum = ca
                .cell()
                .lift3(&cb.cell(), &cc.cell(), |a: &u16, b: &u16, c: &u16| {
                    *a + *b + *c
                });

            let mut values: Vec<u16> = Vec::new();
            let _listener = sum.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                match v % 3 {
                    2 => ca.send(black_box(v)),
                    1 => cb.send(black_box(v)),
                    _ => cc.send(black_box(v)),
                }
            }
        })
    });
    lift.bench_function("lift2 chained", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let ca = ctx.new_cell_sink(0_u16);
            let cb = ctx.new_cell_sink(1_u16);
            let cc = ctx.new_cell_sink(2_u16);
            let sum = ca
                .cell()
                .lift2(&cb.cell(), |a: &u16, b: &u16| *a + *b)
                .lift2(&cc.cell(), |a: &u16, c: &u16| *a + *c);

            let mut values: Vec<u16> = Vec::new();
            let _listener = sum.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                match v % 3 {
                    2 => ca.send(black_box(v)),
                    1 => cb.send(black_box(v)),
                    _ => cc.send(black_box(v)),
                }
            }
        })
    });
}

/// State kept inside the graph: `hold`, `accum` and `collect`.
fn state(c: &mut Criterion) {
    let mut state = c.benchmark_group("state");
    state.bench_function("hold", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let cell = sink.stream().hold(0_u16);

            for v in 0_u16..1000 {
                sink.send(black_box(v));
                black_box(cell.sample());
            }
        })
    });
    state.bench_function("accum", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let total = sink
                .stream()
                .accum(0_u32, |v: &u16, total: &u32| total + u32::from(*v));

            let mut values: Vec<u32> = Vec::new();
            let _listener = total.listen(move |v: &u32| values.push(black_box(*v)));

            for v in 0_u16..1000 {
                sink.send(black_box(v));
            }
        })
    });
    state.bench_function("collect", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let running_max = sink
                .stream()
                .collect(0_u16, |v: &u16, max: &u16| (*v.max(max), *v.max(max)));

            let mut values: Vec<u16> = Vec::new();
            let _listener = running_max.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..1000 {
                sink.send(black_box(v));
            }
        })
    });
    state.bench_function("gate", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let pred = ctx.new_cell_sink(true);
            let gated = sink.stream().gate(&pred.cell());

            let mut values: Vec<u16> = Vec::new();
            let _listener = gated.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..1000 {
                if v % 100 == 0 {
                    pred.send(black_box(v % 200 == 0));
                }
                sink.send(black_box(v));
            }
        })
    });
}

/// Switching rewires the graph while it is running, so it exercises the
/// dependency bookkeeping much harder than the static combinators do.
fn switch(c: &mut Criterion) {
    let mut switch = c.benchmark_group("switch");
    switch.bench_function("switch_s", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sa = ctx.new_stream_sink();
            let sb = ctx.new_stream_sink();
            let selector = ctx.new_cell_sink(sa.stream());
            let out = Cell::switch_s(&selector.cell());

            let mut values: Vec<u16> = Vec::new();
            let _listener = out.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                if v % 50 == 0 {
                    if (v / 50) % 2 == 0 {
                        selector.send(sb.stream());
                    } else {
                        selector.send(sa.stream());
                    }
                }
                sa.send(black_box(v));
                sb.send(black_box(v));
            }
        })
    });
    switch.bench_function("switch_c", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let ca = ctx.new_cell_sink(0_u16);
            let cb = ctx.new_cell_sink(1_u16);
            let selector = ctx.new_cell_sink(ca.cell());
            let out = Cell::switch_c(&selector.cell());

            let mut values: Vec<u16> = Vec::new();
            let _listener = out.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                if v % 50 == 0 {
                    if (v / 50) % 2 == 0 {
                        selector.send(cb.cell());
                    } else {
                        selector.send(ca.cell());
                    }
                }
                ca.send(black_box(v));
                cb.send(black_box(v));
            }
        })
    });
}

/// A router fans one stream out to many keyed streams.
fn router(c: &mut Criterion) {
    let mut router = c.benchmark_group("Router");
    router.bench_function("filter_matches x4", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let router = ctx.new_router(&sink.stream(), |v: &u16| vec![*v % 4]);

            let listeners: Vec<_> = (0_u16..4)
                .map(|k| {
                    let mut values: Vec<u16> = Vec::new();
                    router
                        .filter_matches(&k)
                        .listen(move |v: &u16| values.push(black_box(*v)))
                })
                .collect();

            for v in 0_u16..1000 {
                sink.send(black_box(v));
            }

            black_box(listeners);
        })
    });
}

/// Transactions batch updates: one closed transaction propagates once,
/// regardless of how many sinks were fed inside it.
fn transaction(c: &mut Criterion) {
    let mut transaction = c.benchmark_group("transaction");
    transaction.bench_function("one per send", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let ca = ctx.new_cell_sink(0_u16);
            let cb = ctx.new_cell_sink(0_u16);
            let sum = ca.cell().lift2(&cb.cell(), |a: &u16, b: &u16| *a + *b);

            let mut values: Vec<u16> = Vec::new();
            let _listener = sum.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                ca.send(black_box(v));
                cb.send(black_box(v));
            }
        })
    });
    transaction.bench_function("batched", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let ca = ctx.new_cell_sink(0_u16);
            let cb = ctx.new_cell_sink(0_u16);
            let sum = ca.cell().lift2(&cb.cell(), |a: &u16, b: &u16| *a + *b);

            let mut values: Vec<u16> = Vec::new();
            let _listener = sum.listen(move |v: &u16| values.push(black_box(*v)));

            for v in 0_u16..500 {
                ctx.transaction(|| {
                    ca.send(black_box(v));
                    cb.send(black_box(v));
                });
            }
        })
    });
}

/// Building and dropping a network is not free: nodes register with the
/// context and the collector has to reclaim them again.
fn graph(c: &mut Criterion) {
    let mut graph = c.benchmark_group("graph");
    graph.bench_function("build+drop chain of 100", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let mut stream = sink.stream();
            for _ in 0..100 {
                stream = stream.map(|v: &u16| v.wrapping_add(1));
            }

            let mut values: Vec<u16> = Vec::new();
            let _listener = stream.listen(move |v: &u16| values.push(black_box(*v)));

            sink.send(black_box(1_u16));
        })
    });
    graph.bench_function("build+drop fan-out of 100", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            let listeners: Vec<_> = (0..100)
                .map(|i| {
                    let mut values: Vec<u16> = Vec::new();
                    sink.stream()
                        .map(move |v: &u16| v.wrapping_add(i))
                        .listen(move |v: &u16| values.push(black_box(*v)))
                })
                .collect();

            sink.send(black_box(1_u16));

            black_box(listeners);
        })
    });
    graph.bench_function("listener churn", |b| {
        b.iter(|| {
            let ctx = SodiumCtx::new();

            let sink = ctx.new_stream_sink();
            for v in 0_u16..100 {
                let mut values: Vec<u16> = Vec::new();
                let listener = sink
                    .stream()
                    .map(|v: &u16| v.wrapping_mul(2))
                    .listen(move |v: &u16| values.push(black_box(*v)));
                sink.send(black_box(v));
                listener.unlisten();
            }
        })
    });
}

criterion_group!(benches, lift, state, switch, router, transaction, graph);
criterion_main!(benches);
