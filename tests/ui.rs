//! Compile-time tests for the closure ergonomics of the public API, run with
//! [`trybuild`].
//!
//! The cases live in `tests/ui/`:
//!
//! * `bare_closures.rs` must **compile**. It exercises every function-taking
//!   combinator with an unannotated closure, from outside the crate. This is
//!   the regression guard: if a combinator is moved back onto an
//!   `IsLambda`-style bound, it stops compiling.
//! * The rest record *why* the API is shaped this way. They depend on nothing
//!   from this crate -- each reduces the old failure to a handful of lines --
//!   so the reasoning in `tests/closure_type_inference.rs` stays checked rather
//!   than merely asserted.
//!
//! `combinator_rejects_captured_state.rs`,
//! `listener_accepts_captured_state.rs` and
//! `fnmut_bound_rejects_shared_reference.rs` belong to a separate question:
//! the split between `Fn` on the combinators and `FnMut` on the listeners
//! ([issue #48]). The first two run against the real API -- they are the
//! guard on each side of that split -- and the third stays a reduction,
//! because it is about a property of the traits rather than of this crate.
//! `tests/fn_vs_fnmut.rs` is their runtime half and carries the findings.
//!
//! [issue #48]: https://github.com/SodiumFRP/sodium-rust/issues/48
//!
//! Expected output for a failing case lives beside it in a `.stderr` file. To
//! refresh them after a deliberate change:
//!
//! ```text
//! TRYBUILD=overwrite cargo test --test ui
//! ```
//!
//! rustc's diagnostics are not stable across releases, so the `compile_fail`
//! cases only run on stable; on beta and nightly a wording change would fail
//! the build for no useful reason. The `pass` cases have no expected output, so
//! `bare_closures.rs` and `listener_accepts_captured_state.rs` run everywhere.
//!
//! Everything goes through a single `TestCases`: it drives one shared scratch
//! project under `target/tests/`, so a second instance in the same binary would
//! race with this one.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/bare_closures.rs");
    t.pass("tests/ui/listener_accepts_captured_state.rs");
    diagnostics(&t);
}

/// The cases whose expected stderr is rustc-version sensitive.
#[rustversion::stable]
fn diagnostics(t: &trybuild::TestCases) {
    t.compile_fail("tests/ui/old_islambda_shape.rs");
    t.compile_fail("tests/ui/single_impl.rs");
    t.pass("tests/ui/fn_bound_rescues_closure.rs");
    t.compile_fail("tests/ui/fn_bound_rejects_lambda.rs");
    // `Fn` combinators vs `FnMut` listeners -- see tests/fn_vs_fnmut.rs.
    t.compile_fail("tests/ui/combinator_rejects_captured_state.rs");
    t.compile_fail("tests/ui/fnmut_bound_rejects_shared_reference.rs");
}

#[rustversion::not(stable)]
fn diagnostics(_t: &trybuild::TestCases) {}
