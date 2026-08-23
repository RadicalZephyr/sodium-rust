//! Demonstrates an ergonomics problem in the public Sodium API: closures
//! passed to combinators cannot have their argument type inferred, so every
//! call site has to carry a type annotation on the closure parameter.
//!
//! The minimum annotation that works is a *partial* one naming only the
//! reference-ness of the argument:
//!
//! ```ignore
//! stream.map(|a: &_| *a + 1)
//! ```
//!
//! while the natural spelling fails to compile:
//!
//! ```ignore
//! stream.map(|a| *a + 1)
//! // error[E0282]: type annotations needed
//! ```
//!
//! This is surprising because the element type is already fully determined:
//! `map` is called on a `Stream<i32>`, so there is only one type the argument
//! could possibly have.
//!
//! The tests below are in three parts:
//!
//! * `annotated_*` — the API as it must be written today. These compile, and
//!   exist to pin down exactly how much annotation is required.
//! * `fnmut_bound_*` — the handful of methods that are bounded on `FnMut`
//!   rather than `IsLambda1`. These infer with no annotation at all, which
//!   isolates the trait bound as the variable that matters.
//! * `compile_fail` — invokes `rustc` on standalone snippets to *verify* that
//!   the unannotated forms really are rejected, and to reduce the failure to a
//!   minimal local trait that has nothing to do with FRP.
//!
//! See `THEORY` at the bottom of this file for the diagnosis.

use sodium::{Cell, Listener, SodiumCtx, Stream};
use std::sync::{Arc, Mutex};

/// Test scaffolding: drain a stream into a vector.
///
/// The closure here is fully annotated on purpose -- this helper is
/// infrastructure, not part of what is being demonstrated.
fn collect<A: Clone + Send + 'static>(s: &Stream<A>) -> (Arc<Mutex<Vec<A>>>, Listener) {
    let out: Arc<Mutex<Vec<A>>> = Default::default();
    let sunk = out.clone();
    let l = s.listen(move |a: &A| sunk.lock().unwrap().push(a.clone()));
    (out, l)
}

// ---------------------------------------------------------------------------
// Part 1: what the API costs today.
//
// Every closure below needs `: &_` (or a fuller annotation). Deleting the
// annotation from any one of them is a compile error; `compile_fail::` below
// proves it for a representative sample.
// ---------------------------------------------------------------------------

#[test]
fn annotated_stream_combinators() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let s: Stream<i32> = sink.stream();

    // `|a| *a + 1` => error[E0282]
    let mapped = s.map(|a: &_| *a + 1);
    // `|a| *a % 2 == 0` => error[E0282]
    let filtered = s.filter(|a: &_| *a % 2 == 0);
    // `|a| if *a > 2 { Some(*a) } else { None }` => error[E0282]
    let filter_mapped = s.filter_map(|a: &_| if *a > 2 { Some(*a) } else { None });
    // `|a, b| *a + *b` => error[E0282]
    let merged = s.merge(&mapped, |a: &_, b: &_| *a + *b);
    // `|a, total| *a + *total` => error[E0282]
    let accumulated = s.accum(0, |a: &_, total: &_| *a + *total);

    let (mapped_out, l1) = collect(&mapped);
    let (filtered_out, l2) = collect(&filtered);
    let (filter_mapped_out, l3) = collect(&filter_mapped);
    let (merged_out, l4) = collect(&merged);

    sink.send(4);
    sink.send(1);

    assert_eq!(*mapped_out.lock().unwrap(), vec![5, 2]);
    assert_eq!(*filtered_out.lock().unwrap(), vec![4]);
    assert_eq!(*filter_mapped_out.lock().unwrap(), vec![4]);
    // Simultaneous firings of `s` and `mapped` are combined by the merge fn.
    assert_eq!(*merged_out.lock().unwrap(), vec![9, 3]);
    assert_eq!(accumulated.sample(), 5);

    for l in [l1, l2, l3, l4] {
        l.unlisten();
    }
}

