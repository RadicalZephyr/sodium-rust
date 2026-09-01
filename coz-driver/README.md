# coz-driver

A standalone workload for profiling the `sodium` crate with
[Coz](https://github.com/plasma-umass/coz), a *causal* profiler.

A conventional profiler tells you where time is spent. Coz tells you what
would happen if a given line were faster: it runs virtual speedup experiments
and reports the predicted effect on throughput. The distinction matters for an
FRP graph, where the line that burns the most time and the line worth
optimising are frequently not the same one.

The driver builds a prime-sieve graph out of `StreamSink`, `StreamLoop` and
`Cell::switch_s`, then pushes 60,000 values through it, bracketing each send
with a Coz progress point:

```rust
coz::begin!("stream_send");
ss_input.send(x);
coz::end!("stream_send");
```

Coz measures throughput against that progress point, so the experiments answer
"what makes `send` faster?" rather than "what is slow?".

## Installing Coz

From the repository root:

```shell
tools/install-coz.sh
```

That downloads the pinned Coz release, verifies it against a checksum pinned
per version and architecture (amd64 and arm64), and installs it with `dpkg`.
It needs `sudo` if you are not root, and it is idempotent, so it is safe to
re-run. Expect roughly 171 MB installed — the release ships an unstripped
`libcoz.so`.

Coz is not needed to build or test this repository, which is why it is
installed on demand rather than as part of any setup.

Other platforms, or building from source, are covered by the
[Coz README](https://github.com/plasma-umass/coz#readme).

## Running

```shell
cargo build --release -p coz-driver
coz run --- ./target/release/coz-driver
```

This writes `profile.coz` into the working directory; use
`coz run --output=/path/to/profile.coz` to put it elsewhere. The workload
takes a little under two minutes, and Coz adds only a few percent on top of
that. Then:

```shell
coz plot -i profile.coz
```

## Debug info is required

Coz resolves samples to source lines through DWARF line tables, so the driver
has to be built with debug info. `[profile.release] debug = 1` in the
**workspace root** `Cargo.toml` provides it.

Do not move that setting into `coz-driver/Cargo.toml`. Cargo ignores profiles
declared by non-root workspace members, so it would be silently dropped and
every profile would come back without line numbers.

## Reading the output

Expect most samples to land in the Rust standard library rather than in
`sodium` itself — allocation paths dominate, which is a finding about the
graph's allocation behaviour rather than noise to filter out. The remainder
resolves to `src/impl_/`, and those are the lines the speedup experiments are
worth reading closely.

## Requirements

Linux, with `perf_event_open` available. Coz samples on software perf events,
so no hardware PMU is needed and it works inside VMs and containers that do
not expose one.

The Coz README suggests relaxing `kernel.perf_event_paranoid`. That has not
been necessary for this workload — Coz sets `exclude_kernel`, which the
default `paranoid=2` permits — but if a run produces a profile with no
samples in it, that is the first thing to check.
