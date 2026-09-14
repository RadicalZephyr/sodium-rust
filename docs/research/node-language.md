# A language for Sodium nodes

**Research note, 2026-09-14.** Companion to
[`concurrent-frp.md`](concurrent-frp.md), and like it, evidence rather than a
decision.

That note asked how to run an FRP graph on more than one thread. This one asks a
question underneath it: Appendix E of *Functional Reactive Programming* -- the
denotational semantics of Sodium -- is written in Haskell, and it silently
assumes things about the language its user functions are written in. Rust
supplies almost none of them. So what exactly does the specification need, what
do we lose by not having it, and what would a language built for this job look
like?

The short version: the semantics do not *degrade* under an effectful host
language, they stop applying. But that is true of every specification with
preconditions, and the useful question is not soundness but blast radius -- and
on that measure we are doing badly for reasons that are fixable.

## 1. What Appendix E assumes

Sodium has sixteen primitives. Each is specified as a total function into a
semantic domain:

```haskell
type S a = [(T, a)]        -- streams: time/value pairs, increasing T
type C a = (a, [(T, a)])   -- cells: initial value, then steps

occs  :: Stream a -> S a
steps :: Cell a   -> C a
```

Nothing in that machinery mentions failure, effects, ordering of evaluation, or
threads. All four are assumptions, and they are load-bearing.

### 1.1 Purity, as a well-formedness condition

```haskell
occs (MapS f s) = map (\(t, a) -> (t, f a)) (occs s)
```

This is a *denotation* only if `f a` is determined by `a`. If `f` reads a
mutable global, `f a` is not a value and the equation is not an equation. The
failure mode is not that the semantics give a worse answer -- they give none.

Worth stating plainly because it is easy to read purity as an optimisation
constraint. It is a condition for the specification to mean anything at all.

### 1.2 The semantics are extensional, which licenses re-evaluation

Appendix E says what the answer is, never how many times anything runs. `map f
xs` denotes a list; an implementation may compute each element once, memoise,
recompute, or speculate, and every strategy matches the denotation. The
specification offers no basis for preferring one.

With a pure `f` that freedom is free. With an effectful `f` the evaluation count
becomes observable and the specification cannot adjudicate. This is the
denotational root of the replay problem in `concurrent-frp.md` §1.1: optimistic
rollback needs to re-run node functions, and the spec *grants* that permission
-- it is Rust's closure bounds, not Sodium, that withdraw it.

### 1.3 Totality

`f a` is assumed to have a value. Nowhere is non-termination discussed. A pure
but non-terminating closure hangs the engine, and under any scheduler hangs it
while holding whatever the scheduler handed out. This assumption is invisible in
the Haskell text because there is nothing to write down.

### 1.4 `⊥` inhabits every type

A Haskell function that fails returns `⊥`, which is a *value*. So `f a = ⊥` is
well defined: the occurrence at time `t` has value `⊥`, `map` is lazy in the
elements, and the **spine** of the list -- the event times -- is untouched. Any
other stream's denotation is a separate expression that does not mention `f`.

