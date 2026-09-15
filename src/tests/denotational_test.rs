//! Ports of the .NET binding's `DenotationalSemanticsTests`.
//!
//! Each test drives the network through a simulation: streams are given
//! a schedule of `(time, value)` firings, and the harness runs one
//! transaction per time step from 0 up to the last scheduled firing,
//! with the listener attached inside the transaction for time 0. Tests
//! whose result should not depend on the order the sinks are sent within
//! a transaction run under [`run_permutations`], which tries every
//! ordering.

use crate::{Cell, Lazy, Listener, SodiumCtx, Stream};

use std::sync::{Arc, Mutex};

type Out<A> = Arc<Mutex<Vec<A>>>;

/// The scheduled sends for one stream.
struct Firings {
    entries: Vec<(i32, Box<dyn Fn()>)>,
}

impl Firings {
    fn fire(&self, t: i32) {
        for (time, send) in &self.entries {
            if *time == t {
                send();
            }
        }
    }

    fn max_time(&self) -> Option<i32> {
        self.entries.iter().map(|(t, _)| *t).max()
    }
}

/// A stream sink wired up to fire `firings` on the simulation clock.
fn mk_stream<A: Clone + Send + 'static>(
    sodium_ctx: &SodiumCtx,
    firings: &[(i32, A)],
) -> (Stream<A>, Firings) {
    let s = sodium_ctx.new_stream_sink::<A>();
    let entries = firings
        .iter()
        .map(|(t, v)| {
            assert!(*t >= 0, "all firings must occur at t >= 0");
            let s = s.clone();
            let v = v.clone();
            let send: Box<dyn Fn()> = Box::new(move || s.send(v.clone()));
            (*t, send)
        })
        .collect();
    (s.stream(), Firings { entries })
}

/// As [`mk_stream`], but with a combining function, so that more than
/// one value can be scheduled at the same time.
fn mk_stream_coalescing<A: Clone + Send + 'static, F: FnMut(&A, &A) -> A + Send + 'static>(
    sodium_ctx: &SodiumCtx,
    firings: &[(i32, A)],
    coalesce: F,
) -> (Stream<A>, Firings) {
    let s = sodium_ctx.new_stream_sink_with_coalescer(coalesce);
    let entries = firings
        .iter()
        .map(|(t, v)| {
            assert!(*t >= 0, "all firings must occur at t >= 0");
            let s = s.clone();
            let v = v.clone();
            let send: Box<dyn Fn()> = Box::new(move || s.send(v.clone()));
            (*t, send)
        })
        .collect();
    (s.stream(), Firings { entries })
}

/// Attach the listener, then run one transaction per time step, and
/// return everything the listener saw.
fn run_simulation<A, L>(sodium_ctx: &SodiumCtx, listen: L, firings: &[Firings]) -> Vec<A>
where
    A: Clone + Send + 'static,
    L: FnOnce(Out<A>) -> Listener,
{
    let out: Out<A> = Arc::new(Mutex::new(Vec::new()));
    let max_time = firings.iter().filter_map(Firings::max_time).max();
    let run = |t: i32| {
        for f in firings {
            f.fire(t);
        }
    };
    let l = match max_time {
        Some(max) => {
            let l = sodium_ctx.transaction(|| {
                let l = listen(out.clone());
                run(0);
                l
            });
            for t in 1..=max {
                sodium_ctx.transaction(|| run(t));
            }
            l
        }
        None => listen(out.clone()),
    };
    l.unlisten();
    let lock = out.lock();
    let out: &Vec<A> = lock.as_ref().unwrap();
    out.clone()
}

/// Record `a` into `out`.
fn record<A: Clone + Send + 'static>(out: Out<A>) -> impl FnMut(&A) + Send + Sync + 'static {
    move |a: &A| out.lock().as_mut().unwrap().push(a.clone())
}