#[test]
fn annotated_cell_combinators() {
    let ctx = SodiumCtx::new();
    let ca = ctx.new_cell_sink(2i32);
    let cb = ctx.new_cell_sink(3i32);
    let a: Cell<i32> = ca.cell();
    let b: Cell<i32> = cb.cell();

    // `|x| *x * 10` => error[E0282]
    let mapped = a.map(|x: &_| *x * 10);
    // `|x, y| *x + *y` => error[E0282]
    let lifted = a.lift2(&b, |x: &_, y: &_| *x + *y);

    assert_eq!(mapped.sample(), 20);
    assert_eq!(lifted.sample(), 5);

    ca.send(7);
    assert_eq!(mapped.sample(), 70);
    assert_eq!(lifted.sample(), 10);
}

#[test]
fn annotated_snapshot() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let cell = ctx.new_cell_sink(10i32);

    // `|a, c| *a + *c` => error[E0282]
    let snapped = sink.stream().snapshot(&cell.cell(), |a: &_, c: &_| *a + *c);
    let (out, l) = collect(&snapped);

    sink.send(5);
    assert_eq!(*out.lock().unwrap(), vec![15]);

    l.unlisten();
}

/// The annotation is required even when the closure body would pin the type
/// down on its own. Here `push` onto a `Vec<i32>` leaves the compiler no
/// freedom at all about what `a` is, and it still will not infer it.
#[test]
fn annotated_even_when_body_is_unambiguous() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    let sunk = seen.clone();

    // `|a| sunk.lock().unwrap().push(*a)` => error[E0282], despite `sunk`
    // holding a `Vec<i32>` and the stream being a `Stream<i32>`.
    let l = sink
        .stream()
        .listen(move |a: &_| sunk.lock().unwrap().push(*a));

    sink.send(7);
    assert_eq!(*seen.lock().unwrap(), vec![7]);
    l.unlisten();
}

/// `&_` is the *minimum* annotation, but it is not always sufficient.
///
/// `&_` gives the compiler the reference shape and leaves the referent open, to
/// be filled in later by trait selection. That works as long as the closure body
/// constrains the referent directly. When the body only constrains an
/// *associated type* of it -- here `<?U as Neg>::Output == i32`, which does not
/// determine `?U` -- the referent stays ambiguous and the full type is required.
#[test]
fn ref_underscore_is_not_always_enough() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    let sunk = seen.clone();

    // `|a: &_| sunk.lock().unwrap().push(-*a)` => error[E0282]. The negation is
    // the only difference from the test above; `&i32` is needed here.
    let l = sink
        .stream()
        .listen(move |a: &i32| sunk.lock().unwrap().push(-*a));

    sink.send(7);
    assert_eq!(*seen.lock().unwrap(), vec![-7]);
    l.unlisten();
}

// ---------------------------------------------------------------------------
// Part 2: the contrast case.
//
// A few methods in this crate are bounded directly on `FnMut`/`Fn` instead of
// `IsLambda1`. Those infer with no annotation whatsoever. The two spellings sit
// side by side on `Cell`: `listen` is `IsLambda1<A, ()>` and needs `: &_`,
// while `listen_weak` is `FnMut(&A)` and does not.
// ---------------------------------------------------------------------------

#[test]
fn fnmut_bound_methods_infer_with_no_annotation() {
    let ctx = SodiumCtx::new();
    let cs = ctx.new_cell_sink(1i32);
    let cell = cs.cell();

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    let sunk = seen.clone();

    // `Cell::listen_weak` is bounded `K: FnMut(&A)`. No annotation needed --
    // and note this is the *same closure shape* rejected in Part 1.
    let l = cell.listen_weak(move |a| sunk.lock().unwrap().push(*a));

    cs.send(2);
    assert_eq!(*seen.lock().unwrap(), vec![1, 2]);

    l.unlisten();
}

#[test]
fn fn_bound_split_enum_infers_with_no_annotation() {
    use sodium::Enum2;

    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    // `split_enum2` is bounded `FN: Fn(&A) -> Enum2<B, C>`. `a` infers with no
    // annotation at all.
    let (evens, odds) = sink.stream().split_enum2(|a| {
        if *a % 2 == 0 {
            Enum2::A(*a)
        } else {
            Enum2::B(*a)
        }
    });

    let (evens_out, l1) = collect(&evens);
    let (odds_out, l2) = collect(&odds);

    sink.send(4);
    sink.send(5);

    assert_eq!(*evens_out.lock().unwrap(), vec![4]);
    assert_eq!(*odds_out.lock().unwrap(), vec![5]);

    l1.unlisten();
    l2.unlisten();
}

