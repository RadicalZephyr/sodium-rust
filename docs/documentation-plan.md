# Sodium documentation plan

A plan for bringing `sodium` (the Rust port of Sodium FRP) to a
publishable documentation standard, organised with the
[Diátaxis](https://diataxis.fr/) framework.

The audience is programmers who already know Rust and may know nothing
about FRP. Three readers to keep in mind:

| Reader | Arrives with | Wants |
| --- | --- | --- |
| **The curious Rust dev** | Saw "FRP" in a crate description, knows `Iterator`, channels, maybe `futures::Stream` | To find out in five minutes whether this solves a problem they have, then a path to a working program |
| **The Rx refugee** | Has used RxJS/RxJava/ReactiveX, or a signals crate | A name map, and a straight answer on what Sodium guarantees that Rx doesn't |
| **The book reader** | Has read Blackheath & Jones, knows the Java API | The Rust spelling of every primitive, and what differs from Java Sodium |

The plan is written as decisions plus a work breakdown. Nothing in it
requires a big-bang rewrite; each phase leaves the docs in a
complete, publishable state.

---

## 1. Where things stand

### 1.1 The library surface

The public API is small: 13 types and roughly 90 public functions,
almost all on `Stream` and `Cell`. A third of the functions are
`*_with_deps` siblings and arity variants (`snapshot3..6`,
`lift2..6`), so the number of *concepts* to document is closer to 30.

What exists today:

- **One-line doc comments on nearly every item**, largely lifted from
  Appendix A of the FRP book (the Java API). They are correct but thin:
  they say *what*, rarely *when*, never *what happens in the same
  transaction*.
- **Two doctests** in the whole crate, both on `Stream::filter_map`.
- **A one-sentence crate root** (`//! Sodium is a library for doing
  Functional Reactive Programming (FRP) in Rust.`).
- **No `examples/` directory.** `src/tests.rs` (1,636 lines, 50 tests)
  is the de-facto example set and the README says so.
- **`docs/internals/insights.md`**: four paragraphs on `Node` and
  listener rooting. Useful seed for an Explanation page.
- **A good README** (rewritten 2026-09-01): what/why, install, one
  example, the two pitfalls, repo layout, contributing.

`cargo rustc -- -W missing-docs` reports 48 warnings. Filtering out the
`#[doc(hidden)]` lambda machinery, the user-visible gaps are:

| Item | Problem |
| --- | --- |
| `Stream::split_enum2`, `split_enum3`, `split_opt`, `split_res` | No docs at all |
| `Operational::updates`, `value`, `defer` | No docs (the struct has one line) |
| `Enum2`, `Enum3` and their variants | No docs; exported from `impl_` |
| `Dep`, `Dep::new`, `Dep::gc_node` | No docs, but `Dep` appears in every `*_with_deps` signature |
| `pub impl_` fields on `Cell`, `Stream`, `SodiumCtx`, `StreamSink`, `CellSink`, `StreamLoop`, `Listener` | Public, undocumented, and they leak the entire `impl_` module into docs.rs. Should be `pub(crate)` or `#[doc(hidden)]` |
| `Router` struct doc | Reads "Create a new Router…" (copied from the constructor) |

Typos in existing docs: "wass" (`Stream::snapshot`), "precedenc"
(`or_else`), "additon" (`accum`), "in `once` was invoked" (`once`).

What is *absent from the Rust port* compared with Java Sodium
(Appendix A), and must be stated somewhere so book readers stop
looking: `Cell::apply`, `Stream::listen_once`, `Stream::add_cleanup`,
collection variants of `merge`/`or_else`, `Transaction::on_start`,
and the whole `nz.sodium.time` package (`TimerSystem`). Conversely the
Rust port has things Java doesn't: `Router`, `split_res`/`split_opt`/
`split_enum*`, `filter_map`, `Transaction` as a drop guard, the
`*_with_deps` mechanism, `new_stream_sink_with_coalescer`.

### 1.2 The abandoned branches

Both `documentation` and `doc-thoughts` fork from `3e93021` (March
2024), before the `*_with_deps` API change, and both refer to the
crate as `sodium_rust`. **Their prose ideas are worth keeping; their
code is not** (every sample uses `lambda1` or annotated closures).

| Branch | Keep | Drop |
| --- | --- | --- |
| `documentation` | `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, issue templates, `doc/conduct/reporting-guide.md` (cherry-pick as-is, retarget `master` → `main`). The lib.rs crate-root draft's framing: batch vs event-driven programs, "Sodium is only the internal logic", "generalisation of the Observer pattern", "two phases: build the graph, then run it" | The README rewrite (superseded), `examples/hello-world.rs` (wrong crate name, old API), the `doc/guides/*` links to pages that were never written |
| `doc-thoughts` | `docs/thoughts/introduction.md`: the "draw the flowchart first, find the I/O edges" design method; "lift a `Result` into two streams with `split_res`"; "put expensive data behind `Arc`". `options.md`: `Option` in a `Cell` is awkward. `design-paralysis.md`: "refactoring is cheap, so try something" | `doc/getting-started.md` (a two-paragraph stub; the tic-tac-toe idea is reconsidered in §4.1), the "Process" venting section, the near-empty `insights.md` diff |

### 1.3 Crate identity

This needs a decision before any docs go public:

- crates.io has **two** crates pointing at this repo: `sodium-rust`
  (1.0.0 … 2.1.2, last published 2022-11, 13.8k downloads) and
  `sodium` (0.1.0 … 0.1.3, published Feb–Mar 2024, 6.5k downloads).
- `Cargo.toml` says `sodium` 0.1.3; `CHANGELOG.md` numbers releases
  2.x and has an `[Unreleased]` section above 2.1.2.
- `docs.rs/sodium` therefore shows 0.1.3 without the `*_with_deps` API.

**Assumption used below:** the crate continues as `sodium`, the next
release is a breaking one (the changelog already says so), and the
docs will carry a short "which crate?" note plus a migration guide
from `sodium-rust` 2.x. If you prefer to renumber `sodium` to 3.0 to
match the changelog, nothing else in this plan changes.

### 1.4 What the book gives us

The mdbook is a private rendering of a Manning-copyrighted work. The
plan uses it in two ways only:

1. **As a map of concepts and their teaching order**, which is
   excellent and hard-won: streams before cells, `map` before
   `merge`, simultaneity as a first-class topic, `snapshot`/`hold`
   as the state loop, `switch` and `sample` deferred to their own
   chapter, operational primitives last.
2. **As a specification.** Appendix E (denotational semantics) gives
   a test case per primitive. Those are exactly the doctests the
   reference needs.

All prose in the new docs is written fresh. Book examples (petrol pump,
zombies, spinner) are re-implemented as original Rust programs where
they are used; no book text is included or paraphrased at length. Deep
dives link to the book by chapter for readers who own it.

Chapter-to-documentation mapping, so nothing important is missed:

| Book | Concept | Lands in |
| --- | --- | --- |
| Ch 1, App B | Why FRP; the six plagues of listeners | Explanation: *Why Sodium* |
| Ch 2 | The ten primitives; referential transparency; simultaneity; cheat sheet | Tutorial 1–2; Reference: *Primitives table*, *Rules*; Explanation: *Streams and cells* |
| Ch 3 | Spinner, form validation | Tutorial 2 |
| Ch 4 | Petrol pump; modules; explicit wiring | Tutorial 5 (flagship example) |
| Ch 5 | Compositionality, immutability | Explanation: *Why Sodium* (brief) |
| Ch 6 | Rx equivalence table | Reference: *Coming from Rx* |
| Ch 7 | `sample`, `switch_c`, `switch_s`, dynamic graphs, big merges | Tutorial 4; How-to: *Router* |
| Ch 8 | send/listen, transactions, laziness, `updates`/`value`, `split`/`defer`, scalable addressing | Tutorial 3; Explanation: *Transactions*; How-tos on I/O, loops, Router |
| Ch 9 | Continuous time, timers | How-to: *Build a timer system* (Rust has none built in) |
| Ch 11 | I/O, promises, unit testing | How-tos: *Blocking I/O*, *Test FRP logic* |
| Ch 12 | Calming, pausing, junctions, unique IDs | How-to: *Common helpers* (a recipes page) |
| Ch 13–14 | Refactoring; adding FRP to existing code; `send` inside a listener | How-to: *Wrap a callback API*; Explanation: *Designing a Sodium program* |
| App E | Semantics + tests | Reference doctests |

### 1.5 What ReactiveX does that we should borrow

ReactiveX's docs are the most successful attempt at teaching push-based
reactive programming to mainstream developers. Four devices transfer
directly:

1. **Introduce by contrast with the familiar.** Rx opens with an
   Iterable-vs-Observable table (pull vs push, `next()` vs `onNext`).
   For Rust the analogue is a `Iterator` / `mpsc::Receiver` /
   `Future` / `Stream<A>` comparison, and "a `Cell` is a variable you
   can only read from inside the graph".
2. **Marble diagrams.** Every Rx operator page leads with one. We will
   use a single text notation (§5.3) in rustdoc and the same notation
   rendered as SVG in the guide.
3. **Operators by category, plus a decision tree.** Rx groups by
   Creating / Transforming / Filtering / Combining / Utility, and the
   "I want to…" tree on the operators page is the most-visited part of
   the site. The primitives table and a "which combinator?" page do
   this for Sodium.
4. **A name map from other systems.** Rx documents every language
   binding's spelling side by side. Our version is the three-column
   Sodium-Rust / Java Sodium / Rx table.

What we deliberately do *not* copy: Rx's operator count. Sodium's
pitch is ten primitives; the docs should make that feel like a
feature, and the helpers page should show readers building their own.

---

## 2. Target structure

Diátaxis wants four kinds of documentation kept apart. For a Rust
library the four kinds also have natural *homes*:

```
README.md                      landing page: what, why, 30-second example, four doors
docs.rs (rustdoc)              REFERENCE for every public item + crate-root overview
docs/book/  (mdBook → Pages)   TUTORIALS · HOW-TO GUIDES · EXPLANATION · non-API REFERENCE
examples/                      runnable code that the book includes verbatim
CONTRIBUTING.md                how to write docs for this crate (checklist)
```

Reference lives in rustdoc because that is where Rust programmers look
first and because doctests keep it honest. Everything that is *about*
using the API rather than *of* the API lives in the mdBook, deployed to
GitHub Pages and linked from the crate root, the README, and the
`documentation` field in `Cargo.toml`.

The guide's table of contents, in full:

```
Sodium FRP for Rust
├── Introduction (what this is, who it is for, where to start)
├── Tutorials
│   ├── 1. Your first Sodium program           (StreamSink, map, accum, listen)
│   ├── 2. Cells: values that change            (CellSink, map, lift, hold; form validation)
│   ├── 3. State and feedback                   (snapshot, StreamLoop/CellLoop, transactions)
│   ├── 4. Switching                             (sample, switch_c, switch_s; screens or characters)
│   ├── 5. A real application: the pump          (multi-part; modules; I/O boundary; tests)
│   └── 6. Talking to the outside world          (threads, channels, post, defer, a timer)
├── How-to guides
│   ├── Feed events in from a thread, channel or async runtime
│   ├── Do blocking I/O in response to an event
│   ├── Build a clock and timers
│   ├── Split a Result, Option or enum into separate streams
│   ├── Fan out one stream to many receivers (Router)
│   ├── Send several values in one transaction (coalescing sinks)
│   ├── Build the whole graph in one transaction
│   ├── Create a forward reference without panicking (loops, sample_lazy, hold_lazy)
│   ├── Capture a Cell in a closure correctly (*_with_deps and the Dep contract)
│   ├── Wrap a callback-based API as a Stream
│   ├── Test FRP logic
│   ├── Find a leak or a stuck listener (listen vs listen_weak, RUST_LOG=trace graph dump)
│   ├── Common helpers: calm, pause, unique IDs, junctions
│   ├── Make it faster: node count, filter_map, Router, criterion, coz
│   └── Migrate from sodium-rust 2.x
├── Explanation
│   ├── Why Sodium (the six plagues, compositionality)
│   ├── Streams and cells: why two types
│   ├── Transactions and simultaneity
│   ├── What a closure may do (referential transparency, sample, constructing FRP)
│   ├── Memory management and dependency tracking (why Dep exists)
│   ├── Operational primitives and non-detectability
│   ├── Sodium and Rx: same shapes, different guarantees
│   ├── Sodium and the Rust ecosystem (channels, futures::Stream, signals crates, Elm-style UIs)
│   ├── Designing a Sodium program (flowchart first, I/O edges, functional core)
│   ├── Choosing types (Clone, Arc, Option in cells, Send + Sync)
│   └── Threading model and performance model
└── Reference (non-API)
    ├── The primitives at a glance (table: what goes in, what comes out)
    ├── Which combinator do I want? (decision tree)
    ├── Reading the diagrams (the marble notation)
    ├── The rules (what may not be done where, and what panics)
    ├── Name map: Sodium-Rust · Java Sodium · Rx
    ├── Differences from Java Sodium
    └── Glossary
```

Every page carries a one-line "this is a tutorial / how-to /
explanation / reference" framing at the top, and the sidebar groups
match the four headings. That is the part of Diátaxis readers notice
without knowing the name.

---

## 3. Reference: the rustdoc standard

Reference is the cheapest quadrant to finish and the only one docs.rs
shows, so it comes first.

### 3.1 Per-item template

Every public function gets, in this order:

1. **One sentence** saying what the result is (not what the function
   "does"). *"A stream that fires whenever `self` fires, carrying `f`
   applied to the value."*
2. **Timing semantics.** When it fires relative to its inputs, what it
   sees inside the current transaction, what happens on simultaneous
   events. This is the paragraph almost every current doc is missing
   and it is the one that separates Sodium from Rx.
3. **A marble diagram** in the shared notation (§5.3), for anything
   whose behaviour is about *when*.
4. **`# Examples`**: a doctest that compiles and asserts. Prefer
   Appendix E's test case for the primitive. Use a small shared
   scaffolding idiom (sink → `listen` into a `Vec` → `assert_eq!`) so
   every example reads the same.
5. **`# Panics`** where applicable: `StreamSink::send` twice per
   transaction without a coalescer; `send` inside a listener;
   `loop_` outside the constructing transaction; `sample` on an
   unlooped `CellLoop`; `Lazy::run` before the transaction closes.
6. **See also** links: the `*_with_deps` sibling, the lazy variant,
   the primitive it is built from, the guide page that explains it.

`*_with_deps` siblings do not repeat all of that; they say "same as
`X` and additionally declares…" and link to the *Capture a Cell in a
closure* how-to. The arity variants (`snapshot3..6`, `lift3..6`) get
one sentence and a link to the 2-arity doc.

### 3.2 Type-level and crate-level docs

- `Stream`, `Cell`: the current one-paragraph docs are good starts.
  Add: how values enter (sinks), how they leave (listeners), the
  `Clone + Send + 'static` bound and why, a categorised method index
  (create / transform / filter / combine / state / interface), and a
  marble diagram of each type.
- `SodiumCtx`: "one per application; every node is created from it;
  no global state". Explain `transaction` vs `new_transaction`, and
  `post`.
- `StreamSink`/`CellSink`: the I/O boundary; the rules on `send`.
- `StreamLoop`/`CellLoop`: forward references; must be looped in the
  same transaction; link to the lazy variants.
- `Operational`: what non-detectability means and when breaking it is
  acceptable. Document the three functions (currently undocumented).
- `Router`: fix the struct doc; explain the O(1) dispatch vs N filters.
- `Lazy`, `Dep`, `Enum2/3`, `Listener`, `Transaction`: complete.
- **Crate root (`lib.rs`)**: the front door on docs.rs. About 80
  lines: the two-sentence pitch; the counter example from the README;
  "the two types"; the lifecycle (build the graph in a transaction,
  attach listeners, send, unlisten); the four rules; a module map with
  links; links to the guide's tutorials and to the book. Consider
  `#![doc = include_str!("../README.md")]` only if the README is
  restructured to read well in both places; otherwise keep them
  separate and short.

### 3.3 Hygiene that changes the rendered reference

- Make the `impl_` fields `pub(crate)`, or `#[doc(hidden)]` if any
  external code (the `coz-driver` workload, benches) reaches through
  them. `src/tests/mem_test.rs` uses `ctx.impl_.node_count()`; if a
  leak-check how-to wants that, add a documented
  `SodiumCtx::node_count()` instead of exposing internals.
- `#![warn(missing_docs)]` in `lib.rs` now; promote to CI enforcement
  (`RUSTDOCFLAGS="-D warnings -D missing_docs" cargo doc --no-deps`)
  once the count is zero.
- Add `documentation`, `homepage`, `readme` to `Cargo.toml`, and
  `[package.metadata.docs.rs]` with `all-features = true`.
- Rename `snapshot1` in docs as "the value-ignoring snapshot" and
  cross-link from `snapshot`; readers will not guess the `1`.

### 3.4 Non-API reference pages (in the book)

These are reference in the Diátaxis sense but do not belong to any one
item, so rustdoc cannot host them well:

- **Primitives at a glance**: the Rust version of the book's Table
  2.2, extended with the helpers and marked as primitive/helper/
  operational.
- **Which combinator do I want?** An Rx-style decision tree:
  "I have a stream and want… a cell → `hold`/`accum`; only some events
  → `filter`/`gate`/`filter_map`; to react to it with state →
  `snapshot`/`collect`; to combine with another stream → `merge`/
  `or_else`; to route to many → `Router`; …"
- **Reading the diagrams**: the notation, once.
- **The rules**: consolidated from the book's §8.1.4 and Table 2.1,
  written for Rust (what a closure passed to a combinator may do; what
  a listener may do; what panics).
- **Name map**: Sodium-Rust · Java Sodium · Rx (from the book's Table
  6.2, updated for RxJS 7 names: `withLatestFrom`, `scan`,
  `combineLatest`, `switchMap` ≈ `switch_s`).
- **Differences from Java Sodium**: the list in §1.1.
- **Glossary**: stream, cell, fire, transaction, simultaneous, hold,
  snapshot, sink, listener, node, dependency, non-detectability,
  referential transparency, glitch.

---

## 4. Tutorials

Tutorials are learning-oriented: one path, no choices, a working result
at the end of each, and the reader types every line. Each tutorial is
a runnable program under `examples/` that the book page includes with
`{{#include}}` anchors, so the text can never drift from code that
compiles.

### 4.1 The sequence

| # | Title | Reader builds | Introduces | Length |
| --- | --- | --- | --- | --- |
| 1 | Your first Sodium program | A click counter that prints | `SodiumCtx`, `StreamSink`, `map`, `accum`, `listen`, `unlisten`; "the graph is built, then run" | 15 min |
| 2 | Cells: values that change | A form validator: two text fields, a computed "valid" cell, a message | `CellSink`, `Cell::map`, `lift2`, `hold`, why a cell always has a value | 20 min |
| 3 | State and feedback | A spinner with min/max, then a running total that depends on itself | `snapshot`, `filter`, `merge`, `StreamLoop`/`CellLoop`, `ctx.transaction`, "hold is delayed" | 30 min |
| 4 | Switching | A screen switcher (menu / game / paused) or a character that changes behaviour | `sample`, `switch_c`, `switch_s`, dynamic graphs, why `sample` in `map` is a snapshot | 30 min |
| 5 | A real application | A terminal petrol-pump simulator in five parts: life cycle, counting, dollars, point-of-sale, keypad preset | Modules as functions with input/output structs, explicit wiring, testing each module, `Operational::defer`, `split` | 2 h |
| 6 | Talking to the outside world | Wire tutorial 5's pump to stdin on one thread and a simulated network on another | Threads + channels into sinks, `post`, blocking I/O off the graph, building a timer with `CellSink<Instant>` | 45 min |

Tutorial 5 is the flagship. The petrol pump is the canonical Sodium
example, exists in every other port, and exercises modularity, which is
the argument for Sodium over ad-hoc callbacks. It should be a real
`examples/pump/` crate, not a toy. If a Rust-native flagship (a
key-value server, a TUI) feels more persuasive to Rust readers, swap it
here; the tutorial structure stays the same.

The `doc-thoughts` tic-tac-toe idea is not used: it needs a game loop
and rendering before it teaches anything, whereas the counter and the
form validator teach a primitive per paragraph.

### 4.2 Tutorial rules

- No `*_with_deps` until tutorial 4, and then only once, with the
  explanation that `snapshot` is usually the better answer.
- Never annotate closure parameters (the crate now infers them); the
  README's habit of `|n: &i32|` should go.
- Every tutorial ends with "what you learned" and one link into each
  of the other three quadrants.
- Output is asserted or shown; the reader must be able to check they
  are on track after every step.

---

## 5. How-to guides, Explanation, and shared devices

### 5.1 How-to guides

Task-oriented, assume competence, start from a goal, and end when the
goal is reached. The list in §2 is ordered by how often the question
will be asked. Three deserve notes:

- **Capture a Cell in a closure correctly.** This is the crate's one
  sharp edge (`Dep`s must mirror captures exactly, or the collector's
  bookkeeping is corrupted). The page shows the wrong version, the
  `snapshot` version, and the `*_with_deps` version, and states the
  contract in one sentence.
- **Feed events in from a thread, channel or async runtime.** Rust
  readers will arrive with `tokio`. The guide shows a `std::thread` +
  `mpsc` version and a `tokio` task version, both calling `send` from
  outside any listener, and points at `post` for the reverse
  direction.
- **Migrate from sodium-rust 2.x.** `lambda1(…, vec![…])` →
  `*_with_deps`, crate rename, closure annotations that can be
  dropped, `IsLambda` traits gone from signatures.

### 5.2 Explanation

Understanding-oriented; written to be read away from the keyboard. The
two pages that matter most for adoption:

- **Sodium and Rx.** Rx readers will assume `Cell` is
  `BehaviorSubject`. It is not: cells have no observable steps, updates
  within a transaction are atomic, `lift` cannot glitch, `snapshot`
  sees the pre-transaction value. Show the diamond-dependency glitch in
  Rx and its absence in Sodium; that single diagram justifies the
  crate.
- **Sodium and the Rust ecosystem.** Where it sits relative to
  channels (transport, not logic), `futures::Stream` (pull-based,
  async), signals crates (`futures-signals`, Leptos-style reactive
  graphs: similar cells, no transactions or streams), and Elm-style
  UI frameworks (`iced`: message → update → view, which Sodium can
  implement the `update` of). Honest about the fit: event-driven
  programs yes, batch pipelines no (from `doc-thoughts`).

`docs/internals/insights.md` becomes the seed of *Memory management and
dependency tracking*, extended with why listeners root the graph, what
`Dep` asserts, and what `RUST_LOG=trace` prints (the collector draws
the graph).

### 5.3 One diagram notation

Pick once, use everywhere. Recommended: the RxJS marble-test syntax,
which many readers already know and which is plain text, so it works
in rustdoc without images:

```text
transactions:   1    2    3    4
clicks     ---a----b----c----
count      0--1----2----3----      (a Cell: value shown between ticks)
```

Rules: one column per transaction; `-` is "nothing"; a letter is a
firing; a `Cell` line shows its value in the gaps. Simultaneous events
share a column. The book's page *Reading the diagrams* defines it, and
the guide renders the same diagrams as SVG (mermaid cannot do marble
diagrams; a 40-line script that emits SVG from the text notation is
cheaper than hand-drawing and keeps the two in sync).

### 5.4 Shared example scaffolding

Every doctest and how-to needs "collect what a stream fired". Rather
than repeating the `Arc<Mutex<Vec>>` idiom hundreds of times, add a
documented test helper. Two options, in order of preference:

1. A `#[doc(hidden)] pub mod testing` (or a `testing` feature) with
   `Stream::collect_into_vec`-style helper returning
   `(Arc<Mutex<Vec<A>>>, Listener)`, as `tests/closure_type_inference.rs`
   already defines privately.
2. Keep it private and paste the four-line helper into each doctest
   behind `#` hidden lines.

Option 1 also gives the *Test FRP logic* how-to something to
recommend.

---

## 6. Infrastructure

| Piece | Action |
| --- | --- |
| **mdBook** | `docs/book/` with `book.toml` mirroring `frp-mdbook` (mermaid, linkcheck, `use-default-preprocessors = false` + links). Tools are already installed locally. |
| **Code in the book** | All Rust in the guide comes from `examples/` or `tests/book/*.rs` via `{{#include file:anchor}}`. `cargo test --workspace` then proves every listing. Run `mdbook test` in CI as a second net for the few inline snippets. |
| **`examples/`** | One file or directory per tutorial; `cargo run --example counter`. Listed in the README. |
| **CI additions** | `cargo doc --no-deps` with `-D warnings`; `cargo test --doc`; `mdbook build` (linkcheck fails on a broken link); `mdbook test`; deploy `docs/book/book/html` to GitHub Pages on push to `main`. |
| **Cargo.toml** | `documentation = "https://docs.rs/sodium"`, `homepage` = the Pages URL, `readme = "README.md"`, `[package.metadata.docs.rs]`. |
| **CONTRIBUTING.md** | Cherry-pick from `documentation`, add a "documenting a public item" checklist (the §3.1 template) and "which quadrant does this page belong to?" (the Diátaxis compass in two questions). |
| **Salvage** | Cherry-pick `328bf6d`, `1c9cde0`, `64373b0`, `911951e` from `documentation`; copy the prose fragments listed in §1.2 into the relevant Explanation drafts; then delete both branches. |

---

## 7. Phasing

Each phase ends in a publishable state. Sizes: S = a day or two, M = a
week, L = two to three weeks of focused writing.

### Phase 0 — Decisions and scaffolding (S)

- Settle crate name/version (§1.3) and where Pages will be hosted
  (`RadicalZephyr/sodium-rust` vs the `SodiumFRP` org).
- Hide `impl_` fields; add `#![warn(missing_docs)]`; fix the four
  typos and the `Router` struct doc; document the 12 undocumented
  items with one line each. **Missing-docs count reaches zero.**
- Cherry-pick CoC / CONTRIBUTING / templates.
- Create `docs/book/` skeleton with the §2 table of contents as
  stubs (each stub says what will be there; no empty pages published),
  the CI jobs, and Pages deployment.
- Write *Reading the diagrams* and agree the notation.

### Phase 1 — Reference to standard (M)

- Apply the §3.1 template to `Stream` and `Cell` in this order:
  `map`, `filter`, `merge`/`or_else`, `snapshot`, `hold`, `accum`/
  `collect`, `lift2`, `switch_c`/`switch_s`, `sample`, `once`, `gate`,
  the `split_*` family, `Operational`, sinks, loops, `Router`. Each gets
  a doctest derived from Appendix E where one exists.
- Type-level docs and the crate root.
- Non-API reference pages: primitives table, rules, name map,
  differences from Java, glossary.

Result: docs.rs is complete and every claim is executable.

### Phase 2 — The front door (M)

- README trimmed to a landing page with four doors.
- Tutorials 1 and 2, with `examples/counter.rs` and
  `examples/form.rs`.
- Explanation: *Streams and cells*, *Transactions and simultaneity*
  (the two pages tutorials 1–2 link to).
- How-tos: *Feed events in from a thread*, *Test FRP logic*,
  *Capture a Cell in a closure correctly*, *Migrate from 2.x*.

Result: a newcomer gets from zero to a working program, and the
existing user base can upgrade.

### Phase 3 — Depth (L)

- Tutorials 3, 4 and 6.
- Remaining how-tos (loops/lazy, Router, coalescing, one big
  transaction, blocking I/O, timers, wrapping callbacks, leaks,
  helpers, performance).
- Explanation: *Why Sodium*, *What a closure may do*, *Memory
  management*, *Operational primitives*, *Designing a Sodium program*,
  *Choosing types*, *Threading and performance*.

### Phase 4 — Flagship and positioning (L)

- Tutorial 5 (the pump) as a real example crate with tests.
- Explanation: *Sodium and Rx*, *Sodium and the Rust ecosystem*.
- Reference: *Which combinator do I want?*
- A docs review pass by someone who has not seen the crate, following
  tutorial 1 cold.

### Ongoing

- A PR that changes a public signature must update its rustdoc, its
  doctest, and the primitives table; CI enforces the first two.
- Each release: CHANGELOG entry, `cargo doc` check, Pages redeploy.
- Apply Diátaxis's own advice for maintenance: pick one page, ask
  which quadrant it is in and whether it is doing only that job, fix
  one thing, publish.

---

## 8. Open questions

None of these block Phase 0 or 1. Defaults are stated; overriding any
of them is a small change to the plan.

1. **Crate name and version** (§1.3). Default: `sodium`, next release
   breaking, migration guide covers `sodium-rust` 2.x.
2. **Pages location.** Default: GitHub Pages from this repository,
   `docs/book/`.
3. **Flagship example.** Default: petrol pump as a terminal simulator.
   Alternative: a Rust-native server or TUI.
4. **Public test helper** (§5.4). Default: `#[doc(hidden)] pub mod
   testing` so doctests and users share it.
5. **Diagram rendering.** Default: text notation everywhere, SVG in
   the book generated from the same text.
6. **Book references.** Default: link by chapter to the Manning book;
   no included text; the private mdbook is a research aid only.
