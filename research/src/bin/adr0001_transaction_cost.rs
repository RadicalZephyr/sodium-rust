//! ADR 1: what does a transaction cost, separately from what it propagates?
//!
//! `SodiumCtx::transaction` is only a depth counter. `StreamSink::send` sets
//! the stream's `firing_op` and queues the node; nothing propagates until
//! `end_of_transaction`, which closes the transaction *and* runs the
//! `changed_nodes` loop *and* drains three callback queues *and* calls
//! `collect_cycles`. So "what does closing a transaction cost" cannot be
//! answered by comparing one transaction against many: those arms differ in
//! how much they propagate as well as in how many transactions they open.
//!
//! Two measurements that do separate the two:
//!
//! 1. An empty `ctx.transaction(|| {})` runs all of `end_of_transaction` with
//!    nothing to propagate. That is the per-transaction cost, measured
//!    directly rather than by subtraction.
//! 2. Sweeping how many *distinct* sinks fire inside one transaction gives the
//!    marginal cost of propagation, with the per-transaction cost paid once.
//!
//! **Distinct sinks are the whole point of section 2's structure.**
//! `Stream::_send` with no coalescer sets `data.firing_op = Some(a)`, so a
//! second `send` to the *same* sink inside one transaction overwrites the
//! first and it never propagates at all. Sweeping "sends per transaction" on
//! one sink therefore measures coalescing, not transaction overhead, and would
//! report the transaction as almost free for the wrong reason. Section 3
//! demonstrates the overwrite so the structure above is self-justifying.
//!
//! Run with `cargo run --release -p research --bin adr0001_transaction_cost`.
//!
//! Recorded output, 2026-09-10, 4-core container, sodium-rust at 9c7993d:
//!
//! ```text
//! 1. What does a transaction cost with nothing to propagate?
//! context                                     allocs/txn          ns/txn
//! bare context                                       0.0           272.1
//! 8 live subgraphs on the context                    0.0           277.1
//!
//! 2. Distinct sinks firing inside ONE transaction (sink -> map -> listen)
//! sinks fired                                 allocs/txn          ns/txn marginal allocs     marginal ns
//! 1                                                 36.0          7039.1            36.0          7039.1
//! 2                                                 69.0         13596.2            33.0          6557.1
//! 3                                                101.0         19873.9            32.0          6277.7
//! 4                                                133.0         27303.9            32.0          7430.0
//! 8                                                260.0         53538.3            31.8          6558.6
//!
//! 3. Sending 1, 2, 3 into ONE sink inside ONE transaction
//! sink                                           panics?    listener saw
//! StreamSink::new                                     no             [3]
//! StreamSink::new_with_coalescer(+)                   no             [6]
//! CellSink::new                                       no          [0, 3]
//! ```
//!
//! Conclusions that reached the ADR: a transaction with nothing to propagate
//! costs zero allocations and ~270 ns, flat in the size of the graph, against
//! ~32 allocations and ~6 µs for each firing sink. Essentially all of a send is
//! propagation, so there is no per-transaction overhead worth amortising — and
//! "batch your sends to amortise it" is advice that silently discards events.
//! Section 3 is tracked as issue #42.

use std::hint::black_box;
use std::sync::{Arc, Mutex};

use research::{heading, measure, row, CountingAlloc};
use sodium_rust::{Listener, SodiumCtx, StreamSink};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const N: usize = 20_000;

/// `count` sinks, each behind its own `map` and `listener`, on one context.
fn sinks_on(ctx: &SodiumCtx, count: usize) -> (Vec<StreamSink<u16>>, Vec<Listener>) {
    let mut sinks = Vec::new();
    let mut listeners = Vec::new();
    for _ in 0..count {
        let s: StreamSink<u16> = ctx.new_stream_sink();
        let mapped = s.stream().map(|v: &u16| v.wrapping_add(1));
        listeners.push(mapped.listen(move |v: &u16| {
            black_box(*v);
        }));
        sinks.push(s);
    }
    (sinks, listeners)
}

