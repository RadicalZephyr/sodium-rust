//! ADR 1: what reconfiguring a graph costs, and what a long-lived context does.
//!
//! The first two sections price `switch_s`: reconfiguration turns out to cost
//! in proportion to the branch being switched *in*, and to be flat in whatever
//! sits downstream of the switch.
//!
//! The third section is the one that shaped the ADR. Every benchmark in
//! `benches/sodium.rs` builds a fresh `SodiumCtx` per iteration, so none of
//! them can see what happens to a context that stays alive while subgraphs come
//! and go. What happens is that it gets slower without bound.
//!
//! Two distinct effects, and they are not the same bug:
//!
//! - Releasing a `Listener` by dropping it retains its nodes. `node_count`
//!   climbs by two per cycle and `collect_cycles()` does not bring it back
//!   down.
//! - Releasing it with `unlisten()` does return `node_count` to its starting
//!   value, and sends still get about twelve times more expensive and stay
//!   there.
//!
//! Neither is diagnosed here. They are recorded because they are reproducible
//! and because `docs/benchmark-plan.md` phase 1 turns them into two ignored
//! tests that become the acceptance criteria for fixing them.
//!
//! Run with `cargo run --release -p research --bin adr0001_dynamic_graphs`.

//!
//! Recorded output, 2026-09-08, 4-core container, sodium-rust at 4316728:
//!
//! ```text
//! 1. switch_s flip cost against the graph DOWNSTREAM of the switch
//! downstream maps                            allocs/flip         ns/flip
//! 0                                                 68.0           15267
//! 1                                                 68.0           15069
//! 4                                                 68.0           15294
//! 16                                                68.0           15061
//! 64                                                68.0           15364
//!
//! 2. switch_s flip cost against the branch being switched IN
//! upstream maps                              allocs/flip         ns/flip
//! 0                                                 68.0           15067
//! 1                                                 72.0           17411
//! 4                                                 79.0           23049
//! 16                                               107.0           46216
//! 64                                               209.0          138333
//!
//! 3a. A long-lived context, subgraphs released with drop(listener)
//! state                                       node_count     allocs/send         ns/send
//! fresh                                                2            27.0            4869
//! after 100 cycles                                   202          2558.3          509119
//! after 300 cycles                                   602          7567.3         1616148
//! after 800 cycles                                  1602         20073.3         4485951
//! + collect_cycles()                                1602         20073.2         4459554
//! + two more collects                               1602         20073.2         4481106
//!
//! 3b. The same, released with listener.unlisten()
//! state                                       node_count     allocs/send         ns/send
//! fresh                                                2            27.0            5174
//! after 100 cycles                                     2           127.2           17843
//! after 300 cycles                                     2           327.2           45354
//! after 800 cycles                                     2           827.2          117505
//! + collect_cycles()                                   2           827.0          116241
//! + two more collects                                  2           827.0          112347
//! ```

use std::hint::black_box;

use research::{chain, heading, measure, row, CountingAlloc, Observer};
use sodium_rust::{Cell, Listener, SodiumCtx, Stream, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FLIPS: usize = 500;
const PROBE_SENDS: usize = 50;

/// Flip a `switch_s` between two branches, with `downstream` maps after the
/// switch and `upstream` maps in each branch.
fn flip_cost(upstream: usize, downstream: usize) -> (f64, f64) {
    let ctx = SodiumCtx::new();
    let sa: StreamSink<u16> = ctx.new_stream_sink();
    let sb: StreamSink<u16> = ctx.new_stream_sink();
    let branch_a = chain(&sa, upstream);
    let branch_b = chain(&sb, upstream);

    let which = ctx.new_cell_sink(branch_a.clone());
    let mut switched: Stream<u16> = Cell::switch_s(&which.cell());
    for _ in 0..downstream {
        switched = switched.map(|v: &u16| v.wrapping_add(1));
    }
    let observer = Observer::new();
    let listener = observer.listen(&switched);
    which.send(branch_a.clone());

    let mut flip = 0usize;
    let cost = measure(FLIPS, || {
        flip += 1;
        which.send(black_box(if flip.is_multiple_of(2) {
            branch_a.clone()
        } else {
            branch_b.clone()
        }));
    });
    black_box(observer.total());
    drop(listener);
    (cost.allocs, cost.nanos)
}

fn switch_scaling() {
    heading(
        "1. switch_s flip cost against the graph DOWNSTREAM of the switch",
        &["downstream maps", "allocs/flip", "ns/flip"],
    );
    for downstream in [0usize, 1, 4, 16, 64] {
        let (allocs, nanos) = flip_cost(0, downstream);
        row(&[
            downstream.to_string(),
            format!("{allocs:.1}"),
            format!("{nanos:.0}"),
        ]);
    }

    heading(
        "2. switch_s flip cost against the branch being switched IN",
        &["upstream maps", "allocs/flip", "ns/flip"],
    );
    for upstream in [0usize, 1, 4, 16, 64] {
        let (allocs, nanos) = flip_cost(upstream, 0);
        row(&[
            upstream.to_string(),
            format!("{allocs:.1}"),
            format!("{nanos:.0}"),
        ]);
    }
}

/// How a plain send fares after `cycles` subgraphs have been built and released.
fn churn_then_probe(unlisten: bool) {
    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let observer = Observer::new();
    let _permanent = observer.listen(&sink.stream());
    sink.send(0);

    let probe = |label: &str, ctx: &SodiumCtx, sink: &StreamSink<u16>| {
        let cost = measure(PROBE_SENDS, || sink.send(black_box(1)));
        row(&[
            label.to_string(),
            ctx.impl_.node_count().to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.0}", cost.nanos),
        ]);
    };

    probe("fresh", &ctx, &sink);

    let mut done = 0usize;
    for checkpoint in [100usize, 300, 800] {
        while done < checkpoint {
            let branch = chain(&sink, 1);
            let listener: Listener = observer.listen(&branch);
            if unlisten {
                listener.unlisten();
            } else {
                drop(listener);
            }
            done += 1;
        }
        probe(&format!("after {done} cycles"), &ctx, &sink);
    }

    ctx.impl_.collect_cycles();
    probe("+ collect_cycles()", &ctx, &sink);
    ctx.impl_.collect_cycles();
    ctx.impl_.collect_cycles();
    probe("+ two more collects", &ctx, &sink);

    black_box(observer.total());
}

fn long_lived_context() {
    heading(
        "3a. A long-lived context, subgraphs released with drop(listener)",
        &["state", "node_count", "allocs/send", "ns/send"],
    );
    churn_then_probe(false);

    heading(
        "3b. The same, released with listener.unlisten()",
        &["state", "node_count", "allocs/send", "ns/send"],
    );
    churn_then_probe(true);
}

fn main() {
    switch_scaling();
    long_lived_context();
}
