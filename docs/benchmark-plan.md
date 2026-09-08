# Benchmark suite: implementation plan

Companion to [ADR 1](adr/0001-benchmark-suite-structure.md). That document
argues for the structure; this one says how to build it and in what order. The
measurements both rely on come from [the `research` crate](../research), which
is where to go to check a number or add one.

Every phase is meant to land on its own and leave the suite more useful than
it was. Nothing here depends on a later phase to be worth having.

## Phase 0 — the scenario crate

Create `bench-support/` as a workspace member: a library of graph builders that
every tier shares, so a scenario is described once.

```
bench-support/
  Cargo.toml            # depends on sodium-rust by path
  src/
    lib.rs
    rig.rs              # Rig: a context, its sinks, its listeners, a sink for observed output
    chain.rs            # linear chains, binary trees, fan-in trees, parameterised
    ledger.rs           # the baseline arm plus one-node-difference arms
    graphs/             # the book-derived skeletons (phase 4)
```

The one non-obvious thing: `sodium-rust` dev-depends on `bench-support`, which
depends on `sodium-rust`. Cargo allows this cycle because it runs through a
dev-dependency. `cargo check -p sodium-rust`, which is what the MSRV job runs,
does not build it, so the MSRV stays where it is.

`Rig` exists to solve one problem. Every benchmark needs somewhere for values
to land that the optimiser cannot delete and that does not itself allocate. The
current benches push into a `Vec` that grows to a thousand elements; an
`Arc<AtomicU64>` accumulator measures the same thing without the reallocations.
Verified: swapping one for the other moves a send from 5134 ns to 5157 ns, so
the `Vec` is not currently distorting anything, but the atomic keeps it that
way as scenarios get larger.

**Exit criteria:** `cargo test --workspace` passes, `cargo clippy --workspace
--all-targets -- -D warnings` is clean, and `cargo check -p sodium-rust` still
passes on the MSRV.

## Phase 1 — cost invariants (tier 4)

The cheapest tier, the most sensitive instrument, and the only one that works on
every platform. Do it first.

`tests/graph_cost.rs`, an integration test with its own counting
`#[global_allocator]`. Two families of assertion:

1. **Per-event allocation counts.** For each shape, assert the exact number of
   allocations one event costs. Starting values, measured at 4316728:

   | shape | allocs/event | over baseline |
   |---|---|---|
   | `sink -> listen` | 27 | baseline |
   | `+ map` / `map_to` / `filter` (passing) / `filter_map` | 43 | +16 |
   | `+ filter` (dropping) | 27 | +0 |
   | `+ hold -> updates` | 28 | +1 |
   | `+ snapshot` / `gate` | 47 | +20 |
   | `+ merge` / `or_else` | 51 | +24 |
   | `+ Operational::defer` | 62 | +35 |
   | `+ accum -> updates` | 75 | +48 |
   | `+ collect` | 104 | +77 |
   | `cell_sink -> updates` | 41 | baseline |
   | `+ Cell::map` | 59 | +18 |
   | `+ Cell::value` | 83 | +42 |
   | `+ lift2` | 109 | +68 |
   | `+ lift3` | 173 | +132 |
   | `switch_s`, firing the selected stream | 72 | — |
   | `switch_c`, firing the selected cell | 88 | — |

   Produced by `research/src/bin/adr0001_alloc_ledger.rs`, which is where to go
   to add a row or check one. They reproduce byte-identically across runs.
   Write them as a table the test walks, so adding a combinator is one row.

   One trap, and it is the reason to read the experiment rather than copy the
   numbers: `+ once` measures **15**, which is *below* the 27-allocation
   baseline. Nothing is cheaper than nothing — `once` has already fired by the
   time the steady-state measurement starts, so every later event dies at that
   node. Any combinator whose behaviour changes after the first event needs its
   assertion written against a stated event index, not against a steady-state
   average.

