# research

Experiments that back the [architecture decision records](../docs/adr). Not
published, not a dependency of anything, not run in CI.

An ADR that says "we chose X because Y is slow" is only as good as the
measurement behind Y, and a measurement nobody can re-run is an assertion. So
every claim in an ADR that came from running code has the code that produced it
in here, along with the output it produced at the time.

This is deliberately not the benchmark suite. Benchmarks are a standing
instrument, maintained, run repeatedly, compared against baselines. These are
one-off investigations kept for the record. They answer a question once, the
answer goes into an ADR, and the file stays so the next person can check the
working — or find out that the answer has changed.

## Layout

```
research/
  src/lib.rs                 shared helpers: counting allocator, timing, rigs
  src/bin/adr<NNNN>_*.rs     one experiment per binary
  benches/adr<NNNN>_*.rs     experiments that need a bench harness
```

Every file is named for the ADR it supports, so `git grep adr0001` finds the
whole evidence base for ADR 1 at once.

## Running them

```shell
cargo run --release -p research --bin adr0001_timed_region
```

Release, always. The debug numbers are not comparable to anything, including
each other.

The callgrind experiment needs a bench harness, valgrind, and a runner pinned
to the same version as the crate:

```shell
cargo install --version 0.16.1 iai-callgrind-runner
cargo bench -p research --bench adr0001_instruction_counts
```

It is Linux-only, and builds to a stub elsewhere, in the same way `coz-driver`
is.

## Reading the output

Each file records, in its header, the output it produced on the machine and at
the commit named there. Expect to reproduce the **allocation counts and
instruction counts exactly** and the **timings not at all** — wall clock on a
shared machine moved by 45% between two runs while this crate was being
written, which is most of the argument for the ADR 1 tiering. Compare figures
within one run, not across runs.

## Adding an experiment

1. Name it `adr<NNNN>_<question>.rs`, where NNNN is the ADR it serves.
2. Open with a doc comment saying what question it answers and how to run it.
3. Put the output it produced in that comment, dated, with the machine and the
   sodium-rust commit.
4. Say which conclusions reached the ADR — and, if the experiment turned out to
   be misleading, say that too and leave it in place.

That last point is not a formality. `src/bin/adr0001_cost_model.rs` reports
that nodes off the firing path cost nothing, and it is wrong: they cost about
61 instructions each per event, which
`benches/adr0001_instruction_counts.rs` measures and a timing loop cannot. The
experiment that gives the confident wrong answer is worth keeping next to the
one that corrects it, because the reason it was wrong is the reason the
benchmark suite is built the way it is.
