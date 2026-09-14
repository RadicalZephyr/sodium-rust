# Patches

Variants of the library that [ADR-0002](../../0002-permission-tokens-in-the-node-graph.md)
measured, kept as patches because they cannot be kept as anything else.

A binary in this crate depends on `sodium-rust` the way any consumer does, so
it can measure the library but cannot *modify* it. Every delta ADR-0002 quotes
is a comparison of two builds -- locks against no locks, one module's locks
against another's -- and no binary can express that. The binaries supply the
workload and the denominators; these patches supply the variants.

## Running one

Apply, measure, reverse:

```shell
git apply docs/decisions/research/patches/no-locks.patch
cargo run --release -p adr-research --bin 0002-variant-ab
git apply -R docs/decisions/research/patches/no-locks.patch
```

**Alternate between the two builds and take medians.** This is not fussiness.
The first attempt at these numbers compared a Criterion baseline against a run
taken twenty minutes later, and the drift on a shared machine swamped a 25%
effect -- it reported "no change" for a patch that interleaved runs put at
-25.8%. Six alternating repetitions is enough:

```shell
for rep in 1 2 3 4 5 6; do
    cargo run --release -p adr-research --bin 0002-variant-ab
    git apply docs/decisions/research/patches/no-locks.patch
    cargo run --release -p adr-research --bin 0002-variant-ab
    git apply -R docs/decisions/research/patches/no-locks.patch
done
```

## What each one is

| Patch | What it changes | What it was for |
| --- | --- | --- |
| `display-graph-gate.patch` | Returns early from `GcCtx::display_graph` unless trace logging is on | Isolates the cost of building log output nobody reads |
| `update-node-single-clone.patch` | `update_node` clones its dependency vector once instead of twice, returning `any_changed` through the joiner | Isolates the cost of the redundant clone and the spawn scaffolding |
| `cheap-wins.patch` | Both of the above | The API-neutral candidate: passes `fmt`, `clippy -D warnings` and the suite |
| `no-locks.patch` | Every `parking_lot::Mutex`/`RwLock` in `src/impl_/` replaced by an unsynchronised `UnsafeCell` shim | The ceiling: no lock-elimination scheme can beat removing the locks |
| `no-locks-gc-node-only.patch` | The same shim, in `gc_node.rs` alone | Decomposes the ceiling by module |
| `no-locks-node-only.patch` | The same shim, in `node.rs` alone | Decomposes the ceiling by module |
| `cheap-wins-and-no-locks.patch` | `cheap-wins` and `no-locks` together | Shows the two are additive, and what a token would still be worth afterwards |
| `lock-count.patch` | Counting wrappers around every lock, split by propagation versus collector, plus the binary that reads them | Counts acquisitions per node update |

`lock-count.patch` carries its own binary, `0002-lock-count.rs`, because that
binary reads a counter the patch introduces and so cannot compile against the
unpatched library. A binary in this crate that does not compile breaks
`cargo test --workspace`, which is why it travels with the patch rather than
sitting in `src/bin/`.

## The `no-locks` patches are unsound

They are measurement devices, not proposals. `SodiumCtx` and `Stream<A>` are
`Send + Sync`, so two threads may legitimately send into one context; the shim
removes the synchronisation that makes that safe and would race if they did.
It survives the test suite only because the suite is single-threaded.

That is the whole point of the comparison. The ceiling they measure is what a
scheme *could* win if the locks were free, and the question ADR-0002 asks is
whether a permission token can collect it while keeping the safety the shim
throws away.

## Retirement

These leave when ADR-0002 does, on the same terms as any experiment here --
see [`../README.md`](../README.md). If the record logs `Implemented`, the
variants that became the implementation are redundant and the rest are
measuring internals that no longer exist; if it logs `Deprecated`, all of them
are. Either way they are deleted rather than repaired, and the record cites
the commit that produced its numbers.
