# adr-research

Experiments backing the architecture decision records in [`docs/adr/`](../).

Any code that produces concrete data used to argue an ADR lives here rather
than in a scratch file or a gist. An ADR that cites a measurement is only as
good as a reader's ability to re-run it, and a benchmark that lived in
someone's working tree cannot be re-run at all.

## Layout

Each experiment is a binary named after the ADR it serves:

```text
docs/adr/research/src/bin/0001-some-decision.rs
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

## Retirement

A research binary is maintained while its ADR is a draft. Once the record is
accepted and the change it argued for has landed, a binary that no longer
compiles against the new internals is deleted rather than repaired -- so cite
the commit that produced the numbers in the ADR itself, and the experiment
stays recoverable from history.

Most ADRs here argue for changing the internals. Repairing an experiment whose
conclusion has already been acted on is work in service of nothing.
