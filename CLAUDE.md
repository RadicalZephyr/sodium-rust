# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```shell
cargo test --workspace                                # everything
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Those are the three commands CI runs, so running them before pushing is the
fastest way to know a change will pass.

`--workspace` is load-bearing. The workspace root is itself a package, which
makes it the sole default member, so a bare `cargo test` skips `coz-driver` and
`adr-research` entirely.

Running one test:

```shell
cargo test --lib tests::switch_s                           # unit tests live in src/tests.rs
cargo test --lib tests::mem_test::mem                      # and its submodules
cargo test --test closure_type_inference infers_listeners  # integration tests in tests/
cargo test --test ui                                       # the trybuild suite
cargo test --workspace -- --ignored                        # the known semantic gaps
RUST_LOG=trace cargo test --lib tests::mem_test::mem -- --nocapture
```

`RUST_LOG=trace` is worth reaching for on any memory or propagation bug: the
cycle collector logs the whole graph it is walking, node by node, with the
`NodeName`s from `src/impl_/name.rs` attached.

The `compile_fail` cases under `tests/ui/` carry expected rustc output, which is
not stable across releases. After a deliberate change to a diagnostic:

```shell
TRYBUILD=overwrite cargo test --test ui
```

Only after a deliberate change. A mismatch you did not cause means your
toolchain is not the stable these were blessed against — run `rustup check`
first. Blessing on an older rustc commits its wording and turns CI red, and the
diff looks innocuous: the article in `expected a`/`expected an` is a real
example.

Benchmarks are Criterion, and the causal profiler is a separate workload with
its own setup — see [`coz-driver/README.md`](coz-driver/README.md):

```shell
cargo bench
cargo build --release -p coz-driver && coz run --- ./target/release/coz-driver
```

MSRV is declared as `rust-version` in the root `Cargo.toml` and checked by CI
with `cargo check -p sodium-rust` on that toolchain. The library alone,
deliberately: the dev-dependency tree reaches edition-2024 manifests the MSRV's
cargo cannot parse, so `src/` is what has to hold the line. `benches/` already
opts out with `#![allow(clippy::incompatible_msrv)]`.

## Architecture

### Two layers, and why every combinator is written twice

`src/*.rs` is the public API; `src/impl_/*.rs` is the implementation. Each
public type is a newtype over its implementation counterpart — `Stream<A>`
holds a `pub impl_: impl_::stream::Stream<A>` and forwards.

The split is not ceremony. The public layer is bounded on `Fn`/`FnMut`
directly, because rustc's closure signature deduction only looks through the
`Fn` family; a user-defined trait bound defeats it and forces callers to
annotate every closure parameter. The implementation layer is bounded on the
`IsLambda1`..`IsLambda6` traits, which carry an optional `Vec<Dep>` alongside
the function. The public `*_with_deps` variants wrap a closure with `lambda1`..
`lambda6` on the way through; the plain variants pass the bare closure, whose
`deps_op()` is `None`. `tests/closure_type_inference.rs` documents the full
diagnosis and guards against a regression.

So adding a combinator means touching both layers, and usually adding a
`*_with_deps` sibling.

### The node graph

Everything reduces to `impl_::node::Node`. A `Node` is an update closure plus
its edges: dependencies (upstream, held strongly) and dependents (downstream,
held weakly). A new computation means a new `Node`.

The asymmetry is the whole memory model. A `Listener` roots a chain of nodes
through the strong upstream edges; when the listener is dropped, whatever it
depended on — and nothing else depends on — becomes collectable.
`docs/internals/insights.md` is the short version of this.

An update closure reads its input's pending firing with
`Stream::with_firing_op`, applies the user closure, and calls `Stream::_send`
on its own stream. A firing lives in `StreamData::firing_op` for the duration
of the transaction and is cleared by a `pre_post` callback.

### Transactions

`impl_::sodium_ctx::SodiumCtx` holds the transaction state. `enter_transaction`
/ `leave_transaction` maintain a depth counter, and hitting zero runs
`end_of_transaction`, which is where propagation actually happens:

1. drain `pre_eot` callbacks,
2. loop draining `changed_nodes`, calling `update_node` on each until nothing
   is left,
3. drain `pre_post` (this is where `visited` flags are reset and firings are
   cleared), then `post` (deferred sends),
4. `collect_cycles`, once the outermost transaction is done.

