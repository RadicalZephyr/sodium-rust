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
//! Expected output for a failing case lives beside it in a `.stderr` file. To
//! refresh them after a deliberate change:
//!
//! ```text
//! TRYBUILD=overwrite cargo test --test ui
//! ```
//!
//! rustc's diagnostics are not stable across releases, so the `compile_fail`
//! cases only run on stable; on beta and nightly a wording change would fail
//! the build for no useful reason. `bare_closures.rs` has no expected output
//! and runs everywhere.
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
#[rustversion::stable]
fn diagnostics(t: &trybuild::TestCases) {
    t.compile_fail("tests/ui/old_islambda_shape.rs");
    t.compile_fail("tests/ui/single_impl.rs");
    t.pass("tests/ui/fn_bound_rescues_closure.rs");
    t.compile_fail("tests/ui/fn_bound_rejects_lambda.rs");
}

#[rustversion::not(stable)]
fn diagnostics(_t: &trybuild::TestCases) {}
