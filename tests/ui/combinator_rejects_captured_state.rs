// The complete list of what the `Fn` bound on the combinators costs.
//
// Five closure shapes, which is to say five ways of owning mutable state across
// calls. Every one of them compiled before the bound changed; each is shown
// here exactly as it was written then, so the diagnostic in the `.stderr`
// beside this file is the migration note for anyone who hits it.
//
// `rewrite_*` in `tests/fn_vs_fnmut.rs` is the other half: four of the five
// rewrite through `accum`/`collect` to the output they produced here, and the
// fifth takes interior mutability.
//
// Two things worth noticing in the expected output.
//
// First, the bound is not what fails. rustc infers a closure's class from its
// *body*, so a closure that mutates a capture is `FnMut` and is rejected while
// being built -- at the mutation, naming the binding and the line -- before
// `map` is consulted at all. The fix is therefore never "adjust the call".
//
// Second, there are only ever two error codes: E0594 for assigning to a
// captured binding, E0596 for taking `&mut` of one. Every shape below is one or
// the other, and so is every shape that will ever hit this bound.

use sodium_rust::Stream;
use std::collections::HashMap;

// E0594. Rewrite: `collect(0, |_, n| (*n + 1, *n + 1))`.
pub fn counter(s: &Stream<i32>) -> Stream<i32> {
    let mut n = 0;
    s.map(move |_a| {
        n += 1;
        n
    })
}

// E0596. Rewrite: `collect(Vec::new(), ..)`, building the next window from the
// previous one instead of mutating it in place.
pub fn sliding_window(s: &Stream<i32>) -> Stream<Vec<i32>> {
    let mut buf: Vec<i32> = Vec::new();
    s.map(move |a| {
        buf.push(*a);
        if buf.len() > 3 {
            buf.remove(0);
        }
        buf.clone()
    })
}

// E0594. Rewrite: `collect(None, |a, prev| (*prev != Some(*a), Some(*a)))`.
pub fn edge_detect(s: &Stream<i32>) -> Stream<bool> {
    let mut prev: Option<i32> = None;
    s.map(move |a| {
        let is_new = prev != Some(*a);
        prev = Some(*a);
        is_new
    })
}

// E0596, and the most substantial entry in the census: here the mutation is not
// incidental but how the type is meant to be used, since every RNG in the
// ecosystem advances through `&mut self`. It still rewrites, by threading the
// seed rather than the generator -- see `rewrite_random_number_generator`.
pub struct Lcg(u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
}

pub fn rng(s: &Stream<i32>) -> Stream<i32> {
    let mut rng = Lcg(1);
    s.map(move |a| *a + (rng.next() % 10) as i32)
}

// E0596, and the one shape with no `accum`/`collect` rewrite: a cache is not
// part of the value being computed, so threading it through the signature would
// put an implementation detail into the graph. It takes `Arc<Mutex<_>>`, which
// is what it would have needed anyway the moment two combinators shared it.
pub fn memo_cache(s: &Stream<i32>) -> Stream<i32> {
    let mut cache: HashMap<i32, i32> = HashMap::new();
    s.map(move |a| *cache.entry(*a).or_insert_with(|| *a * *a))
}

fn main() {}
