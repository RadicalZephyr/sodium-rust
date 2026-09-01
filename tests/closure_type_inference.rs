//! Guards the closure ergonomics of the public Sodium API.
//!
//! Every combinator here is bounded on `FnMut`/`Fn` directly, so closure
//! arguments infer and call sites read the way they should:
//!
//! ```ignore
//! stream.map(|a| *a + 1)
//! ```
//!
//! This was not always true. The combinators used to be bounded on the
//! `IsLambda1`..`IsLambda6` traits, and every closure passed to the API needed
//! its parameter annotated -- at minimum `|a: &_|`, sometimes the full `|a: &i32|`
//! -- because rustc's closure signature deduction does not look through a
//! user-defined trait. See `THEORY` at the bottom of this file for the full
//! diagnosis.
//!
//! The tests here are in two parts:
//!
//! * `infers_*` -- the API exercised with bare, unannotated closures. These are
//!   the regression guard: if a combinator is ever moved back onto an
//!   `IsLambda`-style bound, they stop compiling.
//! * `with_deps_*` -- the `*_with_deps` siblings, which take an explicit
//!   `Vec<Dep>` for the rare case where a closure captures FRP nodes that
//!   Sodium cannot see.
//!
//! The compile-time half lives in `tests/ui.rs` and `tests/ui/`, run with
//! `trybuild`: it checks the same inference guarantee from outside the crate,
//! and reduces the *old* failure to a few lines of standalone Rust so the
//! reasoning below stays checked rather than merely asserted.

use sodium::{Cell, Dep, Listener, SodiumCtx, Stream};
use std::sync::{Arc, Mutex};

/// Test scaffolding: drain a stream into a vector.
fn collect<A: Clone + Send + 'static>(s: &Stream<A>) -> (Arc<Mutex<Vec<A>>>, Listener) {
    let out: Arc<Mutex<Vec<A>>> = Default::default();
    let sunk = out.clone();
    let l = s.listen(move |a| sunk.lock().unwrap().push(a.clone()));
    (out, l)
}

// ---------------------------------------------------------------------------
// Part 1: bare closures, no annotations.
//
// Not one closure below names its argument type. Each of these is a live
// assertion that the corresponding method is bounded on `FnMut`/`Fn`.
// ---------------------------------------------------------------------------

#[test]
fn infers_stream_combinators() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let s: Stream<i32> = sink.stream();

    let mapped = s.map(|a| *a + 1);
    let filtered = s.filter(|a| *a % 2 == 0);
    let filter_mapped = s.filter_map(|a| if *a > 2 { Some(*a) } else { None });
    let merged = s.merge(&mapped, |a, b| *a + *b);
    let accumulated = s.accum(0, |a, total| *a + *total);
    let collected = s.collect(0, |a, total| (*a * 10, *a + *total));

    let (mapped_out, l1) = collect(&mapped);
    let (filtered_out, l2) = collect(&filtered);
    let (filter_mapped_out, l3) = collect(&filter_mapped);
    let (merged_out, l4) = collect(&merged);
    let (collected_out, l5) = collect(&collected);

    sink.send(4);
    sink.send(1);

    assert_eq!(*mapped_out.lock().unwrap(), vec![5, 2]);
    assert_eq!(*filtered_out.lock().unwrap(), vec![4]);
    assert_eq!(*filter_mapped_out.lock().unwrap(), vec![4]);
    // `s` and `mapped` fire simultaneously, so the merge fn combines them.
    assert_eq!(*merged_out.lock().unwrap(), vec![9, 3]);
    assert_eq!(*collected_out.lock().unwrap(), vec![40, 10]);
    assert_eq!(accumulated.sample(), 5);

    for l in [l1, l2, l3, l4, l5] {
        l.unlisten();
    }
}