Failure is therefore local to one value at one time, by construction. A strict
language has no such value, which is why
[#48](https://github.com/RadicalZephyr/sodium-rust/issues/48) matters: our
implementation lets one failure kill an entire `SodiumCtx` forever, including
graphs whose denotation cannot mention the failing function.

### 1.5 A single, always-increasing time

> The time at which the simulation is sampled is always increasing.

Stated outright, as one of the reasons the four time-taking primitives are safe.
Concurrency does not make this hard to implement; it removes the thing being
assumed. This is the formal root of `concurrent-frp.md` §5's first question.

Note also that revision 1.1 changed stream times from nondecreasing to
increasing "so that multiple events per time are no longer representable". Two
events at one `T` in one stream is not a value of the model -- which is why
`Merge` ends in `coalesce`. (The prose in E.5.4 still says a stream "can have
simultaneous events"; that reads as a leftover from 1.0, since `knit` produces
ties and `coalesce` immediately removes them. The code is consistent.)

### 1.6 Reads happen at a time

```haskell
Sample :: Cell a -> T -> a
sample c = Reactive (at (steps c))
at (a, sts) t = last (a : map snd (filter (\(tt, a) -> tt < t) sts))
```

> The public interface only allows `Value`, `Hold`, `SwitchC`, and `Sample` to
> be constructed through `Reactive`. [...] The public interface only allows
> streams and cells to be sampled at the current simulation time.

The `tt < t` is strict: a sample at `t` sees the value from before any step at
`t`. Our `Cell::sample()` supplies no time and takes no lock, which is benign on
one thread and produces observable glitches on two --
[#49](https://github.com/RadicalZephyr/sodium-rust/issues/49).

### 1.7 There are no effects anywhere

`Execute :: Stream (Reactive a) -> Stream a` is named as though it admits one,
but `Reactive` is `Reader T`:

```haskell
occs (Execute s) = map (\(t, ma) -> (t, run ma t)) (occs s)
```

Pure. Sixteen primitives, zero effects. Any effect in user code is outside the
specification entirely, not handled badly by it.

### Summary

| Assumption | Where it shows | Rust supplies it? | Observable today |
| --- | --- | --- | --- |
| `f` is pure | every `occs`/`steps` equation | no | not directly |
| evaluation count is free | extensionality | no, with effects | blocks STM (§1.1) |
| `f` is total | unwritten | no | hangs |
| failure is a value | `⊥` in every type | no | [#48] |
| one increasing time | E.4 | no, across threads | [#47] |
| reads happen at a `T` | `Sample :: Cell a -> T -> a` | not in our API | [#49] |

[#47]: https://github.com/RadicalZephyr/sodium-rust/issues/47
[#48]: https://github.com/RadicalZephyr/sodium-rust/issues/48
[#49]: https://github.com/RadicalZephyr/sodium-rust/issues/49

## 2. Soundness is the wrong question

It is tempting to conclude that Sodium's semantics are unsound outside Haskell.
They are not unsound; they are inapplicable to programs outside their domain,
which is a different and much more ordinary situation.

Sodium is not unusual in this. `HashMap` requires `Hash` and `Eq` to agree;
`sort_by` requires a total order. Violating either gives nonsense rather than
undefined behaviour, and no one calls the standard library unsound for it.
Purity of node closures is a precondition of exactly that kind.

What distinguishes a good library from a bad one under precondition violation is
**blast radius**. `sort_by` with an inconsistent comparator gives you a badly
sorted vector; the rest of the program is fine. sodium-rust with a panicking
closure gives you a dead context and every unrelated graph in it. The semantics
say failure is confined to one value at one time; we exceed that by a lot, and
that gap is a bug rather than a fact of life.

So the actionable form of the question is: **for programs that do violate the
precondition, does the runtime fail locally or globally?** Today: globally.

## 3. What the language would need

Taking the obvious constraints as read -- no globals, no I/O, no ambient
mutation, long-lived state in `Cell`s only, explicit nullability, algebraic data
types -- four requirements matter more than they first appear.

**Totality outranks purity.** A pure non-terminating function is as fatal as an
effectful one, and worse under a scheduler. Totality also pays a second time: a
language with bounded recursion admits a static cost estimate per node, which is
precisely the input a granularity decision needs (`concurrent-frp.md` §2.4,
§4). It is the principled version of the runtime profiling Blackheath proposes,
and it is available at compile time.

**Determinism is strictly more than purity.** No pointer-identity hashing, no
`HashMap` iteration order, no floating-point contraction that differs by
backend. Given §1.2's re-evaluation licence, determinism is what makes
re-evaluation *safe* rather than merely permitted.

**Failure must be a value, not control flow.** Partiality cannot be banned, only
relocated. A node function typed `A -> Result<B, E>` reproduces `⊥` faithfully
in a strict language: the timeline survives, the failure is one value at one
time, and nothing downstream is destroyed that did not ask for it. This is the
same change [#48] needs, which is the most useful thing in this note: the
ideal-language question and the panic bug have one answer.

**Captured dependencies must be visible by construction.** `Dep` and the
`*_with_deps` family exist because the cycle collector's tracer cannot see
through a Rust closure -- a trace that misses an edge frees live data. A
language that made captured FRP references structurally visible would delete
that entire API surface, which is currently every combinator written a third
time.

## 4. The inversion

The original proposal claimed "FRP achieves a similar level of lockdown due to
its highly restricted computational model, so STM should work well in an FRP
engine even in more liberal languages." `concurrent-frp.md` §3 argues that is
false, and it is -- but it is false because of *Rust's closure bounds*, not
because of FRP.

Given a total, pure, deterministic node language with failure in the return
type, every objection in that report's §1 dissolves in order. Replay becomes
safe, so optimistic rollback is sound. Opacity stops mattering, because a
transaction reading a torn snapshot can neither diverge nor trap. Abort-freedom
stops being a correctness criterion and becomes a performance concern, which is
what it is in databases.

That is a real contingency in the concurrency note's central argument, and it
cuts both ways: it means the verdict there is about this host language rather
than about FRP, and it means a node language would buy far more than tidiness.

## 5. The cost that kills the naive version

A separate language must be embedded, which puts a boundary at every node.
`concurrent-frp.md` §2.2 measures the current per-node cost at roughly 3.5 µs
and fifteen heap allocations. A Wasm call or an interpreter dispatch per node
could plausibly cost more than the bookkeeping the whole exercise is trying to
delete, and it would be paid on every node of every transaction rather than only
under contention.

Which points at a cheaper middle: not a separate language but a **checked subset
of the host**. A `#[sodium::node]` proc macro can reject `unsafe`, `static`,
paths into `std::io`, unbounded loops, and interior-mutability types, and can
require the return type to carry failure. It is unsound -- a determined caller
routes around it -- but it catches the accidental cases, costs nothing at
runtime, and could plausibly infer the `Dep` set that `*_with_deps` currently
demands by hand.

The trade is enforcement strength against per-node cost, and the report's
measurements say per-node cost is the scarce resource.

## 6. Prior art

- **[Koka](https://koka-lang.github.io/)** -- row-polymorphic effect types. The
  sharpest fit intellectually: you can *write down* that a function is `total`,
  which is the guarantee Rust cannot express and every argument above wants.
- **[Roc](https://www.roc-lang.org/)** -- the platform model. The application is
  pure; a host platform provides effects. Architecturally this is exactly the
  arrangement described here, with Sodium as the platform.
- **[Elm](https://elm-lang.org/)** -- pure, no null, ADTs, effects managed at
  the boundary; and not a coincidence, since Czaplicki designed it around FRP
  ([*Concurrent FRP for Functional GUIs*](https://elm-lang.org/assets/papers/concurrent-frp.pdf),
  and see `concurrent-frp.md` §1.5 for its `async` annotation). Worth knowing
  that Elm removed signals in 0.17 and is no longer an FRP language; the
  constraint list survived the FRP that motivated it.
- **[Dhall](https://dhall-lang.org/)** -- total by construction, guaranteed
  termination, no effects. Demonstrates the totality requirement is achievable,
  though it is not a general-purpose programming language.
- **[Starlark](https://github.com/bazelbuild/starlark)** -- deliberately not
  Turing-complete, deterministic, no I/O, designed for exactly this role of
  user code running inside someone else's engine.
- **WebAssembly with an empty import set** -- an enforcement mechanism rather
  than a language, and source-language agnostic. Genuinely cannot perform I/O.
  It does not give totality (a module can loop forever) and it can trap, so it
  covers §1.1 and §1.7 but not §1.3 or §1.4.
- **Effect handlers** more broadly: Frank, Eff, and Flix's effect system.

## 7. What this note does not settle

- **Whether any of this is worth doing.** Nothing here establishes that the
  precondition is violated often enough in practice to justify a language, a
  macro, or anything else. The honest prior is that most Sodium closures are
  already pure by accident of how people write FRP.
- **Whether the proc-macro subset can infer `Dep`s.** Claimed in §5 as
  plausible. Not investigated; closure capture analysis in a proc macro sees
  syntax rather than types, which may be fatal.
- **What the per-node cost of an embedded language actually is.** §5 asserts it
  could exceed the bookkeeping it replaces. That is an argument from the shape
  of the numbers, not a measurement, and it is measurable: a Wasm call with a
  trivial body against the 3.5 µs baseline would settle it in an afternoon.
- **Whether a total language can express the node functions people write.**
  Filters, maps and folds over bounded data are obviously fine. Whether the
  awkward real cases -- parsing, string formatting, numeric iteration to
  convergence -- fit inside a totality checker without becoming unpleasant is
  unexamined.
