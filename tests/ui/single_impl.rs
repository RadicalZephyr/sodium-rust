// Rules out the other plausible explanation for the old failure.
//
// The old `IsLambda1` had two impls -- one for `Lambda<FN>` and a blanket one
// for `FN` -- so overlap between them is a natural suspect. It was not the
// cause: with a single impl and nothing to be ambiguous with, an unannotated
// closure is rejected identically.

pub trait IsLambda1<A, B> {
    fn call(&mut self, a: &A) -> B;
}

// The only impl in scope. No `Lambda<FN>` to be ambiguous with.
impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
    fn call(&mut self, a: &A) -> B {
        self(a)
    }
}

pub fn apply<A, B, F: IsLambda1<A, B>>(_a: A, _f: F) {}

pub fn broken() {
    apply(1i32, |a| *a + 1); // still rejected
}

fn main() {}