#[test]
fn infers_stream_snapshots() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let s = sink.stream();

    let cb = ctx.new_cell_sink(10i32);
    let cc = ctx.new_cell_sink(100i32);
    let cd = ctx.new_cell_sink(1000i32);
    let ce = ctx.new_cell_sink(10_000i32);
    let cf = ctx.new_cell_sink(100_000i32);

    let s2 = s.snapshot(&cb.cell(), |a, b| *a + *b);
    let s3 = s.snapshot3(&cb.cell(), &cc.cell(), |a, b, c| *a + *b + *c);
    let s4 = s.snapshot4(&cb.cell(), &cc.cell(), &cd.cell(), |a, b, c, d| {
        *a + *b + *c + *d
    });
    let s5 = s.snapshot5(
        &cb.cell(),
        &cc.cell(),
        &cd.cell(),
        &ce.cell(),
        |a, b, c, d, e| *a + *b + *c + *d + *e,
    );
    let s6 = s.snapshot6(
        &cb.cell(),
        &cc.cell(),
        &cd.cell(),
        &ce.cell(),
        &cf.cell(),
        |a, b, c, d, e, f| *a + *b + *c + *d + *e + *f,
    );

    let (o2, l2) = collect(&s2);
    let (o3, l3) = collect(&s3);
    let (o4, l4) = collect(&s4);
    let (o5, l5) = collect(&s5);
    let (o6, l6) = collect(&s6);

    sink.send(1);

    assert_eq!(*o2.lock().unwrap(), vec![11]);
    assert_eq!(*o3.lock().unwrap(), vec![111]);
    assert_eq!(*o4.lock().unwrap(), vec![1111]);
    assert_eq!(*o5.lock().unwrap(), vec![11111]);
    assert_eq!(*o6.lock().unwrap(), vec![111111]);

    for l in [l2, l3, l4, l5, l6] {
        l.unlisten();
    }
}

#[test]
fn infers_cell_combinators() {
    let ctx = SodiumCtx::new();
    let a = ctx.new_cell_sink(1i32).cell();
    let b = ctx.new_cell_sink(10i32).cell();
    let c = ctx.new_cell_sink(100i32).cell();
    let d = ctx.new_cell_sink(1000i32).cell();
    let e = ctx.new_cell_sink(10_000i32).cell();
    let f = ctx.new_cell_sink(100_000i32).cell();

    let mapped = a.map(|x| *x * 2);
    let l2 = a.lift2(&b, |x, y| *x + *y);
    let l3 = a.lift3(&b, &c, |x, y, z| *x + *y + *z);
    let l4 = a.lift4(&b, &c, &d, |w, x, y, z| *w + *x + *y + *z);
    let l5 = a.lift5(&b, &c, &d, &e, |v, w, x, y, z| *v + *w + *x + *y + *z);
    let l6 = a.lift6(&b, &c, &d, &e, &f, |u, v, w, x, y, z| {
        *u + *v + *w + *x + *y + *z
    });

    assert_eq!(mapped.sample(), 2);
    assert_eq!(l2.sample(), 11);
    assert_eq!(l3.sample(), 111);
    assert_eq!(l4.sample(), 1111);
    assert_eq!(l5.sample(), 11111);
    assert_eq!(l6.sample(), 111111);
}

/// Both listener flavours, on both `Stream` and `Cell`, take bare closures.
#[test]
fn infers_listeners() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let cs = ctx.new_cell_sink(1i32);

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();

    let sunk = seen.clone();
    let l1 = sink.stream().listen(move |a| sunk.lock().unwrap().push(*a));
    let sunk = seen.clone();
    let l2 = sink
        .stream()
        .listen_weak(move |a| sunk.lock().unwrap().push(*a * 2));
    let sunk = seen.clone();
    let l3 = cs
        .cell()
        .listen(move |a| sunk.lock().unwrap().push(*a * 100));
    let sunk = seen.clone();
    let l4 = cs
        .cell()
        .listen_weak(move |a| sunk.lock().unwrap().push(*a * 1000));

    seen.lock().unwrap().clear();
    sink.send(3);
    let mut got = seen.lock().unwrap().clone();
    got.sort_unstable();
    assert_eq!(got, vec![3, 6]);

    for l in [l1, l2, l3, l4] {
        l.unlisten();
    }
}

/// A closure body that would previously have needed the *full* `&i32` rather
/// than just `&_`, because the body only constrains an associated type of the
/// argument (`<?U as Neg>::Output`). It now needs nothing.
#[test]
fn infers_even_when_body_only_constrains_an_associated_type() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    let sunk = seen.clone();
    let l = sink
        .stream()
        .listen(move |a| sunk.lock().unwrap().push(-*a));

    sink.send(7);
    assert_eq!(*seen.lock().unwrap(), vec![-7]);
    l.unlisten();
}

