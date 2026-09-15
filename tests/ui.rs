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
//! Anything added to `diagnostics` carries expected stderr, which rustc does not
//! keep stable across releases. The gating below is what keeps that from costing
//! a contributor a red build out of a diff they did not write.

/// The exact stable the `.stderr` snapshots were blessed against.
///
/// Bump this and `NOT_BLESSED_STABLE` together when re-blessing, in the same
/// commit as the new snapshots.
#[rustversion::stable(1.98)]
const ON_BLESSED_STABLE: bool = true;
#[rustversion::not(stable(1.98))]
const ON_BLESSED_STABLE: bool = false;

#[rustversion::stable]
const ON_SOME_STABLE: bool = true;
#[rustversion::not(stable)]
const ON_SOME_STABLE: bool = false;

/// GitHub Actions sets `CI=true`, as does every other runner worth the name.
///
/// `option_env!` is resolved at compile time, and rustc records the read in its
/// dep-info, so cargo rebuilds this target when the variable changes -- checked
/// by flipping it back and forth on 2026-09-15 against rustc 1.94.1. No clean
/// build is needed for it to take effect.
const IN_CI: bool = option_env!("CI").is_some();

/// Locally, run the snapshot cases only on the toolchain they were blessed
/// against -- an older stable renders diagnostics differently and would fail a
/// contributor for someone else's diff. In CI, run them on any stable, because
/// checking the snapshots against the current stable is the point of having
/// them, and a failure there is addressed to us rather than to a bystander.
const RUN_DIAGNOSTICS: bool = ON_BLESSED_STABLE || (IN_CI && ON_SOME_STABLE);

/// The strict gate has to be a special case of the loose one. If it were not,
/// `RUN_DIAGNOSTICS` could turn the snapshots on for a channel that never
/// blessed them, which is the failure this whole arrangement exists to avoid.
const _: () = assert!(!ON_BLESSED_STABLE || ON_SOME_STABLE);

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/bare_closures.rs");
    if RUN_DIAGNOSTICS {
        diagnostics(&t);
    } else {
        // Says why, so a contributor who expected them does not go looking.
        eprintln!("ui: skipping the .stderr snapshot cases; not the blessed stable");
        eprintln!("    blessed={ON_BLESSED_STABLE} stable={ON_SOME_STABLE} ci={IN_CI}");
    }
}

/// The cases whose expected stderr is rustc-version sensitive.
///
/// Empty at present; see the note at the top of this file before adding one.
fn diagnostics(_t: &trybuild::TestCases) {}
