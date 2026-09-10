//! ADR 1: measuring sodium-rust with callgrind instead of a clock.
//!
//! Wall clock on a shared machine carries several percent of noise, which is
//! more than most single-combinator regressions. Callgrind counts instructions
//! instead, and the count is deterministic. This bench establishes the four
//! facts the ADR's tier 1 rests on:
//!
//! 1. `chain_depth` — the marginal cost of a node is *not* uniform, so the
//!    ledger must track each arm's absolute count rather than the difference
//!    between arms. Re-run it: the spread across chain positions is far larger
//!    than the run-to-run variation, so it is structural, not noise.
//! 2. `measured_region` — setup passed via `setup =` is excluded, but the
//!    `Drop` of the rig the body owns is *not*. A body that lets its rig fall
//!    out of scope charges teardown of the whole graph to the combinator under
//!    test. Every body here ends in `std::mem::forget`.
//! 3. `idle_nodes` — what a node that never fires costs depends on whether the
//!    graph's intermediate `Stream` handles are still alive. Dropping one
//!    decrements a `GcNode` refcount and files the node as a candidate cycle
//!    root, and `collect_cycles` walks from those roots at the end of every
//!    transaction. The `dropped` arms therefore pay ~18 instructions per idle
//!    node per event and the `held` arms pay nothing, on graphs that are
//!    otherwise identical. Application code drops its handles, so `dropped` is
//!    the arm that describes a real program. No timing loop resolves either
//!    arm, which is what settled the harness choice —
//!    `src/bin/adr0001_root_set.rs` is the same sweep on a clock, and flat.
//! 4. `heap_padding` — the control for (3). Thousands of unrelated live
//!    allocations do not move the number, so (3) is about sodium's nodes and
//!    not about allocator state.
//!
//! Linux only: callgrind is a valgrind tool. Needs `valgrind` on `PATH` and a
//! matching runner, `cargo install --version 0.16.1 iai-callgrind-runner`; a
//! version skew between runner and crate fails with a message about `$PATH`,
//! which sends you looking in the wrong place.
//!
//! Run with `cargo bench -p research --bench adr0001_instruction_counts`.
//!
//! Recorded output, 2026-09-10, 4-core container, sodium-rust at 9c7993d,
//! 64 events per body:
//!
//! ```text
//! chain_depth    n0    630 176     n1  1 076 165     n2  1 468 995
//!                n3  1 947 190     n4  2 358 304     n8  4 385 993
//!   marginal instructions per node per event: 6 969 / 6 138 / 7 472 / 6 424 / 7 921
//!   the floor, n0, is 9 846 instructions per event before any combinator exists
//!
//! empty_drop     tiny    559       big    559      <- setup excluded (does not scale
//! empty_forget   tiny     16       big     16         with 200 extra nodes), but the
//!                                                     ~543 gap between the two IS the
//!                                                     rig's Drop, inside the measured
//!                                                     region unless you forget it
//!
//! idle_nodes  dropped     0 nodes 1 078 502     8 nodes 1 088 614
//!                        64 nodes 1 142 520   200 nodes 1 314 424
//!   => (1 314 424 - 1 078 502) / 200 / 64 = 18.4 instructions per idle node per event
//!
//!             held        0 nodes 1 063 744     8 nodes 1 063 744
//!                        64 nodes 1 057 787   200 nodes 1 065 511
//!   => flat. Same graph; the handles are still alive, so the collector has no
//!      candidate roots to walk and the idle nodes cost nothing.
//!
//! heap_padding     0 blocks 1 076 165   64 blocks 1 076 165  200 blocks 1 076 165
//!   => bit-identical, so idle_nodes is about sodium's nodes and the collector's
//!      root list, not about allocator state
//! ```

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("adr0001_instruction_counts does nothing here: valgrind is Linux-only.");
}

#[cfg(target_os = "linux")]
mod linux {
    use std::hint::black_box;

    use iai_callgrind::{library_benchmark, library_benchmark_group, main};
    use research::{chain, Observer};
    use sodium_rust::{Listener, SodiumCtx, Stream, StreamSink};

    /// Everything one arm needs, kept alive together.
    ///
    /// The body must `forget` this rather than drop it: `Drop` runs inside the
    /// measured region and would charge graph teardown to the arm.
    // `ctx`, `listeners` and `junk` are never read. They are liveness anchors:
    // a dropped `Listener` takes its subgraph out of the measurement, and the
    // whole point of `junk` is that the blocks stay allocated.
    //
    // The boxing in `junk` is deliberate too, and is what `clippy::vec_box`
    // objects to. `Vec<[u8; 96]>` would be one contiguous allocation; we want
    // N separate live heap blocks, because the question is whether allocator
    // state moves the measurement.
    #[allow(dead_code)]
    #[allow(clippy::vec_box)]
    pub struct Rig {
        ctx: SodiumCtx,
        sink: StreamSink<u16>,
        observer: Observer,
        listeners: Vec<Listener>,
        junk: Vec<Box<[u8; 96]>>,
        // Intermediate `Stream` handles, kept alive only by the `held` arms of
        // `idle_nodes`. Whether these are alive decides whether the collector
        // has candidate roots to walk, which is the entire difference between
        // that benchmark's two arms.
        held: Vec<Stream<u16>>,
    }