// ---------------------------------------------------------------------------
// Part 3: verify the failure, and reduce it.
//
// `cargo test` cannot contain code that fails to compile, so these tests drive
// `rustc` over standalone snippets and assert on the outcome.
// ---------------------------------------------------------------------------

mod compile_fail {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Locate the `sodium` rlib that this test binary was linked against, plus
    /// the `deps` directory it lives in. Returns `None` if the layout is not
    /// what we expect, in which case the calling test reports a skip rather
    /// than a spurious failure.
    fn sodium_rlib() -> Option<(PathBuf, PathBuf)> {
        let exe = std::env::current_exe().ok()?;
        let deps = exe.parent()?.to_path_buf();

        let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir(&deps).ok()? {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?.to_owned();
            if !name.starts_with("libsodium-") || !name.ends_with(".rlib") {
                continue;
            }
            let mtime = path.metadata().ok()?.modified().ok()?;
            let better = match &newest {
                Some((best, _)) => mtime > *best,
                None => true,
            };
            if better {
                newest = Some((mtime, path));
            }
        }

        newest.map(|(_, rlib)| (rlib, deps))
    }

    /// Compile `source` as a standalone crate and return rustc's stderr,
    /// together with whether it succeeded. `link_sodium` controls whether the
    /// snippet gets access to this crate.
    fn compile(dir: &Path, name: &str, source: &str, link_sodium: bool) -> Option<(bool, String)> {
        let src = dir.join(format!("{name}.rs"));
        std::fs::write(&src, source).ok()?;

        let mut cmd = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()));
        cmd.arg("--edition=2021")
            .arg("--crate-type=lib")
            .arg("--emit=metadata")
            .arg("-o")
            .arg(dir.join(format!("{name}.meta")))
            .arg(&src);

        if link_sodium {
            let (rlib, deps) = sodium_rlib()?;
            cmd.arg("-L")
                .arg(format!("dependency={}", deps.display()))
                .arg("--extern")
                .arg(format!("sodium={}", rlib.display()));
        }

        let out = cmd.output().ok()?;
        Some((
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    }

    /// A scratch directory for the snippets. `CARGO_TARGET_TMPDIR` is Cargo's
    /// per-integration-test scratch space, so this stays inside `target/` and
    /// gets cleaned up by `cargo clean`.
    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("inference-{name}"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// The headline result: an unannotated closure is rejected by `Stream::map`
    /// with E0282, and adding nothing but `: &_` makes it compile.
    #[test]
    fn unannotated_closure_is_rejected_but_ref_underscore_compiles() {
        let dir = scratch("stream-map");

        let broken = r#"
            pub fn f(s: sodium::Stream<i32>) -> sodium::Stream<i32> {
                s.map(|a| *a + 1)
            }
        "#;
        let fixed = r#"
            pub fn f(s: sodium::Stream<i32>) -> sodium::Stream<i32> {
                s.map(|a: &_| *a + 1)
            }
        "#;

        let Some((broken_ok, stderr)) = compile(&dir, "broken", broken, true) else {
            eprintln!("skipping: could not locate the sodium rlib next to the test binary");
            return;
        };
        assert!(
            !broken_ok,
            "expected `s.map(|a| *a + 1)` to be rejected, but it compiled"
        );
        assert!(
            stderr.contains("E0282"),
            "expected error[E0282] (type annotations needed), got:\n{stderr}"
        );

        let (fixed_ok, stderr) =
            compile(&dir, "fixed", fixed, true).expect("rustc ran once already");
        assert!(
            fixed_ok,
            "expected `s.map(|a: &_| *a + 1)` to compile, got:\n{stderr}"
        );
    }

    /// The same closure is accepted or rejected purely on the basis of which
    /// trait the method is bounded on: `Cell::listen` (`IsLambda1`) rejects it,
    /// `Cell::listen_weak` (`FnMut`) accepts it.
    #[test]
    fn islambda_rejects_what_fnmut_accepts() {
        let dir = scratch("cell-listen");

        let via_islambda = r#"
            pub fn f(c: sodium::Cell<i32>) -> sodium::Listener {
                c.listen(|a| println!("{}", *a + 1))
            }
        "#;
        let via_fnmut = r#"
            pub fn f(c: sodium::Cell<i32>) -> sodium::Listener {
                c.listen_weak(|a| println!("{}", *a + 1))
            }
        "#;

        let Some((islambda_ok, stderr)) = compile(&dir, "islambda", via_islambda, true) else {
            eprintln!("skipping: could not locate the sodium rlib next to the test binary");
            return;
        };
        assert!(
            !islambda_ok,
            "expected `Cell::listen` (IsLambda1 bound) to reject an unannotated closure"
        );
        assert!(
            stderr.contains("E0282"),
            "expected error[E0282], got:\n{stderr}"
        );

        let (fnmut_ok, stderr) =
            compile(&dir, "fnmut", via_fnmut, true).expect("rustc ran once already");
        assert!(
            fnmut_ok,
            "expected `Cell::listen_weak` (FnMut bound) to accept the very same closure, got:\n{stderr}"
        );
    }

    /// Reduce the problem away from Sodium entirely.
    ///
    /// This snippet has no dependency on this crate. It declares a two-line
    /// trait with the same shape as `IsLambda1` and a single blanket impl, and
    /// reproduces E0282 exactly. Crucially, the element type is *concretely
    /// known* from `Self` (as it is for `Stream<A>::map`), so the ambiguity is
    /// not about `A` being open -- it is about the closure's own signature not
    /// being deducible through a non-`Fn` trait bound.
    #[test]
    fn minimal_reduction_without_sodium() {
        let dir = scratch("reduction");

        let source = r#"
            pub trait IsLambda1<A, B> {
                fn call(&mut self, a: &A) -> B;
            }

            impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
                fn call(&mut self, a: &A) -> B { self(a) }
            }

            pub struct Stream<A>(A);

            impl<A> Stream<A> {
                // Same shape as sodium's `Stream::map`.
                pub fn map_via_trait<B, F: IsLambda1<A, B>>(&self, _f: F) {}
                // Same behaviour, bounded directly on `FnMut`.
                pub fn map_via_fnmut<B, F: FnMut(&A) -> B>(&self, _f: F) {}
            }

            pub fn control(s: &Stream<i32>) {
                s.map_via_fnmut(|a| *a + 1);   // infers fine
            }

            pub fn broken(s: &Stream<i32>) {
                s.map_via_trait(|a| *a + 1);   // rejected
            }
        "#;

        let Some((ok, stderr)) = compile(&dir, "reduction", source, false) else {
            eprintln!("skipping: could not run rustc");
            return;
        };

        assert!(!ok, "expected the reduction to fail to compile");
        assert!(
            stderr.contains("E0282"),
            "expected error[E0282] from the reduction, got:\n{stderr}"
        );
        // Exactly one error: the `FnMut`-bounded call inferred fine, only the
        // trait-bounded one failed.
        assert!(
            stderr.contains("aborting due to 1 previous error"),
            "expected only the trait-bounded call to fail, got:\n{stderr}"
        );
        assert!(
            stderr.contains("map_via_trait") || stderr.contains("*a + 1"),
            "expected the failure to be at the trait-bounded call, got:\n{stderr}"
        );
    }

    /// The two candidate impls (`Lambda<FN>` and the blanket `FN`) are *not*
    /// the cause. This reduction has a single impl and still fails, which rules
    /// out overlap/ambiguity between the two `IsLambda1` impls as the
    /// explanation.
    #[test]
    fn a_single_impl_does_not_fix_it() {
        let dir = scratch("single-impl");

        let source = r#"
            pub trait IsLambda1<A, B> {
                fn call(&mut self, a: &A) -> B;
            }

            // The only impl in scope. No `Lambda<FN>` to be ambiguous with.
            impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
                fn call(&mut self, a: &A) -> B { self(a) }
            }

            pub fn apply<A, B, F: IsLambda1<A, B>>(_a: A, _f: F) {}

            pub fn broken() {
                apply(1i32, |a| *a + 1);   // still rejected
            }
        "#;

        let Some((ok, stderr)) = compile(&dir, "single", source, false) else {
            eprintln!("skipping: could not run rustc");
            return;
        };
        assert!(
            !ok && stderr.contains("E0282"),
            "expected E0282 even with a single impl, got success={ok}:\n{stderr}"
        );
    }

    /// Adding `FnMut(&A) -> B` alongside the `IsLambda1` bound *does* restore
    /// inference -- which confirms the diagnosis, but is not a free fix: it
    /// would also reject the `Lambda` wrapper produced by `lambda1`, since
    /// `Lambda<FN>` is a plain struct and does not implement `FnMut` on stable.
    ///
    /// Both halves are asserted here so the trade-off is recorded, not guessed.
    #[test]
    fn adding_an_fn_bound_fixes_inference_but_excludes_lambda() {
        let dir = scratch("fn-bound");

        let preamble = r#"
            pub trait IsLambda1<A, B> {
                fn call(&mut self, a: &A) -> B;
                fn deps(&self) -> usize;
            }

            pub struct Lambda<FN> { pub f: FN, pub deps: usize }

            impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for Lambda<FN> {
                fn call(&mut self, a: &A) -> B { (self.f)(a) }
                fn deps(&self) -> usize { self.deps }
            }

            impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
                fn call(&mut self, a: &A) -> B { self(a) }
                fn deps(&self) -> usize { 0 }
            }

            pub struct Stream<A>(pub A);

            impl<A> Stream<A> {
                pub fn map<B, F: IsLambda1<A, B> + FnMut(&A) -> B>(&self, _f: F) {}
            }
        "#;

        // Half one: a bare closure now infers.
        let closure_ok = format!(
            "{preamble}
            pub fn f(s: &Stream<i32>) {{
                s.map(|a| *a + 1);
            }}"
        );

        // Half two: but the `Lambda` wrapper no longer type-checks.
        let lambda_rejected = format!(
            "{preamble}
            pub fn f(s: &Stream<i32>) {{
                s.map(Lambda {{ f: |a: &i32| *a + 1, deps: 3 }});
            }}"
        );

        let Some((ok, stderr)) = compile(&dir, "closure", &closure_ok, false) else {
            eprintln!("skipping: could not run rustc");
            return;
        };
        assert!(
            ok,
            "adding an `FnMut` bound should let a bare closure infer, got:\n{stderr}"
        );

        let (ok, stderr) =
            compile(&dir, "lambda", &lambda_rejected, false).expect("rustc ran once already");
        assert!(
            !ok && stderr.contains("E0277"),
            "expected the `Lambda` wrapper to be rejected by the added `FnMut` bound \
             (this is the cost of the fix), got success={ok}:\n{stderr}"
        );
    }
}

