//! An A/B harness for the patches in `patches/`.
//!
//! ADR-0002's deltas all come from comparing two builds of the library, which
//! a research binary cannot express on its own -- it depends on `sodium-rust`
//! as an ordinary dependency and so measures whatever the checkout currently
//! is. This binary supplies the *workload*; `patches/` supplies the variants.
//!
//! It replicates the shapes in `benches/sodium.rs` closely enough to be
//! comparable, but runs them in one short process so that alternating between
//! two builds costs seconds rather than minutes. That matters more than it
//! sounds: on a shared machine the noise between two Criterion runs taken
//! twenty minutes apart swamped a 25% effect, and only interleaving recovered
//! it. Always alternate, and always take medians:
//!
//! ```shell
//! for rep in 1 2 3 4 5 6; do
//!     cargo run --release -p adr-research --bin 0002-variant-ab
//!     git apply docs/decisions/research/patches/no-locks.patch
//!     cargo run --release -p adr-research --bin 0002-variant-ab
//!     git apply -R docs/decisions/research/patches/no-locks.patch
//! done
//! ```
//!
//! See `patches/README.md` for what each patch changes and which numbers in
//! the record it produced.

use std::hint::black_box;
use std::time::Instant;

use sodium_rust::{SodiumCtx, Stream};

/// Milliseconds per repetition, where one repetition builds a fresh graph and
/// sends a thousand values through it -- the same unit `benches/sodium.rs`
/// reports, so the two are directly comparable.
fn time<F: FnMut()>(reps: u32, mut f: F) -> f64 {
    let start = Instant::now();
    for _ in 0..reps {
        f();
    }
    start.elapsed().as_secs_f64() * 1e3 / f64::from(reps)
}

fn main() {
    let reps = 40;

    let simple = time(reps, || {
        let ctx = SodiumCtx::new();
        let sink = ctx.new_stream_sink();
        let mut values: Vec<u16> = Vec::new();
        let _listener = sink
            .stream()
            .listen(move |v: &u16| values.push(black_box(*v)));
        for v in 0_u16..1000 {
            sink.send(black_box(v));
        }
    });

    let mapx2 = time(reps, || {
        let ctx = SodiumCtx::new();
        let sink = ctx.new_stream_sink();
        let mut values: Vec<u16> = Vec::new();
        let stream: Stream<u16> = sink.stream().map(|a: &u16| a + 1).map(|a: &u16| a + 1);
        let _listener = stream.listen(move |v: &u16| values.push(black_box(*v)));
        for v in 0_u16..1000 {
            sink.send(black_box(v));
        }
    });

    let merge2 = time(reps, || {
        let ctx = SodiumCtx::new();
        let sa = ctx.new_stream_sink();
        let sb = ctx.new_stream_sink();
        let mut values: Vec<u16> = Vec::new();
        let _listener = sa
            .stream()
            .merge(&sb.stream(), |a: &u16, b: &u16| *a + *b)
            .listen(move |v: &u16| values.push(black_box(*v)));
        for v in 0_u16..1000 {
            if v % 2 == 0 {
                sa.send(black_box(v));
            } else {
                sb.send(black_box(v));
            }
        }
    });

    let cell_map = time(reps, || {
        let ctx = SodiumCtx::new();
        let sink = ctx.new_cell_sink(0_u16);
        let mut values: Vec<u16> = Vec::new();
        let _listener = sink
            .cell()
            .map(|a: &u16| a + 1)
            .listen(move |v: &u16| values.push(black_box(*v)));
        for v in 0_u16..1000 {
            sink.send(black_box(v));
        }
    });

    println!("simple {simple:.4} mapx2 {mapx2:.4} merge2 {merge2:.4} cellmap {cell_map:.4}");
}