2. **Lifecycle invariants.** After building and releasing a subgraph on a
   long-lived context, `node_count()` must return to its starting value, and the
   per-send allocation count must return to its starting value. Both currently
   fail, and both fail *without bound* —
   `research/src/bin/adr0001_dynamic_graphs.rs` shows a send going from 27
   allocations to 827 over 800 release cycles even on the well-behaved
   `unlisten()` path, where `node_count` stays correct at 2 throughout. Land
   them as `#[ignore]`d tests with the measured numbers recorded, and open an
   issue for each; they become the acceptance criteria for whoever fixes the
   underlying behaviour.

   Write these as a *slope*, not a threshold: assert that the cost after 800
   cycles equals the cost after 100. A threshold would pass again the moment
   someone halves the per-cycle leak without removing it.

Assert on a *range* rather than an exact integer only where a number turns out
not to be stable. So far none of them are unstable.

**Exit criteria:** the passing assertions pass on Linux, macOS and Windows; the
two lifecycle tests are `#[ignore]`d with issue links.

## Phase 2 — the combinator ledger (tier 1)

`benches/combinators.rs` under iai-callgrind, plus a CI job.

- Add `iai-callgrind = "0.16"` as a dev-dependency and a `[[bench]]` entry with
  `harness = false`.
- The runner binary must match the crate version exactly:
  `cargo install --version 0.16.1 iai-callgrind-runner`. Pin both, and pin them
  together — a version skew between them fails at run time with a message about
  `$PATH`, which sends you looking in the wrong place.
- One `#[library_benchmark]` per combinator family, with
  `#[bench::name(args = (..), setup = rig)]` arms. Setup is excluded from the
  measured region — verified: two arms whose setups differ by 200 nodes both
  measure 308 instructions when the body is empty.
- **The body must not drop the rig.** The argument is owned by the benchmark
  function, so its `Drop` runs inside the measured region and charges teardown
  of the whole graph to the combinator. End every body with
  `std::mem::forget(r)`. This is the single easiest way to get this tier wrong.
- Every arm sends a fixed 64 events. The absolute count per arm is the tracked
  number.
- Arms must be identical apart from the node under test. Heap state is not a
  confound (5000 unrelated allocations move a body by <0.2%), but a stray extra
  node is: every node in the context costs ~61 instructions per event even when
  it never fires.

Cover, in this order: the twelve stream combinators and five cell combinators
already measured; then `hold`, `once`, `collect`, `filter_option`, `split_opt`,
`split_res`, `split_enum2`, `split_enum3`, `snapshot1`, `snapshot3` through
`snapshot6`, `lift4` through `lift6`, `Cell::value`, `Operational::updates`,
`Operational::value`, `Router`, `StreamLoop` and `CellLoop` feedback, and
`new_stream_sink_with_coalescer`; then one representative `*_with_deps` pair to
settle whether dependency threading costs anything at fire time.

CI: a Linux job that installs valgrind, installs the pinned runner, and runs
`cargo bench --bench combinators`. Report only to begin with — see the ADR's
open question about gating.

**Exit criteria:** every public combinator either has a ledger entry or a
one-line comment in the bench file saying why it cannot have one; the CI job is
green and its output is legible in the log.

## Phase 3 — structural scaling (tier 2)

`benches/structure.rs` under criterion, parameterised with `BenchmarkId`.

Start by rehabilitating `node-count-benches`. That branch already has linear
chains against binary trees with logarithmic axis scaling; the work is to fix
the confound its own TODO names — the linear arm and the tree arm did not have
equal listener counts, so the comparison was between two things that differed
in two ways. Hold listener count equal, sweep node count, and the curve means
something.

Then add the sweeps the cost model asks for:

| group | swept over | why |
|---|---|---|
| `depth` | 1, 2, 4, ... 256 nodes in a chain | per-node propagation |
| `width` | same node counts, fanned out | whether shape matters beyond count |
| `fan_in` | `merge` trees of the same sizes | join cost |
| `listeners` | 1, 2, 8, 32, 128 on one stream | listener versus node dispatch |
| `batch` | 1, 8, 64, 512 sends per transaction | transaction amortisation |
| `switch_upstream` | 0, 1, 4, 16, 64 nodes in the branch switched in | reconfiguration |
| `switch_downstream` | same range downstream | confirms flat, guards against becoming non-flat |
| `live_subgraphs` | 0, 16, 64, 256 live subgraphs on one context | steady-state accumulation |
| `churn` | 0, 100, 300, 800 release cycles, then measure a send | degradation over a context's lifetime |

