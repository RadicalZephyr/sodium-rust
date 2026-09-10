//! ADR 1: why two experiments disagree about what an idle node costs.
//!
//! `adr0001_cost_model.rs` says a node off the firing path costs nothing.
//! `benches/adr0001_instruction_counts.rs` says it costs about 18 instructions
//! per event. Both are right, and the difference is not the instrument: it is
//! whether the intermediate `Stream` handles were still alive when the sends
//! happened.
//!
//! Dropping a handle decrements a `GcNode` refcount, which files the node as a
//! candidate cycle root. `collect_cycles` runs at the end of every transaction
//! and walks the graph reachable from those roots, so a graph whose handles
//! have been released pays for its idle nodes on every event, and one still
//! holding them pays nothing. `cost_model` binds its chains to locals;
//! `instruction_counts` passes `chain(..)` straight into `listen` as a
//! temporary. That is the whole of the disagreement.
//!
//! This file holds the graph identical across both arms — same nodes, same
//! listeners, same sinks — and varies only handle lifetime, so nothing else
//! can account for the difference. The wall-clock columns are here to show
//! that a timing loop cannot see the effect in either arm, which is the
//! argument for tier 1 counting instructions; the instruction columns are in
//! the companion bench, since counting them needs valgrind.
//!
//! Application code drops its handles — you write
//! `sink.stream().map(f).listen(g)` and keep only the `Listener` — so the
//! dropped arm is the one that describes a real program.
//!
//! Run with `cargo run --release -p research --bin adr0001_root_set`.
//!
//! Recorded output, 2026-09-10, 4-core container, sodium-rust at 9c7993d:
//!
//! ```text
//! 1. Idle nodes on a second, never-fired sink; handles HELD alive
//! idle nodes                                allocs/send         ns/send
//! 0                                                36.0          6403.1
//! 8                                                36.0          6367.5
//! 64                                               36.0          6423.8
//! 200                                              36.0          6390.6
//!
//! 2. The same graph, intermediate handles DROPPED after wiring
//! idle nodes                                allocs/send         ns/send
//! 0                                                36.0          6455.6
//! 8                                                36.0          6485.2
//! 64                                               36.0          6544.9
//! 200                                              36.0          6607.9
//!
//!   companion bench, 64 events, instructions:
//!     held     1 063 741 / 1 063 741 / 1 057 784 / 1 065 508   -> 0 per node per event
//!     dropped  1 078 499 / 1 088 611 / 1 142 517 / 1 314 421   -> 18.4 per node per event
//! ```
//!
//! Conclusions that reached the ADR: an idle node costs 0 or 18.4 instructions
//! per event depending on handle lifetime, no allocations either way, and no
//! measurable time either way. Tier 2 arms therefore have to pin handle
//! lifetime the way tier 4 pins node counts, or two arms that look identical in
//! source will measure different graphs.

use std::hint::black_box;

use research::{heading, measure, row, CountingAlloc, Observer};
use sodium_rust::{SodiumCtx, Stream, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const N: usize = 20_000;

fn sweep(title: &str, hold: bool) {
    heading(title, &["idle nodes", "allocs/send", "ns/send"]);
    for idle in [0usize, 8, 64, 200] {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let side: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let mut held: Vec<Stream<u16>> = Vec::new();
        let mut listeners = Vec::new();

        {
            let s = sink.stream().map(|v: &u16| v.wrapping_add(1));
            listeners.push(observer.listen(&s));
            if hold {
                held.push(s);
            }
        }
        {
            let mut s = side.stream();
            for _ in 0..idle {
                let next = s.map(|v: &u16| v.wrapping_add(1));
                if hold {
                    held.push(s);
                }
                s = next;
            }
            listeners.push(observer.listen(&s));
            if hold {
                held.push(s);
            }
        }

        sink.send(0);
        let cost = measure(N, || sink.send(black_box(1)));
        black_box(observer.total());
        row(&[
            idle.to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.1}", cost.nanos),
        ]);
        drop(listeners);
        drop(held);
    }
}

fn main() {
    sweep(
        "1. Idle nodes on a second, never-fired sink; handles HELD alive",
        true,
    );
    sweep(
        "2. The same graph, intermediate handles DROPPED after wiring",
        false,
    );
}
