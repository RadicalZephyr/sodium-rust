// What a `Fn` bound on the combinators would still accept.
//
// Half one of the pair; `fn_bound_rejects_captured_state.rs` is the other.
//
// Sodium is bounded on `FnMut` today, so a `Fn`-bounded combinator cannot be
// demonstrated against this crate. `map` below is the reduction: the signature
// `Stream::<A>::map` would have under the change proposed in issue #48, with
// nothing else in the file, so what compiles and what does not is attributable
// to the bound alone.
//
// The runtime counterpart is `either_*` in `tests/fn_vs_fnmut.rs`, which
// exercises the same shapes against the real API.

use std::sync::{Arc, Mutex};

pub struct Stream<A>(pub A);

impl<A> Stream<A> {
    // The bound proposed in #48.
    pub fn map<B, F: Fn(&A) -> B + Send + Sync + 'static>(&self, _f: F) {}
}

// A pure closure -- by inspection the overwhelming majority of call sites.
pub fn pure(s: &Stream<i32>) {
    s.map(|a| *a + 1);
}

// A closure that reads captured state without owning the mutation.
pub fn reads_capture(s: &Stream<i32>, bias: i32) {
    s.map(move |a| *a + bias);
}

// Interior mutability walks straight through a `Fn` bound. `Fn` was never a
// purity proof -- what it changes is that mutating shared state becomes a
// visible act rather than the default.
pub fn interior_mutability(s: &Stream<i32>) {
    let seen: Arc<Mutex<Vec<i32>>> = Default::default();
    s.map(move |a| {
        seen.lock().unwrap().push(*a);
        *a
    });
}

// The capability that only `Fn` has: `&F` is callable when `F: Fn`, because
// `Fn::call` takes `&self`. That is not a hypothetical -- it is why
// `Stream::split_enum2` and `Router::new` are already bounded on `Fn` in this
// crate today, both of which reach the user's function through a shared
// reference. `fnmut_bound_rejects_shared_reference.rs` is the same code under
// `FnMut`.
pub fn callable_through_a_shared_reference<F: Fn(&i32) -> i32>(f: F, x: Option<i32>) -> Option<i32> {
    x.as_ref().map(&f)
}

fn main() {}
