//! ADR 1: what one event costs, per combinator, in allocations.
//!
//! Each row is a graph that differs from its baseline by exactly one node. The
//! delta is that combinator's contribution to a single event.
//!
//! The numbers are exact integers and reproduce byte-for-byte across runs on
//! one machine, which is what makes them worth asserting rather than
//! benchmarking. `docs/benchmark-plan.md` phase 1 turns this table into
//! `tests/graph_cost.rs`.
//!
//! Run with `cargo run --release -p research --bin adr0001_alloc_ledger`.
//! Timings vary with machine load; the allocation counts do not.
//!
//! One row needs reading carefully. `+ once` comes out at 15 allocations,
//! *below* the 27 of the bare baseline, which is not a discount: these are
//! steady-state figures, and `once` has already fired long before the
//! measurement starts, so every event counted here dies at that node. Any
//! combinator whose behaviour changes after the first event has to be measured
//! against a stated event index rather than an average, and the same caveat
//! applies to `hold`, whose +1 is the cost of updating a cell nobody samples.

//!
//! Recorded output, 2026-09-08, 4-core container, sodium-rust at 4316728:
//!
//! ```text
//! Stream combinators, one node over the baseline
//! shape                                     allocs/event           delta        ns/event
//! sink -> listen                                    22.0        baseline            3930
//! + map                                             36.0             +14            6631
//! + map_to                                          36.0             +14            6516
//! + filter (passes)                                 36.0             +14            6520
//! + filter (drops)                                  22.0              +0            3882
//! + filter_map (Some)                               36.0             +14            6571
//! + once                                            11.0             -11            2100
//! + merge (one side fires)                          42.0             +20            7923
//! + or_else (one side fires)                        42.0             +20            8073
//! + snapshot                                        36.0             +14            7867
//! + gate (open)                                     36.0             +14            7947
//! + Operational::defer                              50.0             +28            8734
//! + accum -> updates                                63.0             +41           11755
//! + hold -> updates                                 23.0              +1            4006
//! + collect                                         90.0             +68           16303
//!
//! Cell combinators, driven from a CellSink
//! shape                                     allocs/event           delta        ns/event
//! cell_sink -> updates                              35.0        baseline            6243
//! + Cell::map                                       50.0             +15            9243
//! + lift2                                           93.0             +58           17678
//! + lift3                                          150.0            +115           28845
//! + Cell::value                                     70.0             +35           12523
//!
//! Dynamic graph combinators
//! shape                                     allocs/event           delta        ns/event
//! switch_s, fire selected stream                    59.0        baseline           11260
//! switch_c, fire selected cell                      74.0        baseline           13510
//! ```

use std::hint::black_box;

use research::{heading, measure, row, CountingAlloc, Observer};
use sodium_rust::{Cell, Operational, SodiumCtx, Stream, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SENDS: usize = 2000;

/// One arm's graph, built from a sink and the context that owns it.
type BuildStream = Box<dyn Fn(&StreamSink<u16>, &SodiumCtx) -> Stream<u16>>;

/// Measure one arm and print it against a baseline.
fn arm(label: &str, baseline: Option<f64>, allocs: f64, nanos: f64) {
    let delta = match baseline {
        None => "baseline".to_string(),
        Some(b) => format!("{:+.0}", allocs - b),
    };
    row(&[
        label.to_string(),
        format!("{allocs:.1}"),
        delta,
        format!("{nanos:.0}"),
    ]);
}

/// Build a stream graph from a sink, attach the observer, and price one event.
fn stream_arm(build: impl FnOnce(&StreamSink<u16>, &SodiumCtx) -> Stream<u16>) -> (f64, f64) {
    let ctx = SodiumCtx::new();
    let sink: StreamSink<u16> = ctx.new_stream_sink();
    let observer = Observer::new();
    let out = build(&sink, &ctx);
    let listener = observer.listen(&out);
    sink.send(0);

    let cost = measure(SENDS, || sink.send(black_box(1)));
    black_box(observer.total());
    drop(listener);
    (cost.allocs, cost.nanos)
}

fn stream_combinators() {
    heading(
        "Stream combinators, one node over the baseline",
        &["shape", "allocs/event", "delta", "ns/event"],
    );

    let (base, base_ns) = stream_arm(|sink, _| sink.stream());
    arm("sink -> listen", None, base, base_ns);

    let cases: Vec<(&str, BuildStream)> = vec![
        (
            "+ map",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| {
                s.stream().map(|v: &u16| v.wrapping_add(1))
            }),
        ),
        (
            "+ map_to",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| s.stream().map_to(7u16)),
        ),
        (
            "+ filter (passes)",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| s.stream().filter(|_| true)),
        ),
        (
            "+ filter (drops)",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| s.stream().filter(|_| false)),
        ),
        (
            "+ filter_map (Some)",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| {
                s.stream().filter_map(|v: &u16| Some(v.wrapping_add(1)))
            }),
        ),
        (
            "+ once",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| s.stream().once()),
        ),
        (
            "+ merge (one side fires)",
            Box::new(|s: &StreamSink<u16>, c: &SodiumCtx| {
                let other: StreamSink<u16> = c.new_stream_sink();
                let merged = s
                    .stream()
                    .merge(&other.stream(), |a: &u16, b: &u16| a.wrapping_add(*b));
                std::mem::forget(other);
                merged
            }),
        ),
        (
            "+ or_else (one side fires)",
            Box::new(|s: &StreamSink<u16>, c: &SodiumCtx| {
                let other: StreamSink<u16> = c.new_stream_sink();
                let merged = s.stream().or_else(&other.stream());
                std::mem::forget(other);
                merged
            }),
        ),
        (
            "+ snapshot",
            Box::new(|s: &StreamSink<u16>, c: &SodiumCtx| {
                let cell = c.new_cell_sink(1u16);
                let out = s
                    .stream()
                    .snapshot(&cell.cell(), |v: &u16, k: &u16| v.wrapping_add(*k));
                std::mem::forget(cell);
                out
            }),
        ),
        (
            "+ gate (open)",
            Box::new(|s: &StreamSink<u16>, c: &SodiumCtx| {
                let pred = c.new_cell_sink(true);
                let out = s.stream().gate(&pred.cell());
                std::mem::forget(pred);
                out
            }),
        ),
        (
            "+ Operational::defer",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| Operational::defer(&s.stream())),
        ),
        (
            "+ accum -> updates",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| {
                s.stream()
                    .accum(0u16, |v: &u16, acc: &u16| v.wrapping_add(*acc))
                    .updates()
            }),
        ),
        (
            "+ hold -> updates",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| s.stream().hold(0).updates()),
        ),
        (
            "+ collect",
            Box::new(|s: &StreamSink<u16>, _: &SodiumCtx| {
                s.stream().collect(0u16, |v: &u16, st: &u16| {
                    (v.wrapping_add(*st), v.wrapping_add(*st))
                })
            }),
        ),
    ];

    for (label, build) in cases {
        let (allocs, nanos) = stream_arm(|s, c| build(s, c));
        arm(label, Some(base), allocs, nanos);
    }
}

