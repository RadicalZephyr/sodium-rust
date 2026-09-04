// The other half of the split, checked the way a downstream crate would hit it.
//
// `combinator_rejects_captured_state.rs` is the list of closures the
// combinators no longer take. Every one of them is still fine on a listener:
// `listen` and `listen_weak` are bounded on `FnMut`, deliberately, because they
// are the effectful edge of the graph and their handlers are supposed to do
// effectful things. Requiring `Arc<Mutex<_>>` there would be ceremony with
// nothing behind it -- there is no dataflow to make first-class, only a side
// effect to perform.
//
// This is the regression guard for that guarantee. It matters because the
// mutability is absorbed by a `Mutex` inside `listen` rather than carried
// through the graph, so a change to the internals could quietly take it away
// without any combinator test noticing.
//
// Nothing here needs to run: the functions are concrete, so rustc checks their
// bodies -- and therefore every closure's class -- whether or not they are ever
// called. Runtime behaviour is covered by `listener_*` in
// `tests/fn_vs_fnmut.rs`.

use sodium_rust::{Cell, Dep, Listener, Stream};
use std::collections::HashMap;

// The shape `benches/sodium.rs` is made of, 18 times over.
pub fn accumulates_into_a_capture(s: &Stream<i32>) -> Listener {
    let mut seen: Vec<i32> = Vec::new();
    s.listen(move |a| seen.push(*a))
}

pub fn counts(s: &Stream<i32>) -> Listener {
    let mut n = 0;
    s.listen(move |_a| {
        n += 1;
        println!("{n}");
    })
}

// A `&mut self` API driven from a handler -- rejected on a combinator, fine
// here.
pub struct Lcg(u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
}

pub fn drives_a_mut_self_api(s: &Stream<i32>) -> Listener {
    let mut rng = Lcg(1);
    s.listen(move |a| println!("{}", *a + (rng.next() % 10) as i32))
}

pub fn owns_a_map(s: &Stream<i32>) -> Listener {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    s.listen(move |a| *counts.entry(*a).or_default() += 1)
}

// All four entry points, on both `Stream` and `Cell`, including the
// deps-carrying ones: declaring dependencies does not cost you the mutability.
pub fn every_entry_point(s: &Stream<i32>, c: &Cell<i32>, bias: &Cell<i32>) -> Vec<Listener> {
    let deps: Vec<Dep> = vec![bias.to_dep()];
    let b = bias.clone();

    let mut n1 = 0;
    let mut n2 = 0;
    let mut n3 = 0;
    let mut n4 = 0;
    let mut n5 = 0;

    vec![
        s.listen(move |a| {
            n1 += *a;
            println!("{n1}");
        }),
        s.listen_weak(move |a| {
            n2 += *a;
            println!("{n2}");
        }),
        s.listen_with_deps(
            move |a| {
                n3 += *a + b.sample();
                println!("{n3}");
            },
            deps,
        ),
        c.listen(move |a| {
            n4 += *a;
            println!("{n4}");
        }),
        c.listen_weak(move |a| {
            n5 += *a;
            println!("{n5}");
        }),
    ]
}

fn main() {}