/// Run `build` once per ordering of the streams it names, asserting that
/// the result does not depend on which sink is sent first within a
/// transaction. `build` gets the ordering to use and returns the named
/// firings in that order along with the listener to attach.
fn run_permutations<A, B, F>(names: usize, mut build: B, assert: F)
where
    A: Clone + Send + 'static,
    B: FnMut(
        &SodiumCtx,
        &[usize],
    ) -> (
        Vec<(&'static str, Firings)>,
        Box<dyn FnOnce(Out<A>) -> Listener>,
    ),
    F: Fn(&[A]),
{
    for order in permutations(&(0..names).collect::<Vec<usize>>()) {
        let sodium_ctx = SodiumCtx::new();
        let (firings, listen) = build(&sodium_ctx, &order);
        let names: Vec<&'static str> = firings.iter().map(|(name, _)| *name).collect();
        let firings: Vec<Firings> = firings.into_iter().map(|(_, f)| f).collect();
        let out = run_simulation(&sodium_ctx, listen, &firings);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| assert(&out))).unwrap_or_else(
            |e| {
                println!("failed for ordering {{ {} }}", names.join(", "));
                std::panic::resume_unwind(e)
            },
        );
    }
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(i);
        for mut tail in permutations(&rest) {
            let mut perm = vec![*item];
            perm.append(&mut tail);
            out.push(perm);
        }
    }
    out
}

#[test]
fn never() {
    let sodium_ctx = SodiumCtx::new();
    let s: Stream<i32> = Stream::new(&sodium_ctx);
    let out = run_simulation(&sodium_ctx, |o| s.listen(record(o)), &[]);
    assert_eq!(Vec::<i32>::new(), out);
}

#[test]
fn map_s() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(0, 5), (1, 10), (2, 12)]);
    let out = run_simulation(
        &sodium_ctx,
        |o| s.map(|x: &i32| x + 1).listen(record(o)),
        &[sf],
    );
    assert_eq!(vec![6, 11, 13], out);
}

#[test]
fn snapshot() {
    let sodium_ctx = SodiumCtx::new();
    let (s1, s1f) = mk_stream(&sodium_ctx, &[(0, 'a'), (3, 'b'), (5, 'c')]);
    let (s2, s2f) = mk_stream(&sodium_ctx, &[(1, 4), (5, 7)]);
    let c = s2.hold(3);
    let out = run_simulation(
        &sodium_ctx,
        |o| s1.snapshot1(&c).listen(record(o)),
        &[s1f, s2f],
    );
    assert_eq!(vec![3, 4, 4], out);
}

#[test]
fn merge() {
    let sodium_ctx = SodiumCtx::new();
    let (s1, s1f) = mk_stream(&sodium_ctx, &[(0, 0), (2, 2)]);
    let (s2, s2f) = mk_stream(&sodium_ctx, &[(1, 10), (2, 20), (3, 30)]);
    let out = run_simulation(
        &sodium_ctx,
        |o| s1.merge(&s2, |x: &i32, y: &i32| x + y).listen(record(o)),
        &[s1f, s2f],
    );
    assert_eq!(vec![0, 10, 22, 30], out);
}

#[test]
fn filter() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(0, 5), (1, 6), (2, 7)]);
    let out = run_simulation(
        &sodium_ctx,
        |o| s.filter(|x: &i32| x % 2 != 0).listen(record(o)),
        &[sf],
    );
    assert_eq!(vec![5, 7], out);
}

#[test]
fn switch_s() {
    run_permutations(
        3,
        |sodium_ctx, order| {
            let (s1, s1f) = mk_stream(sodium_ctx, &[(0, 'a'), (1, 'b'), (2, 'c'), (3, 'd')]);
            let (s2, s2f) = mk_stream(sodium_ctx, &[(0, 'W'), (1, 'X'), (2, 'Y'), (3, 'Z')]);
            let (switcher, switcher_f) = mk_stream(sodium_ctx, &[(1, s2)]);
            let c = switcher.hold(s1);
            let mut firings = vec![("s1", s1f), ("s2", s2f), ("switcher", switcher_f)];
            let firings = reorder(&mut firings, order);
            (
                firings,
                Box::new(move |o| Cell::switch_s(&c).listen(record(o))),
            )
        },
        |out| assert_eq!(['a', 'b', 'Y', 'Z'], out),
    );
}

