# 1. Benchmark suite structure

**Status:** Draft
**Date:** 2026-09-08

Implementation is laid out in [the benchmark plan](../benchmark-plan.md), with
a checklist in [the TODO](../benchmark-todo.md). Every measurement quoted below
is reproducible: the code that produced it is in [the `research`
crate](../../research), one binary per question, each recording the output it
gave. Allocation and instruction counts reproduce exactly; the timings will
not, and the gap between those two statements is most of the argument here.

## Context

`benches/sodium.rs` has nineteen bench functions across three groups. Between
them they touch ten combinators — `map`, `filter`, `merge`, `snapshot`,
`snapshot3` through `snapshot6`, `Cell::map` read through `sample`, and
`StreamLoop::loop_` — out of fifty-seven public operations, and that count
already sets aside every `*_with_deps` variant. Every one of the nineteen has
the same shape: build a fresh `SodiumCtx`, hang a shallow chain off a sink, push a
thousand values through it, drop the lot.

That shape is not wrong. Before assuming it was, I measured what fraction of
the timed region is graph construction rather than event propagation, because
the whole graph is rebuilt inside every `b.iter()` body:

| maps | whole (µs) | construct only (µs) | construct % |
|---|---|---|---|
| 0 | 4647.8 | 6.6 | 0.1% |
| 1 | 7822.5 | 12.5 | 0.2% |
| 8 | 29406.9 | 79.9 | 0.3% |

Construction is noise. The benches really are measuring propagation.

The problem is narrowness, and it is narrow in one specific direction: these
benches only ever measure a *fresh* context. Almost everything expensive in
sodium-rust happens on a context that has been alive for a while.

Build a one-node subgraph on a live context, release it, and repeat:

| after N release cycles | `node_count` | allocs per send | ns per send |
|---|---|---|---|
| 0 | 2 | 27.0 | 4 755 |
| 100, released with `drop` | 202 | 2 558.3 | 544 267 |
| 300, released with `drop` | 602 | 7 567.3 | 1 621 685 |
| 800, released with `drop` | 1602 | 20 073.3 | 5 032 694 |
| 800, then `collect_cycles()` | 1602 | 20 073.2 | 4 486 103 |
| 100, released with `unlisten()` | 2 | 127.2 | 18 238 |
| 300, released with `unlisten()` | 2 | 327.2 | 45 616 |
| 800, released with `unlisten()` | 2 | 827.2 | 116 029 |
| 800, `unlisten()` then `collect_cycles()` | 2 | 827.0 | 114 508 |

Both halves of that table grow linearly and without bound, and they are not the
same problem. Dropping a `Listener` without calling `unlisten()` retains its
nodes: `node_count` climbs by two per cycle and `collect_cycles()` will not
bring it down. Calling `unlisten()` does keep `node_count` at 2 — and a send
still costs one extra allocation for every subgraph the context has ever seen,
which no amount of collection recovers. A well-behaved program that creates and
releases a subgraph a thousand times has made every subsequent event thirty
times more expensive, with nothing in `node_count` to show for it.

I am not diagnosing either of those here. The point is that both are
reproducible, both are the kind of thing that shows up in a real application
long before a microbenchmark notices, and no benchmark we currently have can
see either one — because every benchmark we currently have throws the context
away after each iteration.

The second half of the problem is attribution. Wall-clock deltas on shared CI
runners carry several percent of noise, which is larger than most single
combinator regressions. `+mapx3` being slower than `+map` tells us three map
nodes cost more than one. It does not tell us whether `map` got more expensive
last Tuesday.

Some things I measured that the design leans on:

- **Cost is dominated by the nodes on the firing path; nodes elsewhere in the
  context are cheap but not free.** A node on the firing path costs roughly
  10 000 instructions per event. A node merely existing in the same context
  costs about 61 — measured by sweeping an unfired side-graph from 0 to 200
  nodes, which moved a 64-event body from 1 585 052 to 2 362 580 instructions,
  linearly. Wall clock could not see this at all: the same sweep looked flat at
  ~7.8 µs, because 61 instructions per node is buried in timing noise. That is
  a useful demonstration of why tier 1 uses callgrind. Allocation counts miss it
  too, and for a better reason — an idle node costs no allocations at all, only
  instructions.

  It is also what makes the degradation above so violent. The dead nodes there
  are not idle bystanders — they hang off the sink being fired, so they are on
  the firing path, at 10 000 instructions each rather than 61.

  Two arms differing by one node therefore differ by that node's firing cost
  plus its global cost, which is the number we want, as long as the arms are
  otherwise identical. Heap state is not the explanation: padding the context
  with 5000 live heap blocks instead of nodes moves the same body by 1.2%, in
  the *opposite* direction, against the 49% that 200 idle nodes add.
