//! ADR 1: what does `benches/sodium.rs` actually time?
//!
//! The existing benchmarks rebuild the whole graph inside every `b.iter()`
//! body, which looks like it ought to swamp the measurement. It does not. This
//! experiment splits the timed region into its parts, checks that the listener
//! body and the send count do not distort it either, and then tries to price
//! the transaction. Section 4 does not succeed at that; see the correction
//! below the recorded output.
//!
//! Run with `cargo run --release -p research --bin adr0001_timed_region`.
//! Release matters; the debug numbers are not comparable to anything.
//!
//! Recorded output, 2026-09-08, 4-core container, sodium-rust at 4316728:
//!
//! ```text
//! 1. The timed region of `Stream::send`, split into its parts (us per iteration)
//! maps                                             whole       construct           sends      construct%
//! 0                                               4647.8             6.6          4606.4            0.1%
//! 1                                               7822.5            12.5          7933.8            0.2%
//! 2                                              10986.3            18.2         10931.8            0.2%
//! 3                                              13926.7            26.1         13728.4            0.2%
//! 8                                              29406.9            79.9         29461.0            0.3%
//!
//! 2. Does per-send cost depend on how many sends we do?
//! sends                                          ns/send
//! 10                                              9399.2
//! 100                                             8217.2
//! 1000                                            7707.3
//! 4000                                            7716.3
//! 16000                                           7784.8
//! 64000                                           7628.3
//!
//! 3. Does the listener body matter? (1000 sends through sink -> map -> listen)
//! listener                                       ns/send
//! Vec::push                                       7491.5
//! atomic add                                      7638.1
//!
//! 4. What does the transaction cost? (2000 sends through sink -> map -> listen)
//! batching                                   allocs/send         ns/send
//! one transaction per send                          43.0          7674.4
//! all sends in one transaction                       1.0           400.6
//! ```
//!
//! Conclusions that reached the ADR: construction is 0.1-0.3% of the timed
//! region, so the existing benches really are measuring propagation; per-send
//! cost is flat, so nothing accumulates within a run on a fresh context; the
//! listener body is irrelevant at these sizes.
//!
//! Section 4's conclusion — that roughly 26 of the 27 allocations a bare send
//! costs are transaction machinery — was wrong, and the ADR now says so. The
//! batched arm sends 2000 times into one sink, and `Stream::_send` without a
//! coalescer just overwrites `firing_op`, so 1999 of those sends never
//! propagate. The arm removes 1999 propagations as well as 1999 transactions,
//! and the difference cannot be attributed to the transaction.
//!
//! Measured separably instead — an empty `ctx.transaction(|| {})`, and a sweep
//! of how many *distinct* sinks fire inside one transaction — a transaction
//! with nothing to propagate costs 0 allocations and 270 ns, flat in the size
//! of the graph, while each additional firing sink costs 32-33 allocations and
//! ~6100 ns. Almost all of a send is propagation. Replacing this section with
//! that sweep is TODO; the numbers above are left as recorded.

use std::hint::black_box;
use std::time::Instant;