// ---------------------------------------------------------------------------
// Part 2: the `*_with_deps` siblings.
//
// Sodium builds its dependency graph from the shape of the network, which it
// cannot see inside a closure. When a closure captures FRP nodes and reaches
// them at call time, those nodes have to be declared explicitly. That is what
// `*_with_deps` is for -- and it is the only reason the `IsLambda` traits
// existed in the first place.
// ---------------------------------------------------------------------------

/// The motivating case: a `Cell<Cell<A>>` built by a closure that picks between
/// two captured cells. Neither `ca` nor `cb` appears in the network shape, so
/// both must be declared as dependencies for `switch_c` to track them.
#[test]
fn with_deps_tracks_cells_captured_by_a_closure() {
    let ctx = SodiumCtx::new();

    let sa = ctx.new_stream_sink::<&'static str>();
    let sb = ctx.new_stream_sink::<&'static str>();
    let ssw = ctx.new_stream_sink::<&'static str>();

    let ca = sa.stream().hold("a0");
    let cb = sb.stream().hold("b0");
    let csw_str = ssw.stream().hold("ca");

    let deps: Vec<Dep> = vec![ca.to_dep(), cb.to_dep()];
    // Note the closure is still bare -- `*_with_deps` infers too.
    let csw = csw_str.map_with_deps(
        move |sw| if *sw == "ca" { ca.clone() } else { cb.clone() },
        deps,
    );

    let out = Cell::switch_c(&csw);
    let (seen, l) = collect(&out.updates());

    sa.send("a1"); // ca = "a1", currently selected
    ssw.send("cb"); // switch to cb, which is holding "b0"
    sb.send("b1"); // cb = "b1"
    ssw.send("ca"); // switch back to ca, still holding "a1"
    sa.send("a2"); // ca = "a2"

    assert_eq!(*seen.lock().unwrap(), vec!["a1", "b0", "b1", "a1", "a2"]);
    l.unlisten();
}

/// The `*_with_deps` variants exist across the API and all take bare closures.
///
/// Each closure below genuinely captures `bias` and samples it -- which is the
/// whole point of the mechanism. A `Dep` asserts to the garbage collector that
/// the closure holds a reference to that node, so declaring one the closure does
/// not actually capture corrupts the graph's ref counting.
#[test]
fn with_deps_variants_infer_too() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let s = sink.stream();

    let bias = ctx.new_cell_sink(100i32).cell();
    let cb = ctx.new_cell_sink(10i32).cell();
    let a = ctx.new_cell_sink(1i32).cell();

    let b = bias.clone();
    let mapped = s.map_with_deps(move |x| *x + b.sample(), vec![bias.to_dep()]);

    let b = bias.clone();
    let filtered = s.filter_with_deps(move |x| *x < b.sample(), vec![bias.to_dep()]);

    let b = bias.clone();
    let filter_mapped = s.filter_map_with_deps(move |x| Some(*x * b.sample()), vec![bias.to_dep()]);

    let b = bias.clone();
    let merged = s.merge_with_deps(
        &mapped,
        move |x, y| *x + *y + b.sample(),
        vec![bias.to_dep()],
    );

    let b = bias.clone();
    let snapped = s.snapshot_with_deps(&cb, move |x, y| *x + *y + b.sample(), vec![bias.to_dep()]);

    let b = bias.clone();
    let accumulated = s.accum_with_deps(0, move |x, t| *x + *t + b.sample(), vec![bias.to_dep()]);

    let b = bias.clone();
    let cell_mapped = a.map_with_deps(move |x| *x + b.sample(), vec![bias.to_dep()]);

    let b = bias.clone();
    let lifted = a.lift2_with_deps(&cb, move |x, y| *x + *y + b.sample(), vec![bias.to_dep()]);

    let (m, l1) = collect(&mapped);
    let (fi, l2) = collect(&filtered);
    let (fm, l3) = collect(&filter_mapped);
    let (mg, l4) = collect(&merged);
    let (sn, l5) = collect(&snapped);

    sink.send(2);

    assert_eq!(*m.lock().unwrap(), vec![102]);
    assert_eq!(*fi.lock().unwrap(), vec![2]);
    assert_eq!(*fm.lock().unwrap(), vec![200]);
    // `s` fires 2 and `mapped` fires 102 in the same transaction.
    assert_eq!(*mg.lock().unwrap(), vec![204]);
    assert_eq!(*sn.lock().unwrap(), vec![112]);
    assert_eq!(accumulated.sample(), 102);
    assert_eq!(cell_mapped.sample(), 101);
    assert_eq!(lifted.sample(), 111);

    for l in [l1, l2, l3, l4, l5] {
        l.unlisten();
    }
}

