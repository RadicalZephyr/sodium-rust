# Sodium

[![CI](https://github.com/RadicalZephyr/sodium-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/RadicalZephyr/sodium-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/sodium.svg)](https://crates.io/crates/sodium)
[![docs.rs](https://docs.rs/sodium/badge.svg)](https://docs.rs/sodium)
[![license](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](LICENSE)

A Functional Reactive Programming (FRP) library for Rust, part of the
[Sodium](https://github.com/SodiumFRP) family of FRP libraries.

FRP lets you express the parts of a program that react to change — input,
timers, network events — as a graph of values instead of a pile of callbacks
and mutable state. Sodium gives you two primitives:

- **`Stream<A>`** — discrete events that occur at points in time.
- **`Cell<A>`** — a value that is always present and varies over time.

You combine them with pure functions (`map`, `filter`, `merge`, `snapshot`,
`hold`, `accum`, `switch_s`, ...). Sodium tracks the dependencies between
them, so an update propagates in dependency order, exactly once per
transaction, with no intermediate glitches.

## Installation

```toml
[dependencies]
sodium = "0.1"
```

## Example

```rust
use sodium::StreamSink;

fn main() {
    // A StreamSink is how values from the outside world enter the graph.
    let clicks: StreamSink<()> = StreamSink::new();

    // Streams are discrete events. Cells are values that persist over time.
    let count = clicks.stream().accum(0, |_click: &(), n: &i32| n + 1);
    let label = count.map(|n: &i32| format!("clicked {n} times"));

    let listener = label.listen(|text: &String| println!("{}", text));

    clicks.send(());
    clicks.send(());
    clicks.send(());

    listener.unlisten();
}
```

`Cell::listen` fires immediately with the cell's current value, so this prints
`clicked 0 times` before any event is sent, then once per `send`.

## Contexts

Every Sodium object belongs to a `SodiumCtx`, and the objects in one FRP
graph must all come from the same one. You do not have to name it: each
thread has an ambient context, created the first time something asks for
it, and the constructors above use it. `sodium::transaction` and
`sodium::post` run against the same ambient context.

The context is ambient per thread rather than per process because a
`SodiumCtx` holds its transaction depth in a single counter. Two threads
running transactions on one context interleave their increments, a
transaction gets closed by the wrong thread, and updates are dropped. One
context per thread means threads never share transaction state by
accident.

When you do want an explicit context -- to run independent graphs in one
thread, or to build one graph from several threads -- every constructor
has an `*_in` sibling that takes one, and `SodiumCtx::enter` makes a
context ambient for a scope:

```rust
use sodium::{SodiumCtx, StreamSink};

let ctx = SodiumCtx::new();
let sink: StreamSink<i32> = ctx.new_stream_sink();

std::thread::scope(|scope| {
    scope.spawn(|| {
        let _guard = ctx.enter();
        // Built in `ctx`, not in this thread's own ambient context, so it
        // can be wired to `sink`'s graph.
        let doubled = sink.stream().map(|a: &i32| a * 2);
        let _keep = doubled.listen(|n: &i32| println!("{n}"));
    });
});
```

Entering a context decides where new objects are built; it does not make
concurrent transactions on one context safe. Drive a given context from
one thread at a time.

Combining objects from two different contexts panics rather than
silently building a broken graph, so a missing `enter` shows up as an
error that names the mistake.

## Documentation

- [API documentation on docs.rs](https://docs.rs/sodium).
- `src/tests.rs` is the most complete set of worked examples in the
  repository — it exercises nearly every combinator.
- `docs/internals/insights.md` covers implementation notes.

## Pitfalls

### Closures That Capture Sodium Objects

Sodium builds its dependency graph from the shape of the FRP network, and it
cannot see inside a closure. If a closure captures a `Cell` or `Stream` and
reaches it at call time -- by calling `sample()` on it, or by returning it --
that node is a real dependency, and Sodium has to be told about it. Every
combinator has a `*_with_deps` sibling for this:

```rust
let bias = bias_cell.clone();
let biased = stream.map_with_deps(
    move |a| *a + bias.sample(),
    vec![bias_cell.to_dep()],
);
```

The `Dep`s must mirror what the closure actually captures. Declaring a node the
closure does not hold a reference to corrupts the collector's bookkeeping.

Prefer expressing the dependency in the network itself where you can --
`snapshot`, `lift2` and friends do this bookkeeping for you -- and reach for
`*_with_deps` only when the closure genuinely has to reach outside the graph.

## Repository layout

This is a Cargo workspace.

| Path | Contents |
| --- | --- |
| `src/` | the `sodium` library |
| `benches/` | Criterion benchmarks (`cargo bench`) |
| `coz-driver/` | a causal-profiling workload — see [`coz-driver/README.md`](coz-driver/README.md) |
| `tools/` | developer scripts, including the Coz installer |
| `docs/` | implementation notes |

## Contributing

Build and test everything in the workspace:

```shell
cargo test --workspace
```

CI runs these three commands, so running them before you push is the fastest
way to know a change will pass:

```shell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Note that `--workspace` is load-bearing. The workspace root is itself a
package, which makes it the sole default member, so a bare `cargo test` will
not compile `coz-driver`.

Tests run against stable, beta and nightly; nightly is allowed to fail.

User-visible changes should get an entry in [`CHANGELOG.md`](CHANGELOG.md),
which follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## License

BSD 3-Clause. See [`LICENSE`](LICENSE).
