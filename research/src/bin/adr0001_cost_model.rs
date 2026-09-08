//! ADR 1: where does the cost of an event actually go?
//!
//! Three questions the ADR's tier 1 and tier 2 designs depend on. Does a node
//! that is not on the firing path cost anything? At an equal node count, does
//! the shape of the graph matter? And what is a `Listener` worth next to a
//! `map` node?
//!
//! Run with `cargo run --release -p research --bin adr0001_cost_model`.
//!
//! **Read the first section with the companion bench.** Wall clock says idle
//! nodes are free. They are not: `benches/adr0001_instruction_counts.rs`
//! measures them at about 61 instructions per node per event, which is real but
//! far below what a timing loop on a shared machine can resolve. This
//! experiment is kept precisely because it gives the wrong answer confidently,
//! which is the argument for tier 1 using callgrind.
//!
//! Timings move with machine load; the allocation counts do not. Compare
//! within a run, not across runs.

//!
//! Recorded output, 2026-09-08, 4-core container, sodium-rust at 4316728:
//!
//! ```text
//! 1. Fire sink A (1-map chain) while an unrelated chain on sink B grows
//! B's maps                                 A allocs/send       A ns/send
//! 0                                                 43.0         10115.5
//! 1                                                 43.0          8963.9
//! 4                                                 43.0          7653.1
//! 16                                                43.0          7447.3
//! 64                                                43.0          7404.5
//! 256                                               43.0          7668.7
//!   (wall clock says flat; the callgrind bench says ~61 instructions per idle node)
//!
//! 2. 64 map nodes on the firing path, arranged three ways
//! arrangement                                allocs/send         ns/send
//! depth 64, 1 listener                             950.0        229134.2
//! width 64, 64 listeners                          1646.0        349592.3
//! width 64 + 63 merges, 1 listener                2102.0        459969.5
//!
//! 3. N listeners attached directly to one sink
//! listeners                                  allocs/send         ns/send
//! 1                                                 27.0          4508.2
//! 2                                                 38.0          6584.5
//! 8                                                114.0         18984.2
//! 32                                               388.0         71856.0
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
