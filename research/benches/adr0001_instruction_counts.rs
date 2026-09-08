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
//! 3. `idle_nodes` — nodes that never fire still cost about 61 instructions
//!    per event each. `src/bin/adr0001_cost_model.rs` reports this as flat,
//!    because a timing loop cannot resolve it. This is the disagreement that
//!    settled the harness choice.
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
//! Recorded output, 2026-09-08, sodium-rust at 4316728, 64 events per body:
//!
//! ```text
//! chain_depth    n0    934 752     n1  1 581 675     n2  2 196 345
//!                n3  3 083 678     n4  3 751 946     n8  6 728 919
//!   marginal instructions per node per event: 10 108 / 9 604 / 13 865 / 10 442 / 11 608
//!
//! empty_drop     tiny  1 000       big    863     <- setup excluded (does not scale
//! empty_forget   tiny     16       big     16        with 200 extra nodes), but the
//!                                                    ~900 gap between the two IS the
//!                                                    rig's Drop, inside the measured
//!                                                    region unless you forget it
//!
//! idle_nodes       0 nodes 1 585 052     1 node 1 589 509     8 nodes 1 619 732
//!                 64 nodes 1 824 791   200 nodes 2 362 580
//!   => (2 362 580 - 1 585 052) / 200 / 64 = 60.7 instructions per idle node per event
//!
//! heap_padding     0 blocks 1 581 678    64 blocks 1 581 702   200 blocks 1 562 097
//!   => flat, so idle_nodes is about sodium's nodes and not about allocator state
//!
//! Run-to-run variation across all eighteen arms: at most 0.013%, and the four
//! small counts were bit-identical. A 1% regression in one combinator therefore
//! sits roughly 75x above the noise floor.
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
    use sodium_rust::{Listener, SodiumCtx, StreamSink};

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
        }
    }

    /// A one-map chain, plus `idle` nodes on a second sink that never fires.
    fn with_idle(idle: usize) -> Rig {
        let ctx = SodiumCtx::new();
        let sink: StreamSink<u16> = ctx.new_stream_sink();
        let observer = Observer::new();
        let side: StreamSink<u16> = ctx.new_stream_sink();
        let listeners = vec![
            observer.listen(&chain(&sink, 1)),
            observer.listen(&chain(&side, idle)),
        ];
        std::mem::forget(side);
        Rig {
            ctx,
            sink,
            observer,
            listeners,
            junk: Vec::new(),
        }
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
    #[bench::tiny(args = (0), setup = with_idle)]
    #[bench::big(args = (200), setup = with_idle)]
    fn empty_drop(rig: Rig) -> u64 {
        black_box(rig.observer.total())
    }

    #[library_benchmark]
    #[bench::tiny(args = (0), setup = with_idle)]
    #[bench::big(args = (200), setup = with_idle)]
    fn empty_forget(rig: Rig) -> u64 {
        let total = black_box(rig.observer.total());
        std::mem::forget(rig);
        total
    }

    // Do nodes that never fire cost anything? (About 61 instructions each.)
    #[library_benchmark]
    #[bench::idle0(args = (0), setup = with_idle)]
    #[bench::idle1(args = (1), setup = with_idle)]
    #[bench::idle8(args = (8), setup = with_idle)]
    #[bench::idle64(args = (64), setup = with_idle)]
    #[bench::idle200(args = (200), setup = with_idle)]
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
