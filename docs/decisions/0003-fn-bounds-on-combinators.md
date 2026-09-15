# 0003 -- `Fn` bounds on the combinators

<details>
<summary><strong>Status:</strong> Accepted 2026-09-14</summary>

| Date | Transition |
| --- | --- |
| 2026-09-14 | Drafted |
| 2026-09-14 | Accepted |

</details>

*`Implemented` is not logged yet. The work is in the same pull request as this
record, so the row belongs on the commit that merges it -- logging it here would
claim an event that has not happened, and the gap between `Accepted` and
`Implemented` is the one number the log exists to make visible.*

## Context

[ADR-0002](0002-closure-bounds-and-dependency-declaration.md) -- in flight as
[#46](https://github.com/RadicalZephyr/sodium-rust/pull/46) at the time this was
written, so that link resolves once it merges -- moved the combinators off the
`IsLambda1`..`IsLambda6` traits and onto the `Fn` family so that closure
arguments would infer. It took the bound that was already there --
`FnMut` -- and carried it across unchanged, because the question it was
answering was about inference and not about mutability.

That left the mutability question where
[upstream issue #48](https://github.com/SodiumFRP/sodium-rust/issues/48) had
left it in 2020. RadicalZephyr opened that issue proposing `Fn` throughout, on
the grounds that a combinator is supposed to be a function of its arguments and
`Fn` is the bound that says so. clinuxrulz turned it down, and the reason he
gave is the whole of this record's context:

> A special use case is squeezing more performance out of collections. Instead
> of having collections being updated in `O(log(N))` time at best. You can
> update collections in `O(1)` time without breaking the denotational
> semantics.

The sketch was a `map` closure owning a `HashMap`, mutating it in place, and
returning it -- with the claim that downstream nodes receiving only a shared
reference makes this "indistinguishable from the immutable collection version
in how it operates".

The thread then ran for a month without anyone testing that claim, and stopped.
Five days later clinuxrulz worked out that the trick needs `Arc<Mutex<..>>`
anyway, since the lambdas return a non-reference type, and said he would be
happy to go from `FnMut` to `Fn` -- but by then the discussion had moved to
inference and the bound never changed.

### The claim, measured

`docs/decisions/research/src/bin/0003-collection-drift.rs` takes both forms of
the trick and asks the question the claim actually makes: does a value handed
to an observer in transaction N still read, later, as it did in transaction N?
That is the property an immutable collection gives for free, and it is what
"indistinguishable" has to mean to be worth anything.

```text
Collection size as each event arrived, and again after the run.

owned, by value        O(n)   at event [1, 2, 3]  observed later [1, 2, 3]  -- values held
shared handle          O(1)   at event [1, 2, 3]  observed later [3, 3, 3]  -- VALUES DRIFTED
```

> Measured 2026-09-14 against this branch, `cargo run --release -p adr-research --bin 0003-collection-drift`.

Returned by value the trick is correct and `O(n)`: a combinator's return type is
owned (`B: Clone + Send + 'static`), so the collection is copied out on every
event. That is worse than the `O(log n)` immutable collection it was introduced
to beat. Returned as a shared handle it is genuinely `O(1)`, and the observers
of events 1 and 2 both end up holding a three-entry map -- precisely the
property being claimed.

**Neither result depends on the bound.** Both closures reach their state through
a shared reference, so both are `Fn` already. What defeats the trick is the
owned return type, which nothing in #48 proposed changing. The argument that
stopped this change for six years does not survive contact with a measurement,
and it would not have survived one in 2020 either.

### Where the bound is actually load-bearing

Before this change, `src/stream.rs` and `src/cell.rs` carried 46 `FnMut`
bounds: 38 on combinators, 8 on `listen`/`listen_weak` and their `*_with_deps`
siblings. Narrowing all 46 to `Fn` breaks 18 call sites in this repository, and
every one of them is a `listen` handler in `benches/sodium.rs` -- `move |v|
values.push(*v)`, eighteen times. Narrowing only the 38 breaks nothing at all:
not the 52 unit tests, not the integration tests, not the benchmarks, not
`coz-driver`.

So the two halves of the API want different answers, which is what RadicalZephyr
proposed in the opening comment of #48 before the thread moved on to whether
`Fn` should be universal.

## Decision

**Combinators take `Fn`. `listen` and `listen_weak` keep `FnMut`.**

- The 38 combinator bounds in `src/stream.rs` and `src/cell.rs` become `Fn`.
- The 8 listener bounds stay `FnMut`. `listen` is the effectful edge of the
  graph and its handler is there to perform effects; requiring `Arc<Mutex<_>>`
  there would be ceremony with no dataflow behind it.
- `fnmut_as_fn` in `src/impl_/lambda.rs` absorbs a handler's mutability in a
  `Mutex` at the API boundary, so nothing `FnMut` reaches the graph. The lock is
  per-handler and private to the closure that owns it, so two listeners never
  contend.
- `IsLambda1`..`IsLambda6` take `call(&self)` rather than `call(&mut self)`, and
  their impls narrow to `Fn`.
- A node's update becomes `RwLock<Box<dyn Fn() + Send + Sync>>`, and every site
  that fires one takes a read lock.

Where a combinator closure was keeping state, the migration is `accum` or
`collect`, which thread the state through the signature instead of a capture:

```rust
// before
let mut n = 0;
stream.map(move |_a| { n += 1; n })
// after
stream.collect(0, |_a, n| (*n + 1, *n + 1))
```

## Consequences

**Five closure shapes stop compiling**, and
`tests/ui/combinator_rejects_captured_state.rs` is the itemised list with the
diagnostics each now produces. Four -- a counter, a sliding window, an
edge-detector, an RNG -- thread through `accum`/`collect` to the output their
captured versions produced. The fifth is a memo cache, which has no natural
rewrite because a cache is not part of the value being computed, and takes
`Arc<Mutex<_>>` instead.

The rewrite is not purely a tax. State threaded through `accum` is a `Cell`:
sampleable, composable, and visible to the graph's dependency tracking. State
captured in a closure is visible to nobody.

**The diagnostic lands on the closure, not the call.** rustc infers a closure's
class from its body, so a closure that mutates a capture is `FnMut` and is
rejected while being built -- at the mutation, naming the binding -- before the
combinator is consulted. There are only ever two error codes: `E0594` for
assigning to a captured binding, `E0596` for taking `&mut` of one.

**Firing a node no longer needs exclusive access to it.** The update was a
`dyn FnMut`, so firing took a write lock -- not because two threads might fire
the same node, but because invoking a `FnMut` needs `&mut`. `SodiumCtx` already
carries a `ThreadedMode` and a `TODO` for a thread-pool mode; a per-node
exclusive lock on every fire was what stood between that scaffolding and an
evaluator that runs independent nodes at once.

This buys nothing measurable today, and the record should not pretend
otherwise: `simple_threaded_mode` spawns and immediately joins, so the evaluator
is serialized and the lock was uncontended either way.

**`Cell::map` drops a lock outright.** It wrapped the user's lambda in
`Arc<Mutex<FN>>` only because a `FnMut` shared between the `init` thunk and the
update closure needs exclusive access. With `call(&self)`, `Arc<FN>` does.

**`Cell::lift2` keeps a lock it no longer needs, and we are not happy about it.**
It has the same shape, but `Arc<FN>` is only `Send` when `FN: Sync`, and
`lift2`'s public bound asks for `Send` alone where `Cell::map`'s asks for
`Send + Sync`. That asymmetry looks like an oversight rather than a design, but
tightening a public bound is a different decision from this one, so the site
carries a comment instead. `lift3`..`lift6` all build on `lift2`, so the whole
family is waiting on it.

**`Fn` is also what the implementation can use.** `Fn::call` takes `&self`, so
`&F` is itself callable; `FnMut::call_mut` takes `&mut self`, so it is not.

**Experiment -- `&F` is a callable only when `F: Fn`**

```rust
// Takes the function by value and calls it once, as `Option::map` does.
// Spelled out rather than calling `Option::map` so the diagnostic points at a
// bound in this file: rustc quotes the source of a foreign bound only when it
// can read it, so `core`'s would render differently depending on whether the
// toolchain has `rust-src`.
fn apply<T, U, F: FnOnce(T) -> U>(f: F, x: T) -> U {
    f(x)
}

// `Fn::call` takes `&self`, so `&F` is itself a `FnOnce`.
fn through_shared_ref_to_fn<F: Fn(&i32) -> i32>(f: F, x: &i32) -> i32 {
    apply(&f, x)
}

// `FnMut::call_mut` takes `&mut self`, so `&F` is not.
fn through_shared_ref_to_fnmut<F: FnMut(&i32) -> i32>(f: F, x: &i32) -> i32 {
    apply(&f, x)
}

fn main() {
    println!("{}", through_shared_ref_to_fn(|a| *a + 1, &1));
    println!("{}", through_shared_ref_to_fnmut(|a| *a + 1, &1));
}
```

```text
error[E0277]: expected an `FnOnce(&i32)` closure, found `&F`
  --> src/main.rs:17:12
   |
17 |     apply(&f, x)
   |     -----  ^ expected an `FnOnce(&i32)` closure, found `&F`
   |     |
   |     required by a bound introduced by this call
   |
   = note: `F` implements `FnMut`, but it must implement `Fn`, which is more general
   = note: required for `&F` to implement `FnOnce(&i32)`
note: required by a bound in `apply`
  --> src/main.rs:6:19
   |
 6 | fn apply<T, U, F: FnOnce(T) -> U>(f: F, x: T) -> U {
   |                   ^^^^^^^^^^^^^^ required by this bound in `apply`
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](MINT-ME-1)

This is not a curiosity: `Stream::split_enum2`, `Stream::split_enum3` and
`Router::new` were bounded on `Fn` already, while every combinator beside them
was `FnMut`, and this is why. `split_enum2` reaches the user's function as
`firing_op.as_ref().map(&f)`. The `Fn` bound there was not a style choice; the
implementation could not be written without it.

## Alternatives considered

**Leave every bound on `FnMut`.** The status quo, and the argument for it is the
`O(1)` collection trick, which the measurement above disposes of. What remains
is that `FnMut` lets a combinator closure keep private state -- which is a
capability the library does not want to offer, because that state is invisible
to the graph and `accum`/`collect` exist to hold it where the graph can see it.

**Narrow every bound to `Fn`, listeners included.** This is #48 as originally
proposed. It breaks 18 handlers in this repository's own benchmarks and would
break the same shape everywhere else, in exchange for nothing: a listener's
state is not dataflow, so there is no `accum` rewrite to migrate it to, only
`Arc<Mutex<_>>` wrapping with no reader to benefit. Rejected as ceremony.

### Function pointers instead of `impl Fn`

The most interesting of the three, and the only one whose attraction is about
something other than mutability.

`*_with_deps` exists for one failure mode: a closure captures a `Cell` or
`Stream`, samples it at call time, and Sodium cannot see that dependency because
it cannot see inside a closure. The API's answer is to ask the caller to declare
it, and ADR-0002 is candid that this is a contract a caller can silently break
-- declare a node the closure does not hold and the collector's bookkeeping is
corrupted; declare nothing and the node can be collected out from under you.

Bounding the combinators on `fn(&A) -> B` instead would appear to close that
hole by construction. A function pointer captures nothing, so there is nothing
for it to have captured and failed to declare, and `*_with_deps` could be
deleted rather than documented.

The expected cost is that call sites lose closure literals. That turns out not
to be the cost.

**Experiment -- a closure literal coerces to a fn pointer, and still infers**

```rust
pub struct Stream<A>(A);

impl<A> Stream<A> {
    // The proposed shape: a function pointer, not a generic `Fn` bound.
    pub fn map_ptr<B>(&self, f: fn(&A) -> B) -> B {
        f(&self.0)
    }
}

fn double(a: &i32) -> i32 {
    *a * 2
}

fn main() {
    let s = Stream(21i32);

    // By name, which is what a function-pointer bound obviously accepts.
    println!("named:      {}", s.map_ptr(double));

    // A closure literal that captures nothing coerces to `fn`, and its
    // argument type is deduced from the pointer's signature -- no annotation.
    println!("lambda:     {}", s.map_ptr(|a| *a + 1));

    // Including where the body only constrains an associated type of the
    // argument, the case that needed a full annotation under a trait bound.
    println!("negated:    {}", s.map_ptr(|a| -*a));
}
```

```text
named:      42
lambda:     22
negated:    -21
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](MINT-ME-2)

So the ergonomic objection is mostly wrong. Non-capturing closure literals
coerce to `fn` pointers, argument types are deduced from the pointer's
signature, and the third case is the one ADR-0002 had to fight for -- a body
constraining only an associated type of the argument, which needed a full `&i32`
under the old trait bound and needs nothing here.

What the bound actually costs is every capture, not every closure:

**Experiment -- a fn-pointer bound refuses every capture, not just FRP nodes**

```rust
#[derive(Clone)]
pub struct Cell<A>(A);

impl<A: Copy> Cell<A> {
    pub fn sample(&self) -> A {
        self.0
    }
}

pub struct Stream<A>(A);

impl<A> Stream<A> {
    pub fn map_ptr<B>(&self, _f: fn(&A) -> B) {}
}

fn main() {
    let s = Stream(1i32);

    // The case `*_with_deps` exists for: a closure holding a Cell it samples.
    let c = Cell(100i32);
    s.map_ptr(move |a| *a + c.sample());

    // A closure over ordinary, non-FRP data. Refused on the same grounds.
    let bias = 10i32;
    s.map_ptr(move |a| *a + bias);
}
```

```text
error[E0308]: mismatched types
  --> src/main.rs:21:15
   |
21 |     s.map_ptr(move |a| *a + c.sample());
   |       ------- ^^^^^^^^^^^^^^^^^^^^^^^^ expected fn pointer, found closure
   |       |
   |       arguments to this method are incorrect
   |
   = note: expected fn pointer `for<'a> fn(&'a i32) -> i32`
                 found closure `{closure@src/main.rs:21:15: 21:23}`
note: closures can only be coerced to `fn` types if they do not capture any variables
  --> src/main.rs:21:29
   |
21 |     s.map_ptr(move |a| *a + c.sample());
   |                             ^ `c` captured here
note: method defined here
  --> src/main.rs:13:12
   |
13 |     pub fn map_ptr<B>(&self, _f: fn(&A) -> B) {}
   |            ^^^^^^^           ---------------

error[E0308]: mismatched types
  --> src/main.rs:25:15
   |
25 |     s.map_ptr(move |a| *a + bias);
   |       ------- ^^^^^^^^^^^^^^^^^^ expected fn pointer, found closure
   |       |
   |       arguments to this method are incorrect
   |
   = note: expected fn pointer `for<'a> fn(&'a i32) -> i32`
                 found closure `{closure@src/main.rs:25:15: 25:23}`
note: closures can only be coerced to `fn` types if they do not capture any variables
  --> src/main.rs:25:29
   |
25 |     s.map_ptr(move |a| *a + bias);
   |                             ^^^^ `bias` captured here
note: method defined here
  --> src/main.rs:13:12
   |
13 |     pub fn map_ptr<B>(&self, _f: fn(&A) -> B) {}
   |            ^^^^^^^           ---------------
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](MINT-ME-3)

The diagnostic is good -- it names the captured binding and points at it. But it
refuses `bias` for exactly the same reason it refuses `c`, and the API has no
way to distinguish them. The dependency problem is about capturing FRP nodes;
the bound that fixes it forbids capturing anything.

That is not a marginal cost here. Flipping just three combinators --
`Stream::map`, `Stream::filter`, `Cell::map` -- to fn pointers breaks the
library's own `Stream::map_to`, which is `self.map(move |_| b.clone())` and
captures the constant it exists to supply; two call sites in `src/tests.rs`; and
`coz-driver`'s prime sieve:

```rust
s_output.snapshot(&c_s_output, |prime: &i64, old_s_output: &Stream<i64>| {
    let prime = *prime;
    old_s_output.filter(move |x: &i64| (*x % prime) != 0)
})
```

> Measured 2026-09-14 against this branch.

The sieve is the one that settles it. It builds a new filter node per prime,
and the filter captures `prime` -- a plain `i64` produced by the running graph,
not a `Cell`. There is no declaration to make and nothing to `snapshot` against:
the value exists only inside the transaction that created the node. Expressing
it under a fn-pointer bound means giving each prime its own `Cell` and routing
`filter` through `snapshot` and `filter_option`, which changes the graph's shape
and adds a node per prime to satisfy a bound. **Dynamic graph construction is
the pattern fn pointers cannot express**, and switch/loop-based FRP is made of
it.

And the guarantee that would be bought is not the one advertised:

**Experiment -- a fn pointer reaches a `Cell` anyway**

```rust
use std::sync::OnceLock;