// ---------------------------------------------------------------------------
//
// THEORY
// ======
//
// Recorded because the fix is a bound change whose motivation is invisible from
// the diff, and because the `tests/ui/` reductions are only meaningful next to
// the explanation.
//
// The problem
// -----------
// Every combinator used to be bounded on `IsLambda1`..`IsLambda6` rather than on
// `FnMut`, and rustc's closure-signature inference does not look through a
// user-defined trait.
//
// When rustc sees `s.map(|a| *a + 1)` it must assign a type to `a` *before* it
// type-checks the closure body. It tries to obtain an "expected signature" from
// the obligations in scope on the closure's type variable. That deduction
// (`deduce_closure_signature`) only fires for a fixed set of sources: the
// `Fn`/`FnMut`/`FnOnce` traits, `AsyncFn*`, and the associated-type projections
// that go with them. An obligation of the form `?F: IsLambda1<i32, ?B>` is not
// one of them, so no expected signature was produced.
//
// With no expected signature, `a` became a bare inference variable `?T` and the
// body was checked immediately. `*a` then required knowing that `?T` was
// dereferenceable, and nothing had said so yet -- hence
// `error[E0282]: type annotations needed`, pointing at the closure parameter.
//
// The information that would have resolved it did exist: selecting the blanket
// impl `impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN` against
// `?F: IsLambda1<i32, ?B>` yields `?F: FnMut(&i32) -> ?B`. But that selection
// happens after the closure body has already been checked, so it arrived too
// late to inform `a`. Closure signature inference is a pre-pass, not a fixpoint.
//
// Why `|a: &_|` used to be enough, and why it sometimes wasn't
// ------------------------------------------------------------
// Writing `&_` supplied the one thing the pre-pass could not: the *shape* of the
// parameter. `a: &'?r ?U` was known to be a reference, so `*a + 1` type-checked
// as `?U: Add<i32>` without `?U` resolved, and `?U` was filled in later from the
// trait obligation. The annotation was not carrying the type -- it was carrying
// the indirection, so that deferred trait selection had something to unify
// against.
//
// That made `&_` a floor, not a guarantee. It worked whenever the body
// constrained the referent directly. When the body only constrained an
// associated type of the referent -- `<?U as Neg>::Output == i32` says nothing
// about `?U`, since `Neg::Output` is not injective -- the referent stayed open
// and the full `&i32` was needed.
// `infers_even_when_body_only_constrains_an_associated_type` is that case, and
// `tests/ui/bare_closures.rs` pins it down from outside the crate.
//
// The same reasoning explains the higher-ranked flavour: `FnMut(&A) -> B`
// desugars to `for<'a> FnMut(&'a A) -> B`, and a closure only gets a late-bound
// lifetime if rustc knew to give it one -- which again required the expected
// signature.
//
// The fix
// -------
// Bound the combinators on `FnMut`/`Fn` directly, and move the
// dependency-carrying form to a `*_with_deps` sibling that takes `Vec<Dep>` as
// an explicit argument:
//
//     stream.map(|a| *a + 1)
//     stream.map_with_deps(move |_| cell.sample(), vec![cell.to_dep()])
//
// `IsLambda1`..`IsLambda6`, `Lambda` and `lambda1`..`lambda6` are still the
// mechanism underneath -- `*_with_deps` builds the `Lambda` for you -- but they
// no longer appear in any public signature and are `#[doc(hidden)]`.
//
// Why not just add `+ FnMut(&A) -> B` to the old bounds? Because the extra
// bound also rejects `Lambda<FN>`, which is a plain struct and cannot implement
// `FnMut` on stable Rust. That would have broken every deps-carrying call site
// -- the one thing `IsLambda1` existed to support.
// `tests/ui/fn_bound_rescues_closure.rs` and
// `tests/ui/fn_bound_rejects_lambda.rs` assert both halves of that.
//
// Making `Lambda<FN>` implement `FnMut` would make the single-bound approach
// work, and is the cleaner end state, but it needs the unstable
// `unboxed_closures` and `fn_traits` features.
//
// ---------------------------------------------------------------------------
