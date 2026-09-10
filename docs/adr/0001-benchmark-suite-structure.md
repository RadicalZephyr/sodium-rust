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
| 0 | 4012.8 | 6.2 | 0.2% |
| 1 | 6748.7 | 10.8 | 0.2% |
| 8 | 25602.7 | 66.7 | 0.3% |

Construction is noise. The benches really are measuring propagation.

The problem is narrowness, and it is narrow in one specific direction: these
benches only ever measure a *fresh* context. Almost everything expensive in
sodium-rust happens on a context that has been alive for a while.

Build a one-node subgraph on a live context, release it, and repeat:

| after N release cycles | `node_count` | allocs per send | ns per send |
|---|---|---|---|
| 0 | 2 | 22.0 | 3 743 |
| 100, released with `drop` | 202 | 2 335.2 | 458 358 |
| 300, released with `drop` | 602 | 6 938.3 | 1 368 871 |
| 800, released with `drop` | 1602 | 18 441.3 | 3 691 062 |
| 800, then `collect_cycles()` | 1602 | 18 441.2 | 3 713 318 |
| 100, released with `unlisten()` | 2 | 122.1 | 17 360 |
| 300, released with `unlisten()` | 2 | 322.1 | 44 188 |
| 800, released with `unlisten()` | 2 | 822.1 | 113 631 |
| 800, `unlisten()` then `collect_cycles()` | 2 | 822.0 | 112 802 |

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

- **Cost is dominated by the nodes on the firing path. What a node off the
  firing path costs depends on whether its handles were dropped.** A node on
  the firing path costs 6 100 to 7 900 instructions per event. A node merely
  existing in the same context costs either 18.4 instructions per event or
  nothing, and the deciding variable is not one the benchmark's source makes
  visible.

  Dropping a `Stream` handle decrements a `GcNode` refcount, which files that
  node as a candidate cycle root; `collect_cycles` runs at the end of every
  transaction and walks the graph reachable from those roots. So a graph that
  has released its intermediate handles pays for its idle nodes on every event,
  and an otherwise identical graph still holding them pays nothing:

  | idle nodes on a second, never-fired sink | 0 | 8 | 64 | 200 | per node per event |
  |---|---|---|---|---|---|
  | handles held | 1 063 741 | 1 063 741 | 1 057 784 | 1 065 508 | 0 |
  | handles dropped | 1 078 499 | 1 088 611 | 1 142 517 | 1 314 421 | 18.4 |

  Same nodes, same listeners, same sinks, same 64 sends. Application code drops
  its handles — you write `sink.stream().map(f).listen(g)` and keep only the
  `Listener` — so the dropped arm is the one that describes a real program.

  Wall clock resolves neither arm: the same sweep on a timing loop is flat to
  within noise in both, and allocation counts are exactly 36 per send at every
  point in both. Instructions are the only instrument that sees this at all,
  which is most of the argument for tier 1, and the reason tier 2 arms have to
  pin handle lifetime the way tier 4 pins node counts.

  Allocator state is not the explanation. The control — padding the context with
  live heap blocks instead of live nodes — returns bit-identical counts at 0, 64
  and 200 blocks, against the 22% that 200 dropped-handle nodes add.

- **Transaction machinery costs essentially nothing; propagation is the whole
  of a send.** `SodiumCtx::transaction` is a depth counter, and `send` only
  sets the stream's `firing_op` and queues the node. Everything happens in
  `end_of_transaction`, which closes the transaction *and* runs the
  `changed_nodes` loop *and* drains three callback queues *and* calls
  `collect_cycles`. Because close and propagate are one function, the cost of
  closing cannot be got at by comparing one transaction against many — those
  arms differ in how much they propagate too. Measuring an empty transaction
  does get at it:

  | | allocations | ns |
  |---|---|---|
  | empty transaction, bare context | 0 | 289 |
  | empty transaction, 8 live subgraphs on the context | 0 | 292 |
  | one distinct sink fired, `sink -> map -> listen` | 36 | 6 930 |
  | marginal per additional distinct sink in one transaction | ~32 | ~6 500 |

  A transaction with nothing to propagate allocates nothing and is flat in the
  size of the graph. There is no per-transaction overhead worth amortising.

  The arms must fire *distinct* sinks. `Stream::_send` with no coalescer sets
  `data.firing_op = Some(a)`, so a second `send` to the same sink inside one
  transaction overwrites the first and it never propagates: sending 1, 2 and 3
  into one plain `StreamSink` in one transaction delivers `[3]`, with no panic
  and no diagnostic. Sweeping *sends per transaction* on a single sink
  therefore measures coalescing and would report the transaction as nearly free
  for entirely the wrong reason. The silent drop is tracked as issue #42; the
  advice it invites — "batch your sends into a transaction to amortise the
  overhead" — is wrong twice over, since there is no overhead to amortise and
  the batching destroys events.

