// Half two of "why was the API split in two rather than given a second bound?"
//
// Same declarations as `fn_bound_rescues_closure.rs`, where adding
// `+ FnMut(&A) -> B` to the bound made a bare closure infer. The cost shows up
// here: that same bound rejects `Lambda<FN>`, which is a plain struct and
// cannot implement `FnMut` on stable Rust.
//
// `Lambda` was the only thing `IsLambda1` existed to accept, so the "one-line
// fix" would have broken every dependency-carrying call site. Splitting the API
// -- `map` for closures, `map_with_deps` for the deps case -- keeps both.

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
    s.map(Lambda {
        f: |a: &i32| *a + 1,
        deps: 3,
    });
}

fn main() {}
