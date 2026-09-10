//! ADR 1: where inside a node does the budget go?
//!
//! The other experiments here count things — allocations, instructions, events.
//! Counting says an arm moved; it does not say which of `malloc`, the
//! collector, the locks or the hasher it moved in. That needs a profile, and
//! this binary exists to be the thing profiled: a minimal
//! `sink -> map -> listen` driven a fixed number of times, with nothing else in
//! the process to dilute the attribution.
//!
//! It is not run for its output. Run it under callgrind:
//!
//! ```shell
//! cargo build --release -p research --bin adr0001_profile_target
//! valgrind --tool=callgrind --callgrind-out-file=cg.out \
//!     ./target/release/adr0001_profile_target
//! callgrind_annotate --threshold=60 cg.out
//! ```
//!
//! To attribute only the sends and not graph construction or teardown, collect
//! against the `drive_sends` symbol, which is `#[no_mangle]` and
//! `#[inline(never)]` for exactly that purpose:
//!
//! ```shell
//! valgrind --tool=callgrind --collect-atstart=no --toggle-collect=drive_sends \
//!     --callgrind-out-file=cg.out ./target/release/adr0001_profile_target
//! ```
//!
//! Recorded attribution, 2026-09-10, 4-core container, 2000 events through
//! `sink -> map -> listen`, whole process, as a share of total instructions:
//!
//! ```text
//! sodium-rust at 9c7993d, 34 045 397 Ir
//!   11.65%  malloc.c:_int_free
//!    9.26%  malloc.c:malloc
//!    5.76%  malloc.c:free
//!    4.66%  parking_lot raw_rwlock.rs, in Node::new::{{closure}}
//!    3.84%  impl_/node.rs, in Node::new::{{closure}}
//!    2.22%  core atomic.rs, in Node::new::{{closure}}
//!    2.06%  malloc.c:_int_malloc
//!    1.65%  std alloc/unix.rs:__rdl_alloc
//!    1.48%  impl_/node.rs:Node::clone
//!    1.30%  impl_/gc_node.rs:GcNode::dec_ref
//!
//!   allocator, summed                              ~30%
//!   lock and atomic traffic in the update closure   ~11%
//!   core::fmt                                        0%   (absent entirely)
//!
//! The same shape at 4316728, before the display_graph fix, was 49 867 025 Ir,
//! with core::fmt at ~9% of it and SipHash over `HashSet<*const GcNodeData>` at
//! ~4%. Both were `display_graph`; both are gone.
//! ```
//!
//! Conclusions that reached the ADR: nothing in a `sink -> map -> listen` graph
//! formats anything, and `core::fmt` was 9% of its instructions.
//! `GcCtx::mark_roots` called `display_graph` unconditionally — a walk over
//! every reachable node building a `String` per node, handed to a `trace!` that
//! discarded it whenever trace was off. That is the finding this file is here
//! to make repeatable, and the reason tier 1 stores a profile alongside its
//! counts rather than counts alone.

use std::hint::black_box;

use research::{chain, Observer};
use sodium_rust::{SodiumCtx, StreamSink};

const SENDS: u16 = 2000;

/// The measured region, kept out of line so callgrind can toggle on it.
#[inline(never)]
#[no_mangle]
pub extern "C" fn drive_sends(sink: &StreamSink<u16>, observer: &Observer) -> u64 {
    for v in 0..SENDS {
        sink.send(black_box(v));
    }
    black_box(observer.total())
}

fn main() {
    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let observer = Observer::new();
    // Passed as a temporary, so the intermediate handles are released during
    // wiring: this is the shape application code produces. See
    // `adr0001_root_set.rs` for why that matters.
    let listener = observer.listen(&chain(&sink, 1));

    let total = drive_sends(&sink, &observer);
    println!("{total} (this binary is for profiling, not for its output)");

    drop(listener);
}