The last two are the ones that catch what the current suite cannot see. Note
that `switch_downstream` is expected to be flat; it is there so that if it ever
stops being flat we find out.

Construction hoisted out of `b.iter()` throughout.

**Exit criteria:** every group produces a readable curve; the `churn` group
reproduces the degradation recorded in the ADR.

## Phase 4 — book graph shapes (tier 3)

`benches/graphs.rs` under criterion, with the skeletons in
`bench-support/src/graphs/`.

Each is a distilled shape, not a port: the same combinator inventory, depth,
fan-in, feedback loops and switch sites as the book's chapter, driven by a
synthetic event sequence, with the GUI stripped and the application arithmetic
replaced by something trivial. Every one runs on a long-lived context with
construction outside the timed region.

A survey of the book's twelve chapters turned up forty-seven distinct
benchmarkable shapes. Adopting all of them would be scope bloat, and most are
variations on a smaller set of structures. What follows is the set that covers
the structural ground with the least code, in two waves. The rest are listed as
a backlog at the end.

Node counts below are sodium-rust primitives after desugaring, using the
survey's accounting: `Cell::map` is 2 nodes (map + hold), `lift2` is 5,
`lift3` is 10, `lift4` is 15.

### Wave 1 — structural idioms

Each is small, parameterised, and shared by several chapters. That last part is
the selection criterion: a shape four chapters independently arrive at is an
idiom, not an accident.

| scenario | shape | parameter | drawn from |
|---|---|---|---|
| `fold_lift2` | left-leaning `lift2` accumulator, depth 5N | N = 1..128 | FrFlow, formvalidation, zombicus `sequence`, `CellJunction` |
| `broadcast_demux` | one sink → N `snapshot`+`filter_option` branches → `or_else` fan-in; N−1 die every event | N = 1..32 | petrol pump `capturePrice`, zombicus `sBite` |
| `state_machine_loop` | `CellLoop` read by four `snapshot`s, written from two merged paths | N nozzles = 1..8 | petrol pump `LifeCycle`, `NotifyPointOfSale` |
| `self_reading_accum` | `CellLoop` that snapshots itself, resettable in the same transaction | none (event rate) | petrol pump `accumulate` |
| `wide_feedback_ring` | one global cell, N-wide fan-out, N-wide fan-in, closed through a loop | N = 1..128 | zombicus world cell, fridgets focus ring |

Three of these want a **contrast arm** built from a sodium-rust primitive the
book's language does not have. Those arms are the most directly actionable
output of the whole tier, because they answer "would this graph be cheaper
written the other way":

- `fold_lift2` against a balanced binary fold — depth 5N versus depth 5·log₂N.
  Neither library has an n-ary `Cell::lift`, which is *why* the book folds
  binary lifts into a linear chain. The chain is the finding, not a defect to
  paper over.
- `broadcast_demux` against `Router` + `filter_matches`. `Router` exists in
  sodium-rust and appears nowhere in the book; this is its natural use, and the
  cost curves should diverge sharply as N grows.
- `self_reading_accum` against `Stream::accum`, which collapses the book's
  five-node gadget plus loop into three nodes and no loop resolution at all.

### Wave 2 — dynamic graphs and one composite

| scenario | shape | parameter | drawn from |
|---|---|---|---|
| `population_churn` | N agent subgraphs (~22 nodes each) created and destroyed on one long-lived context while it runs | N = 10..1000 | zombicus `dynamic.java` |
| `gesture_switch_churn` | `switch_s` retargeted to a freshly built subgraph per gesture, old one released | gestures per iteration | battle drag gesture |
| `junction_rebuild` | membership change rebuilds an N-wide merge fold behind a `switch_s` | N = 1..256 | writable-remote `StreamJunction` |
| `eager_recompute_ratio` | a `lift4` recomputed on every cell update but consumed by a `snapshot` that fires rarely | ratio 1:1 … 1:1000 | petrol pump `sSaleComplete` |
| `petrol_pump` | the full application graph, 77 nodes, 4 `CellLoop`s, 1 `StreamLoop`, driven through a synthetic sale | six sizes: 7, 24, 30, 44, 65, 77 nodes | petrol pump, whole chapter |

