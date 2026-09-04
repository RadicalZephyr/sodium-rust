//! What the `Fn` bound on the combinators enforces, and what it cost.
//!
//! The combinators are bounded on `Fn` and `listen`/`listen_weak` on `FnMut`.
//! That split is the resolution of [issue #48], which proposed `Fn` throughout
//! and stalled in 2020 on a counter-argument: that `Stream::map` is
//! deliberately allowed to mutate, because updating a collection in place is
//! `O(1)` where rebuilding an immutable one is `O(log n)`, and that this is
//! "indistinguishable from the immutable collection version in how it
//! operates".
//!
//! Nothing in that thread was measured. These tests are the measurement that
//! settled it, kept as a live record because the reasoning is not recoverable
//! from the diff.
//!
//! * `combinator_*` -- what the `Fn` bound accepts.
//! * `listener_*` -- why `listen` deliberately does not follow it.
//! * `rewrite_*` -- the closure shapes `Fn` rejects, in the form the API
//!   already had for them. `tests/ui/combinator_rejects_captured_state.rs`
//!   holds the rejected originals and the diagnostics they now produce.
//! * `claim_*` -- the `O(1)` collection argument from #48, put on a scale.
//! * `state_*` -- whether state kept outside the graph stays in step with it.
//!
//! `FINDINGS` at the bottom collects the conclusions.
//!
//! [issue #48]: https://github.com/SodiumFRP/sodium-rust/issues/48

use sodium_rust::{Cell, Listener, Operational, SodiumCtx, Stream};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Test scaffolding: drain a stream into a vector.
///
/// Note this handler mutates a capture, and is a `listen` rather than a
/// combinator. That is the whole distinction this file is about.
fn drain<A: Clone + Send + 'static>(s: &Stream<A>) -> (Arc<Mutex<Vec<A>>>, Listener) {
    let out: Arc<Mutex<Vec<A>>> = Default::default();
    let sunk = out.clone();
    let l = s.listen(move |a| sunk.lock().unwrap().push(a.clone()));
    (out, l)
}

// ---------------------------------------------------------------------------
// Part 1: what a `Fn` bound accepts.
//
// Which is very nearly everything anyone writes. `Fn` is a subtrait of
// `FnMut`, so the bound only narrows the API for closures that own mutable
// state across calls -- and Part 3 is the complete list of those.
// ---------------------------------------------------------------------------

/// Ordinary pure combinator closures -- by inspection the overwhelming majority
/// of real call sites, and the whole of this crate's own test suite.
#[test]
fn combinator_pure_closures() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let s = sink.stream();

    let mapped = s.map(|a| *a + 1);
    let filtered = s.filter(|a| *a % 2 == 0);
    let merged = s.merge(&mapped, |a, b| *a + *b);

    let (m, l1) = drain(&mapped);
    let (f, l2) = drain(&filtered);
    let (g, l3) = drain(&merged);

    sink.send(4);
    sink.send(1);

    assert_eq!(*m.lock().unwrap(), vec![5, 2]);
    assert_eq!(*f.lock().unwrap(), vec![4]);
    assert_eq!(*g.lock().unwrap(), vec![9, 3]);

    for l in [l1, l2, l3] {
        l.unlisten();
    }
}

/// A closure that captures FRP nodes and samples them. `Cell::sample` takes
/// `&self`, so the deps-carrying form -- the one thing `IsLambda` existed for
/// -- is `Fn`-clean too.
#[test]
fn combinator_closures_capturing_frp_nodes() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let bias = ctx.new_cell_sink(100i32).cell();

    let b = bias.clone();
    let biased = sink
        .stream()
        .map_with_deps(move |a| *a + b.sample(), vec![bias.to_dep()]);

    let (out, l) = drain(&biased);
    sink.send(2);
    assert_eq!(*out.lock().unwrap(), vec![102]);
    l.unlisten();
}

/// Interior mutability. A `Fn` bound is not a purity proof and was never
/// claimed to be -- `Arc<Mutex<_>>` walks straight through it. What it changes
/// is that reaching for one is a visible act rather than the default.
#[test]
fn combinator_interior_mutability() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    let s = seen.clone();
    let tapped = sink.stream().map(move |a| {
        s.lock().unwrap().push(*a);
        *a * 2
    });

    let (out, l) = drain(&tapped);
    sink.send(3);
    sink.send(4);

    assert_eq!(*out.lock().unwrap(), vec![6, 8]);
    assert_eq!(*seen.lock().unwrap(), vec![3, 4]);
    l.unlisten();
}