- **Transaction overhead dominates fine-grained sends.** 2000 sends through
  `sink -> map -> listen` cost 43 allocations each; the same 2000 sends inside
  one `ctx.transaction()` cost 1 each. Roughly 26 of the 27 baseline
  allocations are transaction machinery.
- **`switch_s` reconfiguration costs O(the branch switched in)**, not
  O(downstream) and not O(graph). Growing the downstream graph from 0 to 64
  nodes leaves a flip at 68 allocations flat; growing the *upstream* branch
  over the same range takes it from 68 to 209.
- **Allocation counts are exactly reproducible.** Repeated runs of a
  twenty-shape ledger return byte-identical integers; only the timing column
  moves.
- **Every `switch_s` and `switch_c` site pays for a `Cell::map` it does not
  need.** Both wrappers do `csa.map(|sa| sa.impl_.clone())` before reaching the
  switch nodes proper (`src/cell.rs:366` and `:373`), purely to unwrap a
  newtype — two extra nodes per switch site, which neither the Java nor the F#
  Sodium pays. The petrol pump has nine switch sites in its output plumbing, so
  that is eighteen nodes of pure overhead in one application. Worth an issue in
  its own right; noted here because it is the sort of thing the suite is
  supposed to find, and it turned up before the suite exists.
- **Callgrind instruction counts vary by at most 0.013% run to run**, across
  all eighteen arms of the bench, and the four small ones came back
  bit-identical. A 1% regression in a single combinator sits roughly 75× above
  that noise floor.

Every number above is reproducible, and phase 1 of the plan turns the
allocation counts into assertions so they stay that way.

There is also prior art in the repository. The abandoned `node-count-benches`
branch already had the right instinct — construction hoisted out of the timed
region, `BenchmarkId` parameterisation, a linear chain measured against a
binary tree — and its commit message records a real finding, that a `Listener`
is slightly cheaper than a `map` node. It stalled on a confound its own TODO
names: the two arms did not have equal listener counts, so the shapes were not
comparable. That work should be rehabilitated rather than rewritten.

## Decision

Four tiers, two harnesses, one shared scenario crate.

### Tier 1 — the combinator ledger (`benches/combinators.rs`, iai-callgrind)

One entry per public combinator. Each entry is a graph that differs from a
named baseline arm by exactly one node, driven by a fixed number of sends.
Callgrind reports instructions, cache hits and estimated cycles.

The ledger's regression signal is **the absolute instruction count of each
arm**, compared against a stored baseline — not the difference between arms.
That distinction matters, because the marginal cost of a node is not uniform:

| chain length | instructions (64 sends) | marginal instr/node/send |
|---|---|---|
| 0 | 934 752 | — |
| 1 | 1 581 675 | 10 108 |
| 2 | 2 196 345 | 9 604 |
| 3 | 3 083 678 | 13 865 |
| 4 | 3 751 946 | 10 442 |
| 8 | 6 728 919 | 11 608 |

Those numbers reproduce to within 0.013%, so the spread is structural — what a
node costs depends on where in the chain it sits. Subtracting arms would
therefore give a stable number that is not quite "the cost of `map`". Reading
each arm's absolute count against its own history gives exactly what we want:
*this arm got 4% more expensive*. The baseline arm stays in the suite for
interpretation, so we can still say roughly what a combinator contributes.

iai-callgrind rather than criterion for this tier, because the tier's entire
job is attribution and criterion cannot deliver it at this resolution.
Callgrind is Linux-only and counts instructions rather than time, which the
repository already has a precedent for in `coz-driver`.

### Tier 2 — structural scaling (`benches/structure.rs`, criterion)

The independent variables the cost model identifies, each swept over a range,
each isolating one term:

| variable | isolates |
|---|---|
| chain depth | per-node propagation cost |
| fan-out width at equal node count | whether shape matters beyond count |
| fan-in width (`merge` tree) | join cost |
| listener count | listener dispatch versus node dispatch |
| transaction batch size | transaction overhead amortisation |
| upstream branch size at a `switch_s` | reconfiguration cost |
| live subgraphs on one context | steady-state cost as a graph accumulates |
| release cycles on one context | degradation over a context's lifetime |

This is where `node-count-benches` is rehabilitated, with its listener-count
confound fixed by holding listener count equal across arms.