#[test]
fn updates() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(1, 'b'), (3, 'c')]);
    let c = s.hold('a');
    let out = run_simulation(&sodium_ctx, |o| c.updates().listen(record(o)), &[sf]);
    assert_eq!(vec!['b', 'c'], out);
}

#[test]
fn value1() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(1, 'b'), (3, 'c')]);
    let c = s.hold('a');
    let out = run_simulation(&sodium_ctx, |o| c.value().listen(record(o)), &[sf]);
    assert_eq!(vec!['a', 'b', 'c'], out);
}

// The cell is updated in the same transaction the listener is attached
// in, so the initial value is never seen.
#[test]
fn value2() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(0, 'b'), (1, 'c'), (3, 'd')]);
    let c = s.hold('a');
    let out = run_simulation(&sodium_ctx, |o| c.value().listen(record(o)), &[sf]);
    assert_eq!(vec!['b', 'c', 'd'], out);
}

#[test]
fn listen_c1() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(1, 'b'), (3, 'c')]);
    let c = s.hold('a');
    let out = run_simulation(&sodium_ctx, |o| c.listen(record(o)), &[sf]);
    assert_eq!(vec!['a', 'b', 'c'], out);
}

#[test]
fn listen_c2() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(0, 'b'), (1, 'c'), (3, 'd')]);
    let c = s.hold('a');
    let out = run_simulation(&sodium_ctx, |o| c.listen(record(o)), &[sf]);
    assert_eq!(vec!['b', 'c', 'd'], out);
}

#[test]
fn split() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream_coalescing(
        &sodium_ctx,
        &[(0, vec!['a', 'b']), (1, vec!['c']), (1, vec!['d', 'e'])],
        |x: &Vec<char>, y: &Vec<char>| x.iter().chain(y.iter()).copied().collect(),
    );
    let out = run_simulation(&sodium_ctx, |o| s.split().listen(record(o)), &[sf]);
    assert_eq!(vec!['a', 'b', 'c', 'd', 'e'], out);
}

#[test]
fn constant() {
    let sodium_ctx = SodiumCtx::new();
    let c = sodium_ctx.new_cell('a');
    let out = run_simulation(&sodium_ctx, |o| c.listen(record(o)), &[]);
    assert_eq!(vec!['a'], out);
}

#[test]
fn constant_lazy() {
    let sodium_ctx = SodiumCtx::new();
    let c = Stream::new(&sodium_ctx).hold_lazy(Lazy::new(|| 'a'));
    let out = run_simulation(&sodium_ctx, |o| c.listen(record(o)), &[]);
    assert_eq!(vec!['a'], out);
}

#[test]
fn hold() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(1, 'b'), (3, 'c')]);
    let c = s.hold('a');
    let out = run_simulation(&sodium_ctx, |o| c.listen(record(o)), &[sf]);
    assert_eq!(vec!['a', 'b', 'c'], out);
}

#[test]
fn map_c() {
    let sodium_ctx = SodiumCtx::new();
    let (s, sf) = mk_stream(&sodium_ctx, &[(2, 3), (3, 5)]);
    let c = s.hold(0);
    let out = run_simulation(
        &sodium_ctx,
        |o| c.map(|x: &i32| x + 1).listen(record(o)),
        &[sf],
    );
    assert_eq!(vec![1, 4, 6], out);
}

#[test]
fn switch_c1() {
    run_permutations(
        3,
        |sodium_ctx, order| {
            let (s1, s1f) = mk_stream(sodium_ctx, &[(0, 'b'), (1, 'c'), (2, 'd'), (3, 'e')]);
            let c1 = s1.hold('a');
            let (s2, s2f) = mk_stream(sodium_ctx, &[(0, 'W'), (1, 'X'), (2, 'Y'), (3, 'Z')]);
            let c2 = s2.hold('V');
            let (switcher, switcher_f) = mk_stream(sodium_ctx, &[(1, c2)]);
            let c = switcher.hold(c1);
            let mut firings = vec![("s1", s1f), ("s2", s2f), ("switcher", switcher_f)];
            let firings = reorder(&mut firings, order);
            (
                firings,
                Box::new(move |o| Cell::switch_c(&c).listen(record(o))),
            )
        },
        |out| assert_eq!(['b', 'X', 'Y', 'Z'], out),
    );
}

