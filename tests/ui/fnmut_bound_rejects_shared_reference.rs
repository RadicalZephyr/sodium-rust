// The asymmetry runs both ways: what `FnMut` costs that `Fn` does not.
//
// `Fn::call` takes `&self`, so `&F` is itself callable when `F: Fn`.
// `FnMut::call_mut` takes `&mut self`, so it is not: a `FnMut` can only be
// invoked through something that owns it or has it uniquely borrowed.
//
// This is not a curiosity. `Stream::split_enum2`, `Stream::split_enum3` and
// `Router::new` are bounded on `Fn` while every neighbouring combinator was
// bounded on `FnMut`, and the reason is exactly this. `split_enum2` reaches the
// user's function as `firing_op.as_ref().map(&f)`, which needs `&F: FnOnce`,
// which holds only for `F: Fn`. The `Fn` bound was not a style choice there;
// the implementation could not be written without it.
//
// So a `Fn` bound is not purely a restriction traded for a guarantee. It also
// admits implementation strategies -- sharing one function across nodes,
// calling it behind a shared reference, calling it from more than one thread at
// once -- that `FnMut` forecloses. See finding 5 in `tests/fn_vs_fnmut.rs` for
// the one that mattered most here: the node update in `src/impl_/node.rs` was a
// `dyn FnMut`, so firing a node took a write lock.
//
// `apply` stands in for `Option::map`, which is what `split_enum2` actually
// calls. It is spelled out here rather than used directly so that the expected
// diagnostic points at a bound in this file. Pointing it at `core::option`
// would make the snapshot depend on whether the toolchain has the `rust-src`
// component installed -- rustc quotes the source of a foreign bound only when
// it can read it, so the same rustc produces two different renderings.

// Takes the function by value and calls it once, as `Option::map` does.
pub fn apply<T, U, F: FnOnce(T) -> U>(f: F, x: T) -> U {
    f(x)
}

// `F: Fn`, so `&F` is a `FnOnce` and this compiles -- the shape `split_enum2`
// relies on.
pub fn shared_reference_to_an_fn<F: Fn(&i32) -> i32>(f: F, x: &i32) -> i32 {
    apply(&f, x)
}

// The same call with the bound weakened to `FnMut`. It does not.
pub fn shared_reference_to_an_fnmut<F: FnMut(&i32) -> i32>(f: F, x: &i32) -> i32 {
    apply(&f, x)
}

fn main() {}
