//! Compile-time tests for the closure ergonomics of the public API, run with
//! [`trybuild`].
//!
//! `tests/ui/bare_closures.rs` must **compile**. It exercises every
//! function-taking combinator with an unannotated closure, from outside the
//! crate, which is the regression guard: if a combinator is ever moved back onto
//! an `IsLambda`-style bound, it stops compiling. It carries no expected output,
//! so it runs on every channel.
//!
//! A `compile_fail` case belongs here when the *rejection* is something this
//! crate promises -- a closure shape our own bounds refuse. It does not belong
//! here when it demonstrates a *compiler* behaviour: that is evidence for a
//! decision, and goes in the record arguing from it as a Playground experiment.
//! [ADR-0002](../docs/decisions/0002-closure-bounds-and-dependency-declaration.md)
//! is the worked example -- its reductions sat in this directory until that
//! record gave them a better home, which is why `diagnostics` below is currently
//! empty.
//!
//! Anything added to `diagnostics` carries expected stderr, which is why it is
//! gated to stable: CI runs beta and nightly, where a wording change would turn
//! the build red for no useful reason. The gate does not cover an *older* stable
//! than the snapshots were blessed against, which is a cost each case has to be
//! worth.
//!
//! Everything goes through a single `TestCases`: it drives one shared scratch
//! project under `target/tests/`, so a second instance in the same binary would
//! race with this one.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/bare_closures.rs");
    diagnostics(&t);
}

/// The cases whose expected stderr is rustc-version sensitive.
///
/// Empty at present; see the note above before adding one.
#[rustversion::stable]
fn diagnostics(_t: &trybuild::TestCases) {}

#[rustversion::not(stable)]
fn diagnostics(_t: &trybuild::TestCases) {}