use research::{heading, measure, row, CountingAlloc, Observer};
use sodium_rust::{SodiumCtx, Stream, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SENDS: u16 = 1000;
const REPS: u32 = 200;

/// The graph `benches/sodium.rs` builds, listener body and all.
fn build_like_the_bench(ctx: &SodiumCtx, maps: usize) -> (StreamSink<u16>, sodium_rust::Listener) {
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let mut s: Stream<u16> = sink.stream();
    for _ in 0..maps {
        s = s.map(|v: &u16| black_box(*v).wrapping_add(10));
    }
    let mut values: Vec<u16> = Vec::new();
    let listener = s.listen(move |v: &u16| values.push(black_box(*v)));
    (sink, listener)
}

fn micros_per_rep(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() / f64::from(REPS) * 1e6
}

fn split_the_timed_region() {
    heading(
        "1. The timed region of `Stream::send`, split into its parts (us per iteration)",
        &["maps", "whole", "construct", "sends", "construct%"],
    );
    for maps in [0usize, 1, 2, 3, 8] {
        // (a) exactly what the bench times: build, a thousand sends, drop.
        let t = Instant::now();
        for _ in 0..REPS {
            let ctx = SodiumCtx::new();
            let (sink, listener) = build_like_the_bench(&ctx, maps);
            for v in 0..SENDS {
                sink.send(black_box(v));
            }
            black_box(&listener);
        }
        let whole = micros_per_rep(t);

        // (b) the same, with the sends removed.
        let t = Instant::now();
        for _ in 0..REPS {
            let ctx = SodiumCtx::new();
            let (sink, listener) = build_like_the_bench(&ctx, maps);
            black_box(&sink);
            black_box(&listener);
        }
        let construct = micros_per_rep(t);

        // (c) the sends alone, on a graph built once outside the timed region.
        let ctx = SodiumCtx::new();
        let (sink, listener) = build_like_the_bench(&ctx, maps);
        let t = Instant::now();
        for _ in 0..REPS {
            for v in 0..SENDS {
                sink.send(black_box(v));
            }
        }
        let sends = micros_per_rep(t);
        drop(listener);

        row(&[
            maps.to_string(),
            format!("{whole:.1}"),
            format!("{construct:.1}"),
            format!("{sends:.1}"),
            format!("{:.1}%", construct / whole * 100.0),
        ]);
    }
}

fn per_send_cost_is_flat() {
    heading(
        "2. Does per-send cost depend on how many sends we do?",
        &["sends", "ns/send"],
    );
    for sends in [10u32, 100, 1_000, 4_000, 16_000, 64_000] {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let listener = observer.listen(&research::chain(&sink, 1));
        sink.send(0);

        let cost = measure(sends as usize, || {
            sink.send(black_box(1));
        });
        black_box(observer.total());
        drop(listener);

        row(&[sends.to_string(), format!("{:.1}", cost.nanos)]);
    }
}

fn listener_body_is_irrelevant() {
    heading(
        "3. Does the listener body matter? (1000 sends through sink -> map -> listen)",
        &["listener", "ns/send"],
    );

    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let mapped = research::chain(&sink, 1);
    let mut values: Vec<u16> = Vec::new();
    let listener = mapped.listen(move |v: &u16| values.push(black_box(*v)));
    sink.send(0);
    let vec_push = measure(SENDS as usize, || sink.send(black_box(1)));
    drop(listener);

    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let observer = Observer::new();
    let listener = observer.listen(&research::chain(&sink, 1));
    sink.send(0);
    let atomic = measure(SENDS as usize, || sink.send(black_box(1)));
    black_box(observer.total());
    drop(listener);

    row(&["Vec::push".to_string(), format!("{:.1}", vec_push.nanos)]);
    row(&["atomic add".to_string(), format!("{:.1}", atomic.nanos)]);
}

fn transactions_dominate() {
    const N: usize = 2000;
    heading(
        "4. What does the transaction cost? (2000 sends through sink -> map -> listen)",
        &["batching", "allocs/send", "ns/send"],
    );

    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let observer = Observer::new();
    let listener = observer.listen(&research::chain(&sink, 1));
    sink.send(0);
    let per_send = measure(N, || sink.send(black_box(1)));
    black_box(observer.total());
    drop(listener);

    // One transaction around the whole burst. The sink coalesces, so only one
    // value reaches the listener -- the point is what the transaction costs,
    // not what propagation costs.
    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let observer = Observer::new();
    let listener = observer.listen(&research::chain(&sink, 1));
    sink.send(0);
    let batched = measure(1, || {
        ctx.transaction(|| {
            for v in 0..N {
                sink.send(black_box(v as u16));
            }
        });
    });
    black_box(observer.total());
    drop(listener);

    row(&[
        "one transaction per send".to_string(),
        format!("{:.1}", per_send.allocs),
        format!("{:.1}", per_send.nanos),
    ]);
    row(&[
        "all sends in one transaction".to_string(),
        format!("{:.1}", batched.allocs / N as f64),
        format!("{:.1}", batched.nanos / N as f64),
    ]);
}

fn main() {
    split_the_timed_region();
    per_send_cost_is_flat();
    listener_body_is_irrelevant();
    transactions_dominate();
}
