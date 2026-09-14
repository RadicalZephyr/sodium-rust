//! What one `send` costs in the library as it stands, and how much of that is
//! heap traffic.
//!
//! This is the denominator for every ratio ADR-0002 quotes. The record's
//! question is what fraction of propagation a permission token could win back,
//! and that fraction is meaningless without knowing what a `send` costs in the
//! first place and where it goes.
//!
//! Two shapes of measurement:
//!
//! - **ns per send**, over graphs of varying depth, so the per-node cost can be
//!   separated from the fixed per-transaction cost.
//! - **allocations per send**, via a counting global allocator, because the
//!   profile puts roughly a third of all instructions in malloc and free.
//!
//! The `idle` column adds nodes that are in the graph but never fire. If the
//! cycle collector walked the whole graph on every transaction, ns per send
//! would climb with it; it does not climb much, which is how we know the
//! collector's cost tracks the nodes actually touched.
//!
//! ```shell
//! cargo run --release -p adr-research --bin 0002-propagation-profile
//! ```
//!
//! Run it under a profiler for the instruction-level breakdown the record
//! quotes:
//!
//! ```shell
//! valgrind --tool=callgrind --callgrind-out-file=cg.out \
//!     ./target/release/0002-propagation-profile
//! callgrind_annotate cg.out
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use sodium_rust::{Listener, SodiumCtx, Stream, StreamSink};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static COUNTING: AtomicBool = AtomicBool::new(false);

/// Forwards to the system allocator, counting allocations while armed. Only
/// `alloc` is counted: the record quotes allocations per send, and every one of
/// them is freed again within the transaction.
struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// A sink feeding a chain of `depth` `map` nodes, ending in a listener. One
/// send therefore updates `depth + 2` nodes.
fn chain(ctx: &SodiumCtx, depth: usize) -> (StreamSink<i32>, Listener) {
    let sink: StreamSink<i32> = ctx.new_stream_sink();
    let mut stream: Stream<i32> = sink.stream();
    for _ in 0..depth {
        stream = stream.map(|a: &i32| a.wrapping_add(1));
    }
    let listener = stream.listen(|a: &i32| {
        black_box(a);
    });
    (sink, listener)
}

fn measure(depth: usize, idle: usize) -> (f64, f64) {
    let ctx = SodiumCtx::new();
    let (sink, listener) = chain(&ctx, depth);
    // A second chain that never fires, to separate graph size from the number
    // of nodes a transaction actually updates.
    let (_idle_sink, idle_listener) = chain(&ctx, idle);

    for i in 0..500 {
        sink.send(black_box(i));
    }

    let sends = 4_000u64;
    let start = Instant::now();
    for i in 0..sends {
        sink.send(black_box(i as i32));
    }
    let ns_per_send = start.elapsed().as_nanos() as f64 / sends as f64;

    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    let counted = 200u64;
    for i in 0..counted {
        sink.send(black_box(i as i32));
    }
    COUNTING.store(false, Ordering::Relaxed);
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) as f64 / counted as f64;

    drop(listener);
    drop(idle_listener);
    (ns_per_send, allocations)
}

fn main() {
    println!(
        "{:<8} {:>6} {:>8} {:>14} {:>16} {:>14}",
        "depth", "idle", "updated", "ns per send", "ns per update", "allocs/send"
    );
    for (depth, idle) in [(1usize, 0usize), (16, 0), (16, 64), (16, 256), (64, 0)] {
        let updated = depth + 2;
        let (ns, allocations) = measure(depth, idle);
        println!(
            "{:<8} {:>6} {:>8} {:>14.1} {:>16.1} {:>14.1}",
            depth,
            idle,
            updated,
            ns,
            ns / updated as f64,
            allocations
        );
    }
}
