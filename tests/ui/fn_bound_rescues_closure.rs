// Half one of "why was the API split in two rather than given a second bound?"
//
// Adding `+ FnMut(&A) -> B` alongside the old `IsLambda1` bound does restore
// closure inference, so it looks like a one-line fix. This file is that fix,
// and it compiles.
//
// See `fn_bound_rejects_lambda.rs` for the half that makes it unworkable.

pub trait IsLambda1<A, B> {
    fn call(&mut self, a: &A) -> B;
    fn deps(&self) -> usize;
}

pub struct Lambda<FN> {
    pub f: FN,
    pub deps: usize,
}

impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for Lambda<FN> {
    fn call(&mut self, a: &A) -> B {
        (self.f)(a)
    }
    fn deps(&self) -> usize {
        self.deps
    }
}

impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
    fn call(&mut self, a: &A) -> B {
        self(a)
    }
    fn deps(&self) -> usize {
        0
    }
}

pub struct Stream<A>(pub A);

impl<A> Stream<A> {
    pub fn map<B, F: IsLambda1<A, B> + FnMut(&A) -> B>(&self, _f: F) {}
}

pub fn f(s: &Stream<i32>) {
    s.map(|a| *a + 1); // the extra bound makes this infer
}

fn main() {}