Criterion here, not callgrind: these are wall-clock scaling curves where a few
percent of noise does not obscure the shape, and criterion's parameterised
`BenchmarkId` plots are exactly the right output.

### Tier 3 — graph shapes from the book (`benches/graphs.rs`, criterion)

Distilled skeletons of the Sodium book's sample applications: the combinator
inventory, depth, fan-in, feedback loops and switch sites of each chapter's
graph, driven by synthetic event sequences, with all GUI scaffolding stripped.
Faithful where it affects cost, invented where it does not.

Crucially these run on a **long-lived context**, with construction hoisted out
of the timed region, so they exercise the regime tier 1 and tier 2 deliberately
hold still.

### Tier 4 — cost invariants (`tests/graph_cost.rs`, plain `#[test]`)

Allocation counts and node counts as exact assertions, under a counting global
allocator scoped to that one integration-test binary. `SodiumCtx::impl_` is
public, so `node_count()`, `node_ref_count()` and `collect_cycles()` are all
reachable from an integration test.

These are tests, not benchmarks, because the numbers are exact integers. A
change from 43 allocations per event to 44 is a hard failure with a diff,
available on every platform, in every CI run, with no statistics and no
baseline storage. That is a strictly better instrument than a timing harness
for the thing it can measure.

### Shared scenarios (`bench-support/`, a workspace member)

The graph builders live in one crate that all four tiers depend on, and that
`coz-driver` can use too — its prime sieve is already a scenario, just one
that happens to live in a binary. Cargo permits the dev-dependency cycle this
creates (`sodium-rust` dev-depends on `bench-support`, which depends on
`sodium-rust`); I verified this, and `cargo check -p sodium-rust`, which is what
the MSRV job runs, does not build it.

## Consequences

- CI grows a Linux-only benchmark job that installs valgrind. It is the second
  Linux-only tool in the repository, after Coz.
- `benches/sodium.rs` is superseded. Its `Stream::send` and `Cell::send` groups
  become tier 1 entries with better attribution; its `snapshot` group becomes a
  tier 1 entry per arity. Nothing in it is lost.
- Instruction counts are not time. A change that trades instructions for cache
  locality will read as a regression in tier 1 and an improvement in tier 2.
  That is a feature — two instruments disagreeing is information — but it
  needs saying out loud, or someone will chase a phantom.
- Callgrind baselines are machine-specific. They must be regenerated when the
  runner image changes, and they cannot be compared across architectures.
- Tier 4 will fail the moment anyone changes an allocation count, including
  deliberately. That is the point, but it means a performance change comes with
  an expected-value update in the same commit, and reviewers need to read those
  diffs rather than rubber-stamping them.

## Alternatives considered

**Criterion alone, with saved baselines.** `--save-baseline` plus `critcmp` is
zero new infrastructure and works well for a human doing a before-and-after
during a tuning session. It cannot be gated in CI at the resolution we need,
and it depends on remembering to run it. Rejected as the *only* mechanism;
kept as the workflow for tiers 2 and 3.

**Codspeed.** Deterministic per-PR comparisons with the least local work, but
it puts a third-party service on the critical path of a two-person project.
Rejected for now; the iai-callgrind tier gets the same determinism without the
dependency.

**Faithful headless ports of the book applications.** Maximally representative,
but the petrol pump's sale state machine and fridgets' layout algebra are real
application code that would rot, and their cost would be dominated by our
port's arithmetic as much as by sodium. Rejected in favour of skeletons that
reproduce the graph shape.

**A counting allocator inside the benchmarks.** Would give allocation counts
alongside timings in one harness, and criterion supports a custom measurement
for exactly this. Rejected because the numbers are exact integers and therefore
belong in an assertion rather than a distribution: a benchmark that reports
"43.0 allocations, ±0.0" is a test wearing a costume. Scoping the allocator to
one integration-test binary also keeps it off every other target.

**Keeping construction inside the timed region.** The measurement above says it
would not actually distort anything at these sizes. Hoisting it out anyway,
because it makes the timed region mean one thing, and because tier 3's
long-lived contexts require it regardless.

## Open questions

- Should tier 1 gate CI (fail the build on a regression over threshold) or only
  report? Gating catches drift; reporting avoids blocking unrelated work on a
  runner image change. I lean towards reporting first and gating once we have
  seen how stable the baselines are in practice.
- Where do callgrind baselines live — committed to the repository, or stored
  as CI artifacts keyed on the runner image?
- The `*_with_deps` variants double the ledger's size. Do they warrant their
  own entries, or is one representative pair enough to prove the dependency
  threading costs nothing extra?
