// What a `Fn` bound on the combinators would reject.
//
// Half two of the pair; `fn_bound_accepts_shared_state.rs` is the other, and
// carries the note on why these are reductions rather than calls into the real
// API.
//
// Both errors below are worth reading carefully, because neither lands where
// you would expect. The bound is not what fails: rustc infers the closure's
// class from its *body*, so a closure that assigns to a capture is `FnMut` and
// is rejected while being built, at the mutation, before `map` is consulted at
// all. That makes the diagnostic a good one -- it names the binding and the
// line -- but it also means the fix is never "adjust the call".
//
// The two error codes here are the whole census: E0594 for assigning to a
// captured binding, E0596 for taking `&mut` of one. Every shape in
// `fnmut_only_*` in `tests/fn_vs_fnmut.rs` reduces to one or the other, and
// `rewrite_*` beside them shows what each becomes without the capture.

pub struct Stream<A>(pub A);

impl<A> Stream<A> {
    // The bound proposed in #48.
    pub fn map<B, F: Fn(&A) -> B + Send + Sync + 'static>(&self, _f: F) {}
}

// E0594: assigning to a captured binding. The counter from
// `fnmut_only_counter`, whose rewrite is `collect(0, |_, n| (*n + 1, *n + 1))`.
pub fn counter(s: &Stream<i32>) {
    let mut n = 0;
    s.map(move |_a| {
        n += 1;
        n
    });
}

// E0596: calling a `&mut self` method on a captured value. Standing in for
// every RNG in the ecosystem, all of which advance through `&mut self`. The
// most substantial entry in the census, since here the mutation is not
// incidental -- it is how the type is meant to be used. It still rewrites, by
// threading the seed instead of the generator; see
// `rewrite_random_number_generator`.
pub struct Lcg(u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
}

pub fn rng(s: &Stream<i32>) {
    let mut rng = Lcg(1);
    s.map(move |a| *a + (rng.next() % 10) as i32);
}

fn main() {}
