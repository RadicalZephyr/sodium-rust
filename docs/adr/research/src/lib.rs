//! Experiments backing the architecture decision records in `docs/adr/`.
//!
//! Any code that produces concrete data used to argue an ADR lives here rather
//! than in a scratch file or a gist. An ADR that cites a measurement is only as
//! good as a reader's ability to re-run it, and a benchmark that lived in
//! someone's working tree cannot be re-run at all.
//!
//! Each experiment is a binary named after the ADR it serves:
//!
//! ```text
//! docs/adr/research/src/bin/0001-some-decision.rs
//! ```
//!
//! and is run from the repository root:
//!
//! ```shell
//! cargo run --release -p adr-research --bin 0001-some-decision
//! ```
//!
//! Print results in whatever shape the ADR needs to quote them, and link the
//! binary from the ADR section that relies on the numbers. Experiments outlive
//! the decision they informed: leave them in place once an ADR is accepted, so
//! a later ADR that revisits the question starts from a reproducible baseline.
//!
//! This crate is a workspace member, so `cargo test --workspace` and
//! `cargo clippy --workspace --all-targets` cover it. Dependencies added here
//! land in the `cargo-deny` graph like any other, so they have to be
//! license-compatible with BSD-3-Clause.
