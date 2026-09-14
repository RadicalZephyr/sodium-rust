# 0002 -- Closure bounds and dependency declaration

<details>
<summary><strong>Status:</strong> Implemented 2026-09-02</summary>

| Date | Transition |
| --- | --- |
| 2026-09-02 | Implemented |
| 2026-09-14 | Drafted |
| 2026-09-14 | Accepted |

</details>

*This record is a reconstruction. The change merged on 2026-09-02, before
`docs/decisions/` existed; the record was written on 2026-09-14 from
[the pull request](https://github.com/RadicalZephyr/sodium-rust/pull/30),
[the issue it closes](https://github.com/RadicalZephyr/sodium-rust/issues/14),
[the upstream thread that issue descends from](https://github.com/SodiumFRP/sodium-rust/issues/48),
the commit messages, the `THEORY` write-up that shipped in
`tests/closure_type_inference.rs` -- now a pointer to this record rather than a
second copy of it -- and the diff. So the log runs backwards, and the rule that
the last row is the current state does not hold here; the summary line is. That
is the honest reading of a record written after the fact, and we are deliberately
not generalising it: [`README.md`](README.md)'s rule stands, because writing
records late is not something we plan to do again. Three things below come from
outside those sources and are marked where they appear: two are this record's own
reading of the API it replaced, and the account of why the diagnosis stalled for
two and a half years is the author's, given while the record was being written.*

## Context

This starts upstream, in
[SodiumFRP/sodium-rust#48](https://github.com/SodiumFRP/sodium-rust/issues/48),
and it starts as an aside in a thread about something else. That issue, opened
2020-06-19, is about whether the `IsLambda*` traits should be bounded on `Fn`
rather than `FnMut`. On 2020-07-14 the inference problem arrives in it:

> I also think there is another breaking change that could result in a
> significant improvement in using Sodium. I believe the `IsLambda*` traits block
> type inferencing from working as it normally does when passing closures to
> functions that use those traits as a bound.

That is the conclusion of this record, four years before anything confirmed it
and six before anything acted on it. What follows in that thread, argued between
RadicalZephyr and clinuxrulz over about a week, is most of the *Alternatives*
section below -- including the design that eventually shipped. It ends
unresolved.

[Issue #14](https://github.com/RadicalZephyr/sodium-rust/issues/14), opened
2024-03-03 in this fork under the title *Type boilerplate makes it look like
Java*, restates it as a complaint rather than an aside:

> There is quite a lot of syntactic overhead in writing Sodium code, especially
> when compared to the very small amounts of actual working code. One of the
> major contributors to this is needing to annotate the types on the closures
> passed to all the combinator methods.

It was filed as a pain point and put on the 3.0 milestone -- a budget set in the
2020 thread, where clinuxrulz had already ruled that "any changes to the existing
API will be changing the 1st number in the version", and concluded: "No choice
but to go v3 for inference. Which is OK with me, inference improves readability."
So a major version was the agreed price four years before the fix existed. The
complaint is also not about inference as such. It is about
a ratio -- ceremony to working code -- and that is the thing to keep in view,
because it is what decides between the alternatives further down. Two designs
can both restore inference and still differ on what they charge a reader.

The annotations trace back to how this library asks a caller to declare what a
closure captures.

Sodium's garbage collector has to know what every node in the graph holds a
reference to. Most of the time it can see that from the shape of the network:
`snapshot` names the cell it samples, `lift2` names both its inputs. A closure
defeats this. When a closure captures a `Cell` and calls `sample()` on it, that
is a real edge in the graph and nothing in the network's shape reveals it, so the
caller has to declare it. `Dep` is a handle on a `GcNode`, and declaring one is
how the collector's tracer is told about an edge it cannot introspect.

This is not a Rust-specific requirement -- the family's other ports carry the
same obligation, and the original README justified our spelling of it by
pointing at one: *"Sodium objects within lambda expressions are traced via
lambda1, lambda2, etc. just like the TypeScript version does."*

The spelling was a family of traits. `IsLambda1`..`IsLambda6` each had two
implementations: a blanket one for any `FN: FnMut(&A) -> B`, whose `deps_op()`
returned `None`, and one for `Lambda<FN>`, a struct pairing a function with a
`Vec<Dep>`. Every function-taking combinator was bounded on the trait, so one
method accepted both shapes:

```rust
stream.map(|a: &i32| *a + 1)
stream.map(lambda1(move |a: &i32| *a + c.sample(), vec![c.to_dep()]))
```

## What the bound cost

Closure arguments stopped inferring. Every closure passed to the API needed its
parameter annotated -- and the annotation the README's own worked example needed
was not a tidy one:

```rust
let csw = csw_str.map(lambda1(
    move |s: &&'static str| if *s == "ca" { ca.clone() } else { cb.clone() },
    deps,
));
```

The cause is that rustc's closure signature deduction only fires for the `Fn`
family. Before it type-checks a closure body it looks for an *expected signature*
among the obligations on the closure's type variable, and `deduce_closure_signature`
considers a fixed set of sources: `Fn`/`FnMut`/`FnOnce`, `AsyncFn*`, and the
associated-type projections that go with them. An obligation of the form
`?F: IsLambda1<i32, ?B>` is not one of them, so no expected signature came back,
`a` stayed a bare inference variable, and `*a` failed with
`error[E0282]: type annotations needed`.

The information that would have resolved it did exist -- selecting the blanket
impl against `?F: IsLambda1<i32, ?B>` yields `?F: FnMut(&i32) -> ?B` -- but that
selection happens after the body has been checked. Closure signature inference is
a pre-pass, not a fixpoint.

**Experiment -- a non-`Fn` trait bound defeats closure signature deduction**

```rust
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
    // The shape `Stream::map` used to have.
    pub fn map_via_trait<B, F: IsLambda1<A, B>>(&self, _f: F) {}
    // The shape it has now.
    pub fn map_via_fnmut<B, F: FnMut(&A) -> B>(&self, _f: F) {}
}

fn main() {
    let s = Stream(1i32);
    s.map_via_fnmut(|a| *a + 1); // infers
    s.map_via_trait(|a| *a + 1); // rejected
}
```

```text
error[E0282]: type annotations needed
  --> src/main.rs:23:22
   |
23 |     s.map_via_trait(|a| *a + 1); // rejected
   |                      ^  -- type must be known at this point
   |
help: consider giving this closure parameter an explicit type
   |
23 |     s.map_via_trait(|a: /* Type */| *a + 1); // rejected
   |                       ++++++++++++
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=877a2a8f0bb0962594d3ac5bc1d88c19)

Two things that reduction pins down. The element type is concretely known from
`Self`, exactly as it is for `Stream<i32>::map`, so the failure was never about
`A` being open. And the `FnMut`-bounded method sitting beside it infers from the
same call, so it is the bound and nothing else.

The other natural suspect was overlap between the two impls. It was not that
either:

**Experiment -- one impl, no ambiguity, same rejection**

```rust
pub trait IsLambda1<A, B> {
    fn call(&mut self, a: &A) -> B;
}

// The only impl in scope: no `Lambda<FN>` for it to be ambiguous with.
impl<A, B, FN: FnMut(&A) -> B> IsLambda1<A, B> for FN {
    fn call(&mut self, a: &A) -> B {
        self(a)
    }
}

pub fn apply<A, B, F: IsLambda1<A, B>>(_a: A, _f: F) {}

fn main() {
    apply(1i32, |a| *a + 1); // still rejected
}
```

```text
error[E0282]: type annotations needed
  --> src/main.rs:15:18
   |
15 |     apply(1i32, |a| *a + 1); // still rejected
   |                  ^  -- type must be known at this point
   |
help: consider giving this closure parameter an explicit type
   |
15 |     apply(1i32, |a: /* Type */| *a + 1); // still rejected
   |                   ++++++++++++
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=a7063ccd096f5ee32403ef055f1397e7)

`|a: &_|` was usually enough to get past this, and understanding why is what
tells you the annotation burden had no ceiling. The `&_` supplied the one thing
the pre-pass could not: the *shape* of the parameter. With `a: &'?r ?U` known to
be a reference, `*a + 1` type-checks as `?U: Add<i32>` without `?U` resolved, and
`?U` gets filled in later from the trait obligation. The annotation was carrying
the indirection, not the type.

Which made `&_` a floor rather than a guarantee. It worked whenever the body
constrained the referent directly, and stopped working when the body only
constrained an associated type of it -- `<?U as Neg>::Output == i32` says nothing
about `?U`, because `Neg::Output` is not injective. There the full `&i32` was
needed. `infers_even_when_body_only_constrains_an_associated_type` in
[`tests/closure_type_inference.rs`](../../tests/closure_type_inference.rs) is that
case, now passing with no annotation at all.

## Why this took six years

Nobody had to think of the answer. clinuxrulz proposed it on 2020-07-14, in the
shape it eventually shipped:

> Another option to keep dependency tracking and type inference is to make twins
> for all the API methods. E.g.
>
> ```rust
> fn map<FN:Fn(A)->B>(&self, fn: FN) -> Stream<B>
> fn map_w_deps<FN:Fn(A)->B>(&self, fn: FN, deps: Vec<Dep>) -> Stream<B>
> ```
>
> Just a bit painful with all the boilerplate.

That is the decision below, down to the `Vec<Dep>` parameter; only the name moved,
`map_w_deps` to `map_with_deps`. Three days later he answered the implementation
question too -- "It's just delegating the 'non with deps' method calls to the
'with dep' ones carrying an empty `Vec` of deps" -- which is exactly what the base
methods do today. Even the cost is his: *just a bit painful with all the
boilerplate* is the 23 siblings, priced correctly on sight.

So the six years are not a story about an idea nobody had. They are a story about
an idea nobody could **choose**, because it was the expensive option on a list
with two cheaper-looking ones and no way to eliminate either:

- **`Fn(...) + Deps`** -- RadicalZephyr's proposal the same day: move `deps_op`
  into a trait of its own and bound on the combination. One extra bound on the
  methods that exist, instead of 23 new ones.
- **`impl Fn for Lambda`** -- clinuxrulz's question the same day, "Will Rust let
  us implement `Fn` or `FnMut` for our own types anyway? Such as `Lambda`?",
  which would have dissolved the problem entirely.

Both are settled under *Alternatives* below, by experiments that take five minutes
and did not exist in 2020: one shows `Fn(...) + Deps` rejecting every
deps-carrying call site, the other shows `impl Fn for Lambda` still behind a
feature gate six years on.

And the one thing the thread did establish empirically pointed away from the
cause. clinuxrulz found the `&_` workaround on 2020-07-19 -- "This works: `let s2
= s1.map(|x: &_| *x + 1);` It infers the type of `x`, but you still need to say it
is a reference type" -- and drew the natural conclusion from it: "It means type
inference should work fully (no type hints), if the `IsLambda` did not use `&` on
its input types." That is wrong, and wrong in the direction that hides the
problem. `&_` works because it supplies the *indirection* the pre-pass could not,
not because the `&` in the bound was at fault; strip the `&` and the deduction
still never fires through a non-`Fn` trait. The thread's only measurement made the
bound look incidental, and the argument moved on to whether arguments should be
passed by reference at all.

RadicalZephyr closed the 2020 discussion there: "I think my thoughts on this API
change are pretty half-baked right now, and not really grounded in how sodium is
actually implemented currently."

The 2024 comment on issue #14 is the same wall from the other side, posted a
minute after the issue itself:

> My current suspicion about what is causing the type inference to fail is the
> `IsLambda*` traits. I haven't been able to successfully replace them in order
> to prove this tho.

Correct again, stalled again, and for the same reason both times: the only
experiment in view was replacing the traits in the library, and replacing them
*is* the change. The cost of testing the hypothesis was the cost of acting on it,
which is a bad position to reason from -- you cannot afford to be wrong, so you
do not try.

What broke the deadlock is that the failure does not need the library at all. It
reproduces in a trait declaration, a blanket impl and one call, which is the first
experiment above. Once that costs five minutes, so does every follow-up: *is it
the two impls overlapping?* *Would an extra bound fix it?* *What does the
deps-carrying call site do under that bound?* Each of those is a question the
six-year version of this problem could not afford to ask, and each of them moved
the argument -- the first ruled out the obvious alternative cause, and the other
two are what finally made the twin-methods design choosable rather than merely
available.

None of that last part is visible in the commits; it comes from the author's
account, given while this record was being written. The question was put fresh in
2026 and the standalone reproduction came back immediately, which is the whole of
the difference between 2020 and 2026: the hypothesis did not improve and the
design did not change, the cost of checking them collapsed.

Which is [ADR-0001](0001-recording-important-decisions.md)'s argument for keeping
research cheap and minimal, arrived at from the other end and before that record
existed. This one is its first customer, and the evidence it needed turned out to
be evidence nobody had been able to produce for six years, while the only
available experiment was the change itself.

## Two costs the commits did not name

Neither of these is recovered from the change; they are this record's own reading
of the API it replaced. Both argue in the same direction as the decision, which
is worth saying out loud -- a reconstruction has every opportunity to flatter the
choice that was made.

**The public surface was inverted.** `IsLambda1`..`IsLambda6` were exported and
visible in rustdoc. `Dep`, `Lambda` and `lambda1`..`lambda6` were all
`#[doc(hidden)]`. So the trait a caller never needs to name was the documented
part, and every constructor a caller *did* need was hidden -- including the one
the README told them to reach for. The old arrangement documented the mechanism
and hid the interface.

**The scheme was already not uniform, invisibly.** `Cell::listen_weak` was
bounded on `FnMut` rather than `IsLambda1`, at both layers, so a dependency could
not be declared on a cell's weak listener at all -- while `Stream::listen_weak`
and `Cell::listen` beside it accepted them. Nobody noticed, and the reason they
could not notice is structural: under the old scheme, *whether a method accepts
dependencies* was a fact about a trait bound rather than a fact about the method
list. You had to read the signature to find out, and the answer looked the same
whether it was deliberate or an oversight.

## Decision

Split the API by shape -- clinuxrulz's twin methods from 2020, built.



- Every function-taking combinator is bounded on `FnMut`/`Fn` directly, so bare
  closures infer: `stream.map(|a| *a + 1)`.
- Every one of them gains a `*_with_deps` sibling taking an explicit `Vec<Dep>`
  -- 23 methods across `Stream` and `Cell`. The base method delegates to its
  sibling with an empty `Vec`, so there is one implementation per combinator.
- `IsLambda1`..`IsLambda6`, `Lambda` and `lambda1`..`lambda6` remain the
  mechanism underneath, and the traits join the rest of it behind
  `#[doc(hidden)]`. `Dep`, `Cell::to_dep` and `Stream::to_dep` come out from
  behind it, because they now appear in public signatures. The visibility of the
  module is exactly inverted, which was the point.

```rust
stream.map(|a| *a + 1)
stream.map_with_deps(move |_| cell.sample(), vec![cell.to_dep()])
```

The dependency-carrying form stays exactly as expressive as it was; what changes
is that its cost is paid by the call sites that use it rather than by all of
them.

## Alternatives considered

**Leave the bounds alone and annotate.** The status quo, and the cost is
permanent and paid per call site. It is also unbounded rather than fixed:
`|a: &_|` is a floor, and the cases that need the full type are not signposted --
you discover them by getting `E0282` and widening the annotation until it stops.

**Keep a trait for the deps and put an `Fn`-family bound beside it.** This is the
2020 thread's other proposal: separate `deps_op` into a `Deps` trait of its own,
implement it for both `Lambda` and bare functions, and bound the combinators on
`Fn(...) + Deps`. The reduction below tests the same shape in the form closest to
what this crate actually had, `IsLambda1<A, B> + FnMut(&A) -> B`, because the
question it settles -- what happens to `Lambda` when an `Fn`-family bound is in
the list -- does not care which trait carries `deps_op` or how many there are. It is the cheap option -- one extra bound against 23 new
methods -- and it went six years without anyone establishing whether it works. It
half does. The first half works, which is what kept it alive:

**Experiment -- the extra bound rescues the closure and rejects the `Lambda`**

```rust
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

pub struct Stream<A>(A);

impl<A> Stream<A> {
    // The old bound, with `+ FnMut(&A) -> B` bolted on.
    pub fn map<B, F: IsLambda1<A, B> + FnMut(&A) -> B>(&self, _f: F) {}
}

fn main() {
    let s = Stream(1i32);
    s.map(|a| *a + 1); // the extra bound makes this infer
    s.map(Lambda {
        f: |a: &i32| *a + 1,
        deps: 3,
    }); // and rejects this
}
```

```text
error[E0277]: expected an `FnMut(&i32)` closure, found `Lambda<{closure@src/main.rs:40:12: 40:21}>`
  --> src/main.rs:39:11
   |
39 |       s.map(Lambda {
   |  _______---_^
   | |       |
   | |       required by a bound introduced by this call
40 | |         f: |a: &i32| *a + 1,
41 | |         deps: 3,
42 | |     }); // and rejects this
   | |_____^ expected an `FnMut(&i32)` closure, found `Lambda<{closure@src/main.rs:40:12: 40:21}>`
   |
help: the trait `for<'a> FnMut(&'a i32)` is not implemented for `Lambda<{closure@src/main.rs:40:12: 40:21}>`
  --> src/main.rs:6:1
   |
 6 | pub struct Lambda<FN> {
   | ^^^^^^^^^^^^^^^^^^^^^
note: required by a bound in `Stream::<A>::map`
  --> src/main.rs:33:40
   |
33 |     pub fn map<B, F: IsLambda1<A, B> + FnMut(&A) -> B>(&self, _f: F) {}
   |                                        ^^^^^^^^^^^^^^ required by this bound in `Stream::<A>::map`
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=fcc3c4845a3f949e77cdee7540ba1124)

One error, on the second call. The bare closure infers, so the extra bound does
fix inference -- and the same bound rejects `Lambda<FN>`, which is a plain struct
and cannot implement `FnMut` on stable. The one-line fix breaks every
dependency-carrying call site, which is the only thing `IsLambda1` ever existed
to accept. `Deps` as a separate trait changes nothing about this: whatever else
is in the bound, `Fn(...)` is in it, and `Lambda` cannot satisfy it. The cheap
option was never available, and five minutes at any point after 2020 would have
said so.

**Make `Lambda<FN>` implement `FnMut`.** clinuxrulz raised this on 2020-07-14 --
"Will Rust let us implement `Fn` or `FnMut` for our own types anyway? Such as
`Lambda`? Last time I tried, Rust would not let me" -- then found the `Fn` impls
for `Box<dyn Fn>` added in Rust 1.35 and took them for general permission.
RadicalZephyr corrected it three days later: those are impls *on* a std type, and
writing your own is still gated. This is the clean end state, and worth being
precise about, because it is the alternative most likely to become available. It collapses the whole problem: with those impls, a single `FnMut`
bound accepts a bare closure and a `Lambda` alike, so there is no split and no
`*_with_deps`. It needs `unboxed_closures` and `fn_traits`.

**Experiment -- the collapsed design, and what blocks it**

```rust
#![feature(unboxed_closures, fn_traits)]

pub struct Lambda<FN> {
    pub f: FN,
    pub deps: usize,
}

impl<FN: FnMut(&i32) -> i32> FnOnce<(&i32,)> for Lambda<FN> {
    type Output = i32;
    extern "rust-call" fn call_once(mut self, args: (&i32,)) -> i32 {
        (self.f)(args.0)
    }
}

impl<FN: FnMut(&i32) -> i32> FnMut<(&i32,)> for Lambda<FN> {
    extern "rust-call" fn call_mut(&mut self, args: (&i32,)) -> i32 {
        (self.f)(args.0)
    }
}

// With those impls, one bound accepts both shapes.
pub fn map<F: FnMut(&i32) -> i32>(_f: F) {}

fn main() {
    map(|a| *a + 1);
    map(Lambda {
        f: |a: &i32| *a + 1,
        deps: 3,
    });
}
```

```text
error[E0554]: `#![feature]` may not be used on the stable release channel
 --> src/main.rs:1:1
  |
1 | #![feature(unboxed_closures, fn_traits)]
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=2e9a07756dac9e163e53362cb6620a1e)

Nothing is wrong with the implementation -- the same file compiles and runs on
nightly 1.100.0 (2026-09-13, checked 2026-09-14). The gate is the whole
objection, and a library whose MSRV is 1.71 cannot take it. Six years after
RadicalZephyr wrote that it "doesn't really look like it's even ready to be
stabilized any time soon", that assessment has held, which is the most useful
thing this experiment says: the option is not arriving on its own.

**Declare dependencies after construction**, as `stream.map(f).with_deps(...)`.
Mechanically this is available: `Node::add_update_dependencies` exists and
`split_enum2` already calls it after building its node. We turned it down on
three counts, the last of which is not stylistic. It detaches the declaration
from the closure it describes, so a reviewer reading the closure cannot see
whether its captures were declared. The returned handle is often not the node
holding the closure -- `accum` returns a `Cell` and `collect` a `Stream` whose
closure lives in an intermediate node -- so the method would have to be defined
per combinator anyway. And it publishes a node whose dependency set is knowingly
incomplete: today `map` adds `f_deps` inside the constructor, before the node is
reachable, whereas a postfix call leaves a window during which the collector's
view of the graph is wrong, and `collect_cycles` runs at the end of every
outermost transaction. We did not build a repro, so this is a hazard rather than
a demonstrated bug -- but it is the kind of hazard this library cannot absorb,
and avoiding it costs nothing.

**A macro taking the closure and the deps together.** Rejected quickly: closure
inference through a macro is the thing we set out to fix, and a macro costs
method-position completion, which is most of what makes a combinator library
navigable.

**A macro that works the deps out for itself.** The 2020 thread's most ambitious
idea, and a better one than the above. clinuxrulz found that
[`serde_closure_derive`](https://docs.rs/serde_closure_derive/0.3.1/src/serde_closure_derive/lib.rs.html)
visits the variables a closure captures, on stable, and proposed
`s1.map(lambda!(move |x| ca.sample() + x))` -- where `lambda!` discovers `ca`
itself -- "Or even a `get_deps!` to use on regular closures and eliminate the need
for `IsLambda`/`Lambda` altogether." He also filed the objection: "The example
from serde seems quite complex. Worried if it would keep working as the Rust
language changes over time."

His objection stands, and this proposal deserves better than the one above it: it
does not lose inference the way a deps-passing macro does, because it takes an
ordinary closure. The reason to turn it down is elsewhere, and is not the one we
expected. We assumed a capture visitor would fail *loudly* -- it yields
identifiers rather than types, so a closure capturing an ordinary `i32` alongside
a cell would emit `offset.to_dep()` and not compile. That is wrong twice over.

**Experiment -- a capture visitor filters correctly and still misses the node**

```rust
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
struct Dep(&'static str);

#[derive(Clone)]
struct Cell(Arc<&'static str>);

impl Cell {
    fn new(name: &'static str) -> Cell {
        Cell(Arc::new(name))
    }
    fn to_dep(&self) -> Dep {
        Dep(*self.0)
    }
    fn sample(&self) -> &'static str {
        *self.0
    }
}

// A capture visitor yields identifiers, not types, so the code it emits has to
// decide per capture whether that identifier is a node. Inherent methods beat
// trait methods, which gives the filter on stable: `Cell` takes the inherent
// `dep`, everything else falls through to the trait.
struct Probe<T>(T);

impl<'a> Probe<&'a Cell> {
    fn dep(&self) -> Option<Dep> {
        Some(self.0.to_dep())
    }
}

trait NotANode {
    fn dep(&self) -> Option<Dep>;
}

impl<T> NotANode for Probe<T> {
    fn dep(&self) -> Option<Dep> {
        None
    }
}

/// Stands in for `lambda!`. The bracketed list is what a capture visitor finds:
/// the free identifiers of the closure body. Deps are collected before the
/// closure is built, since building it moves the captures.
macro_rules! lambda {
    ([$($cap:ident),* $(,)?] $f:expr) => {{
        let deps: Vec<Dep> = {
            let mut d = Vec::new();
            $( if let Some(x) = Probe(&$cap).dep() { d.push(x); } )*
            d
        };
        ($f, deps)
    }};
}

fn main() {
    let ca = Cell::new("ca");
    let cb = Cell::new("cb");
    let hidden = Cell::new("hidden");
    let offset = 100i32;

    // 1. Two cells named directly -- the `switch_c` shape.
    let (a, b) = (ca.clone(), cb.clone());
    let (picks, picks_deps) =
        lambda!([a, b] move |left: bool| if left { a.sample() } else { b.sample() });

    // 2. A cell alongside an ordinary captured value.
    let c = ca.clone();
    let (biased, biased_deps) = lambda!([c, offset] move |n: i32| (c.sample(), n + offset));

    // 3. The same dependency, one shared handle away.
    let registry: Arc<Mutex<Vec<Cell>>> = Arc::new(Mutex::new(vec![hidden.clone()]));
    let (looks_up, looks_up_deps) =
        lambda!([registry] move |i: usize| registry.lock().unwrap()[i].sample());

    println!("picks    [a, b]     -> {:?}", picks_deps);
    println!("biased   [c, offset] -> {:?}", biased_deps);
    println!("looks_up [registry] -> {:?}", looks_up_deps);
    println!();
    println!("picks(true) = {}", picks(true));
    println!("biased(1)   = {:?}", biased(1));
    println!("looks_up(0) = {}", looks_up(0));
}
```

```text
picks    [a, b]     -> [Dep("ca"), Dep("cb")]
biased   [c, offset] -> [Dep("ca")]
looks_up [registry] -> []

picks(true) = ca
biased(1)   = ("ca", 101)
looks_up(0) = hidden
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=d4660d00360181b937d49c7dc7fd3f5f)

Filtering by type is not the obstacle: an inherent method beats a trait method, so
`Probe` sorts nodes from ordinary captures on stable with no specialization
feature, and the mixed case compiles and reports exactly the one real dependency.
The obstacle is the third line. `looks_up` reaches `hidden` -- the output below it
says so -- and the visitor reports nothing, because the identifier it can see is
`registry`, and `registry` is not a node.

Which is the same failure, in the same place, as the refcount experiment below:
one layer of sharing between the closure and the node, and the node is invisible.
That is not a coincidence. **Whether a closure reaches a node is a property of
what its handles point at, which is neither a syntactic property of the closure's
text nor a shallow property of its captures.** A capture visitor reads the text. A
refcount delta reads one level of `Arc::clone`. Both answer a question adjacent to
the one that matters, and both answer it confidently.

That rules out these two techniques. It does not rule out automatic discovery,
and an earlier draft of this record let the first slide into the second. The
property being sought is reachability through arbitrary data, and the answer the
Rust GC ecosystem converged on is a `Trace` trait: every type declares how to find
the collector's pointers inside it, recursively, with a derive for the common case
-- [`ferris_gc::Trace`](https://docs.rs/ferris-gc/latest/ferris_gc/trait.Trace.html)
is one of several (checked 2026-09-14). Recursion is the thing both techniques
above lack, and neither can be patched into having it.

We already have the visitor half. `src/impl_/gc_node.rs` declares

```rust
pub type Tracer<'a> = dyn FnMut(&GcNode) + 'a;
pub type Trace = dyn Fn(&mut Tracer) + Send + Sync;
```

and every `GcNode` carries one. What is missing is a *trait*, letting that visitor
walk a user's data, and a derive so that writing one is not a per-type obligation.
In that vocabulary `Dep` is a hand-rolled trace for the single boundary the
protocol cannot cross -- a closure, whose captures have no type to implement
anything on -- and `*_with_deps` asks the caller to write the depth-one case
themselves. That is a fair description of what this crate can check today rather
than a claim about what is checkable. Whether it should grow into a real trait is
in *Open questions*.

So we are not willing to buy ergonomics with a mechanism that silently
under-reports, when a `Dep` that is wrong corrupts the collector's reference
counting. `*_with_deps` makes the same mistake possible, puts it at the call site
where a reviewer reads it, and claims nothing about completeness. Worth revisiting
only if capture analysis gains a supported footing *and* a way to see through
indirection -- and the second is the hard half.

**Derive the deps by watching reference counts.** clinuxrulz's other 2020 idea,
and the only proposal in that thread nobody answered:

> The idea is to execute `clone` on a lambda, then work out which sodium objects
> just had their reference count increased. That way you can work out which
> sodium objects are referenced in a lambda without the end user telling the
> library.

It is a good idea, and it half works, which is the problem.

**Experiment -- refcount deltas miss a dependency held behind a shared handle**

```rust
use std::sync::{Arc, Mutex};

/// Stands in for a sodium node. What the technique observes is its strong count.
#[derive(Clone)]
struct Node(Arc<&'static str>);

impl Node {
    fn new(name: &'static str) -> Node {
        Node(Arc::new(name))
    }
    fn count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

/// The 2020 proposal: clone the lambda, and whichever nodes just had their
/// reference count go up are the ones it captured.
///
/// `candidates` is a gift. The real library has no enumeration of live nodes --
/// not knowing what a closure touched is the whole problem -- so this is the
/// technique at its most favourable.
fn deps_by_clone<F: Clone>(f: &F, candidates: &[(&'static str, &Node)]) -> Vec<&'static str> {
    let before: Vec<usize> = candidates.iter().map(|(_, n)| n.count()).collect();
    let twin = f.clone();
    let after: Vec<usize> = candidates.iter().map(|(_, n)| n.count()).collect();
    drop(twin);

    candidates
        .iter()
        .zip(before.iter().zip(after.iter()))
        .filter(|(_, (b, a))| a > b)
        .map(|((name, _), _)| *name)
        .collect()
}

fn main() {
    let ca = Node::new("ca");
    let cb = Node::new("cb");
    let hidden = Node::new("hidden");
    let untouched = Node::new("untouched");

    // The motivating case: a closure picking between two captured cells, which
    // is what `switch_c` needs declared. Both are real dependencies.
    let (a, b) = (ca.clone(), cb.clone());
    let picks = move |left: bool| if left { a.clone() } else { b.clone() };

    // The same dependency, reached through one shared handle -- a closure
    // holding a collection of nodes rather than a named one.
    let registry: Arc<Mutex<Vec<Node>>> = Arc::new(Mutex::new(vec![hidden.clone()]));
    let looks_up = move |i: usize| registry.lock().unwrap()[i].clone();

    let candidates = [
        ("ca", &ca),
        ("cb", &cb),
        ("hidden", &hidden),
        ("untouched", &untouched),
    ];

    println!("picks    captures ca, cb -> {:?}", deps_by_clone(&picks, &candidates));
    println!("looks_up reaches  hidden -> {:?}", deps_by_clone(&looks_up, &candidates));

    // Both closures really do reach those nodes.
    println!();
    println!("picks(true)  = {}", *picks(true).0);
    println!("looks_up(0)  = {}", *looks_up(0).0);
}
```

```text
picks    captures ca, cb -> ["ca", "cb"]
looks_up reaches  hidden -> []

picks(true)  = ca
looks_up(0)  = hidden
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=7351b48d415c963df5a406db61a81fca)

The first line is the technique working on precisely the case that motivates
`*_with_deps` -- `with_deps_tracks_cells_captured_by_a_closure` in
[`tests/closure_type_inference.rs`](../../tests/closure_type_inference.rs) has the
same shape, and both cells are found.

The second line is why it cannot be adopted. `looks_up` reaches `hidden` at call
time, as the output below it shows, and the technique reports that it depends on
nothing. `Arc::clone` is shallow by construction: cloning the closure bumps the
count on the one handle it captured and on nothing the handle points at. One
layer of sharing between a closure and a node makes the node invisible, and
*sharing is what a handle is for* -- a closure that holds a collection of nodes,
or any node reached through a structure the closure clones by pointer, lands
here.

That is the worst failure this library has. A trace that misses an edge frees
live data, and this misses it while returning a confident empty answer rather
than an error. Compare the failure mode of `*_with_deps`: a caller who forgets a
`Dep` makes the same mistake, but the declaration is at the call site where a
reviewer reads it, and the mechanism never tells anyone the list is complete.

Three further costs, none of which the experiment needed to reach. The candidate
list it is handed does not exist: the library has no enumeration of live nodes,
and building one means a registry of every node in the graph, whose entries are
themselves references. `F: Clone` would have to join every combinator bound,
which is its own breaking change and excludes any closure capturing something
unclonable. And a strong count is global state, so a concurrent clone or drop of
the same node between the two reads changes the answer -- these bounds are
`Send + Sync` and `ThreadedMode` exists to make the evaluator concurrent.

Worth revisiting only under the same condition as the capture-visiting macro
above, and for the same structural reason: dependency discovery would have to see
through indirection, which a refcount delta cannot do by construction.

## Consequences

**It is a breaking change, and a mechanical one.** `combinator(lambda1(f, deps))`
stops compiling; the migration is `combinator_with_deps(f, deps)`, and it drops
an annotation rather than adding one. `CHANGELOG.md` carries the before/after.
The breakage was budgeted rather than discovered, and budgeted early: clinuxrulz
settled in the 2020 thread that a change to the existing API means a major
version, issue #14 carried the 3.0 milestone from 2024, and the price was never
in dispute at any point in between. The change is unreleased at the time of
writing -- the crate is at 2.1.3 and the entry is under `Unreleased`.

**The function-taking surface doubles.** 23 base methods gained 23 siblings, all
of which appear in rustdoc, and `map` now sorts next to `map_to` and
`map_with_deps`. This is a real cost, it lands on the reader of the documentation,
and it is the cost clinuxrulz named in the same breath as the design -- *just a
bit painful with all the boilerplate*. Six years of looking for something cheaper
did not turn one up. Against issue #14's ratio, that is the trade we chose: ceremony
moves off the call sites, where it was charged to everyone on every closure, and
onto the method index, where it is charged once to whoever is reading it -- and
the alternative was charging every call site for a feature few call sites use.
It is still the cost most likely to be regretted, and the one that disappears if
`fn_traits` ever stabilises.

**The capability is now visible in the method list.** Whether a combinator can
take dependencies used to be a fact about a trait bound; it is now a fact about
whether a name exists. That is what makes `Cell::listen_weak`'s gap a bug that
gets fixed rather than an asymmetry nobody can see -- it gained
`listen_weak_with_deps` along with everything else.

**We gave up name-level correspondence with the other ports.** *(This record's
reading, not the change's.)* The original README justified `lambda1` by pointing
at the TypeScript port, and someone carrying code across now has to translate
`lambda1(f, deps)` into `map_with_deps(f, deps)`. The correspondence that
matters is conceptual -- declare what a closure captures -- and `*_with_deps`
keeps it, arguably more legibly, since the declaration is now named at the call
rather than wrapped around the function. What the ports cannot share is the
*cost* of a spelling, which falls differently in each language. In Rust it fell
on closure inference at every call site, deps or no deps, and that is not a bill
the family convention was ever asking us to pay.

**`Dep` correctness became a documented obligation.** The `*_with_deps`
signatures put `Vec<Dep>` in front of users, so the README now has to say what it
did not before: the declared deps must mirror what the closure actually captures,
and naming a node the closure does not hold corrupts the collector's reference
counting. The hazard is not new. It was previously reachable only through a
`#[doc(hidden)]` constructor, which is worse.

## Where this record's evidence lives

*This section is a decision made on 2026-09-14, by this record. It was not part
of the original change.*

The first three reductions above shipped with the change as `compile_fail` cases
under `tests/ui/` -- four files, since trybuild needs the two halves of the
`+ FnMut` argument split into a passing case and a failing one, where a
Playground needs only the one file. They were never regression guards. A
`compile_fail` case guards something when the rejection is ours to make -- a
closure shape this crate's bounds refuse is part of the contract, and worth
pinning. These four reject nothing of ours. They establish that rustc's closure
signature deduction does not look through a user-defined trait, which is a fact
about the compiler that holds whatever this library does, and their own comments
said as much: they were there to keep the reasoning checked rather than merely
asserted. That is the definition of evidence, and
[ADR-0001](0001-recording-important-decisions.md) says where evidence goes and
why trybuild is the wrong home for it: a `compile_fail` case pins diagnostic
wording we neither control nor promise, and `rustversion` gating excludes beta
and nightly but not an *older* stable, so a contributor behind the toolchain the
files were blessed against gets a red build out of a diff they did not write.

That was not hypothetical. ADR-0001's own context names these files as the
prototyping artifacts that "have been failing for reasons unrelated to this
library ever since," and on 2026-09-14 `cargo test --test ui` on rustc 1.94.1
still failed `fn_bound_rejects_lambda` on the article in `expected a` versus
`expected an`.

So the reductions move here, as Playground experiments, and leave the test suite.
Three of the experiments are new. The one on `fn_traits` was a claim in a commit
message with nothing behind it, and is the claim in this record most likely to
expire. The two on automatic dependency discovery answer 2020 proposals
that nobody answered at the time -- one of them by contradicting what this record
first assumed about it -- which is the other thing a record is for: a rejected
alternative stays rejected only while the reason is on file and checkable.
`tests/ui/bare_closures.rs` stays: it exercises every function-taking combinator
with an unannotated closure from outside the crate, which is a property we do
promise, and it carries no expected output, so it runs on every channel. The
`#[rustversion::stable]` gate the reductions needed stays too, empty, because the
distinction above is between kinds of case rather than a ban -- a case that pins
a rejection this crate promises still needs somewhere gated to sit.

## Open questions

**`split_enum2` and `split_enum3` have no dependency path at all.** They are
bounded on `Fn` in both layers and build their nodes without consulting a
`Lambda`, so there is nowhere for deps to enter -- they were already outside the
old scheme and they are outside the new one. A closure passed to `split_enum2`
that captures a `Cell` and samples it has no way to say so. Whether this is a gap
to close with two more siblings or a sign that these combinators want a different
treatment, we have not worked out; the change did not touch them and this record
is not deciding it.

**The bad-diagnostics thread was never pulled.** Issue #14 also said that "in
some cases the error messages when you have incorrect types are actually quite
bad (this might be worth reporting to the Rust compiler)". Nobody followed that
up, and the fix has made it hard to reach: with `FnMut` bounds the diagnostics
are now the ordinary ones rustc gives any closure, so whatever was bad about them
is mostly no longer reachable through this API. What we do not know is whether
there was a reportable rustc bug in there, distinct from the deduction behaviour
documented above -- which is working as designed and not a bug at all. If anyone
still has one of those error messages, it is worth a look before the memory of
them goes.

**Whether closure captures should be traced rather than declared.** `Dep` is a
manual, depth-one trace for the one boundary the collector's visitor cannot cross,
and the ecosystem's answer to the general problem -- a `Trace` trait plus a derive
-- recurses, so it does not have the blind spot both experiments above found.
Adapting it here is not a small change, and at least three questions come first. A
trait cannot be implemented for an anonymous closure type, so the closure would
have to be desugared into a named struct with typed fields -- the capture-visiting
macro again, this time doing something it can actually do. Our nodes are
`Arc`-backed and the graph is full of `parking_lot` locks while the collector runs
at the end of the outermost transaction, so tracing *through* a lock is a deadlock
question rather than a traversal one; it is worth establishing whether the
ecosystem's `Trace` impls cover `Arc`, `Mutex` and `RwLock` at all, or stop at
owned containers for exactly that reason. And a trace that misses an edge frees
live data, so whether such a trait is `unsafe` is a real decision rather than a
style one.

**What the 2020 thread was actually about.** Issue #48 opened on whether the
combinators should be bounded on `Fn` rather than `FnMut`; closure inference was
the aside that grew. This record settles the aside and leaves the original
question, which is live work rather than an open question in the usual sense:
[PR #34](https://github.com/RadicalZephyr/sodium-rust/pull/34) moves 38
combinator bounds to `Fn` while the 8 listener bounds stay `FnMut`, and was open
at the time of writing. It resolves #48 and deserves its own record -- it turns on
a measurement, and on a counter-argument from the same 2020 thread that this one
does not touch. Where the text above says a combinator is bounded on
"`FnMut`/`Fn`", that describes `main` as of 2026-09-14, and #34 is the reason to
check rather than trust it.

**Whether stabilisation should collapse the API back.** If `unboxed_closures`
and `fn_traits` stabilise, `Lambda<FN>` can implement `FnMut` and the 23 siblings
can go away. That would be a superseding record rather than an edit to this one:
someone following the decision above would then be adding methods that should not
exist. It would also be a second breaking change to the same surface, and whether
the ergonomic win is worth charging users for that a second time is exactly the
argument that record would have to make.