- **`switch_s` reconfiguration costs O(the branch switched in) in time**, not
  O(downstream) and not O(graph), and is flat in allocations either way. A flip
  costs 54 allocations whichever side grows. Growing the downstream graph from
  0 to 64 nodes leaves it at ~12.6 µs flat; growing the *upstream* branch over
  the same range takes it from 12.7 µs to 92.6 µs.
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
| 0 | 630 176 | — |
| 1 | 1 076 165 | 6 969 |
| 2 | 1 468 995 | 6 138 |
| 3 | 1 947 190 | 7 472 |
| 4 | 2 358 304 | 6 424 |
| 8 | 4 385 993 | 7 921 |

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
| distinct sinks fired per transaction | per-transaction cost against propagation |
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
change from 36 allocations per event to 37 is a hard failure with a diff,
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

### What the suite has to measure to make a node cheaper

The tiers above answer "did this change make `map` slower". The slimming
programme needs them to answer "what is a node spending its budget on", which is
a different instrument.

[Issue #18](https://github.com/RadicalZephyr/sodium-rust/issues/18) asks whether
to redesign Sodium so the compiler can fuse chains of combinators into single
nodes. There are two ways to answer it — make a node disappear, or make a node
cheap — and the second has more headroom. Fusion is bounded by what it is legal
to fuse: only where the upstream node has exactly one consumer, which excludes
every shared node, and in the applications I have written sharing is the rule.
Slimming is bounded only by how much of a node's cost is essential.

Very little of it is:

| | allocations | ns | instructions/event |
|---|---|---|---|
| `sink -> listen`, no combinator at all | 22 | ~4 300 | 9 846 |
| each additional `map` on the firing path | 13–14 | ~2 700 | 6 100–7 900 |

A `map` node runs one closure over a `u16`. Against fourteen allocations, none
of that budget is the user's computation. A profile of `sink -> map -> listen`
says where it goes: the allocator is ~30% of all instructions, and lock and
atomic traffic inside the node update closure another ~11%.

That profile is also how the largest single win so far was found, and the way it
was found is the argument for this section. `GcCtx::mark_roots` called
`display_graph` unconditionally — a walk over every reachable node that hashes
each node pointer into a `HashSet`, builds a `String` per node and a `write!`
per edge, and hands the result to `trace!`, which discards it whenever trace is
off. Nothing in that graph formats anything, and yet `core::fmt` was 9% of its
instructions. Making the walk lazy took 31.7% off a 2 000-event run
(49 867 025 to 34 045 397 instructions), five to seven allocations off every
send depending on shape, and 15–18% off wall clock at every graph shape
measured. It is also most of what the old idle-node and `switch_s` allocation
figures were measuring.

Nothing in the four tiers as drafted would have caught it. `benches/sodium.rs`
ran for two years without seeing it, and could not have: a uniform tax on every
arm changes no comparison between arms. Neither would tier 1 comparing arms
against their own history, because the tax was already there when the first
baseline would have been taken. Four requirements follow, and only the last is
already implied above.

**The floor is a tracked number, not a subtraction baseline.** `sink -> listen`
costs 22 allocations and ~4.3 µs before any combinator exists — 63% of a
one-`map` event. No amount of per-node slimming touches it, and nothing in the
four tiers names it as a quantity to drive down. The `n0` arm is not only there
to interpret the others; it is the largest single line item for small graphs.

**Attribution is a stored artifact, not an ad-hoc profiling session.** A count
says an arm moved; only a profile says which of `malloc`, the collector, the
locks or the hasher it moved in. Tier 1 should commit a function-level
breakdown for one canonical arm alongside its instruction count, so "where does
a node's budget go" has a stored answer that moves when the code moves, and so a
4% regression arrives with a first guess attached.
`research/src/bin/adr0001_profile_target.rs` is the shape that should be
promoted into the suite.

**Allocations rank opportunities; instructions only detect change.** The two
instruments come apart in both directions, consistently. Removing
`display_graph` moved allocations, instructions and wall clock together. Adding
200 idle nodes adds no allocations, moves instructions by 22%, and moves wall
clock by nothing — those are real instructions that cost no time. So tier 1 is
the better *regression* detector, because its numbers are deterministic, and
tier 4 is the better *optimisation target*, because its numbers correspond to
something the machine charges for. Tier 4 should therefore assert allocations
per event per call site, not only per event: the programme is a sequence of
"which of these 22 allocations can go", and a total that moves 27 to 22 says
only that something did.

**Arms must pin handle lifetime, not just node count.** Two rigs with identical
graphs, node counts, listeners and sends measured 0 and 18.4 instructions per
idle node per event, differing only in whether their intermediate `Stream`
handles were still alive. Nothing in either benchmark's shape shows which it is.
`bench-support` scenario builders should state and assert what they hold and
what they release, the way tier 4 asserts node counts, or two arms that read
identically will measure different graphs.

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
- If instruction counts and wall clock disagree as sharply as the idle-node
  sweep says they can, is tier 1 gating the right idea at all? A change that
  removes 20% of the instructions and none of the time would pass a gate and
  buy nothing. I still want tier 1 for its determinism, but the gate may belong
  on tier 4's allocation counts instead.
- What is the floor made of? Not the transaction: an empty one is 0 allocations
  and ~290 ns, against 22 allocations and ~4.3 µs for one event through the two
  nodes `sink -> listen` cannot do without. So the floor is node cost too, and
  the slimming programme has no separate transaction workstream to open. I have
  not attributed those 22 allocations to call sites, which is the next thing to
  do and what the third requirement above asks the suite to make routine.
