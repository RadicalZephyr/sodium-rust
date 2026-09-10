//! ADR 1: what is one node's budget, and how much of an event is floor?
//!
//! The slimming programme in ADR 1 turns on two numbers: what an event costs
//! before any combinator exists, and what each additional node on the firing
//! path adds. Both are measured here by sweeping chain depth with the graph
//! built outside the timed region.
//!
//! Depth 0 is `sink -> listen`: one send, one transaction, and the two nodes a
//! graph cannot do without. Nothing in the four tiers names that number, and it
//! is the largest single line item in a small graph.
//!
//! The chains are built with `research::chain`, which drops each intermediate
//! `Stream` as it goes — `s = s.map(..)` releases the previous handle. That is
//! what application code does, and it is not cost-neutral: see
//! `adr0001_root_set.rs`, which is why this file says so rather than leaving
//! the reader to assume the handles are still alive.
//!
//! Run with `cargo run --release -p research --bin adr0001_node_budget`.
//!
//! Recorded output, 2026-09-10, 4-core container, sodium-rust at 9c7993d:
//!
//! ```text
//! 1. Cost of an event against chain depth (sink -> map*N -> listen)
//! maps on the firing path                    allocs/send         ns/send marginal allocs     marginal ns
//! 0                                                 22.0          4333.3               -               -
//! 1                                                 36.0          6817.3            14.0          2484.0
//! 2                                                 49.0          9526.4            13.0          2709.1
//! 4                                                 77.0         15177.6            14.0          2825.6
//! 8                                                131.0         26296.0            13.5          2779.6
//! 16                                               237.0         46942.7            13.3          2580.8
//! ```
//!
//! Conclusions that reached the ADR: the floor is ~3.7 µs and 22 allocations
//! before any combinator exists, and each `map` node adds ~2.6 µs and exactly
//! 14 allocations. A `map` node runs one closure over a `u16`; against 14
//! allocations, none of that budget is the user's computation.

use std::hint::black_box;

use research::{chain, heading, measure, row, CountingAlloc, Observer};
use sodium_rust::{SodiumCtx, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const N: usize = 20_000;

fn main() {
    heading(
        "1. Cost of an event against chain depth (sink -> map*N -> listen)",
        &[
            "maps on the firing path",
            "allocs/send",
            "ns/send",
            "marginal allocs",
            "marginal ns",
        ],
    );

    let mut prev: Option<(usize, f64, f64)> = None;
    for depth in [0usize, 1, 2, 4, 8, 16] {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let listener = observer.listen(&chain(&sink, depth));

        sink.send(0); // warm: the first event through a fresh graph is one-off
        let cost = measure(N, || sink.send(black_box(1)));
        black_box(observer.total());

        let (ma, mn) = match prev {
            None => ("-".to_string(), "-".to_string()),
            Some((pd, pa, pns)) => {
                let d = (depth - pd) as f64;
                (
                    format!("{:.1}", (cost.allocs - pa) / d),
                    format!("{:.1}", (cost.nanos - pns) / d),
                )
            }
        };
        row(&[
            depth.to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.1}", cost.nanos),
            ma,
            mn,
        ]);
        prev = Some((depth, cost.allocs, cost.nanos));
        drop(listener);
    }
}