/// `accum` and `collect` carry state through the *signature* rather than
/// through a capture, so stateful stream processing needs no mutable capture at
/// all. This is what makes Part 3 cheap.
#[test]
fn combinator_state_threaded_through_the_signature() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let s = sink.stream();

    let total = s.accum(0, |a, t| *a + *t);
    let running = s.collect(0, |a, t| (*a + *t, *a + *t));

    let (out, l) = drain(&running);
    sink.send(3);
    sink.send(4);

    assert_eq!(total.sample(), 7);
    assert_eq!(*out.lock().unwrap(), vec![3, 7]);
    l.unlisten();
}

// ---------------------------------------------------------------------------
// Part 2: why `listen` keeps `FnMut`.
//
// `listen` is the effectful edge of the graph and its handler is supposed to do
// effectful things. Requiring `Arc<Mutex<_>>` there would be pure ceremony:
// there is no dataflow to make first-class, only a side effect to perform.
//
// The handler's mutability is absorbed by a `Mutex` inside `listen` itself, so
// nothing `FnMut` reaches the graph and the internals stay `Fn` throughout.
// ---------------------------------------------------------------------------

/// A handler accumulating into a capture. This is what `benches/sodium.rs` is
/// made of -- 18 of them, every one `move |v| values.push(*v)`.
#[test]
fn listener_may_own_mutable_state() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let mut total = 0;
    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    let s = seen.clone();
    let l = sink.stream().listen(move |a| {
        total += *a;
        s.lock().unwrap().push(total);
    });

    sink.send(3);
    sink.send(4);
    assert_eq!(*seen.lock().unwrap(), vec![3, 7]);
    l.unlisten();
}

/// All four listener entry points take `FnMut`, including the deps-carrying
/// ones, so declaring dependencies does not cost you the mutability.
#[test]
fn listener_all_entry_points_take_fnmut() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();
    let cs = ctx.new_cell_sink(1i32);
    let bias = ctx.new_cell_sink(100i32).cell();

    let seen: Arc<Mutex<Vec<i32>>> = Default::default();

    let (s, mut n) = (seen.clone(), 0);
    let l1 = sink.stream().listen(move |a| {
        n += 1;
        s.lock().unwrap().push(*a + n);
    });

    let (s, mut n) = (seen.clone(), 0);
    let l2 = sink.stream().listen_weak(move |a| {
        n += 1;
        s.lock().unwrap().push(*a * 10 + n);
    });

    let (s, mut n, b) = (seen.clone(), 0, bias.clone());
    let l3 = sink.stream().listen_with_deps(
        move |a| {
            n += 1;
            s.lock().unwrap().push(*a + b.sample() + n);
        },
        vec![bias.to_dep()],
    );

    let (s, mut n) = (seen.clone(), 0);
    let l4 = cs.cell().listen(move |a| {
        n += 1;
        s.lock().unwrap().push(*a * 1000 + n);
    });

    seen.lock().unwrap().clear();
    sink.send(3);

    let mut got = seen.lock().unwrap().clone();
    got.sort_unstable();
    assert_eq!(got, vec![4, 31, 104]);

    for l in [l1, l2, l3, l4] {
        l.unlisten();
    }
}

/// The `Mutex` inside `listen` is per-handler, so one handler being invoked
/// never blocks another, and a handler that fires repeatedly is unaffected.
#[test]
fn listener_handlers_do_not_contend_with_each_other() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let (a, mut na) = (Arc::new(Mutex::new(Vec::new())), 0);
    let (b, mut nb) = (Arc::new(Mutex::new(Vec::new())), 0);
    let (sa, sb) = (a.clone(), b.clone());

    let l1 = sink.stream().listen(move |v| {
        na += 1;
        sa.lock().unwrap().push((na, *v));
    });
    let l2 = sink.stream().listen(move |v| {
        nb += 1;
        sb.lock().unwrap().push((nb, *v));
    });

    for v in [7, 8, 9] {
        sink.send(v);
    }

    assert_eq!(*a.lock().unwrap(), vec![(1, 7), (2, 8), (3, 9)]);
    assert_eq!(*b.lock().unwrap(), vec![(1, 7), (2, 8), (3, 9)]);
    l1.unlisten();
    l2.unlisten();
}

