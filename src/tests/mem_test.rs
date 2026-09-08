use crate::Cell;
use crate::CellSink;
use crate::Operational;
use crate::SodiumCtx;
use crate::StreamSink;

use crate::tests::{assert_memory_freed, init};

// SET RUST_LOG=trace
#[test]
fn log_test() {
    init();
    log::info!("a");
    log::debug!("a");
    log::debug!("b");
}

#[test]
fn mem() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let s = sodium_ctx.new_stream::<i32>();
        let s2 = s.map_to(5);
        let s3 = s2.map_to(3);
        let l = s3.listen_weak(|_: &i32| {});
        l.unlisten();
    }
    sodium_ctx.impl_.collect_cycles();
    let node_count = sodium_ctx.impl_.node_count();
    let node_ref_count = sodium_ctx.impl_.node_ref_count();
    println!();
    println!("node_count {}", node_count);
    println!("node_ref_count {}", node_ref_count);
    assert_eq!(node_count, 0);
}

#[test]
fn map_s_mem() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let ss1: StreamSink<i32> = sodium_ctx.new_stream_sink();
        let s1 = ss1.stream();
        let _s2 = s1.map_to(5);
        let l = _s2.listen_weak(|_: &u32| {});
        println!("l: {:?}", l.impl_);
        l.unlisten();
    }
    sodium_ctx.impl_.collect_cycles();
    let node_count = sodium_ctx.impl_.node_count();
    let node_ref_count = sodium_ctx.impl_.node_ref_count();
    println!();
    println!("node_count {}", node_count);
    println!("node_ref_count {}", node_ref_count);
    assert_eq!(node_count, 0);
}

#[test]
fn map_c_mem() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let cs1: CellSink<i32> = sodium_ctx.new_cell_sink(3);
        let c1 = cs1.cell();
        let _c2 = c1.map(|a: &i32| a + 5);
        //let l = _c2.listen_weak(|_:&i32| {});
        //l.unlisten();
    }
    sodium_ctx.impl_.collect_cycles();
    let node_count = sodium_ctx.impl_.node_count();
    let node_ref_count = sodium_ctx.impl_.node_ref_count();
    println!();
    println!("node_count {}", node_count);
    println!("node_ref_count {}", node_ref_count);
    assert_eq!(node_count, 0);
}

// A stream loop that defers into itself: nothing is ever fired through
// it, since that would loop forever. This is purely a check that the
// resulting cycle can still be collected.
//
// Ported from sodium-typescript's StreamSink.spec.ts.
// FIXME: this cycle cannot be collected. `collect_cycles` panics from
// inside the collector with "freed node ref count did not drop to zero".
#[ignore = "collect_cycles panics on the self-deferring stream loop"]
#[test]
fn defer_split_memory_cycle() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let sl = sodium_ctx.transaction(|| {
            let sl = sodium_ctx.new_stream_loop::<i32>();
            sl.loop_(&Operational::defer(&sl.stream()));
            sl.stream()
        });
        let l = sl.listen(|_: &i32| {});
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

/// Pump `send` and assert that the graph does not grow per event.
///
/// The upstream Java `MemoryTestN` classes are `main` programs that pump
/// a sink in an unbounded loop while printing the heap size, so that a
/// human can watch for growth. Here we pump a bounded number of events
/// and assert that the node count is unchanged, which catches the same
/// per-event leak without a human in the loop.
fn assert_no_per_event_growth<K: FnMut(i32)>(sodium_ctx: &SodiumCtx, mut send: K) {
    // Warm up first, so that any one-off setup is already accounted for.
    for i in 0..WARMUP {
        send(i);
    }
    sodium_ctx.impl_.collect_cycles();
    let before = sodium_ctx.impl_.node_count();
    for i in WARMUP..WARMUP + ITERATIONS {
        send(i);
    }
    sodium_ctx.impl_.collect_cycles();
    let after = sodium_ctx.impl_.node_count();
    assert_eq!(
        before, after,
        "node count grew from {} to {} over {} events",
        before, after, ITERATIONS
    );
}

const WARMUP: i32 = 100;
const ITERATIONS: i32 = 1000;

#[test]
fn memory_test1() {
    init();
    let sodium_ctx = SodiumCtx::new();
    run_memory_test1(&sodium_ctx);
}

// FIXME: memory_test1's network does not grow per event, but tearing it
// down leaves three nodes behind. That is the same residue as
// tests::lift_with_nested_data_map, and the same shape: a switch_c over
// a cell of cells built by mapping.
#[ignore = "the switch_c-over-mapped-cell network leaks nodes on teardown"]
#[test]
fn memory_test1_frees_memory() {
    init();
    let sodium_ctx = SodiumCtx::new();
    run_memory_test1(&sodium_ctx);
    assert_memory_freed(&sodium_ctx);
}

fn run_memory_test1(sodium_ctx: &SodiumCtx) {
    {
        let et = sodium_ctx.new_stream_sink::<i32>();
        let t = et.stream().hold(0);
        let change_tens = et
            .stream()
            .snapshot(
                &t,
                |neu: &i32, old: &i32| {
                    if neu == old {
                        None
                    } else {
                        Some(*neu)
                    }
                },
            )
            .filter_option();
        let oout = {
            let t2 = t.clone();
            change_tens
                .map(move |tens: &i32| {
                    let tens = *tens;
                    t2.map(move |tt: &i32| (tens, *tt))
                })
                .hold(t.map(|tt: &i32| (0, *tt)))
        };
        let out = Cell::switch_c(&oout);
        let l = out.listen(|_: &(i32, i32)| {});
        assert_no_per_event_growth(sodium_ctx, |i| et.send(i));
        l.unlisten();
    }
}

#[test]
fn memory_test3() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let et = sodium_ctx.new_stream_sink::<i32>();
        let t = et.stream().hold(0);
        let e_change = sodium_ctx.new_stream_sink::<i32>();
        let oout = {
            let t2 = t.clone();
            e_change
                .stream()
                .map(move |_: &i32| t2.clone())
                .hold(t.clone())
        };
        let out = Cell::switch_c(&oout);
        let l = out.listen(|_: &i32| {});
        assert_no_per_event_growth(sodium_ctx, |i| e_change.send(i));
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn memory_test4() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let et = sodium_ctx.new_stream_sink::<i32>();
        let e_change = sodium_ctx.new_stream_sink::<i32>();
        let oout = {
            let et2 = et.clone();
            e_change
                .stream()
                .map(move |_: &i32| et2.stream())
                .hold(et.stream())
        };
        let out = Cell::switch_s(&oout);
        let l = out.listen(|_: &i32| {});
        assert_no_per_event_growth(sodium_ctx, |i| e_change.send(i));
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn memory_test5() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let e_change = sodium_ctx.new_stream_sink::<i32>();
        let out = e_change.stream().hold(0);
        let l = out.listen(|_: &i32| {});
        assert_no_per_event_growth(sodium_ctx, |i| e_change.send(i));
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}
