//! Compile-time test for the closure ergonomics of the public API, run with
//! [`trybuild`].
//!
//! `tests/ui/bare_closures.rs` must **compile**. It exercises every
//! function-taking combinator with an unannotated closure, from outside the
//! crate, which is the regression guard: if a combinator is ever moved back
//! onto an `IsLambda`-style bound, it stops compiling.
//!
//! There are deliberately no `compile_fail` cases here. The reductions that
//! show *why* the API is shaped this way are evidence rather than guards --
//! nothing this library promises has the form "rustc rejects this program" --
//! and they live in
//! [ADR-0002](../docs/decisions/0002-closure-bounds-and-dependency-declaration.md)
//! as Playground experiments. `docs/decisions/README.md` has the reasoning; the
//! short version is that pinning rustc's diagnostic wording turns the build red
//! on any stable older than the one the snapshots were blessed against.
//!
//! `bare_closures.rs` carries no expected output, so it runs on every channel.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/bare_closures.rs");
}
