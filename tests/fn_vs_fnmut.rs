//! What the `FnMut` bound on the combinators actually buys, measured.
//!
//! Sodium's combinators are bounded on `FnMut`. [Issue #48] proposes moving
//! them to `Fn`, on the grounds that a combinator lambda is supposed to be a
//! pure function and `Fn` is the bound that says so. The thread stalled on a
//! counter-argument: `Stream::map` is deliberately allowed to mutate, because
//! mutating a collection in place is `O(1)` where rebuilding an immutable one
//! is `O(log n)`, and Sodium's semantics are claimed to survive that.
//!
//! Nothing in the thread was measured. These tests are the measurement. They
//! run against the current `FnMut` API, so everything here compiles today; what
//! they establish is which parts would *still* compile if the bound were `Fn`,
//! and what the parts that would not are actually worth.
//!
//! * `either_*` -- uses no captured mutable state. Unaffected by the bound.
//! * `fnmut_only_*` -- would stop compiling under `Fn`. Each one names the
//!   error it would get and, where there is one, the rewrite that avoids it.
//! * `rewrite_*` -- the `Fn`-clean equivalents, asserted to produce identical
//!   output to the `FnMut` originals above them.
//! * `claim_*` -- the `O(1)` collection argument from #48, put on a scale.
//!
//! The compile-time half lives in `tests/ui/fn_bound_accepts_shared_state.rs`
//! and `tests/ui/fn_bound_rejects_captured_state.rs`, which pin down what a
//! `Fn` bound does and does not admit without needing a `Fn`-bounded build of
//! this crate to point at.
//!
//! `FINDINGS` at the bottom of this file collects the conclusions.
//!
//! [Issue #48]: https://github.com/SodiumFRP/sodium-rust/issues/48

use sodium_rust::{Cell, Listener, Operational, SodiumCtx, Stream};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Test scaffolding: drain a stream into a vector.
///
/// Note this handler is itself a mutating closure, and stays legal under any
/// proposal in #48: the argument there is about the combinators, not about
/// `listen`, which is the designated place for effects.
fn drain<A: Clone + Send + 'static>(s: &Stream<A>) -> (Arc<Mutex<Vec<A>>>, Listener) {
    let out: Arc<Mutex<Vec<A>>> = Default::default();
    let sunk = out.clone();
    let l = s.listen(move |a| sunk.lock().unwrap().push(a.clone()));
    (out, l)
}

// ---------------------------------------------------------------------------
// Part 1: the surface that does not care which bound it is.
//
// `Fn` is a subtrait of `FnMut`, so anything that satisfies `Fn` already
// satisfies the bound we have. Everything below would compile unchanged.
// ---------------------------------------------------------------------------

