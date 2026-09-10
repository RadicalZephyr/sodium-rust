# adr-research

Experiments backing the decision records in [`docs/decisions/`](../).

Any code that produces concrete data used to argue an ADR lives here rather
than in a scratch file or a gist. An ADR that cites a measurement is only as
good as a reader's ability to re-run it, and a benchmark that lived in
someone's working tree cannot be re-run at all.

## Layout

Each experiment is a binary named after the ADR it serves:

```text
docs/decisions/research/src/bin/0001-some-decision.rs
```

and is run from the repository root:

```shell
cargo run --release -p adr-research --bin 0001-some-decision
```

Print results in whatever shape the ADR needs to quote them, and link the
binary from the section that relies on the numbers. `src/lib.rs` is for
fixtures shared between binaries -- a graph builder two experiments both need,
a timing harness -- and is empty until something is actually shared. Resist
putting an experiment's own scaffolding there.

This is a workspace member, so `cargo test --workspace` and
`cargo clippy --workspace --all-targets` cover it, and dependencies added here
land in the `cargo-deny` graph like any other. They have to be
license-compatible with BSD-3-Clause.

## What does not go here

Research produces *evidence*, not tests. It is not asserting that the library
is correct, and it is not expected to keep passing -- it answers a question
that was open at the time an ADR was written.

The one place an ADR does produce a test is when its argument turns on our
implementation diverging from Sodium's denotational semantics. That test
belongs in `src/tests.rs` with the rest of the suite, and it is written to
fail. See [`CONTRIBUTING.md`](../../../CONTRIBUTING.md#tests-and-research-are-not-the-same-thing).

An experiment that needs nothing from this crate does not belong here either.
If it depends only on rustc and std -- a probe into inference, a diagnostic
worth quoting -- it goes in a Rust Playground share link recorded in the ADR
instead, which is also the only route available to a case that has to *fail* to
compile. [`../README.md`](../README.md) has the routing rule and the version
stamp such a record has to carry.

## Retirement

An experiment is maintained while the decision it serves is still being argued
or built. It leaves the moment that decision is done -- which the record dates
exactly, in the `Implemented` row of its status log -- through one of two exits:

- **Deleted** -- it measured internals the ADR replaced. A binary that no
  longer compiles against the new code is deleted rather than repaired; cite
  the commit that produced the numbers in the ADR and history keeps it. Most
  records here argue for changing the internals an experiment was measuring, so
  breaking is how it ends rather than a regression.
- **Promoted** -- it still answers a live question, which means it stopped
  being research. A measurement worth re-running is a benchmark and moves to
  `benches/`; something asserting a property we promise is a test and moves to
  `src/tests.rs`.

What an experiment never does is linger. This crate is a **staging area, not an
archive**: everything in it has a scheduled exit, and a healthy crate trends
toward empty. Accumulation is the signal to look for something miscategorised
-- a benchmark that was never promoted, or an experiment whose ADR quietly
landed months ago.