/// Price one event on a cell graph driven from a `CellSink`.
fn cell_arm(
    build: impl FnOnce(&sodium_rust::CellSink<u16>, &SodiumCtx) -> Stream<u16>,
) -> (f64, f64) {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_cell_sink(0u16);
    let observer = Observer::new();
    let listener = observer.listen(&build(&sink, &ctx));
    sink.send(1);

    let mut v = 1u16;
    let cost = measure(SENDS, || {
        v = v.wrapping_add(1);
        sink.send(black_box(v));
    });
    black_box(observer.total());
    drop(listener);
    (cost.allocs, cost.nanos)
}

fn cell_combinators() {
    heading(
        "Cell combinators, driven from a CellSink",
        &["shape", "allocs/event", "delta", "ns/event"],
    );

    let (base, base_ns) = cell_arm(|sink, _| sink.cell().updates());
    arm("cell_sink -> updates", None, base, base_ns);

    let (a, n) = cell_arm(|sink, _| sink.cell().map(|v: &u16| v.wrapping_add(1)).updates());
    arm("+ Cell::map", Some(base), a, n);

    let (a, n) = cell_arm(|sink, ctx| {
        let other = ctx.new_cell_sink(0u16);
        let out = sink
            .cell()
            .lift2(&other.cell(), |x: &u16, y: &u16| x.wrapping_add(*y))
            .updates();
        std::mem::forget(other);
        out
    });
    arm("+ lift2", Some(base), a, n);

    let (a, n) = cell_arm(|sink, ctx| {
        let b = ctx.new_cell_sink(0u16);
        let c = ctx.new_cell_sink(0u16);
        let out = sink
            .cell()
            .lift3(&b.cell(), &c.cell(), |x: &u16, y: &u16, z: &u16| {
                x.wrapping_add(*y).wrapping_add(*z)
            })
            .updates();
        std::mem::forget(b);
        std::mem::forget(c);
        out
    });
    arm("+ lift3", Some(base), a, n);

    let (a, n) = cell_arm(|sink, _| sink.cell().value());
    arm("+ Cell::value", Some(base), a, n);
}

fn dynamic_combinators() {
    heading(
        "Dynamic graph combinators",
        &["shape", "allocs/event", "delta", "ns/event"],
    );

    // switch_s, firing the selected stream.
    let ctx = SodiumCtx::new();
    let observer = Observer::new();
    let selected: StreamSink<u16> = ctx.new_stream_sink();
    let which = ctx.new_cell_sink(selected.stream());
    let listener = observer.listen(&Cell::switch_s(&which.cell()));
    selected.send(0);
    let cost = measure(SENDS, || selected.send(black_box(1)));
    black_box(observer.total());
    drop(listener);
    arm(
        "switch_s, fire selected stream",
        None,
        cost.allocs,
        cost.nanos,
    );

    // switch_c, firing the selected cell.
    let ctx = SodiumCtx::new();
    let observer = Observer::new();
    let selected = ctx.new_cell_sink(1u16);
    let which = ctx.new_cell_sink(selected.cell());
    let listener = observer.listen(&Cell::switch_c(&which.cell()).updates());
    selected.send(2);
    let mut v = 2u16;
    let cost = measure(SENDS, || {
        v = v.wrapping_add(1);
        selected.send(black_box(v));
    });
    black_box(observer.total());
    drop(listener);
    arm(
        "switch_c, fire selected cell",
        None,
        cost.allocs,
        cost.nanos,
    );
}

fn main() {
    stream_combinators();
    cell_combinators();
    dynamic_combinators();
}