`population_churn` is the important one. It is the shape the ADR's degradation
measurement was a stripped-down version of, and it is the reason this tier
exists at all: the book's zombicus chapter creates and destroys agent subgraphs
continuously on a context that lives for the whole run, which is exactly the
regime nothing we have today can see.

`petrol_pump` is the flagship. The book builds it incrementally across six
sections — `KeypadPump` at 7 nodes, `LifeCyclePump` at 24, then 30, 44, 65, and
`PresetAmountPump` at 77 — so the same application gives a free size
progression without inventing anything. It is the one scenario where a regression can
be reported as "a sale got 8% more expensive", which is the sentence a user of
the library would actually care about.

### Notes for whoever writes the ports

The survey turned up several things that will bite:

- **Java samples cells inside closures without declaring it.** `HomoSapiens`,
  `FrTextField` and others call `x.sample()` inside a `snapshot` or `map`
  lambda. sodium-rust builds its dependency graph from network shape and cannot
  see inside a closure, so these must become `snapshot3` or the `*_with_deps`
  variant. This is not optional — getting it wrong produces a graph that is
  cheaper *and wrong*, which would quietly flatter the benchmark.
- **`Listener::append` does not exist.** Java accumulates listeners into one
  handle; keep a `Vec<Listener>` in the harness instead. Bookkeeping only, no
  graph-shape impact — but see the ADR on why `unlisten()` versus `drop` is not
  a free choice.
- **There is no `TimerSystem`.** The whole continuous-time chapter is built on
  `Transaction.onStart` refreshing a clock `CellSink`, and sodium-rust's `post`
  runs after a transaction rather than before. The driver has to send the clock
  value itself as the first event of each frame. This is one reason the
  continuous-time shapes sit in the backlog.
- **`Cell::switch_s` and `switch_c` insert a hidden `Cell::map`.** The wrappers
  do `csa.map(|sa| sa.impl_.clone())` before the switch nodes proper, so every
  switch site costs two nodes that neither the Java nor the F# version pays.
  Worth measuring on its own, and worth an issue: if that clone can be avoided,
  every dynamic graph in the library gets cheaper.
- **`Stream::split` carries an extra `Sync` bound** that Java does not, so
  payloads in any split benchmark must be `Sync`.

### Backlog

Shapes the survey found that are worth having and are not in the two waves
above: `calm` (`collect_lazy` de-duplication), the `defer` ladder,
`Operational::split`'s transaction storm, the bidirectional lens stack and
`Junction` coalescing from writable-remote, the `Promise` lift tree and its
cross-diamond from real-world, the pausable clock from patterns, and the
double-loop signal integrator from continuous-time. Pick from these once the
two waves are in and we know which structures the ledger cannot already
explain.


**Exit criteria:** each scenario runs headless and deterministically; each one's
node count is asserted in tier 4 so a scenario cannot silently change shape.

## Phase 5 — retire `benches/sodium.rs`

Once tiers 1 through 3 are in, the original file is fully superseded: its
`Stream::send` and `Cell::send` groups become ledger entries with real
attribution, and its `snapshot` group becomes one ledger entry per arity. Delete
it in its own commit, with the commit message naming where each group went.

Do this last. Until the replacements exist and have been run, it is the only
benchmark data we have.

## What this does not cover

- **Multi-threaded contention.** sodium-rust is `Send + Sync` over `parking_lot`,
  and nothing here drives one graph from several threads. Worth doing; out of
  scope for this plan, because a contention benchmark needs a stable
  single-threaded baseline to be read against, and that baseline is what these
  five phases build.
- **Memory high-water mark.** Tier 4 counts allocations, not live bytes. Peak
  RSS under a churning workload is a different question and probably wants a
  different tool.
- **Comparison against other FRP libraries.** Interesting, but it answers
  "should I use this" rather than "did my change make this slower", and only the
  second question is what this suite is for.
