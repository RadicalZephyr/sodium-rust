//! ADR 1: where does the cost of an event actually go?
//!
//! Three questions the ADR's tier 1 and tier 2 designs depend on. Does a node
//! that is not on the firing path cost anything? At an equal node count, does
//! the shape of the graph matter? And what is a `Listener` worth next to a
//! `map` node?
//!
//! Run with `cargo run --release -p research --bin adr0001_cost_model`.
//!
//! **Read the first section with the companion bench and with
//! `adr0001_root_set.rs`.** Section 1 says idle nodes are free. On a clock they
//! are: `adr0001_root_set.rs` sweeps the same shape and is flat to within noise.
//! On instructions they are not — `benches/adr0001_instruction_counts.rs`
//! measures the same shape at 18.4 instructions per idle node per event. Both
//! readings are correct, and the gap between them is the point: these are real
//! instructions that cost no measurable time.
//!
//! Section 1 binds nothing, passing `chain(..)` straight into `listen` as a
//! temporary, so its intermediate `Stream` handles are released during wiring.
//! That is what puts nodes on the collector's candidate-root list in the first
//! place, and it is what application code does. `adr0001_root_set.rs` holds the
//! handles instead and the instruction cost goes to zero, so the shape below is
//! the expensive one, not the cheap one.
//!
//! Timings move with machine load; the allocation counts do not. Compare
//! within a run, not across runs.

//!
//! Recorded output, 2026-09-08, 4-core container, sodium-rust at 4316728:
//!
//! ```text
//! 1. Fire sink A (1-map chain) while an unrelated chain on sink B grows
//! B's maps                                 A allocs/send       A ns/send
//! 0                                                 36.0          6639.0
//! 1                                                 36.0          6533.2
//! 4                                                 36.0          6211.4
//! 16                                                36.0          6231.7
//! 64                                                36.0          6192.9
//! 256                                               36.0          6645.6
//!   (wall clock says flat; the callgrind bench says ~61 instructions per idle node)
//!   (wall clock says flat; the callgrind bench says 18.4 instructions per idle node)
//!
//! 2. 64 map nodes on the firing path, arranged three ways
//! arrangement                                allocs/send         ns/send
//! depth 64, 1 listener                             865.0        185622.7
//! width 64, 64 listeners                          1496.0        292074.0
//! width 64 + 63 merges, 1 listener                1938.0        403368.1
//!
//! 3. N listeners attached directly to one sink
//! listeners                                  allocs/send         ns/send
//! 1                                                 22.0          3876.6
//! 2                                                 32.0          5679.8
//! 8                                                 96.0         16458.6
//! 32                                               340.0         58761.1
//! ```

use std::hint::black_box;

use research::{chain, heading, measure, row, CountingAlloc, Observer};
use sodium_rust::{Listener, SodiumCtx, Stream, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SENDS: usize = 2000;

fn idle_nodes_look_free() {
    heading(
        "1. Fire sink A (1-map chain) while an unrelated chain on sink B grows",
        &["B's maps", "A allocs/send", "A ns/send"],
    );
    for b_maps in [0usize, 1, 4, 16, 64, 256] {
        let ctx = SodiumCtx::new();
        let observer = Observer::new();
        let sa: StreamSink<u16> = ctx.new_stream_sink();
        let sb: StreamSink<u16> = ctx.new_stream_sink();
        let _la = observer.listen(&chain(&sa, 1));
        let _lb = observer.listen(&chain(&sb, b_maps));
        sa.send(0);

        let cost = measure(SENDS, || sa.send(black_box(1)));
        black_box(observer.total());
        row(&[
            b_maps.to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.1}", cost.nanos),
        ]);
    }
    println!("  (wall clock says flat; the callgrind bench says ~61 instructions per idle node)");
}

fn shape_at_equal_node_count() {
    const NODES: usize = 64;
    heading(
        "2. 64 map nodes on the firing path, arranged three ways",
        &["arrangement", "allocs/send", "ns/send"],
    );

    // Deep: one chain, one listener.
    let ctx = SodiumCtx::new();
    let observer = Observer::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let _l = observer.listen(&chain(&sink, NODES));
    sink.send(0);
    let deep = measure(SENDS, || sink.send(black_box(1)));
    black_box(observer.total());

    // Wide: 64 parallel maps, each with its own listener.
    let ctx = SodiumCtx::new();
    let observer = Observer::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let mut listeners: Vec<Listener> = Vec::new();
    for _ in 0..NODES {
        listeners.push(observer.listen(&chain(&sink, 1)));
    }
    sink.send(0);
    let wide = measure(SENDS, || sink.send(black_box(1)));
    black_box(observer.total());

    // Wide, rejoined: 64 parallel maps merged back to one listener, so the
    // listener count matches the deep arm.
    let ctx = SodiumCtx::new();
    let observer = Observer::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let mut joined: Option<Stream<u16>> = None;
    for _ in 0..NODES {
        let branch = chain(&sink, 1);
        joined = Some(match joined {
            None => branch,
            Some(j) => j.merge(&branch, |a: &u16, b: &u16| a.wrapping_add(*b)),
        });
    }
    let _l = observer.listen(&joined.expect("NODES > 0"));
    sink.send(0);
    let rejoined = measure(SENDS, || sink.send(black_box(1)));
    black_box(observer.total());

    for (label, cost) in [
        ("depth 64, 1 listener", deep),
        ("width 64, 64 listeners", wide),
        ("width 64 + 63 merges, 1 listener", rejoined),
    ] {
        row(&[
            label.to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.1}", cost.nanos),
        ]);
    }
}

fn what_a_listener_costs() {
    heading(
        "3. N listeners attached directly to one sink",
        &["listeners", "allocs/send", "ns/send"],
    );
    for n in [1usize, 2, 8, 32] {
        let ctx = SodiumCtx::new();
        let observer = Observer::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let stream = sink.stream();
        let listeners: Vec<Listener> = (0..n).map(|_| observer.listen(&stream)).collect();
        sink.send(0);

        let cost = measure(SENDS, || sink.send(black_box(1)));
        black_box(observer.total());
        black_box(&listeners);
        row(&[
            n.to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.1}", cost.nanos),
        ]);
    }
}

fn main() {
    idle_nodes_look_free();
    shape_at_equal_node_count();
    what_a_listener_costs();
}
