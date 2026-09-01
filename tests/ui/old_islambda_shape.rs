// Why the combinators are bounded on `FnMut`/`Fn` rather than on a trait of
// our own.
//
// No dependency on `sodium`: this is a two-line trait with the same shape the
// old `IsLambda1` had, and it reproduces the `error[E0282]` that used to hit
// every call site in the crate.
//
// The element type is concretely known from `Self` here, exactly as it is for
// `Stream<A>::map`. So the failure was never about `A` being open -- it is
// about the closure's own signature not being deducible through a non-`Fn`
// trait bound. Only the trait-bounded call fails; the `FnMut`-bounded one
// beside it infers fine.

pub trait IsLambda1<A, B> {
    fn call(&mut self, a: &A) -> B;
}

impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
    fn call(&mut self, a: &A) -> B {
        self(a)
    }
}

pub struct Stream<A>(A);

impl<A> Stream<A> {
    // The shape sodium's `Stream::map` used to have.
    pub fn map_via_trait<B, F: IsLambda1<A, B>>(&self, _f: F) {}
    // The shape it has now.
    pub fn map_via_fnmut<B, F: FnMut(&A) -> B>(&self, _f: F) {}
}

pub fn control(s: &Stream<i32>) {
    s.map_via_fnmut(|a| *a + 1); // infers fine
}

pub fn broken(s: &Stream<i32>) {
    s.map_via_trait(|a| *a + 1); // rejected
}

fn main() {}