`update_node` is a depth-first walk guarded by an atomic `visited` flag: it
updates a node's dependencies before the node itself, runs the update closure
only if some dependency actually changed, and then pushes into dependents. That
ordering is what makes the graph glitch-free — a node never observes a
half-updated set of inputs.

Nested transactions are ordinary: only the outermost one propagates.

### Garbage collection

An FRP graph is genuinely cyclic — `switch_s`, `switch_c`, `CellLoop` and
`StreamLoop` all close loops — so reference counting alone leaks.
`src/impl_/gc_node.rs` is a synchronous Bacon–Rajan cycle collector: nodes are
coloured Black/Gray/Purple/White, candidate roots are buffered, and
`mark_roots` / `scan_roots` / `collect_roots` run at the end of the outermost
transaction.

Every `GcNode` carries two closures the collector depends on:

- a **deconstructor**, which drops the node's outgoing references, and
- a **trace**, which hands the collector every `GcNode` this one holds.

Both have to be exactly right. A trace that misses an edge frees live data; a
trace that names an edge the node does not hold corrupts the reference-count
adjustment and leaks. This is why a closure capturing FRP nodes has to declare
them: `Dep` is just a handle on a `GcNode`, and `*_with_deps` exists so the
tracer can see through a closure it cannot introspect.

`GcNodeData` carries hand-written `unsafe impl Send/Sync`, which is why CI runs
macOS and Windows in addition to Linux.

### Threading

`ThreadedMode` abstracts how `update_node` fans out over dependencies.
`single_threaded_mode` (run inline) is the only one wired up;
`simple_threaded_mode` (thread per fan-out) exists and is dead code, and a
thread-pool mode is a TODO. Anything touching propagation should keep working
under both.

## Tests

- `src/tests.rs` and `src/tests/` — the main suite, inside the crate so it can
  reach `impl_`. It is also the most complete set of worked examples in the
  repository. Most tests end in `assert_memory_freed`, which collects cycles
  and asserts `node_count == 0`: a new combinator with a wrong trace or
  deconstructor fails here rather than leaking quietly.
- `tests/closure_type_inference.rs` — the closure ergonomics guarantee, from
  outside the crate. `infers_*` uses bare unannotated closures; `with_deps_*`
  covers the explicit-`Dep` siblings.
- `tests/ui/` via `tests/ui.rs` — the same guarantee at compile time, plus
  reduced repros of the *old* failure so the reasoning stays checked rather
  than merely asserted. The `compile_fail` cases are gated to stable with
  `rustversion`.
- `#[ignore = "ADR-NNNN: ..."]` tests in `src/tests.rs` — known gaps between
  what Sodium's denotational semantics require and what this implementation
  does. They are meant to fail; see the ADR conventions below before adding
  or "fixing" one. There are none at present, which is itself a claim: no
  divergence from the mandated semantics is currently known and recorded. So
  `cargo test --workspace -- --ignored` runs nothing until one is.

## Conventions

### Architecture decision records

Decision records live in [`docs/decisions/`](docs/decisions/), one per file,
`NNNN-kebab-case-title.md`. They are **living documents** — edited to stay
current rather than frozen on acceptance, because git is the log and the
document is the projection. New information goes in as a **dated addition marked
as arriving after the decision**, never as a silent revision of the original
reasoning. Read the directory as the current state of this repository's
decisions. [`0001-recording-important-decisions.md`](docs/decisions/0001-recording-important-decisions.md)
argues for all of it; [`README.md`](docs/decisions/README.md) beside it has the
rules, and where a conflict or trade-off came up it belongs in the section it
concerns rather than a changelog at the bottom.

A record's status is a **dated transition log** in a collapsed `<details>` block
at the top of the file, with the current state in the `<summary>` line:

```markdown
<details>
<summary><strong>Status:</strong> Implemented 2026-11-03</summary>

| Date | Transition |
| --- | --- |
| 2026-09-10 | Drafted |
| 2026-09-24 | Accepted |
| 2026-11-03 | Implemented |

</details>
```

Transitions are `Drafted` (replacing a separate `Date` field), `Accepted` (a
commit before the record's PR merges), `Implemented` (the PR that finishes the
work), `Superseded by NNNN`, and `Deprecated`. The last row is the current state
and the summary restates it.