// ---------------------------------------------------------------------------
// Part 3: what the `Fn` bound cost.
//
// A census, not a sample. The shapes a combinator can no longer take are
// exactly the shapes that own mutable state across calls, and there are five of
// them. `tests/ui/combinator_rejects_captured_state.rs` is that list as it was
// written before, with the errors it now produces.
//
// Four of the five are stateful *stream processing*, which Sodium already had a
// first-class way to express: `accum` and `collect` take the state as an
// argument and return the next one. Each test below is the rewrite, asserting
// the output the captured version produced.
// ---------------------------------------------------------------------------

/// Was: `let mut n = 0; map(move |_| { n += 1; n })` -- E0594.
#[test]
fn rewrite_counter() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let counted = sink.stream().collect(0, |_a, n| (*n + 1, *n + 1));

    let (out, l) = drain(&counted);
    for v in [7, 7, 8, 9] {
        sink.send(v);
    }
    assert_eq!(*out.lock().unwrap(), vec![1, 2, 3, 4]);
    l.unlisten();
}

/// Was: `let mut buf = Vec::new(); map(move |a| { buf.push(*a); .. })` -- E0596.
#[test]
fn rewrite_sliding_window() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let windowed = sink.stream().collect(Vec::<i32>::new(), |a, buf| {
        let mut next = buf.clone();
        next.push(*a);
        if next.len() > 3 {
            next.remove(0);
        }
        (next.clone(), next)
    });

    let (out, l) = drain(&windowed);
    for v in [7, 7, 8, 9] {
        sink.send(v);
    }
    assert_eq!(
        *out.lock().unwrap(),
        vec![vec![7], vec![7, 7], vec![7, 7, 8], vec![7, 8, 9]]
    );
    l.unlisten();
}

/// Was: `let mut prev = None; map(move |a| { .. prev = Some(*a); .. })` -- E0594.
#[test]
fn rewrite_edge_detect() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let changed = sink
        .stream()
        .collect(None::<i32>, |a, prev| (*prev != Some(*a), Some(*a)));

    let (out, l) = drain(&changed);
    for v in [7, 7, 8, 9] {
        sink.send(v);
    }
    assert_eq!(*out.lock().unwrap(), vec![true, false, true, true]);
    l.unlisten();
}

/// The recurrence behind `rewrite_random_number_generator`, run outside the
/// graph so the test asserts against an independent computation rather than
/// against itself.
fn lcg(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1)
}

/// Was: `let mut rng = Lcg(1); map(move |a| *a + rng.next() % 10)` -- E0596.
///
/// The most substantial entry in the census, since here the mutation was not
/// incidental: every RNG in the ecosystem advances through `&mut self`. It
/// still rewrites, by threading the seed rather than the generator, and the
/// sequence is identical because it is the same recurrence.
#[test]
fn rewrite_random_number_generator() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let jittered = sink.stream().collect(1u64, |a, seed| {
        let next = lcg(*seed);
        (*a + (next % 10) as i32, next)
    });

    let (out, l) = drain(&jittered);
    let inputs = [7, 7, 8, 9];
    for v in inputs {
        sink.send(v);
    }

    let mut seed = 1u64;
    let expected: Vec<i32> = inputs
        .iter()
        .map(|a| {
            seed = lcg(seed);
            *a + (seed % 10) as i32
        })
        .collect();

    assert_eq!(*out.lock().unwrap(), expected);
    l.unlisten();
}

/// The fifth shape, a memo cache, is the one with no `accum`/`collect` rewrite:
/// a cache is not part of the value being computed, so threading it through the
/// signature would put an implementation detail into the graph. It takes the
/// `Arc<Mutex<_>>` instead -- which is what it would have had to be anyway the
/// moment two combinators wanted to share the cache.
#[test]
fn rewrite_memo_cache_via_interior_mutability() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let cache: Arc<Mutex<HashMap<i32, i32>>> = Default::default();
    let (c, misses) = (cache.clone(), Arc::new(Mutex::new(0usize)));
    let m = misses.clone();
    let squared = sink.stream().map(move |a| {
        *c.lock().unwrap().entry(*a).or_insert_with(|| {
            *m.lock().unwrap() += 1;
            *a * *a
        })
    });

    let (out, l) = drain(&squared);
    for v in [7, 7, 8] {
        sink.send(v);
    }
    assert_eq!(*out.lock().unwrap(), vec![49, 49, 64]);
    assert_eq!(*misses.lock().unwrap(), 2, "the cache still caches");
    l.unlisten();
}