    impl Rig {
        fn drive(self) -> u64 {
            for v in 0..64u16 {
                self.sink.send(black_box(v));
            }
            let total = black_box(self.observer.total());
            std::mem::forget(self);
            total
        }
    }

    /// A `depth`-deep chain from one sink to one listener.
    fn deep(depth: usize) -> Rig {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let listeners = vec![observer.listen(&chain(&sink, depth))];
        Rig {
            ctx,
            sink,
            observer,
            listeners,
            junk: Vec::new(),
            held: Vec::new(),
        }
    }

    /// A one-map chain, plus `idle` nodes on a second sink that never fires.
    ///
    /// `hold` decides whether the intermediate `Stream` handles survive the
    /// wiring. It is the only difference between the two arms: same nodes,
    /// same listeners, same sinks, same sends. Releasing a handle decrements a
    /// `GcNode` refcount, which files that node as a candidate cycle root for
    /// `collect_cycles` to walk at the end of every transaction, so the graph
    /// that let its handles go pays for its idle nodes on every event.
    fn with_idle(idle: usize, hold: bool) -> Rig {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let side: StreamSink<u16> = ctx.new_stream_sink();
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
        std::mem::forget(side);
        Rig {
            ctx,
            sink,
            observer,
            listeners,
            junk: Vec::new(),
            held,
        }
    }

    fn with_idle_dropped(idle: usize) -> Rig {
        with_idle(idle, false)
    }

    fn with_idle_held(idle: usize) -> Rig {
        with_idle(idle, true)
    }

    /// The control for `with_idle`: the same one-map chain, but the context is
    /// padded with live heap blocks instead of live nodes.
    fn with_heap_padding(blocks: usize) -> Rig {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let listeners = vec![observer.listen(&chain(&sink, 1))];
        let junk = (0..blocks * 25).map(|_| Box::new([0u8; 96])).collect();
        Rig {
            ctx,
            sink,
            observer,
            listeners,
            junk,
            held: Vec::new(),
        }
    }

    // Is the marginal cost of a node uniform along a chain? (It is not.)
    #[library_benchmark]
    #[bench::n0(args = (0), setup = deep)]
    #[bench::n1(args = (1), setup = deep)]
    #[bench::n2(args = (2), setup = deep)]
    #[bench::n3(args = (3), setup = deep)]
    #[bench::n4(args = (4), setup = deep)]
    #[bench::n8(args = (8), setup = deep)]
    fn chain_depth(rig: Rig) -> u64 {
        rig.drive()
    }

    // What is inside the measured region? Both arms have an empty body, and
    // their setups differ by 200 nodes.
    //
    // `empty_drop` lets the rig fall out of scope, so its `Drop` is measured.
    // `empty_forget` does not. Equal counts across tiny and big prove setup is
    // excluded; the gap between the two functions is what teardown costs.
    #[library_benchmark]
    #[bench::tiny(args = (0), setup = with_idle_dropped)]
    #[bench::big(args = (200), setup = with_idle_dropped)]
    fn empty_drop(rig: Rig) -> u64 {
        black_box(rig.observer.total())
    }

    #[library_benchmark]
    #[bench::tiny(args = (0), setup = with_idle_dropped)]
    #[bench::big(args = (200), setup = with_idle_dropped)]
    fn empty_forget(rig: Rig) -> u64 {
        let total = black_box(rig.observer.total());
        std::mem::forget(rig);
        total
    }

    // Do nodes that never fire cost anything? Only if their handles were
    // dropped: ~18 instructions each, against 0 for the identical held graph.
    #[library_benchmark]
    #[bench::dropped0(args = (0), setup = with_idle_dropped)]
    #[bench::dropped8(args = (8), setup = with_idle_dropped)]
    #[bench::dropped64(args = (64), setup = with_idle_dropped)]
    #[bench::dropped200(args = (200), setup = with_idle_dropped)]
    #[bench::held0(args = (0), setup = with_idle_held)]
    #[bench::held8(args = (8), setup = with_idle_held)]
    #[bench::held64(args = (64), setup = with_idle_held)]
    #[bench::held200(args = (200), setup = with_idle_held)]
    fn idle_nodes(rig: Rig) -> u64 {
        rig.drive()
    }

    // The control: live heap blocks instead of live nodes. (Flat.)
    #[library_benchmark]
    #[bench::heap0(args = (0), setup = with_heap_padding)]
    #[bench::heap64(args = (64), setup = with_heap_padding)]
    #[bench::heap200(args = (200), setup = with_heap_padding)]
    fn heap_padding(rig: Rig) -> u64 {
        rig.drive()
    }

    library_benchmark_group!(
        name = adr0001;
        benchmarks = chain_depth, empty_drop, empty_forget, idle_nodes, heap_padding
    );
    main!(library_benchmark_groups = adr0001);

    /// `main!` defines a private `main`; re-expose it for the real entry point.
    pub fn run() {
        main()
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux::run()
}