// ---------------------------------------------------------------------------
//
// THEORY
// ======
//
// The cause is not `Stream`, not `Cell`, and not the element type being
// under-determined. It is that every combinator is bounded on `IsLambda1` (or
// `IsLambda2`, ...) rather than on `FnMut`, and rustc's closure-signature
// inference does not look through a user-defined trait.
//
// How rustc types a closure argument
// ----------------------------------
// When rustc sees `s.map(|a| *a + 1)` it must assign a type to `a` *before* it
// type-checks the closure body. It tries to obtain an "expected signature" for
// the closure from the obligations in scope on the closure's type variable. That
// deduction (`deduce_closure_signature`) only fires for a fixed set of sources:
// the `Fn`/`FnMut`/`FnOnce` traits, `AsyncFn*`, and the associated-type
// projections that go with them. An obligation of the form
// `?F: IsLambda1<i32, ?B>` is not one of them, so no expected signature is
// produced.
//
// With no expected signature, `a` becomes a bare inference variable `?T` and the
// body is checked immediately. `*a` then requires knowing that `?T` is something
// dereferenceable, and nothing has said so yet -- hence
// `error[E0282]: type annotations needed ... type must be known at this point`,
// pointing at the closure parameter.
//
// The information that would resolve it does exist: selecting the blanket impl
// `impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN` against
// `?F: IsLambda1<i32, ?B>` yields `?F: FnMut(&i32) -> ?B`. But that selection
// happens after the closure body has already been checked, so it arrives too
// late to inform `a`. Closure signature inference is a pre-pass, not a fixpoint.
//
// Why `|a: &_|` is enough
// -----------------------
// Writing `&_` supplies the one thing the pre-pass could not: the *shape* of the
// parameter. `a: &'?r ?U` is now known to be a reference, so `*a + 1` type-checks
// as `?U: Add<i32>` without needing `?U` resolved yet. `?U` is then filled in
// later, from the trait obligation, once the impl is selected. The annotation is
// not carrying the type -- it is carrying the indirection, so that the deferred
// trait selection has something to unify against.
//
// It follows that `&_` is a floor, not a guarantee. It works whenever the body
// constrains the referent directly. When the body only constrains an associated
// type of the referent -- `<?U as Neg>::Output == i32` says nothing about `?U`,
// since `Neg::Output` is not injective -- the referent is still open and the
// full `&i32` is needed. `ref_underscore_is_not_always_enough` above is exactly
// that case: adding a unary minus to an otherwise identical closure pushes the
// required annotation from `&_` to `&i32`.
//
// This also explains the higher-ranked flavour of the problem. `FnMut(&A) -> B`
// desugars to `for<'a> FnMut(&'a A) -> B`. A closure only gets a late-bound
// lifetime in its signature if rustc knew to give it one, which again requires
// the expected signature. Inferred from the body alone, the closure ends up with
// an early-bound region and can then fail to satisfy the `for<'a>` bound even
// when the types line up.
//
// What the tests above pin down
// -----------------------------
// * `minimal_reduction_without_sodium` reproduces E0282 with a two-line trait
//   and no Sodium at all, with `A` concretely known from `Self` -- so neither
//   this crate's complexity nor an open element type is required.
// * `a_single_impl_does_not_fix_it` removes the `Lambda<FN>` impl, leaving one
//   candidate. It still fails, so impl ambiguity is not the cause either.
// * `ref_underscore_is_not_always_enough` shows the annotation burden is not
//   even a fixed `&_` tax; it varies with the closure body.
// * `islambda_rejects_what_fnmut_accepts` runs the identical closure through
//   `Cell::listen` and `Cell::listen_weak`, whose only difference is
//   `IsLambda1<A, ()>` versus `FnMut(&A)`. That is the whole variable.
//
// Why the API is shaped this way
// ------------------------------
// `IsLambda1` is not gratuitous. It exists so a closure can optionally carry a
// `Vec<Dep>` of extra FRP dependencies: `deps_op()` returns `None` for a plain
// closure and `Some(deps)` for a `Lambda` built by `lambda1(f, deps)`. Sodium
// needs those deps to build its dependency graph correctly when a closure
// captures a `Cell` and calls `sample()` on it. The trait is the mechanism that
// lets one parameter accept both forms.
//
// Possible remedies, and their costs
// ----------------------------------
// 1. Add `+ FnMut(&A) -> B` to the existing bounds. Inference is restored --
//    `adding_an_fn_bound_fixes_inference_but_excludes_lambda` shows the bare
//    closure compiling. But the same test shows the cost: `Lambda<FN>` is a
//    struct and does not implement `FnMut` on stable Rust, so `lambda1` call
//    sites stop compiling. Not viable on its own.
//
// 2. Split the API by shape. Bound the common methods on `FnMut(&A) -> B`
//    directly, and offer the dependency-carrying form as a separate method
//    (`map_with_deps(f, deps)`, or a `map_lambda`). The overwhelmingly common
//    call site -- a plain closure -- then infers, and the rare one stays
//    explicit. This is a breaking change to the `lambda1` call sites but not to
//    ordinary closure call sites, and it needs no new trait machinery. This
//    seems like the best trade.
//
// 3. Keep `IsLambda1` but make `Lambda<FN>` implement `FnMut`. This would make
//    option 1 work unconditionally, but `impl FnMut` for a user type requires
//    the unstable `unboxed_closures` + `fn_traits` features, so it is not
//    available on stable.
//
// 4. Leave the bounds alone and reduce the annotation burden with a macro or
//    with helper constructors that pin the argument type. This does not fix the
//    inference; it only hides it, and it does not help with method chains.
//
// Worth noting that this crate is already inconsistent about it: `Cell::listen`
// takes `IsLambda1<A, ()>` while `Cell::listen_weak` right beside it takes
// `FnMut(&A)`, and `split_enum2`/`split_enum3` take `Fn(&A) -> _`. Those methods
// have never supported deps, and they are the ones that infer cleanly.
//
// ---------------------------------------------------------------------------