/// The rewrites are not merely workarounds -- they buy something. State
/// threaded through `accum` is a `Cell`: sampleable, composable, and visible to
/// the graph's dependency tracking. State captured in a closure is none of
/// those.
#[test]
fn rewrite_makes_the_state_first_class() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let total = sink.stream().accum(0, |a, t| *a + *t);
    let doubled = total.map(|t| *t * 2);

    sink.send(3);
    sink.send(4);

    assert_eq!(total.sample(), 7);
    assert_eq!(doubled.sample(), 14);
}

// ---------------------------------------------------------------------------
// Part 4: the argument from #48, measured.
//
// The case for keeping `FnMut` was a mutable collection updated in place:
//
//     let mut myEntities = HashMap::<Entity>::new();
//     let cEntities = sChange.map(move |change| { /* apply */; myEntities });
//
// -- `O(1)` per update instead of `O(log n)`, and claimed to be
// "indistinguishable from the immutable collection version in how it operates",
// because downstream nodes receive only a shared reference.
//
// The test of "indistinguishable" is whether a value handed to an observer in
// transaction N still reads, later, as it did in transaction N. That is the
// property an immutable collection gives for free. The two tests below take
// each variant of the trick and check exactly that.
//
// Both are written with interior mutability, because both have to be: a
// combinator's return type is owned (`B: Clone + Send + 'static`), so a closure
// can never lend out a borrow of its own state whatever the bound. That is why
// this section survived the change to `Fn` unaltered in substance -- the trick
// was never a `FnMut` capability.
// ---------------------------------------------------------------------------

/// Sizes recorded at event time, the values themselves, and the listener
/// keeping the probe alive.
type SizeProbe<M> = (Arc<Mutex<Vec<usize>>>, Arc<Mutex<Vec<M>>>, Listener);

/// Record, for each event, the collection's size *at the time of the event*,
/// and keep the value so the same measurement can be repeated at the end. If
/// the two disagree, values are changing under observers that already have
/// them.
fn size_probe<M, F>(s: &Stream<M>, size_of: F) -> SizeProbe<M>
where
    M: Clone + Send + 'static,
    F: Fn(&M) -> usize + Send + Sync + 'static,
{
    let at_event: Arc<Mutex<Vec<usize>>> = Default::default();
    let kept: Arc<Mutex<Vec<M>>> = Default::default();
    let (a, k) = (at_event.clone(), kept.clone());
    let l = s.listen(move |m| {
        a.lock().unwrap().push(size_of(m));
        k.lock().unwrap().push(m.clone());
    });
    (at_event, kept, l)
}

/// Variant A: return the collection by value, as the #48 sketch does.
///
/// Semantically correct -- past values do not move. But the collection has to
/// be copied out on every event to produce the owned return value, which is
/// `O(n)`: worse than the `O(log n)` the trick was introduced to beat.
#[test]
fn claim_owned_collection_is_correct_but_copies() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<(String, i32)>();

    let table: Arc<Mutex<HashMap<String, i32>>> = Default::default();
    let t = table.clone();
    let updated = sink.stream().map(move |(k, v): &(String, i32)| {
        let mut table = t.lock().unwrap();
        table.insert(k.clone(), *v);
        table.clone() // <-- the O(n) the trick was meant to avoid
    });

    let (at_event, kept, l) = size_probe(&updated, |m: &HashMap<String, i32>| m.len());

    sink.send(("a".into(), 1));
    sink.send(("b".into(), 2));
    sink.send(("c".into(), 3));

    let later: Vec<usize> = kept.lock().unwrap().iter().map(|m| m.len()).collect();
    assert_eq!(*at_event.lock().unwrap(), vec![1, 2, 3]);
    assert_eq!(later, vec![1, 2, 3], "past values must not move");
    l.unlisten();
}

/// Variant B: the `O(1)` version. Hand downstream a shared handle to the one
/// live collection, so nothing is copied.
///
/// This is what the trick has to become once the return type is owned -- a
/// conclusion clinuxrulz reached himself in #48, immediately before saying he
/// would be happy to move to `Fn`. It is `O(1)`, and it breaks the property the
/// trick was claimed to preserve: every observer ends up looking at the final
/// state, whatever transaction it observed in.
#[test]
fn claim_shared_collection_is_o1_but_values_drift() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<(String, i32)>();

    let table: Arc<Mutex<HashMap<String, i32>>> = Default::default();
    let t = table.clone();
    let updated = sink.stream().map(move |(k, v): &(String, i32)| {
        t.lock().unwrap().insert(k.clone(), *v);
        t.clone() // O(1): clones the Arc, not the map
    });

    let (at_event, kept, l) = size_probe(&updated, |m: &Arc<Mutex<HashMap<String, i32>>>| {
        m.lock().unwrap().len()
    });

    sink.send(("a".into(), 1));
    sink.send(("b".into(), 2));
    sink.send(("c".into(), 3));

    let later: Vec<usize> = kept
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.lock().unwrap().len())
        .collect();

    assert_eq!(*at_event.lock().unwrap(), vec![1, 2, 3]);
    assert_eq!(
        later,
        vec![3, 3, 3],
        "the first two observers' values grew after they received them"
    );
    l.unlisten();
}