#[derive(Clone)]
pub struct Cell<A>(A);

impl<A: Copy> Cell<A> {
    pub fn sample(&self) -> A {
        self.0
    }
}

pub struct Stream<A>(A);

impl<A> Stream<A> {
    pub fn map_ptr<B>(&self, f: fn(&A) -> B) -> B {
        f(&self.0)
    }
}

// A fn pointer captures nothing. It can still reach a Cell.
static BIAS: OnceLock<Cell<i32>> = OnceLock::new();

fn via_static(a: &i32) -> i32 {
    *a + BIAS.get().unwrap().sample()
}

// The same through a thread-local, which is the shape an ambient context takes.
thread_local! {
    static LOCAL_BIAS: Cell<i32> = Cell(7);
}

fn via_thread_local(a: &i32) -> i32 {
    LOCAL_BIAS.with(|c| *a + c.sample())
}

fn main() {
    BIAS.set(Cell(100)).ok();
    let s = Stream(1i32);

    // Both accepted: no captures, and an undeclared dependency on a Cell.
    println!("via static:       {}", s.map_ptr(via_static));
    println!("via thread_local: {}", s.map_ptr(via_thread_local));
}
```

```text
via static:       101
via thread_local: 8
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](MINT-ME-4)

"Captures nothing" is not the same property as "cannot reach a `Cell`". A fn
pointer reaches one through a `static` or a `thread_local!` without capturing
anything, and the undeclared dependency is exactly as undeclared as it would
have been in a closure. The bound closes the *common* path to the bug, not the
capability.