#[test]
fn switch_c2() {
    run_permutations(
        3,
        |sodium_ctx, order| {
            let (s1, s1f) = mk_stream(sodium_ctx, &[(0, 'b'), (1, 'c'), (2, 'd'), (3, 'e')]);
            let c1 = s1.hold('a');
            let (s2, s2f) = mk_stream(sodium_ctx, &[(1, 'X'), (2, 'Y'), (3, 'Z')]);
            let c2 = s2.hold('W');
            let (switcher, switcher_f) = mk_stream(sodium_ctx, &[(1, c2)]);
            let c = switcher.hold(c1);
            let mut firings = vec![("s1", s1f), ("s2", s2f), ("switcher", switcher_f)];
            let firings = reorder(&mut firings, order);
            (
                firings,
                Box::new(move |o| Cell::switch_c(&c).listen(record(o))),
            )
        },
        |out| assert_eq!(['b', 'X', 'Y', 'Z'], out),
    );
}

#[test]
fn switch_c3() {
    run_permutations(
        3,
        |sodium_ctx, order| {
            let (s1, s1f) = mk_stream(sodium_ctx, &[(0, 'b'), (1, 'c'), (2, 'd'), (3, 'e')]);
            let c1 = s1.hold('a');
            let (s2, s2f) = mk_stream(sodium_ctx, &[(2, 'Y'), (3, 'Z')]);
            let c2 = s2.hold('X');
            let (switcher, switcher_f) = mk_stream(sodium_ctx, &[(1, c2)]);
            let c = switcher.hold(c1);
            let mut firings = vec![("s1", s1f), ("s2", s2f), ("switcher", switcher_f)];
            let firings = reorder(&mut firings, order);
            (
                firings,
                Box::new(move |o| Cell::switch_c(&c).listen(record(o))),
            )
        },
        |out| assert_eq!(['b', 'X', 'Y', 'Z'], out),
    );
}

#[test]
fn switch_c4() {
    run_permutations(
        4,
        |sodium_ctx, order| {
            let (s1, s1f) = mk_stream(sodium_ctx, &[(0, 'b'), (1, 'c'), (2, 'd'), (3, 'e')]);
            let c1 = s1.hold('a');
            let (s2, s2f) = mk_stream(sodium_ctx, &[(0, 'W'), (1, 'X'), (2, 'Y'), (3, 'Z')]);
            let c2 = s2.hold('V');
            let (s3, s3f) = mk_stream(sodium_ctx, &[(0, '2'), (1, '3'), (2, '4'), (3, '5')]);
            let c3 = s3.hold('1');
            let (switcher, switcher_f) = mk_stream(sodium_ctx, &[(1, c2), (3, c3)]);
            let c = switcher.hold(c1);
            let mut firings = vec![
                ("s1", s1f),
                ("s2", s2f),
                ("s3", s3f),
                ("switcher", switcher_f),
            ];
            let firings = reorder(&mut firings, order);
            (
                firings,
                Box::new(move |o| Cell::switch_c(&c).listen(record(o))),
            )
        },
        |out| assert_eq!(['b', 'X', 'Y', '5'], out),
    );
}

#[test]
fn sample() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<char>();
    let c = s.stream().hold('a');
    let sample1 = c.sample();
    s.send('b');
    let sample2 = c.sample();
    assert_eq!('a', sample1);
    assert_eq!('b', sample2);
}

#[test]
fn sample_lazy() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<char>();
    let c = s.stream().hold('a');
    let sample1 = c.sample_lazy();
    s.send('b');
    let sample2 = c.sample_lazy();
    assert_eq!('a', sample1.run());
    assert_eq!('b', sample2.run());
}

/// Take `firings` in the order given by `order`.
fn reorder(
    firings: &mut Vec<(&'static str, Firings)>,
    order: &[usize],
) -> Vec<(&'static str, Firings)> {
    let mut slots: Vec<Option<(&'static str, Firings)>> = firings.drain(..).map(Some).collect();
    order
        .iter()
        .map(|i| slots[*i].take().expect("each index used once"))
        .collect()
}