// ---------------------------------------------------------------------------
// Part 5: does state kept outside the graph stay in step with it?
//
// A combinator closure's `Arc<Mutex<_>>` is invisible to Sodium, so it advances
// exactly as often as the node happens to be evaluated. Whether that matches
// "once per event" is an implementation property, not a guarantee -- worth
// pinning down both as a check on the evaluator and as a regression guard for
// anyone relying on the count.
// ---------------------------------------------------------------------------

/// A node with two dependents fires its update once per transaction, not once
/// per dependent.
///
/// The specific guard here is on the lock: a node's update is `dyn Fn` and is
/// fired under a *shared* lock, so nothing about being reachable twice forces
/// -- or prevents -- a second evaluation. If the diamond ever started
/// double-firing, a combinator closure counting its own calls would silently
/// disagree with the event count.
#[test]
fn state_fires_once_for_a_node_with_two_dependents() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let calls = Arc::new(Mutex::new(0usize));
    let c = calls.clone();
    let shared = sink.stream().map(move |a| {
        *c.lock().unwrap() += 1;
        *a
    });

    let (left, l1) = drain(&shared.map(|a| *a + 1));
    let (right, l2) = drain(&shared.map(|a| *a + 100));

    sink.send(1);
    sink.send(2);

    assert_eq!(*calls.lock().unwrap(), 2, "two events, two evaluations");
    assert_eq!(*left.lock().unwrap(), vec![2, 3]);
    assert_eq!(*right.lock().unwrap(), vec![101, 102]);
    l1.unlisten();
    l2.unlisten();
}

/// Invocations track events one-for-one, and adding a listener does not re-run
/// an upstream closure. Note the second assertion: a combinator closure runs
/// whether or not anything is listening.
#[test]
fn state_advances_once_per_event() {
    let ctx = SodiumCtx::new();
    let csink = ctx.new_cell_sink(0i32);

    let calls = Arc::new(Mutex::new(0usize));
    let c = calls.clone();
    let mapped: Cell<usize> = csink.cell().map(move |_a| {
        let mut n = c.lock().unwrap();
        *n += 1;
        *n
    });

    assert_eq!(*calls.lock().unwrap(), 0, "not run before any send");

    csink.send(1);
    csink.send(2);
    assert_eq!(*calls.lock().unwrap(), 2, "runs unlistened");

    let l = mapped.listen(|_| {});
    assert_eq!(
        *calls.lock().unwrap(),
        2,
        "adding a listener does not re-run it"
    );

    csink.send(3);
    assert_eq!(*calls.lock().unwrap(), 3);
    assert_eq!(mapped.sample(), 3);
    l.unlisten();
}

/// `Operational::value` re-delivers a cell's current value to each new
/// listener. That happens downstream of the map, so upstream state is not
/// disturbed by it.
#[test]
fn state_undisturbed_by_operational_value() {
    let ctx = SodiumCtx::new();
    let csink = ctx.new_cell_sink(0i32);

    let calls = Arc::new(Mutex::new(0usize));
    let c = calls.clone();
    let mapped = csink.cell().map(move |a| {
        *c.lock().unwrap() += 1;
        *a
    });

    csink.send(1);
    let before = *calls.lock().unwrap();

    let l1 = Operational::value(&mapped).listen(|_| {});
    let l2 = Operational::value(&mapped).listen(|_| {});

    assert_eq!(*calls.lock().unwrap(), before);
    l1.unlisten();
    l2.unlisten();
}