The thread-local half is not a contrivance either. There is an open proposal to
add an ambient per-thread `SodiumCtx`
([#32](https://github.com/RadicalZephyr/sodium-rust/pull/32)); if that lands,
reaching FRP state from a fn pointer stops being something you have to go out of
your way to arrange and becomes the ordinary way to write one.

**Rejected.** It forbids dynamic graph construction, which the library's own
`map_to` and the prime sieve both depend on, in exchange for a guarantee that is
strong by convention rather than by construction. The `*_with_deps` contract
being breakable is a real problem and this is not the fix for it; ADR-0002's
open question about a capture-visiting macro remains the more promising
direction, because it removes the *declaration* rather than the *capture*.

## Where this record's evidence lives

| Claim | Evidence |
| --- | --- |
| The `O(1)` collection trick drifts, and the bound is irrelevant to it | `docs/decisions/research/src/bin/0003-collection-drift.rs` |
| `&F` is callable only when `F: Fn` | Playground, above |
| A closure literal coerces to a fn pointer and infers | Playground, above |
| A fn-pointer bound refuses every capture | Playground, above |
| A fn pointer reaches a `Cell` through a `static` | Playground, above |
| The `Fn` bound's rejections, as callers see them | `tests/ui/combinator_rejects_captured_state.rs` |
| Listeners still take a mutable handler | `tests/ui/listener_accepts_captured_state.rs`, `tests/fn_vs_fnmut.rs` |

The two `tests/ui/` entries are not research. They are compile-time checks on a
rejection and an acceptance this crate *promises*, which is what keeps them on
the right side of the rule in
[`CONTRIBUTING.md`](../../CONTRIBUTING.md#tests-and-research-are-not-the-same-thing);
the compiler behaviour this record argues *from* is in the Playground blocks
instead.

## Open questions

- **`Cell::lift2`'s `Sync` asymmetry.** Tightening its bound to match
  `Cell::map`'s would drop a lock from `lift2`..`lift6`. It is a public bound
  change, so it wants its own record --
  [#33](https://github.com/RadicalZephyr/sodium-rust/pull/33) is already moving
  in the opposite direction, relaxing bounds and dropping `Sync` from closures,
  and the two need to be reconciled before either lands.
- **Whether the parallel evaluator ever arrives.** The read lock is justified by
  a `TODO`. If a thread-pool mode is never built, this change bought correctness
  of intent and a lock removal in `Cell::map`, and that is all -- which would
  still have been worth it, but the record should not be read later as having
  promised throughput.
