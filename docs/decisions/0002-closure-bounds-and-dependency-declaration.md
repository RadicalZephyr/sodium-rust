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

[Issue #14](https://github.com/RadicalZephyr/sodium-rust/issues/14), opened on
2024-03-03 under the title *Type boilerplate makes it look like Java*, is where
this starts:

> There is quite a lot of syntactic overhead in writing Sodium code, especially
> when compared to the very small amounts of actual working code. One of the
> major contributors to this is needing to annotate the types on the closures
> passed to all the combinator methods.

It was filed as a pain point and put on the 3.0 milestone, which is the part
worth noticing: this was expected to cost a major version before anyone knew
what the fix was. The complaint is also not about inference as such. It is about
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

## Why the diagnosis took two and a half years

The hypothesis was right on the first day. A comment on issue #14, posted a
minute after the issue itself on 2024-03-03:

> My current suspicion about what is causing the type inference to fail is the
> `IsLambda*` traits. I haven't been able to successfully replace them in order
> to prove this tho.

That is the conclusion this record argues for, stated correctly, two and a half
years early -- together with the reason it stayed a suspicion. The only
experiment in view was replacing the traits in the library, and replacing them
*is* the change. So the cost of testing the hypothesis was the cost of acting on
it, which is a bad position to reason from: you cannot afford to be wrong, so you
do not try.

What broke the deadlock is that the failure does not need the library at all. It
reproduces in a trait declaration, a blanket impl and one call, which is the
first experiment above. Once that costs five minutes, so does every follow-up:
*is it the two impls overlapping?* *Would an extra bound fix it?* *What does the
deps-carrying call site do under that bound?* Each of those is a question the
two-and-a-half-year version of this problem could not afford to ask, and each of
them moved the argument -- the first ruled out the obvious alternative cause, and
the other two are why the API was split in two rather than given a second bound.

None of that is visible in the commits; it comes from the author's account, given
while this record was being written. The question was put fresh in 2026 and the
standalone reproduction came back immediately, which is the whole of the
difference between 2024 and 2026: the hypothesis did not improve, the cost of
checking it collapsed.

Which is [ADR-0001](0001-recording-important-decisions.md)'s argument for keeping
research cheap and minimal, arrived at from the other end and before that record
existed. This one is its first customer, and the evidence it needed turned out to
be evidence nobody had been able to produce while the only available experiment
was the change itself.

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

Split the API by shape.

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

**Add `+ FnMut(&A) -> B` to the existing bound.** This looks like a one-line fix,
and the first half of it works. The second half is why it is not:

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
to accept.

**Make `Lambda<FN>` implement `FnMut`.** This is the clean end state, and worth
being precise about, because it is the alternative most likely to become
available. It collapses the whole problem: with those impls, a single `FnMut`
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
objection, and a library whose MSRV is 1.71 cannot take it.

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

## Consequences

**It is a breaking change, and a mechanical one.** `combinator(lambda1(f, deps))`
stops compiling; the migration is `combinator_with_deps(f, deps)`, and it drops
an annotation rather than adding one. `CHANGELOG.md` carries the before/after.
The breakage was budgeted rather than discovered: issue #14 sat on the 3.0
milestone from 2024, so a major version was already the expected price. The
change is unreleased at the time of writing -- the crate is at 2.1.3 and the
entry is under `Unreleased`.

**The function-taking surface doubles.** 23 base methods gained 23 siblings, all
of which appear in rustdoc, and `map` now sorts next to `map_to` and
`map_with_deps`. This is a real cost and it lands on the reader of the
documentation. Against issue #14's ratio, that is the trade we chose: ceremony
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
Playground needs only the one file. They were never regression guards. Nothing
this library promises has the form "rustc rejects this program", and their own
comments said as much -- they were there to keep the reasoning checked rather
than merely asserted. That is the definition of evidence, and
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
The fourth experiment, on `fn_traits`, is new: it was a claim in a commit message
with nothing behind it, and it is the claim in this record most likely to expire.
`tests/ui/bare_closures.rs` stays: it exercises every function-taking combinator
with an unannotated closure from outside the crate, which is a property we do
promise, and it carries no expected output, so it runs on every channel.

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

**Whether stabilisation should collapse the API back.** If `unboxed_closures`
and `fn_traits` stabilise, `Lambda<FN>` can implement `FnMut` and the 23 siblings
can go away. That would be a superseding record rather than an edit to this one:
someone following the decision above would then be adding methods that should not
exist. It would also be a second breaking change to the same surface, and whether
the ergonomic win is worth charging users for that a second time is exactly the
argument that record would have to make.