/// The one place the two clocks visibly separate: a stream switched away from
/// keeps firing, so state hanging off it keeps advancing while nothing
/// downstream can see it. Downstream observes 1 then 4 -- events 2 and 3
/// happened to the closure but not to the observer.
///
/// Defensible (the source really did fire four times), but it is the shape of
/// surprise that out-of-band state invites and threaded state does not: with
/// `accum` the skipped values are still a `Cell` anyone can sample.
#[test]
fn state_advances_while_switched_away() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let calls = Arc::new(Mutex::new(0usize));
    let c = calls.clone();
    let counted = sink.stream().map(move |_a| {
        let mut n = c.lock().unwrap();
        *n += 1;
        *n
    });

    let elsewhere = ctx.new_stream_sink::<usize>().stream();
    let which = ctx.new_cell_sink(counted.clone());
    let selected = which.cell();
    let switched = Cell::switch_s(&selected);
    let (out, l) = drain(&switched);

    sink.send(1); // routed through `counted`
    which.send(elsewhere); // switch away
    sink.send(2); // `counted` still fires; nobody downstream sees it
    sink.send(3);
    which.send(counted); // switch back
    sink.send(4);

    assert_eq!(*out.lock().unwrap(), vec![1, 4]);
    assert_eq!(*calls.lock().unwrap(), 4);
    l.unlisten();
}

// ---------------------------------------------------------------------------
//
// FINDINGS
// ========
//
// Recorded here because the question in #48 was a design question, and the
// tests above are only evidence if what they were weighing is written down next
// to them.
//
// 1. The bound was load-bearing in exactly one place.
//
//    Before the change, `src/stream.rs` and `src/cell.rs` carried 46 `FnMut`
//    bounds: 38 on combinators, 8 on the listeners. Nothing in this repository
//    -- 52 unit tests, the integration tests, the benchmarks, `coz-driver` --
//    passed a mutable-capture closure to a combinator. The benchmarks passed 18
//    of them to `listen`.
//
//    So the two halves of the API wanted different answers, which is what
//    RadicalZephyr proposed in the opening comment of #48: `Fn` for the
//    combinators, a mutable bound for the effectful edge. That is what the API
//    now has, and Parts 1 and 2 are the two halves.
//
// 2. Type inference does not depend on the choice.
//
//    `Fn` and `FnMut` are both in the family rustc deduces closure signatures
//    from, so everything `tests/closure_type_inference.rs` guards holds either
//    way. The bound change and the inference fix were independent.
//
// 3. The argument that stalled the thread does not survive being measured.
//
//    Part 4 is the O(1)-collection claim. Returned by value it is correct and
//    `O(n)` -- worse than the immutable collection it was meant to beat, since
//    a combinator's return type is owned and a closure cannot lend out its own
//    state. Returned as a shared handle it is `O(1)` and its values drift: in
//    `claim_shared_collection_is_o1_but_values_drift` the observers of events 1
//    and 2 both end up holding a three-entry map. That is precisely the
//    "indistinguishable from the immutable version" property the trick claimed.
//
//    Neither result depended on the bound. Both variants are `Fn` and always
//    were -- the constraint that defeats the trick is the owned return type,
//    which nothing in #48 proposed changing.
//
// 4. What the `Fn` bound cost was five closure shapes, four of which rewrite
//    exactly.
//
//    Part 3 is the bill, and `tests/ui/combinator_rejects_captured_state.rs` is
//    the itemisation. Counter, window, edge-detect and RNG all thread through
//    `accum`/`collect` with the output their captured versions produced, and
//    come out better for it: `rewrite_makes_the_state_first_class` shows the
//    state becoming a `Cell` that can be sampled and composed, where a capture
//    is visible to nobody. Only the memo cache has no natural rewrite, and it
//    is a cache -- an implementation detail that does not belong in the graph,
//    and that wants `Arc<Mutex<_>>` the moment it is shared.
//
// 5. There was a reason to prefer `Fn` beyond taste.
//
//    Each node's update used to be stored as
//    `RwLock<Box<dyn FnMut() + Send + Sync>>`, so firing one took a *write*
//    lock. A `Fn` graph makes that a read lock. `SodiumCtx` already carries a
//    `ThreadedMode` and a `TODO` for a thread-pool mode, and a per-node
//    exclusive lock on every fire was what stood between that scaffolding and
//    an evaluator that can run independent nodes at once.
//
//    Still prospective, not a present win: `simple_threaded_mode` spawns and
//    immediately joins, so today's evaluator is serialized and the lock is
//    uncontended either way.
//
// 6. `listen` keeps `FnMut` without the graph keeping it.
//
//    The two halves did not have to be traded off. A `Mutex` at the API
//    boundary absorbs a handler's mutability -- `listen` accepts `FnMut`, wraps
//    it, and hands the graph a `Fn` -- which left the internals free to be
//    `dyn Fn` and kept all 18 benchmark handlers compiling untouched. Part 2
//    is the check that this is real and not merely type-level.
// ---------------------------------------------------------------------------