fn empty_transaction() {
    heading(
        "1. What does a transaction cost with nothing to propagate?",
        &["context", "allocs/txn", "ns/txn"],
    );

    let ctx = SodiumCtx::new();
    for _ in 0..1000 {
        ctx.transaction(|| {});
    }
    let bare = measure(N, || ctx.transaction(|| {}));
    row(&[
        "bare context".to_string(),
        format!("{:.1}", bare.allocs),
        format!("{:.1}", bare.nanos),
    ]);

    let (sinks, listeners) = sinks_on(&ctx, 8);
    for _ in 0..1000 {
        ctx.transaction(|| {});
    }
    let live = measure(N, || ctx.transaction(|| {}));
    row(&[
        "8 live subgraphs on the context".to_string(),
        format!("{:.1}", live.allocs),
        format!("{:.1}", live.nanos),
    ]);
    drop(listeners);
    drop(sinks);
}

fn distinct_sinks() {
    heading(
        "2. Distinct sinks firing inside ONE transaction (sink -> map -> listen)",
        &[
            "sinks fired",
            "allocs/txn",
            "ns/txn",
            "marginal allocs",
            "marginal ns",
        ],
    );

    let ctx = SodiumCtx::new();
    let (sinks, listeners) = sinks_on(&ctx, 8);
    let mut prev: Option<(usize, f64, f64)> = None;
    for n in [1usize, 2, 3, 4, 8] {
        // Warm: the first event through a graph pays one-off costs.
        for s in &sinks[..n] {
            s.send(0);
        }
        let cost = measure(N, || {
            ctx.transaction(|| {
                for s in &sinks[..n] {
                    s.send(black_box(7u16));
                }
            })
        });
        let (ma, mn) = match prev {
            None => (cost.allocs, cost.nanos),
            Some((pn, pa, pns)) => {
                let d = (n - pn) as f64;
                ((cost.allocs - pa) / d, (cost.nanos - pns) / d)
            }
        };
        row(&[
            n.to_string(),
            format!("{:.1}", cost.allocs),
            format!("{:.1}", cost.nanos),
            format!("{ma:.1}"),
            format!("{mn:.1}"),
        ]);
        prev = Some((n, cost.allocs, cost.nanos));
    }
    drop(listeners);
    drop(sinks);
}

fn repeated_sends_to_one_sink() {
    heading(
        "3. Sending 1, 2, 3 into ONE sink inside ONE transaction",
        &["sink", "panics?", "listener saw"],
    );
    let ctx = SodiumCtx::new();

    let plain = ctx.new_stream_sink::<u32>();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink_seen = seen.clone();
    let l = plain
        .stream()
        .listen(move |v: &u32| sink_seen.lock().unwrap().push(*v));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction(|| {
            plain.send(1);
            plain.send(2);
            plain.send(3);
        })
    }))
    .is_err();
    row(&[
        "StreamSink::new".to_string(),
        if panicked { "yes" } else { "no" }.to_string(),
        format!("{:?}", seen.lock().unwrap()),
    ]);
    drop(l);

    let coalescing = ctx.new_stream_sink_with_coalescer::<u32, _>(|a: &u32, b: &u32| a + b);
    let seen2 = Arc::new(Mutex::new(Vec::new()));
    let sink_seen2 = seen2.clone();
    let l2 = coalescing
        .stream()
        .listen(move |v: &u32| sink_seen2.lock().unwrap().push(*v));
    let panicked2 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction(|| {
            coalescing.send(1);
            coalescing.send(2);
            coalescing.send(3);
        })
    }))
    .is_err();
    row(&[
        "StreamSink::new_with_coalescer(+)".to_string(),
        if panicked2 { "yes" } else { "no" }.to_string(),
        format!("{:?}", seen2.lock().unwrap()),
    ]);
    drop(l2);

    let cell = ctx.new_cell_sink::<u32>(0);
    let seen3 = Arc::new(Mutex::new(Vec::new()));
    let sink_seen3 = seen3.clone();
    let l3 = cell
        .cell()
        .listen(move |v: &u32| sink_seen3.lock().unwrap().push(*v));
    let panicked3 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction(|| {
            cell.send(1);
            cell.send(2);
            cell.send(3);
        })
    }))
    .is_err();
    row(&[
        "CellSink::new".to_string(),
        if panicked3 { "yes" } else { "no" }.to_string(),
        format!("{:?}", seen3.lock().unwrap()),
    ]);
    drop(l3);
}

fn main() {
    empty_transaction();
    distinct_sinks();
    repeated_sends_to_one_sink();
}
