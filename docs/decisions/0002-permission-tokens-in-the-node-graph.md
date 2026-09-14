# 0002 -- Permission tokens in the node graph

<details>
<summary><strong>Status:</strong> Drafted 2026-09-14</summary>

| Date | Transition |
| --- | --- |
| 2026-09-14 | Drafted |

</details>

*This record is a draft, and the Decision section is deliberately empty. The
measurements below are finished and re-derivable; the argument they feed is
not. Per [`README.md`](README.md) a draft may be edited freely without dating
anything, so nothing here carries a dated addition yet.*

## Context

The [GhostCell paper](https://plv.mpi-sws.org/rustbelt/ghostcell/) (Yanovski,
Dang, Jung and Dreyer, ICFP 2021) separates the *permission* to touch a data
structure from the data itself: a `GhostCell<'id, T>` holds the data and a
single `GhostToken<'id>` holds the right to read or write every cell sharing
its brand. `borrow` and `borrow_mut` are type coercions that compile to
nothing. The paper proposes this as the way to build graphs in safe Rust, and
the [`qcell`](https://crates.io/crates/qcell) crate implements it four ways.

This library is a graph, and it is full of locks. The suggestion on the table
is that a token could replace most of them, and that this would be faster.

Two of those claims are worth separating, because only one of them turned out
to be in doubt.

### What the locking costs

Today the only wired-up propagation mode is `single_threaded_mode`, so every
lock in a `send` is uncontended. That does not make it free.
[`0002-sync-primitive-cost`](research/src/bin/0002-sync-primitive-cost.rs)
times one uncontended acquire-and-release of each mechanism against a floor of
the same store reached through a plain `&mut`:

| mechanism | ns per op |
| --- | --- |
| plain `&mut` write (floor) | 0.72 |
| `parking_lot::RwLock::write` | 18.9 |
| `parking_lot::RwLock::read` | 23.7 |
| `parking_lot::Mutex::lock` | 18.9 |
| `qcell::QCell::rw` | 0.71 |
| `qcell::LCell::rw` (GhostCell) | 0.72 |

Measured 2026-09-14 on an Intel Xeon @ 2.10GHz (4 vCPU), rustc 1.94.1,
`--release`. Read it for its order of magnitude: a lock costs about
twenty-six times a plain store, and **both** `qcell` flavours are
indistinguishable from one. That second point matters more than it looks.
`LCell` is GhostCell and its access compiles away; `QCell` compares owner ids
and branches to a panic, and the paper's own related-work section holds that
against it. At this granularity the check does not show up, so the flavour
that keeps `Stream<A>`'s shape is not measurably worse than the flavour that
does not.

How many of those does a `send` perform?
`patches/lock-count.patch` instruments every lock and splits the count by
phase. On a sink feeding a chain of sixteen `map` nodes into a listener --
eighteen nodes updated per send -- one `send` takes **982 lock acquisitions**,
or about **55 per node updated**. The split is the surprise:

| phase | acquisitions per send | share |
| --- | --- | --- |
| propagation | 270 | 28% |
| cycle collector | 712 | 72% |

Nearly three quarters of the lock traffic is the Bacon--Rajan collector, which
walks the reachable graph roughly seven times per `collect_cycles` and takes
four read locks per node per walk -- `GcNodeData::trace`, then
`dependencies`, `update_dependencies` and `keep_alive` inside the trace
closure.

[`0002-propagation-profile`](research/src/bin/0002-propagation-profile.rs)
gives the denominator: 58.8 us per send for that eighteen-node graph and 227.7
us for a sixty-six-node one, so about 3.2--3.4 us per node updated, and 268
and 950 heap allocations per send respectively. Adding nodes that never fire
barely moves the figure, which is how we know the collector's cost tracks the
nodes actually touched rather than the size of the graph.

### The ceiling

`patches/no-locks.patch` replaces every `parking_lot::Mutex` and `RwLock` in
`src/impl_/` with an unsynchronised `UnsafeCell` shim. That is the hard upper
bound on any lock-elimination scheme: `LCell` access compiles to nothing, so
nothing can do better than removing the locks outright. Measured with
[`0002-variant-ab`](research/src/bin/0002-variant-ab.rs), six interleaved
repetitions, medians, on 2026-09-14:

| workload | `cheap-wins` | `no-locks` | both |
| --- | --- | --- | --- |
| `send/simple` | -13.0% | **-35.1%** | -50.0% |
| `send/+mapx2` | -15.8% | **-28.6%** | -44.9% |
| `send/merge2` | -19.9% | **-31.7%** | -49.2% |
| `Cell::send/+map` | -21.7% | **-28.6%** | -46.8% |

So the prize is real: roughly **30%** of propagation is lock overhead. The
two columns are also very nearly additive, which means the win does not
evaporate once the cheap work lands -- after `cheap-wins`, removing the locks
is still worth a further 32--43%.

**All 52 `cargo test --lib` tests pass with every lock removed.** That is not a
licence to delete them. `SodiumCtx` and `Stream<A>` are `Send + Sync`, so two
threads may legitimately send into one context; the shim would race if they
did, and survives only because the suite is single-threaded. It is precisely
GhostCell's pitch that the same 30% is available *without* giving that up,
because the permission moves into one token instead of being spread across the
fields.

There is no cheap subset. Applying the shim one module at a time:

| scope | win |
| --- | --- |
| `gc_node.rs` only | -3.8% to -9.0% |
| `node.rs` only | -11.7% to -16.6% |
| everything | -28.6% to -35.1% |

A port scoped to the collector -- where, tantalisingly, no user code is on the
stack and a token would fit cleanly -- collects under a third of the win.

### What a token costs

The paper is explicit that its permission is coarse, and the sentence that
matters for this library is in §3.1: *"we cannot hold an interior pointer to
one node while mutating another"*. That is exactly what `update_node` does.
The four experiments below are minimal reproductions of GhostCell -- the
paper's own thirty-line implementation, since `qcell` is not available on the
Playground -- with sodium's structures cut down to the shape under test.

Each experiment is the harness below plus its own body; the share links carry
the complete file.

```rust
use std::cell::UnsafeCell;
use std::marker::PhantomData;

// GhostCell, cut down to the API of the paper's §3.1. `qcell::LCell` is the
// same type with `ro`/`rw` in place of `borrow`/`borrow_mut`.
type Brand<'id> = PhantomData<fn(&'id ()) -> &'id ()>;

struct Token<'id>(Brand<'id>);

struct GhostCell<'id, T: ?Sized> {
    _brand: Brand<'id>,
    value: UnsafeCell<T>,
}

impl<'id, T> GhostCell<'id, T> {
    fn new(value: T) -> Self {
        GhostCell { _brand: PhantomData, value: UnsafeCell::new(value) }
    }
    fn borrow<'a>(&'a self, _: &'a Token<'id>) -> &'a T {
        unsafe { &*self.value.get() }
    }
    fn borrow_mut<'a>(&'a self, _: &'a mut Token<'id>) -> &'a mut T {
        unsafe { &mut *self.value.get() }
    }
}

fn with_token<R>(f: impl for<'new> FnOnce(Token<'new>) -> R) -> R {
    f(Token(PhantomData))
}
```

**Experiment -- `update_node`'s shape does not compile under a single token**

```rust
// A node cut down to what `SodiumCtx::update_node` touches: the update
// closure, and the firing slot that closure writes into downstream.
struct Node<'id> {
    firing: Option<u32>,
    update: Box<dyn FnMut(&mut Token<'id>) + 'id>,
}

fn main() {
    with_token(|mut token| {
        let mapped = GhostCell::new(Node { firing: None, update: Box::new(|_| {}) });

        // `SodiumCtx::update_node`: reach into the node for its update closure
        // and run it. Today that is `node.data.update.write()` then `update()`.
        let node = mapped.borrow_mut(&mut token);
        (node.update)(&mut token);

        println!("{:?}", mapped.borrow(&token).firing);
    });
}
```

```text
error[E0499]: cannot borrow `token` as mutable more than once at a time
  --> src/main.rs:45:23
   |
44 |         let node = mapped.borrow_mut(&mut token);
   |                                      ---------- first mutable borrow occurs here
45 |         (node.update)(&mut token);
   |         ------------- ^^^^^^^^^^ second mutable borrow occurs here
   |         |
   |         first borrow later used by call
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=db785a8c50d2483689795c1c77a14596)

**Experiment -- neither does `with_firing_op`'s**

```rust
impl<'id, T> GhostCell<'id, T> {
    // `Stream::with_data`, which today takes the stream's lock and hands the
    // guard to a closure. Under a token the guard is `&mut T`, and the token
    // stays borrowed for as long as the closure runs.
    fn with_data<R>(&self, token: &mut Token<'id>, k: impl FnOnce(&mut T) -> R) -> R {
        k(self.borrow_mut(token))
    }
}

struct StreamData<A> {
    firing_op: Option<A>,
}

fn main() {
    with_token(|mut token| {
        let src = GhostCell::new(StreamData { firing_op: Some(7u32) });
        let dst = GhostCell::new(StreamData { firing_op: None::<u32> });

        // `Stream::map`'s update closure, transcribed:
        //
        //     self_.with_firing_op(|firing_op: &mut Option<A>| {
        //         if let Some(ref firing) = firing_op {
        //             s.unwrap()._send(f.call(firing));
        //         }
        //     })
        //
        // `firing` is an interior pointer into the source stream, and `_send`
        // mutates the target stream while it is still live.
        src.with_data(&mut token, |src_data| {
            if let Some(ref firing) = src_data.firing_op {
                dst.with_data(&mut token, |dst_data| {
                    dst_data.firing_op = Some(*firing + 1);
                });
            }
        });

        println!("{:?}", dst.borrow(&token).firing_op);
    });
}
```

```text
error[E0499]: cannot borrow `token` as mutable more than once at a time
  --> src/main.rs:60:35
   |
60 |         src.with_data(&mut token, |src_data| {
   |             --------- ----------  ^^^^^^^^^^ second mutable borrow occurs here
   |             |         |
   |             |         first mutable borrow occurs here
   |             first borrow later used by call
61 |             if let Some(ref firing) = src_data.firing_op {
62 |                 dst.with_data(&mut token, |dst_data| {
   |                                    ----- second borrow occurs due to use of `token` in closure
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=7c9c191cca07158276bcd5a654cb3fde)

**Experiment -- a branded `SodiumCtx` cannot escape its scope**

```rust
// `SodiumCtx` as it would have to be spelled if the node graph lived in
// GhostCells: the context owns the permission, so it carries the brand.
struct SodiumCtx<'id> {
    token: Token<'id>,
}

impl<'id> SodiumCtx<'id> {
    fn new() -> SodiumCtx<'static> {
        with_token(|token| SodiumCtx { token })
    }
}

fn main() {
    let ctx = SodiumCtx::new();
    let _ = ctx.token;
}
```

```text
error: lifetime may not live long enough
  --> src/main.rs:40:28
   |
40 |         with_token(|token| SodiumCtx { token })
   |                     ------ ^^^^^^^^^^^^^^^^^^^ returning this value requires that `'1` must outlive `'2`
   |                     |    |
   |                     |    return type of closure is SodiumCtx<'2>
   |                     has type `Token<'1>`
   |
   = note: requirement occurs because of the type `SodiumCtx<'_>`, which makes the generic argument `'_` invariant
   = note: the struct `SodiumCtx<'id>` is invariant over the parameter `'id`
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=11f79f38c9d0c7b76f1c7bb1b5847e5a)

`qcell::LCellOwner::scope` returns `()`, so under the real crate nothing
escapes at all. The ordinary FRP pattern -- build the graph, hold a `Listener`
for the life of the program, fire events from an event loop -- would have to
be rewritten so that the whole program body lives inside the scope callback,
and no branded handle could satisfy a `'static` bound.

**Experiment -- the rewrite the first two experiments force is feasible**

```rust
struct StreamData<A> {
    firing_op: Option<A>,
}

/// `Stream::map`'s update closure, rewritten to the discipline GhostCell
/// forces: take the value out of the node, let the borrow end, act, put it
/// back. No interior pointer is ever live across a use of the token.
fn map_update<'id, A, B>(
    src: &GhostCell<'id, StreamData<A>>,
    dst: &GhostCell<'id, StreamData<B>>,
    f: &mut dyn FnMut(&A) -> B,
    token: &mut Token<'id>,
    observe: &mut dyn FnMut(&mut Token<'id>),
) {
    let taken = src.borrow_mut(token).firing_op.take();
    if let Some(a) = taken {
        let b = f(&a);
        observe(token); // whatever the user's closure does, mid-update
        dst.borrow_mut(token).firing_op = Some(b);
        src.borrow_mut(token).firing_op = Some(a);
    }
}

fn main() {
    with_token(|mut token| {
        let src = GhostCell::new(StreamData { firing_op: Some(7u32) });
        let dst = GhostCell::new(StreamData { firing_op: None::<u32> });

        // Note: `A` is never cloned, so this costs no new `Clone` bound. But
        // while the update runs, the source reports that it is not firing.
        map_update(&src, &dst, &mut |a: &u32| a + 1, &mut token, &mut |t| {
            println!("during the update, src.firing_op = {:?}", src.borrow(t).firing_op);
        });

        println!("after:  src = {:?}, dst = {:?}",
                 src.borrow(&token).firing_op, dst.borrow(&token).firing_op);
    });
}
```

```text
during the update, src.firing_op = None
after:  src = Some(7), dst = Some(8)
```

> rustc 1.98.1 (released 2026-09-01) - output checked 2026-09-14 - [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=d7508de141497a4e518f885cca7dd776)

So §3.1 is a constraint on how the engine is written, not a proof that it
cannot be. Take-and-restore costs a pointer move, needs no new `Clone` bound,
and would have to be applied to all 22 `Node::new` call sites. It has one
visible consequence: for the duration of an update the source reports that it
is not firing, where today the same re-entrant read would deadlock.

### Where the real obstacle is

Not `update_node`, which take-and-restore handles, but the user's own
closures. `tests/closure_type_inference.rs` drives `move |x| *x + b.sample()`
through `map`, and `Cell::sample` reads graph state -- so the user's closure
needs the permission. Give `map` a bound of `FnMut(&mut Owner, &A)` and the
closure-ergonomics guarantee that `tests/ui/bare_closures.rs` exists to
protect breaks in call shape: `s.map(|a| *a + 1)` becomes
`error[E0593]: closure is expected to take 2 arguments, but it takes 1`. The
inference *mechanism* survives, because the bound is still in the `Fn` family;
every documented call site does not.

`qcell` offers `rw2`/`rw3` on all six of its owner types for touching two or
three cells at once, but they take their cells at a single call site and panic
at runtime on aliasing, so they do not reach a recursive graph walk.

## Decision

**Open.** This draft records the measurements, not a conclusion. The
questions it has to answer:

1. Does `ThreadedMode` survive? A single token serialises all mutation, so a
   port forecloses `simple_threaded_mode` and the thread-pool TODO --
   and the largest API-neutral speedup available also wants `ThreadedMode`
   gone. Both paths converge on a decision this record does not own.
2. Is there a partial port worth pricing -- tokenising `NodeData` and
   `GcNodeData` while leaving `StreamData` and `CellData` under locks? Update
   closures are library code and can carry the token; user closures would
   touch only values and keep `FnMut(&A)`. That is the `node.rs` plus
   `gc_node.rs` share above, with the public API intact. It is unmeasured.
3. Should the performance question and the permission question be one record
   or two? They have different risk profiles and different blast radii.

## Consequences

Unwritten pending the decision. What the evidence already fixes:

- Whatever is chosen, the **30% is not available for free**: it is available
  either by giving up the thread-safety the public API promises, or by moving
  the permission somewhere a compiler can check it.
- The **cheap work is not in competition with the token work**. The two
  columns above are additive, so the API-neutral changes can land first
  without spending the prize.
- If a port happens, it should be **`QCell`, not `LCell`**, unless something
  other than speed argues otherwise. The measured cost of the owner check is
  nil, and `LCell`'s brand cannot leave its scope.

## Alternatives considered

**An arena with indices, rather than a token.** The paper's own Fig. 3 puts
petgraph -- a `Vec` of nodes with index edges, safe Rust, no locks -- at
1.60 ms against GhostCell's 1.11--1.36 ms on the same traversal, so most of
the win is available without any branding. It would also dissolve the cycle
collector, since indices do not own. The paper reaches for arenas for exactly
this reason, and concedes the cost: *"The downside of arenas is that
individual nodes cannot be deallocated."* That is the one property this
library cannot give up -- `assert_memory_freed` exists to enforce it. Not
ruled out, but it is a different record.

**Relaxing atomic orderings.** Measured and dropped. A blanket
`SeqCst` to `Relaxed` sweep lands between noise and a slight regression across
five workloads. On x86-64 only stores differ, and this library's atomic
traffic is dominated by read-modify-writes.

**One coarse `Mutex` over all graph state.** Gets the same lock-count
reduction as a token with none of the type-level machinery, and is what the
paper says a token's holder should do anyway when it needs synchronisation --
*"the GhostToken can be put into a lock"*. Cheaper to build and much cheaper
to abandon; strictly worse for a future parallel propagation, which a single
token also forecloses. Worth costing against a `QCell` port if the decision
goes that way.

**Doing nothing.** Defensible on the old understanding that the locks were
incidental. The ceiling measurement is what removes it: 30% of propagation is
not incidental.
