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
//! 0                                                 54.0           12583
//! 1                                                 54.0           12591
//! 4                                                 54.0           12455
//! 16                                                54.0           12805
//! 64                                                54.0           12557
//!
//! 2. switch_s flip cost against the branch being switched IN
//! upstream maps                              allocs/flip         ns/flip
//! 0                                                 54.0           12721
//! 1                                                 54.0           13797
//! 4                                                 54.0           17731
//! 16                                                54.0           32385
//! 64                                                54.0           92567
//!
//! 3a. A long-lived context, subgraphs released with drop(listener)
//! state                                       node_count     allocs/send         ns/send
//! fresh                                                2            22.0            3743
//! after 100 cycles                                   202          2335.2          458358
//! after 300 cycles                                   602          6938.3         1368871
//! after 800 cycles                                  1602         18441.3         3691062
//! + collect_cycles()                                1602         18441.2         3713318
//! + two more collects                               1602         18441.2         3688056
//!
//! 3b. The same, released with listener.unlisten()
//! state                                       node_count     allocs/send         ns/send
//! fresh                                                2            22.0            3764
//! after 100 cycles                                     2           122.1           17360
//! after 300 cycles                                     2           322.1           44188
//! after 800 cycles                                     2           822.1          113631
//! + collect_cycles()                                   2           822.0          112802
//! + two more collects                                  2           822.0          112767
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