/// Ordinary pure combinator closures -- by inspection the overwhelming majority
/// of real call sites, and the whole of this crate's own test suite.
#[test]
fn either_pure_closures() {
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
/// -- is `Fn`-clean as well.
#[test]
fn either_closures_capturing_frp_nodes() {
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

/// Interior mutability. A `Fn` bound is not a purity proof and never claimed to
/// be -- `Arc<Mutex<_>>` walks straight through it. What `Fn` changes is that
/// reaching for one is a visible act rather than the default.
#[test]
fn either_interior_mutability() {
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
/// through a capture, so stateful stream processing is already expressible
/// without any mutable capture at all. This matters for Part 3.
#[test]
fn either_state_threaded_through_the_signature() {
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
// Part 2: the closures a `Fn` bound would reject.
//
// A census, not a sample: these are the shapes that need `FnMut`, which is to
// say the shapes that own mutable state across calls. Each compiles today.
// Under `Fn` each fails to *build the closure*, at the point of mutation --
// E0594 for an assignment to a captured binding, E0596 for a `&mut` borrow of
// one -- rather than at the call to the combinator.
//
// `tests/ui/fn_bound_rejects_captured_state.rs` holds those diagnostics.
// ---------------------------------------------------------------------------

/// (1) A counter. Under `Fn`: E0594, cannot assign to `n`.
#[test]
fn fnmut_only_counter() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let mut n = 0;
    let counted = sink.stream().map(move |_a| {
        n += 1;
        n
    });

    let (out, l) = drain(&counted);
    for v in [7, 7, 8, 9] {
        sink.send(v);
    }
    assert_eq!(*out.lock().unwrap(), vec![1, 2, 3, 4]);
    l.unlisten();
}

/// (2) A sliding window. Under `Fn`: E0596, cannot borrow `buf` as mutable.
#[test]
fn fnmut_only_sliding_window() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let mut buf: Vec<i32> = Vec::new();
    let windowed = sink.stream().map(move |a| {
        buf.push(*a);
        if buf.len() > 3 {
            buf.remove(0);
        }
        buf.clone()
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

/// (3) Remembering the previous value. Under `Fn`: E0594.
#[test]
fn fnmut_only_edge_detect() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let mut prev: Option<i32> = None;
    let changed = sink.stream().map(move |a| {
        let is_new = prev != Some(*a);
        prev = Some(*a);
        is_new
    });

    let (out, l) = drain(&changed);
    for v in [7, 7, 8, 9] {
        sink.send(v);
    }
    assert_eq!(*out.lock().unwrap(), vec![true, false, true, true]);
    l.unlisten();
}

/// (4) Anything driving a `&mut self` API -- here a linear congruential
/// generator, standing in for every RNG in the ecosystem, all of which take
/// `&mut self` to produce a value. Under `Fn`: E0596.
///
/// The most persuasive entry in the census, because unlike (1)-(3) the
/// mutation is not incidental: it is how the type is meant to be used.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
}

#[test]
fn fnmut_only_random_number_generator() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let mut rng = Lcg(1);
    let jittered = sink.stream().map(move |a| *a + (rng.next() % 10) as i32);

    let (out, l) = drain(&jittered);
    for v in [7, 7, 8, 9] {
        sink.send(v);
    }
    assert_eq!(out.lock().unwrap().len(), 4);
    l.unlisten();
}

/// (5) A memo cache owned by the closure. Under `Fn`: E0596.
///
/// The one entry in the census with no `accum`/`collect` rewrite. A cache is
/// not part of the value being computed, so threading it through the signature
/// would put an implementation detail into the graph. Under `Fn` this becomes
/// `Arc<Mutex<HashMap<..>>>` -- which is what it would have to be anyway the
/// moment two combinators wanted to share the cache.
#[test]
fn fnmut_only_memo_cache() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<i32>();

    let mut cache: HashMap<i32, i32> = HashMap::new();
    let squared = sink
        .stream()
        .map(move |a| *cache.entry(*a).or_insert_with(|| *a * *a));

    let (out, l) = drain(&squared);
    for v in [7, 7, 8] {
        sink.send(v);
    }
    assert_eq!(*out.lock().unwrap(), vec![49, 49, 64]);
    l.unlisten();
}

/// (6) A stateful *listener*. Under `Fn`: E0594.
///
/// Listed separately from (1)-(5) because it is the one place in the census
/// where mutation is not a workaround for anything -- `listen` is the effectful
/// edge of the graph, and its handler is supposed to do effectful things.
///
/// This crate's own `benches/sodium.rs` is made of these: 18 handlers, every
/// one of them `move |v| values.push(*v)`. They are also the only 18 sites in
/// this repository that a blanket `Fn` bound would break.
#[test]
fn fnmut_only_stateful_listener() {
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

// ---------------------------------------------------------------------------
// Part 3: what the census costs.
//
// Five of the six entries above are stateful *stream processing*, which Sodium
// already has a first-class way to express: `accum` and `collect` take the
// state as an argument and return the next one, so no capture is mutated and
// the closures are `Fn`. Each test below asserts the rewrite is not merely
// possible but produces exactly the output its `fnmut_only_*` counterpart did.
// ---------------------------------------------------------------------------

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

/// Even the RNG rewrites, by threading the generator's seed instead of the
/// generator. The sequence is identical to `fnmut_only_random_number_generator`
/// because it is the same recurrence -- asserted here rather than eyeballed.
#[test]
fn rewrite_random_number_generator() {
    let ctx = SodiumCtx::new();

    let captured = {
        let sink = ctx.new_stream_sink::<i32>();
        let mut rng = Lcg(1);
        let s = sink.stream().map(move |a| *a + (rng.next() % 10) as i32);
        let (out, l) = drain(&s);
        for v in [7, 7, 8, 9] {
            sink.send(v);
        }
        let got = out.lock().unwrap().clone();
        l.unlisten();
        got
    };

    let threaded = {
        let sink = ctx.new_stream_sink::<i32>();
        let s = sink.stream().collect(1u64, |a, seed| {
            let next = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (*a + (next % 10) as i32, next)
        });
        let (out, l) = drain(&s);
        for v in [7, 7, 8, 9] {
            sink.send(v);
        }
        let got = out.lock().unwrap().clone();
        l.unlisten();
        got
    };

    assert_eq!(captured, threaded);
}

/// The rewrite is not merely a workaround -- it buys something. State threaded
/// through `accum` is a `Cell`: sampleable, composable, and visible to the
/// graph's dependency tracking. State captured in a closure is none of those.
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
/// Semantically correct -- past values do not move. But the return type of a
/// combinator is owned (`B: Clone + Send + 'static`), so a closure cannot hand
/// out a borrow of its own state and the collection is cloned on every event.
/// That is `O(n)`, which is worse than the `O(log n)` the trick was introduced
/// to beat.
#[test]
fn claim_owned_collection_is_correct_but_copies() {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<(String, i32)>();

    let mut table: HashMap<String, i32> = HashMap::new();
    let updated = sink.stream().map(move |(k, v): &(String, i32)| {
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
///
/// Note what this test does *not* depend on: `t` is reached through a shared
/// reference, so this closure is `Fn`. The trick's failure is a consequence of
/// the owned return type, and a `Fn` bound would neither cause it nor fix it.
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
// Part 5: does captured state stay in step with the graph?
//
// A `FnMut` closure's state is invisible to Sodium, so it advances exactly as
// often as the node happens to be evaluated. Whether that matches "once per
// event" is an implementation property, not a guarantee -- so it is worth
// pinning down, both as a check on the current evaluator and as a regression
// guard for anyone who relies on the count.
// ---------------------------------------------------------------------------

/// Invocations track events one-for-one, and adding a listener does not
/// re-run an upstream closure. Note the third assertion: a combinator closure
/// runs whether or not anything is listening, which is what makes the #48
/// side-effect trick viable at all.
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
/// listener. That happens downstream of the map, so upstream captured state is
/// not disturbed by it.
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
/// keeps firing, so its captured state keeps advancing while nothing downstream
/// can see it. Downstream observes 1 then 4 -- events 2 and 3 happened to the
/// closure but not to the observer.
///
/// This is defensible (the source really did fire four times) but it is the
/// shape of surprise that captured state invites and threaded state does not:
/// with `accum` the skipped values are still a `Cell` anyone can sample.
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
// Recorded here because the question in #48 is a design question, and the tests
// above are only evidence if what they were weighing is written down next to
// them.
//
// 1. The bound is load-bearing in exactly one place.
//
//    `src/stream.rs` and `src/cell.rs` carry 46 `FnMut` bounds: 38 on
//    combinators, 8 on `listen`/`listen_weak` and their `*_with_deps` siblings.
//    Nothing in this repository -- 52 unit tests, the integration tests, the
//    benchmarks, `coz-driver` -- passes a mutable-capture closure to a
//    combinator. The benchmarks pass 18 of them to `listen`.
//
//    So the two halves of the API want different answers, which is what
//    RadicalZephyr proposed in the opening comment of #48: `Fn` for the
//    combinators, a separate mutable bound for the effectful edge.
//
// 2. Type inference does not depend on the choice.
//
//    `Fn` and `FnMut` are both in the family rustc deduces closure signatures
//    from, so everything `tests/closure_type_inference.rs` guards holds either
//    way. The bound change and the inference fix are independent.
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
//    Neither result depends on the bound. Variant B's closure is already `Fn`.
//    The constraint that defeats the trick is the owned return type, which no
//    proposal in #48 changes.
//
// 4. What a `Fn` bound would actually cost is five closure shapes, four of
//    which rewrite exactly.
//
//    Part 2 is the census and Part 3 is the bill. Counter, window, edge-detect
//    and RNG all thread through `accum`/`collect` with identical output, and
//    come out better for it: `rewrite_makes_the_state_first_class` shows the
//    state becoming a `Cell` that can be sampled and composed, where a capture
//    is visible to nobody. Only the memo cache has no natural rewrite, and it
//    is a cache -- an implementation detail that does not belong in the graph,
//    and that wants `Arc<Mutex<_>>` the moment it is shared.
//
// 5. There is a reason to prefer `Fn` beyond taste.
//
//    `src/impl_/node.rs` stores each node's update as
//    `RwLock<Box<dyn FnMut() + Send + Sync>>`, and firing one takes a *write*
//    lock (`src/impl_/sodium_ctx.rs`, in `update_node`). A `Fn` graph makes
//    that a read lock. `SodiumCtx` already carries a `ThreadedMode` and a
//    `TODO` for a thread-pool mode, and a per-node exclusive lock on every fire
//    is what stands between that scaffolding and an evaluator that can run
//    independent nodes at once.
//
//    This is prospective, not a present win: the `simple_threaded_mode` in the
//    tree spawns and immediately joins, so today's evaluator is serialized and
//    the lock is uncontended either way.
//
// 6. `listen` can keep `FnMut` without the graph keeping it.
//
//    The two halves do not have to be traded off. A `Mutex` at the API boundary
//    absorbs a handler's mutability -- `listen` accepts `FnMut`, wraps it, and
//    hands the graph a `Fn` -- which leaves the internals free to be `dyn Fn`
//    and keeps all 18 benchmark handlers compiling untouched.
//
// Together: a `Fn` bound on the combinators is a real breaking change and a
// small one. It costs five closure shapes, four of which have better
// replacements already in the API, and the use case that blocked it in 2020 is
// broken on its own terms under either bound.
// ---------------------------------------------------------------------------