Two things the log is **not**. It is not an edit log — only state transitions go
in it, and changes to a record's content are dated additions in the body next to
the reasoning they concern. It is not a changelog at the bottom — it records
state, never reasoning, which is what keeps it consistent with writing conflicts
and trade-offs into the section they belong to. A row that wants a sentence of
explanation means that sentence belongs in the body.

Editing versus superseding: edit while the decision is still the decision; write a new record
when someone following the old one would now do the wrong thing. Mechanically —
if the change can be a dated addition it is an edit; if it means deleting a claim
someone may have acted on, the old claim earns its own record.

**Any code that produces concrete data used in the argumentation of an ADR must
be committed somewhere a reader can run it** — a number quoted in an ADR has to
be re-derivable from a checkout, or the record is asserting rather than arguing.
Which of two homes depends on what the experiment needs:

- **Needs `sodium-rust`** → a binary in the `adr-research` workspace crate at
  [`docs/decisions/research/`](docs/decisions/research/), named after the record
  (`src/bin/0001-some-decision.rs`, run with `cargo run --release -p
  adr-research --bin 0001-some-decision`). A dependency added there is checked
  for advisories but not for licenses: `deny.toml`'s allow list covers what we
  redistribute, and nothing there is.
- **Needs only rustc and std** → a Rust Playground share link recorded in the
  ADR. This is also the only route for a compile-time experiment: a case that
  must *fail* to compile cannot be a research binary, because a binary that does
  not compile breaks the workspace build.

A Playground record carries three things: the link (live convenience), the
source in a code block (the frozen record — what the ADR argued from), and **the
rustc version the quoted output came from**. That version stamp is required and
is checked in review. `version=stable` in a share link is a channel, not a
version, so the link drifts; without the stamp a reader cannot tell whether
rustc moved or the record was wrong.

An experiment is maintained while the decision it serves is still being argued
or built, and leaves the moment that decision is done — which the record dates
exactly, in the `Implemented` row of its status log, or in the `Deprecated` row
if the decision was withdrawn rather than built. It goes by one of two exits —
**deleted** (it measured internals the ADR replaced; a binary that no longer
compiles is deleted, not repaired, and the ADR cites the commit that produced
its numbers) or **promoted** (it still answers a live question, so it stopped
being research: a measurement worth re-running moves to `benches/`, a property
we promise moves to `src/tests.rs`). It never lingers.

So do not fix up a research binary that `cargo test --workspace` breaks on
without first checking whether its record has logged `Implemented` or
`Deprecated`; most records argue for changing the internals the experiment was
measuring, and breaking is the expected end of its life. The crate is a staging
area, not an archive, and should trend toward empty.

### Test what is mandated, measure what is chosen

This crate is one port in the Sodium family, so the API's shape and semantics
are mandated by Sodium's denotational semantics and are not up for debate here.
Practically every ADR is therefore about the internal implementation, or about
how those semantics should be spelled in Rust specifically — both things an ADR
*intends* to change.

So **do not write tests to motivate an ADR.** A test written against the
structure an ADR exists to replace has to be rewritten when the change lands: it
guarded nothing and only enlarged the diff. Support the argument with an
experiment in `adr-research`, and let the existing suite keep checking that
behaviour did not change while the internals did.

The exception is an ADR whose argument is that our operational semantics diverge
from Sodium's denotational semantics. That is a bug report rather than a design
preference, and it does get a test — written against the semantics, so it will
still be correct after the fix. Write it as the behaviour the library **ought**
to have, so it fails, and mark it:

```rust
#[test]
#[ignore = "ADR-0007: switch_c ought to take the inner cell's value in the same transaction"]
```

It sits with its neighbours in `src/tests.rs` rather than in a quarantine
module. `#[ignore]` keeps CI green while the reason still prints on every
ordinary `cargo test --workspace` run, so the gap advertises itself; run them
deliberately with `cargo test --workspace -- --ignored`. Landing the fix deletes
the attribute and nothing else.

Do not "fix" a failing ignored test by editing the test. It encodes what the
library owes Sodium; if it looks wrong, the ADR it names is the thing to argue
with.

[`CONTRIBUTING.md`](CONTRIBUTING.md) says all of this for human contributors.

### Changelog

User-visible changes get an entry in `CHANGELOG.md` under `Unreleased`, which
follows Keep a Changelog and semver. Breaking changes are marked
`**Breaking:**` and show a before/after snippet.

### Cargo.lock

Deliberately not committed, so the scheduled `cargo-deny` advisory run resolves
fresh every night and catches new advisories against unchanged code.
