# Concurrent and parallel FRP

**Research note, 2026-09-14.** This is not a decision. It is the evidence a
decision would be built on, gathered in response to the suggestion -- from
Blackheath and Jones' *Functional Reactive Programming*, §"The future" -- that
an FRP engine's knowledge of its own dependency graph makes it an unusually
good candidate for automatic parallelisation, with software transactional
memory as the natural implementation.

The short version: the published attempt to do exactly that measured its own
approach and found it lost to a global lock on cheap updates. Our updates are
cheap. But not for the reason I expected, and that inversion is the most
useful thing in this document.

## Findings at a glance

1. **sodium-rust is not correctly serial across threads today.** Transaction
   depth is a single global counter rather than a thread-local one, so two
   threads in transactions merge into one. Two causally independent events
   fired from two threads are observed as *simultaneous* by `merge`. This is a
   divergence from Sodium's denotational semantics, demonstrated in
   [Experiments 1 and 2](#experiment-1--transaction-boundaries-merge-across-threads),
   and filed as [#47][issue47]. Java Sodium does not have this bug; it holds
   one coarse global lock for the whole transaction.
2. **A node update costs ~3.5 µs and ~15 heap allocations**
   ([Experiments 3 and 4](#experiment-3--what-a-node-update-costs)) for a
   closure that adds one to an integer. Essentially all of it is runtime
   bookkeeping, and it already passes through two global mutexes per node per
   update.
3. **The literature's verdict on optimistic rollback for reactive runtimes is
   not "hard" but "incorrect".** Reactive computations have side effects;
   aborting and replaying them is unsound. The OOPSLA 2018 work makes
   abort-freedom an explicit correctness criterion and disqualifies its own STM
   implementation on those grounds.
4. **The break-even for thread-safe scheduling to pay is ≈160 µs of *user*
   computation per update.** Our user computation is nanoseconds. The 3.5 µs is
   overhead, not work -- so parallelising it divides the wrong quantity.
5. **`simple_threaded_mode` does not parallelise anything.** It spawns one
   thread around the whole dependency loop and immediately joins.

## 1. What the literature says

### 1.1 The paper that is our problem exactly

Drechsler, Mogk, Salvaneschi and Mezini, [*Thread-Safe Reactive
Programming*][tsrp] (OOPSLA 2018), built the system Blackheath describes,
built the STM version alongside it as a comparison, and measured both. It is
the single most relevant piece of work and it is worth reading in full.

Their framing of the problem matches ours:

> Existing RP languages either disable multi-threading or handle it at the cost
> of reducing expressiveness or weakening consistency.

**They add abort-freedom as a correctness criterion.** Not as a performance
concern -- as correctness:

> recall that transactions of RP updates may contain side-effects, and thus
> cannot be aborted to resolve deadlocks. Because such aborts have become a
> common, widely accepted, and often even expected practice in many domains
> (e.g., databases, STM), we consider it necessary to explicitly state
> abort-free as an additional criterion for correctness.

**Their own STM implementation fails that bar.** `STM-RP` stores node variables
in ScalaSTM and wraps each transaction in `atomic{...}`:

> STM-RP thus does not fulfil our correctness: it provides strict
> serializability, but is not abort-free. It is therefore applicable only to
> applications without side-effects; for compatibility, all our benchmarks
> adhere to this.

That last clause is doing a lot of work. They had to *restrict the benchmark
suite* to make the STM variant measurable at all.

**Their algorithm is pessimistic, not optimistic.** MV-RP (also called FullMV)
combines conservative two-phase locking for reevaluations with multi-version
concurrency control for reads. Transactions are ordered in a stored
serialization graph as late as the constraints allow. A read that would violate
the established order is served an older version rather than causing an abort.
Dynamic dependency changes -- the `switch_s`/`switch_c` case -- are handled by
what they call *retrofitting*: the edge insertion is allowed to commit and the
ordering is repaired, rather than aborting the transaction that made it.

**The numbers.** These are the ones worth carrying around:

| Measurement | Value |
| --- | --- |
| User computation per update needed to beat scheduling overhead up to ~16 threads | **≈160 µs**, spread over ≥16 nodes (≈10 µs/node) |
| Single-threaded cost of MV-RP vs a global lock, on 6.5 µs updates | **−25%** |
| Single-threaded cost of STM-RP vs a global lock, same workload | **−55%** |
| STM-RP under high contention, any thread count | ~50% below global lock, never scales |
| MV-RP under extreme contention with cheap updates | 5–30% below global lock, despite successfully parallelising |

In their words:

> The necessary computational cost of user computations per update to overcome
> the overhead cost of scheduling up to almost 16 threads is only ca. 160 µs
> (on our hardware, and assuming that work is spread evenly across at least 16
> nodes).

and, on the cheap-update case:

> in a setting with extreme contention and very cheap updates the performance of
> MV-RP remains between 5% and 30% lower than G-Lock, i.e., MV-RP fails to
> improve performance despite successfully parallelizing updates.

A correct, formally verified, purpose-built reactive scheduler that genuinely
achieves parallelism still loses to `synchronized` when updates are cheap. That
is not a criticism of the work -- the authors say so themselves and are clear
about where the approach pays. It is the central datum for us.

### 1.2 Glitch freedom is not serializability

Blackheath's argument opens with "an FRP engine knows all the dependencies and
data flows in the FRP logic, so it can be guaranteed to give the right answer
in all cases." That begs the question of which answer is right.

Margara and Salvaneschi, [*On the Semantics of Distributed Reactive
Programming: the Cost of Consistency*][cost] (IEEE TSE 2018), formalise a
hierarchy of propagation semantics -- causal, glitch-free, atomic -- and price
each one. The key result for us is that **glitch freedom is strictly weaker
than serializability**: two independent sources can reach different parts of
the graph in different relative orders and the execution is still glitch-free,
because no node ever observes a half-updated set of *its own* inputs. Their
earlier [DREAM][dream] (DEBS 2014) implements all three levels so the costs can
be compared directly.

Sodium's semantics sit at the strong end. A transaction is the unit of
simultaneity, and `merge`'s tie-break is deterministic against a single global
timeline. Concurrency reintroduces the question of what that timeline is, and
the hierarchy is the map of the available answers. Any parallel design has to
name its point on it, and "the engine knows the dependencies" does not choose
one.

[Bainomugisha et al.'s survey][survey] (ACM Computing Surveys, 2013) is the
standard taxonomy for the surrounding design space -- push versus pull,
lifting, multidirectionality, glitch avoidance, distribution -- and is a
reasonable orientation document for anyone coming to this cold.

### 1.3 Determinism without rollback

Two lines of work get concurrency safety without ever aborting anything, which
is the property the reactive setting actually needs.

**Concurrent Revisions.** Burckhardt and Leijen, [*Semantics of Concurrent
Revisions*][revisions] (ESOP 2011) and *Prettier Concurrency* (2011). Each
forked task gets a conceptual copy of all shared state; state changes integrate
at join, where write-write conflicts are resolved by deterministic,
user-supplied merge functions. The model is explicitly modelled on branching
version control, the consistency guarantee is snapshot isolation plus
conflict resolution, and the calculus is [proven confluent][determinacy]. No
rollback, ever. The cost is that someone has to write the merge function.

For FRP this maps unusually well onto the structure we already have: a
transaction *is* a fork/join, cells already have well-defined update-at-end-of-
transaction semantics, and `merge` is already a user-supplied conflict
resolver. Whether that correspondence survives contact with `switch_c` is an
open question I have not chased.

**LVars.** Kuper and Newton, [*LVars: lattice-based data structures for
deterministic parallelism*][lvars] (FHPC 2013). Determinism from monotonicity:
writes are least-upper-bounds against a user-specified lattice, reads are
threshold reads that block until a lower bound is crossed. The
[freeze-after-writing extension][freeze] relaxes this to quasi-determinism --
every run gives the same answer or raises an error.

LVars are a bad fit for `Cell`, whose value is not monotone. They are a good
fit for the *scheduler's own metadata*: `visited`, `changed`, and the
changed-node set are all monotone within a transaction. That is the part of a
parallel propagation scheme most likely to be gotten subtly wrong by hand, and
there is a principled construction for it.

**Glitch.** Sean McDirmid's [Glitch][glitch] and [*Programming with Managed
Time*][managedtime] are the closest thing to Blackheath's optimistic proposal
that anyone actually shipped. Glitch re-executes nodes progressively as they
become inconsistent. The catch is the precondition: operations on shared state
must be **undoable and commutative**. That is a constraint Rust's type system
cannot express and our `FnMut` bounds do not impose.

### 1.4 Weakening the semantics on purpose

The design the quote does not consider is making concurrency opt-in at a named
place rather than inferred everywhere.

Czaplicki and Chong, [*Asynchronous Functional Reactive Programming for
GUIs*][elm] (PLDI 2013), is Elm's `async`: a programmer annotation marking a
signal computation whose event ordering need not remain globally synchronised.
Everything unannotated keeps the full guarantee; the annotated subgraph is
allowed to fall behind. Czaplicki's earlier [*Elm: Concurrent FRP for
Functional GUIs*][elmthesis] has the longer development.

This is the honest version of the trade, and it is the one most compatible with
a library whose semantics are mandated rather than chosen: it does not weaken
Sodium's guarantees, it adds a place where a user can explicitly decline them.

### 1.5 STM proper

If we do pursue the STM route, these are the reference points.

**Algorithms.** [TL2][tl2] (Dice, Shalev and Shavit, DISC 2006) is the standard
blueprint: commit-time locking plus a global version clock for validation, so
that user code is guaranteed to run on a coherent state. [NOrec][norec]
(Dalessandro, Spear and Scott, PPoPP 2010) abolishes ownership records
entirely, giving the lowest fast-path latency of any design admitting
concurrent updates, along with publication and privatization safety and no
false conflicts from hash collisions -- at the price of not scaling past tens
of threads. [Composable Memory Transactions][cmt] (Harris, Marlow, Peyton Jones
and Herlihy, PPoPP 2005) is the Haskell design, and the source of `retry` and
`orElse`.

**Correctness.** Guerraoui and Kapalka, [*On the Correctness of Transactional
Memory*][opacity] (PPoPP 2008), define **opacity**: strict serializability plus
the requirement that even transactions that will eventually abort never observe
inconsistent state. This matters for us specifically. A doomed transaction
running a user's `map` closure over a torn snapshot can divide by zero or fail
to terminate, and Rust gives us no way to interrupt it. Any optimistic scheme
here needs opacity, not merely serializability.

**The case against.** Cascaval et al., [*Software Transactional Memory: Why Is
It Only a Research Toy?*][toy] (CACM 2008), whose core complaint is
instrumentation overhead: every transactional read or write becomes a call into
the STM runtime where sequential code had a single instruction. There is a
[published rebuttal][nottoy] in the same venue, and the argument is not
settled in general -- but the specific overheads it names are the ones that
bite hardest when the transactions are tiny, which is our regime. The hardware
assist that was supposed to fix this, Intel TSX, was [disabled by default via
microcode][tsx] across Skylake through Coffee Lake following a memory-ordering
erratum.

**Rust.** The [`stm`][stmcrate] crate follows the Haskell design closely
(log reads and writes, validate, commit or re-run). `async-stm` returns an
`Arc` from a `TVar` read so the clone is deferred until mutation.
`swym-htm` exposes raw x86-64 HTM primitives, with the caveat above.

### 1.6 Adjacent work worth knowing about

- **[Parallel Functional Reactive Programming][pfrp]** (Peterson, Trifonov and
  Serjantov, PADL 2000) is the historical first attempt, extending FRP to
  parallel systems.
- **[Distributed REScala / SID-UP][sidup]** (OOPSLA 2014) achieves complete
  glitch freedom without centralised knowledge of the dependency topology,
  which is the constraint a distributed setting imposes and a multi-threaded
  one does not -- but the algorithm is instructive regardless.
- **[Efficient Parallel Self-Adjusting Computation][psac]** (Anderson et al.)
  and the earlier [proposal for parallel self-adjusting computation][psac07]
  are the incremental-computation community working the same problem with
  actual complexity bounds. Most prior parallel results in that space are
  restricted to map-reduce-shaped computations or are ad hoc.
- **[Differential dataflow][dd]** on [timely dataflow][td] is, I would argue,
  the working data-parallel FRP. It parallelises *within* a node over
  collections. The quote dismisses GPU-style data parallelism as a poor fit,
  which is right for scalar cells and wrong for collections -- the axis exists,
  it is just not the one the quote is looking along. Note also their warning
  that progress-tracking overhead is quadratic in concurrent timestamps and
  swamps the work if timestamps are too fine-grained; that is the same
  granularity failure in a different costume.
- **[Concurrent Cycle Collection in Reference Counted Systems][baconrajan]**
  (Bacon and Rajan, ECOOP 2001) is the algorithm in `src/impl_/gc_node.rs`. We
  run the synchronous variant. The original is *concurrent* -- collecting in
  the presence of simultaneous mutation, with 6 ms maximum mutator pauses in
  the Jalapeño implementation. If we ever go multi-threaded, the concurrent
  form is already in the paper we are citing.

## 2. Where sodium-rust actually stands

All measurements below are from commit-time `main`, release profile
(`debug = 1`, per the root manifest), rustc 1.94.1, on a 4-core Intel Xeon
@ 2.80 GHz in a shared cloud container. **Absolute times are soft** -- shared
tenancy, and four cores is not a parallelism testbed. Timing spread across five
runs was 3–7%, and the linearity and the ratios are the robust parts.
**Allocation counts are exact and deterministic.** Sources for every experiment
are in [Reproducing the measurements](#reproducing-the-measurements).

### 2.1 Transaction boundaries merge across threads

`SodiumCtxData::transaction_depth` is a single `u32` behind the context's one
global mutex. It is not thread-local. `enter_transaction` increments it and
`leave_transaction` decrements, running `end_of_transaction` only when it hits
zero -- which means that when two threads are in transactions simultaneously,
whichever leaves last runs the propagation for both.

[Experiment 1](#experiment-1--transaction-boundaries-merge-across-threads)
shows the mechanics. Thread A holds a transaction open for 300 ms; thread B
calls `send` in the middle of it:

```text
A: opened transaction
B: about to send
B: send returned          <-- returned having propagated nothing
A: closing transaction
a fired 1
b fired 2                 <-- B's event propagated on A's thread
```

`send` is supposed to complete a transaction. Here it returns having done
nothing, and B's event is propagated later, on another thread, inside a
transaction B knows nothing about.

[Experiment 2](#experiment-2--independent-events-observed-as-simultaneous)
shows why that is a semantic problem and not merely a latency one. The same
setup, with the two sinks joined by `merge`:

```text
!! coalescer ran: 1 and 2 were treated as SIMULTANEOUS
merged stream fired with: [3]
expected two separate firings [1, 2]; got 1 firing(s)
```

Two causally independent events became simultaneous. In Sodium's semantics a
transaction *is* the unit of simultaneity, so this is not a race in the
data-corruption sense -- everything is behind `Arc<Mutex>` and `SeqCst`, and
it is memory-safe -- it is the library reporting a false coincidence.

**Java Sodium does not have this bug.** `nz.sodium.Transaction` holds one
coarse lock:

```java
// Coarse-grained lock that's held during the whole transaction.
static final Object transactionLock = new Object();
```

and every entry point is wrapped in `synchronized (transactionLock)`, with the
current transaction a plain `static` field. The comment in `runVoid` states the
invariant our port dropped:

```java
// If we are already inside a transaction (which must be on the same
// thread otherwise we wouldn't have acquired transactionLock), then
// keep using that same transaction.
```

Java Sodium is fully serialised and never wrong. We have `Send + Sync`
throughout and no such lock, which is the reverse trade: we permit the
concurrency and lose the guarantee.

Per [`CONTRIBUTING.md`][contributing] and the ADR conventions, a divergence
between our operational semantics and Sodium's denotational semantics is a bug
report rather than a design preference, and is the one case that earns an
`#[ignore]`-marked test written against the semantics. Experiment 2 is
essentially that test already. Adding it is a change to the suite and a claim
about the library, so it is proposed rather than made -- [#47][issue47]
carries the bug report and a paste-ready version of the test.

### 2.2 What a node update costs

Marginal cost of adding one `map` node to a chain, five runs each:

| nodes | ns/update | marginal ns/node | allocs/update | marginal allocs/node |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 7,758 | -- | 43 | -- |
| 2 | 11,023 | 3,434 | 58 | 15.0 |
| 4 | 18,503 | 3,740 | 90 | 16.0 |
| 8 | 30,733 | 3,057 | 151 | 15.2 |
| 16 | 60,295 | 3,575 | 268 | 14.6 |
| 32 | 114,606 | 3,455 | 497 | 14.3 |
| 64 | 224,120 | 3,537 | -- | -- |
| 128 | 441,257 | 3,335 | -- | -- |

**Fifteen heap allocations to run `|v| v + 1`.** Cleanly linear in path length,
~3.5 µs and ~15 allocations per node.

It is not the cycle collector. [Experiment 3b](#experiment-3--what-a-node-update-costs)
holds the propagation path at 16 nodes and varies the size of a second, live,
never-fired chain:

| path nodes | live-but-idle nodes | ns/update |
| ---: | ---: | ---: |
| 16 | 0 | 61,485 |
| 16 | 16 | 59,486 |
| 16 | 64 | 59,487 |
| 16 | 256 | 59,296 |
| 0 | 256 | 4,620 |
| 64 | 0 | 224,569 |

Cost tracks the propagation path and is entirely insensitive to the rest of the
live graph. So `collect_cycles` running at the end of every outermost
transaction is not the problem; per-node propagation is.

Reading `SodiumCtx::update_node`, a single node update performs:

- `box_clone_vec_is_node` on the dependency vector -- **twice**, once for the
  local copy and once more for the copy moved into the `ThreadedMode::spawn`
  closure. Each cloned `Box<dyn IsNode>` is a heap allocation, and each
  `Node::clone` also does `inc_node_ref_count` plus `gc_node.inc_ref()`.
- `box_clone_vec_is_weak_node` on the dependents vector.
- A boxed `pre_post` closure, pushed under the context's global mutex.
- The `ThreadedMode::spawn` dance -- an `Arc<Mutex<Option<R>>>`, a boxed
  closure, a boxed joiner -- **in single-threaded mode too**, where the
  spawner just calls the closure inline.
- A `SodiumCtx::clone`, which is three `Arc` clones.
- Inside `Stream::_send`, a nested `enter_transaction`/`leave_transaction` pair
  and another `pre_post` push, each taking the global mutex again.
- On drop, `GcNode::dec_ref` → `possible_root` → `GcCtx::add_possible_root`,
  which takes the **GC's own global mutex**. `collect_cycles` clears the
  `buffered` flag every transaction, so each node is re-buffered once per
  transaction.

That is roughly two to four acquisitions of one of two global mutexes, per node,
per update, plus fifteen allocations. **The propagation path is already
funnelled through global locks at exactly the granularity any parallel scheme
would want to fan out over.**

### 2.3 `simple_threaded_mode` does not parallelise

```rust
handle = self.threaded_mode.spawn(move || {
    for dependency in &dependencies {
        // ... recurse
    }
});
handle.join();
```

`spawn` is called once *outside* the loop and joined immediately. Under
`simple_threaded_mode` this moves the whole dependency loop onto one other
thread and blocks the caller waiting for it -- zero parallelism, one thread
spawn and one join of pure overhead per node, and a chain of blocked threads at
depth. CLAUDE.md describes it as "thread per fan-out", which is the intent
rather than the code.

## 3. Reading the proposal against all of this

### Where it holds

The engine does know the dependency graph, and within a transaction that graph
genuinely is a complete schedule. Conflict rate genuinely is the quantity that
decides whether optimism pays. GPUs genuinely are the wrong target for scalar
FRP.

### Where it breaks

**The lockdown we are said to have, we do not have.** The claim is that "FRP
achieves a similar level of lockdown due to its highly restricted computational
model." Sodium-the-model does. sodium-rust-the-library does not. `Stream::map`
takes `FnMut(&A) -> B`; nothing prevents that closure printing, incrementing a
captured `AtomicUsize`, or sending on a channel. Haskell's STM works because
the `STM` type constructor makes irrevocable effects *unrepresentable* inside a
transaction -- it is a type-level guarantee, not a convention. We have no
effect system and no equivalent. This is precisely why the OOPSLA authors
promoted abort-freedom to a correctness criterion and then disqualified their
own STM implementation against it.

There is a partial rescue worth developing. The `post` queue already exists to
defer side effects, and listener callbacks drain there. If the rollback
boundary were "before `post` drains", no listener would ever observe
rolled-back state. But that covers only effects routed through Sodium. It does
nothing about a closure that writes to a `Mutex` it captured, and nothing about
a closure that fails to terminate on a torn snapshot -- the opacity problem.

**Two different parallelisms are conflated.** The quote describes
**inter-transaction** parallelism -- many transactions at once -- which is the
hard case and the one STM and MVCC address. The tractable case is
**intra-transaction** parallelism: one transaction, fanned out across the
graph's antichains. That needs no STM at all, because within a transaction the
dependency graph *is* the schedule and each node writes only its own state.
`ThreadedMode` is clearly reaching for this.

Worth noting: Java Sodium's rank-ordered `PriorityQueue<Entry>` materialises
those antichains explicitly -- everything of equal rank is mutually
independent. Our depth-first walk guarded by the atomic `visited` flag does
not. Our design is the harder one to parallelise, not the easier one, and the
`visited` flag being atomic today is not evidence to the contrary.

**The efficiency argument runs backwards.** My first guess was that our node
updates would sit orders of magnitude below the 160 µs break-even. They do not:
a 16-node update costs ~60 µs, within 3× of it. But that is the wrong reading,
and the correct one is the useful finding in this report.

The 160 µs figure is a threshold on **user computation** -- the quantity
parallelism can divide. Our user computation is a closure that adds one:
nanoseconds. The ~60 µs is *our own bookkeeping*. Threading it would spend
cores parallelising our allocator traffic, and would contend on the very global
mutexes that traffic passes through. We are not near the break-even; we are
nowhere near it, and the overhead that makes us look close is the thing to
delete rather than to distribute.

**A note on the JIT suggestion.** The quote's fallback -- runtime profiling to
make scheduling decisions -- is premature for a different reason: we do not
need measurement to know that a `map` node's user closure is nanoseconds. A
static cost estimate plus a sequential cutoff, which is the standard granularity
control in parallel functional languages, covers this without a profiler.

## 4. What the evidence suggests

Three separable pieces, in dependency order. Only the first looks unambiguously
worth doing.

1. **Correctness before speed.** Thread-local transaction depth plus a global
   transaction lock -- Java Sodium's design, known-correct, and cheap when
   uncontended. This turns §2.1 into a non-bug. It is also a prerequisite for
   everything else: no scheduler can be evaluated against a baseline that is
   semantically wrong.
2. **Spend the single-threaded overhead before spending threads.** Fifteen
   allocations per node is a large multiple available without touching
   concurrency, and it directly shrinks the synchronisation surface any future
   scheduler must contend on. The OOPSLA numbers say thread-safe scheduling
   costs ~25% single-threaded; paying that on top of a runtime fifteen
   allocations deep is paying twice.
3. **If parallelism is still wanted: intra-transaction, pessimistic, and
   benchmarked against a global lock as the baseline that must be beaten.**
   The literature's verdict on optimistic rollback in this setting is not that
   it is difficult but that it is unsound in the presence of side effects --
   and that where it was made sound by restricting the workload, it lost to
   `synchronized`.

## 5. What this report does not settle

- **Whether there is a workload that wants this at all.** Everything above is
  about feasibility. None of it establishes demand.
- **Whether the Concurrent Revisions correspondence survives `switch_c`.** The
  fork/join-with-deterministic-merge shape maps suspiciously well onto
  transactions and `merge`. I did not chase it, and dynamic topology is where
  such correspondences usually break.
- **Whether opt-in async (§1.4) is acceptable here.** If explicit annotation is
  allowed, the design space collapses dramatically and the STM question mostly
  goes away. If parallelism must be automatic to be worth having, it does not.
- **Where pre-ADR experiments live.** The four experiments below produce
  numbers this document argues from, and the repository's rule is that such
  code must be re-runnable from a checkout. But `adr-research` names its
  binaries after the record they serve, and there is no record here yet. They
  are inlined below rather than filed under an invented convention or a
  squatted number. If this becomes an ADR they should move there properly; if
  it does not, a research note that carries its own sources is self-contained,
  which is arguably the better property for a document whose measurements are
  of internals an ADR would replace.
- **Whether Experiment 2 should become an `#[ignore]`d test.** Argued for in
  §2.1 and in [#47][issue47], not done. The convention wants an ADR number in
  the `#[ignore]` reason string, and there is no record yet; `CLAUDE.md`'s
  claim that there are currently no such tests would need updating alongside.

## Reproducing the measurements

Each block is a standalone integration test. Drop it in `tests/`, run the given
command, delete it.

### Experiment 1 -- transaction boundaries merge across threads

`cargo test --test scratch -- --nocapture`

```rust
use sodium_rust::SodiumCtx;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn concurrent_transactions_from_two_threads() {
    let ctx = SodiumCtx::new();
    let sink_a = ctx.new_stream_sink::<i32>();
    let sink_b = ctx.new_stream_sink::<i32>();
    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let l1 = {
        let log = log.clone();
        sink_a.stream().listen(move |v: &i32| {
            log.lock().unwrap().push(format!("a fired {}", v));
        })
    };
    let l2 = {
        let log = log.clone();
        sink_b.stream().listen(move |v: &i32| {
            log.lock().unwrap().push(format!("b fired {}", v));
        })
    };

    let ctx2 = ctx.clone();
    let log2 = log.clone();
    let t = thread::spawn(move || {
        ctx2.transaction(|| {
            log2.lock().unwrap().push("A: opened transaction".into());
            sink_a.send(1);
            thread::sleep(Duration::from_millis(300));
            log2.lock().unwrap().push("A: closing transaction".into());
        });
        log2.lock().unwrap().push("A: transaction returned".into());
    });

    thread::sleep(Duration::from_millis(100));
    log.lock().unwrap().push("B: about to send".into());
    sink_b.send(2);
    log.lock().unwrap().push("B: send returned".into());

    t.join().unwrap();
    thread::sleep(Duration::from_millis(50));
    for line in log.lock().unwrap().iter() {
        println!("{}", line);
    }
    drop(l1);
    drop(l2);
}
```

### Experiment 2 -- independent events observed as simultaneous

`cargo test --test scratch -- --nocapture`

```rust
use sodium_rust::SodiumCtx;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Two *independent* events, fired from two threads, must be two separate
/// transactions: `merge` should fire twice, never once with the coalescer run.
#[test]
fn independent_events_from_two_threads_must_not_coalesce() {
    let ctx = SodiumCtx::new();
    let sink_a = ctx.new_stream_sink::<i32>();
    let sink_b = ctx.new_stream_sink::<i32>();

    let merged = sink_a.stream().merge(&sink_b.stream(), |a: &i32, b: &i32| {
        println!("  !! coalescer ran: {} and {} were treated as SIMULTANEOUS", a, b);
        a + b
    });

    let fired: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));
    let l = {
        let fired = fired.clone();
        merged.listen(move |v: &i32| fired.lock().unwrap().push(*v))
    };

    let ctx2 = ctx.clone();
    let t = thread::spawn(move || {
        ctx2.transaction(|| {
            sink_a.send(1);
            thread::sleep(Duration::from_millis(300));
        });
    });

    thread::sleep(Duration::from_millis(100));
    sink_b.send(2);

    t.join().unwrap();
    thread::sleep(Duration::from_millis(50));

    let fired = fired.lock().unwrap().clone();
    println!("merged stream fired with: {:?}", fired);
    println!("expected two separate firings [1, 2]; got {} firing(s)", fired.len());
    drop(l);
}
```

### Experiment 3 -- what a node update costs

`cargo test --release --test scratch -- --nocapture`

```rust
use sodium_rust::{Listener, SodiumCtx, Stream};
use std::time::Instant;

fn measure(path: usize, live: usize, n: u64) -> f64 {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<u64>();
    let other = ctx.new_stream_sink::<u64>();

    let mut s: Stream<u64> = sink.stream();
    for _ in 0..path {
        s = s.map(|v: &u64| v.wrapping_add(1));
    }
    let mut sum: u64 = 0;
    let _l = s.listen(move |v: &u64| sum = sum.wrapping_add(*v));

    // A second chain, kept alive by its own listener, never fired into.
    let mut o: Stream<u64> = other.stream();
    for _ in 0..live {
        o = o.map(|v: &u64| v.wrapping_add(1));
    }
    let mut sum2: u64 = 0;
    let _l2: Listener = o.listen(move |v: &u64| sum2 = sum2.wrapping_add(*v));

    for v in 0..500 {
        sink.send(v);
    }
    let t0 = Instant::now();
    for v in 0..n {
        sink.send(v);
    }
    t0.elapsed().as_nanos() as f64 / n as f64
}

#[test]
fn cost_scaling() {
    println!("{:>6} {:>12} {:>14}", "nodes", "ns/update", "ns/node");
    let mut prev = 0.0f64;
    for &nodes in &[1usize, 2, 4, 8, 16, 32, 64, 128] {
        let ns = measure(nodes, 0, if nodes > 32 { 5_000 } else { 20_000 });
        let marginal = if prev > 0.0 { (ns - prev) / (nodes as f64 / 2.0) } else { f64::NAN };
        println!("{:>6} {:>12.0} {:>14.0}   marginal/node: {:.0} ns", nodes, ns, ns / nodes as f64, marginal);
        prev = ns;
    }
}

/// If cost tracks `live`, it is graph-wide work (cycle collection); if it
/// tracks only `path`, it is propagation.
#[test]
fn propagation_or_graph_wide() {
    println!("{:>6} {:>6} {:>12}", "path", "live", "ns/update");
    for &(path, live) in &[(16usize, 0usize), (16, 16), (16, 64), (16, 256), (0, 256), (64, 0)] {
        println!("{:>6} {:>6} {:>12.0}", path, live, measure(path, live, 10_000));
    }
}
```

### Experiment 4 -- heap allocations per node update

`cargo test --release --test scratch -- --nocapture`

```rust
use sodium_rust::{SodiumCtx, Stream};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn allocs_per_update(nodes: usize) -> f64 {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<u64>();
    let mut s: Stream<u64> = sink.stream();
    for _ in 0..nodes {
        s = s.map(|v: &u64| v.wrapping_add(1));
    }
    let mut sum: u64 = 0;
    let _l = s.listen(move |v: &u64| sum = sum.wrapping_add(*v));
    for v in 0..100 {
        sink.send(v);
    }
    const N: u64 = 2000;
    let before = ALLOCS.load(Ordering::Relaxed);
    for v in 0..N {
        sink.send(v);
    }
    (ALLOCS.load(Ordering::Relaxed) - before) as f64 / N as f64
}

#[test]
fn heap_allocations_per_update() {
    println!("{:>6} {:>16} {:>16}", "nodes", "allocs/update", "allocs/node");
    let mut prev = 0.0;
    for &nodes in &[1usize, 2, 4, 8, 16, 32] {
        let a = allocs_per_update(nodes);
        let marginal = if prev > 0.0 { (a - prev) / (nodes as f64 / 2.0) } else { f64::NAN };
        println!("{:>6} {:>16.1} {:>16.1}   marginal: {:.1}", nodes, a, a / nodes as f64, marginal);
        prev = a;
    }
}
```

## Bibliography

**Reactive runtimes and concurrency**

- Drechsler, Mogk, Salvaneschi & Mezini. [Thread-Safe Reactive
  Programming][tsrp]. OOPSLA 2018.
- Margara & Salvaneschi. [On the Semantics of Distributed Reactive Programming:
  the Cost of Consistency][cost]. IEEE TSE 2018.
- Margara & Salvaneschi. [We Have a DREAM: Distributed Reactive Programming
  with Consistency Guarantees][dream]. DEBS 2014.
- Drechsler, Salvaneschi, Mogk & Mezini. [Distributed REScala: An Update
  Algorithm for Distributed Reactive Programming][sidup]. OOPSLA 2014.
- Czaplicki & Chong. [Asynchronous Functional Reactive Programming for
  GUIs][elm]. PLDI 2013. And Czaplicki, [Elm: Concurrent FRP for Functional
  GUIs][elmthesis], 2012.
- Peterson, Trifonov & Serjantov. [Parallel Functional Reactive
  Programming][pfrp]. PADL 2000.
- Bainomugisha, Carreton, Van Cutsem, Mostinckx & De Meuter. [A Survey on
  Reactive Programming][survey]. ACM Computing Surveys 2013.

**Determinism without rollback**

- Burckhardt & Leijen. [Semantics of Concurrent Revisions][revisions]. ESOP
  2011. See also the [mechanised determinacy proof][determinacy].
- Kuper & Newton. [LVars: lattice-based data structures for deterministic
  parallelism][lvars]. FHPC 2013, and [Freeze After Writing][freeze].
- McDirmid. [Glitch: A Live Programming Model][glitch] and [Programming with
  Managed Time][managedtime].

**Transactional memory**

- Dice, Shalev & Shavit. [Transactional Locking II][tl2]. DISC 2006.
- Dalessandro, Spear & Scott. [NOrec: Streamlining STM by Abolishing Ownership
  Records][norec]. PPoPP 2010.
- Harris, Marlow, Peyton Jones & Herlihy. [Composable Memory
  Transactions][cmt]. PPoPP 2005.
- Guerraoui & Kapalka. [On the Correctness of Transactional Memory][opacity]
  (opacity). PPoPP 2008.
- Cascaval et al. [Software Transactional Memory: Why Is It Only a Research
  Toy?][toy]. CACM 2008, and the [rebuttal][nottoy].
- Intel. [TSX Memory Ordering Issue / deprecation notice][tsx].
- Rust: [`stm`][stmcrate].

**Incremental and dataflow**

- Anderson et al. [Efficient Parallel Self-Adjusting Computation][psac], and
  Hammer et al. [A proposal for parallel self-adjusting computation][psac07],
  DAMP 2007.
- McSherry et al. [Timely dataflow][td] and [differential dataflow][dd].

**Garbage collection**

- Bacon & Rajan. [Concurrent Cycle Collection in Reference Counted
  Systems][baconrajan]. ECOOP 2001.

[tsrp]: https://programming-group.com/assets/pdf/papers/2018-Thread-safe-reactive-programming.pdf
[cost]: https://re.public.polimi.it/retrieve/e0c31c11-3bae-4599-e053-1705fe0aef77/11311-1059154_Margara.pdf
[dream]: https://margara.faculty.polimi.it/papers/dream_debs14.pdf
[sidup]: https://programming-group.com/assets/pdf/papers/2014_Distributed_REScala_An_Update_Algorithm_for_Distributed_Reactive_Programming.pdf
[elm]: https://people.seas.harvard.edu/~chong/pubs/pldi13-elm.pdf
[elmthesis]: https://elm-lang.org/assets/papers/concurrent-frp.pdf
[pfrp]: https://link.springer.com/chapter/10.1007/3-540-46584-7_2
[survey]: https://soft.vub.ac.be/Publications/2012/vub-soft-tr-12-13.pdf
[revisions]: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/semantics-revisions-2010.pdf
[determinacy]: https://arxiv.org/html/1912.09741
[lvars]: https://users.soe.ucsc.edu/~lkuper/papers/lvars-fhpc13.pdf
[freeze]: https://users.soe.ucsc.edu/~lkuper/papers/lindsey-kuper-dissertation.pdf
[glitch]: https://www.microsoft.com/en-us/research/publication/glitch-a-live-programming-model/
[managedtime]: https://www.microsoft.com/en-us/research/publication/programming-with-managed-time/
[tl2]: https://dcl.epfl.ch/site/_media/education/4.pdf
[norec]: https://dl.acm.org/doi/10.1145/1693453.1693464
[cmt]: https://simonmar.github.io/bib/papers/stm.pdf
[opacity]: https://infoscience.epfl.ch/bitstreams/9f16872d-7c62-4a6f-bdb9-21df82549c71/download
[toy]: https://cacm.acm.org/practice/software-transactional-memory-why-is-it-only-a-research-toy/
[nottoy]: https://cacm.acm.org/research/why-stm-can-be-more-than-a-research-toy/
[tsx]: https://www.intel.com/content/www/us/en/support/articles/000059422/processors.html
[stmcrate]: https://crates.io/keywords/stm
[psac]: https://arxiv.org/pdf/2105.06712
[psac07]: https://dl.acm.org/doi/10.1145/1248648.1248651
[td]: https://github.com/TimelyDataflow/timely-dataflow
[dd]: https://github.com/TimelyDataflow/differential-dataflow
[baconrajan]: https://link.springer.com/chapter/10.1007/3-540-45337-7_12
[contributing]: ../../CONTRIBUTING.md
[issue47]: https://github.com/RadicalZephyr/sodium-rust/issues/47
