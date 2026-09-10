# Contributing

## Running the checks

CI runs these three commands, so running them before you push is the fastest
way to know a change will pass:

```shell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`--workspace` is load-bearing. The workspace root is itself a package, which
makes it the sole default member, so a bare `cargo test` skips `coz-driver`
and `adr-research` entirely.

Running one test:

```shell
cargo test --lib tests::switch_s                       # unit tests live in src/tests.rs
cargo test --test closure_type_inference infers_map    # integration tests in tests/
cargo test --test ui                                   # the trybuild suite
cargo test --workspace -- --ignored                    # the known semantic gaps, below
```

`RUST_LOG=trace` is worth reaching for on any memory or propagation bug -- the
cycle collector logs the whole graph it walks, node by node:

```shell
RUST_LOG=trace cargo test --lib tests::mem_test::mem -- --nocapture
```

The `compile_fail` cases under `tests/ui/` carry expected rustc output, which
is not stable across releases. After a deliberate change to a diagnostic,
re-bless them with `TRYBUILD=overwrite cargo test --test ui`.

Only after a deliberate change, though. A mismatch you did not cause means your
toolchain is not the stable these were blessed against -- run `rustup check`
before reaching for `overwrite`, because blessing on an older rustc commits its
wording and turns CI red. The diff looks harmless when it happens: the article
in `expected a`/`expected an` is a real example.

Tests run against stable, beta and nightly, on Linux, macOS and Windows;
nightly is allowed to fail. The MSRV is whatever `rust-version` in the root
`Cargo.toml` says, and CI checks it against the library alone.

User-visible changes should get an entry in [`CHANGELOG.md`](CHANGELOG.md),
which follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Architecture decision records

Decisions whose *reasoning* is the valuable part get a record in
[`docs/adr/`](docs/adr/). See [`docs/adr/README.md`](docs/adr/README.md) for
the file convention and the draft/accepted lifecycle.

Code that produces concrete data used in the argumentation of an ADR must live
in the `adr-research` workspace crate at
[`docs/adr/research/`](docs/adr/research/), as a binary named after the record
it serves. A number quoted in an ADR has to be re-derivable from a checkout;
an experiment that only ever existed in a scratch buffer makes the ADR an
assertion rather than an argument.

## Tests and research are not the same thing

Sodium-rust is one port in the [Sodium](https://github.com/SodiumFRP) family.
The shape of the API and the semantics it is supposed to have are mandated by
Sodium's denotational semantics -- they are not ours to renegotiate, and they
are not what an ADR here is about. What *is* ours is the internal
implementation, and the question of how those semantics should be spelled in
Rust specifically. Practically every ADR in this repository will be about one
of those two.

Which gives a rule with a sharp edge: **test what is mandated, measure what is
chosen.**

An ADR arguing for a different internal design should not be accompanied by
tests. Such a test is written against the very structure the ADR exists to
replace, so landing the change means rewriting it -- it never guarded
anything, it just made the diff bigger. Support that argument with an
experiment in `adr-research` instead, and let the existing suite go on
checking that behaviour did not change while the internals did.

### The exception: a gap between denotational and operational semantics

When an ADR's argument is that the library does not do what Sodium says it
should, that is not a design preference, it is a bug report, and it *does* get
a test. That test does not churn: it is written against semantics we do not
control, so it will still be correct after the fix.

Write it as the behaviour the library **ought** to have, so it fails, and mark
it with the record that explains why:

```rust
#[test]
#[ignore = "ADR-0007: switch_c ought to take the inner cell's value in the same transaction"]
fn switch_c_simultaneous() {
    // written against the semantics, not against what we currently do
}
```

Some notes on why it is shaped that way:

- It sits with its neighbours in `src/tests.rs`. Quarantining known gaps in a
  separate module makes them easy to stop reading.
- `#[ignore]` keeps CI green, but the reason is printed on every ordinary
  `cargo test --workspace` run -- `test switch_c_simultaneous ... ignored,
  ADR-0007: ...` -- so the gap advertises itself instead of hiding. Run them
  deliberately with `cargo test --workspace -- --ignored`.
- Landing the fix deletes one attribute line. The test itself does not change,
  which is the whole point.
