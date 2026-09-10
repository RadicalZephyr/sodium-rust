//! ADR 1: what does `benches/sodium.rs` actually time?
//!
//! The existing benchmarks rebuild the whole graph inside every `b.iter()`
//! body, which looks like it ought to swamp the measurement. It does not. This
//! experiment splits the timed region into its parts, checks that the listener
//! body and the send count do not distort it either, and then prices a burst of
//! sends to a single sink.
//!
//! Run with `cargo run --release -p research --bin adr0001_timed_region`.
//! Release matters; the debug numbers are not comparable to anything.
//!
//! Recorded output, 2026-09-08, 4-core container, sodium-rust at 4316728:
//!
//! ```text
//! 1. The timed region of `Stream::send`, split into its parts (us per iteration)
//! maps                                             whole       construct           sends      construct%
//! 0                                               3942.4             6.4          3952.8            0.2%
//! 1                                               6677.8            10.5          6632.1            0.2%
//! 2                                               9307.6            15.6          9438.6            0.2%
//! 3                                              12214.6            22.3         12017.6            0.2%
//! 8                                              25965.7            64.5         25851.7            0.2%
//!
//! 2. Does per-send cost depend on how many sends we do?
//! sends                                          ns/send
//! 10                                              6353.9
//! 100                                             6559.3
//! 1000                                            6537.2
//! 4000                                            6645.0
//! 16000                                           6439.4
//! 64000                                           6504.0
//!
//! 3. Does the listener body matter? (1000 sends through sink -> map -> listen)
//! listener                                       ns/send
//! Vec::push                                       6453.7
//! atomic add                                      6555.0
//!
//! 4. What do repeated sends to ONE sink cost? (2000 sends, sink -> map -> listen)
//! batching                                   allocs/send         ns/send
//! one transaction per send                          36.0          6576.5
//! all sends in one transaction                       1.0           428.3
//! ```
//!
//! Conclusions that reached the ADR: construction is 0.1-0.3% of the timed
//! region, so the existing benches really are measuring propagation; per-send
//! cost is flat, so nothing accumulates within a run on a fresh context; the
//! listener body is irrelevant at these sizes.
//!
//! **Section 4 does not price the transaction, and is not structured to.** Its
//! batched arm sends 2000 times into *one* sink, and `Stream::_send` with no
//! coalescer sets `data.firing_op = Some(a)`, so 1999 of those sends are
//! overwritten and never propagate. The arm therefore removes 1999
//! propagations as well as 1999 transactions, and the gap between the two rows
//! is the cost of the propagations, not of the transactions. What it does show
//! is what a burst of repeated sends to one sink costs, which is: almost
//! nothing, because almost none of them happen. That silent overwrite is
//! tracked as issue #42.
//!
//! For the transaction itself, see `adr0001_transaction_cost.rs`, which prices
//! an empty `ctx.transaction(|| {})` directly and sweeps *distinct* sinks
//! firing inside one transaction, so the two are separable by construction.

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
        "4. What do repeated sends to ONE sink cost? (2000 sends, sink -> map -> listen)",
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
