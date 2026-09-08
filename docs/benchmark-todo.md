# Benchmark suite: TODO

Tracks [the implementation plan](benchmark-plan.md). Phases are ordered; items
within a phase mostly are not. Numbers to assert against come from [the
`research` crate](../research).

## Phase 0 — scenario crate

- [ ] Add `bench-support` to the workspace `members` list
- [ ] `bench-support/Cargo.toml`, depending on `sodium-rust` by path
- [ ] `Rig`: context, sinks, listeners, an `Arc<AtomicU64>` observation sink
- [ ] `chain.rs`: linear chain, binary tree, fan-in tree builders, each taking a size
- [ ] Confirm `cargo check -p sodium-rust` still passes on the MSRV (1.71)
- [ ] Confirm `cargo clippy --workspace --all-targets -- -D warnings` is clean

## Phase 1 — cost invariants

- [ ] `tests/graph_cost.rs` with a counting `#[global_allocator]` scoped to that binary
- [ ] Table-driven per-event allocation assertions for the 13 measured shapes
- [ ] Extend the table to every combinator the ledger will cover
- [ ] `node_count()` returns to baseline after a subgraph is released — expect **fail**, mark `#[ignore]`
- [ ] Per-send allocation count returns to baseline after release — expect **fail**, mark `#[ignore]`
- [ ] Write both as a slope (cost after 800 cycles == cost after 100), not a threshold
- [ ] Note in the test file that `once` reads below baseline in steady state, and why
- [ ] File an issue for `drop(Listener)` retaining nodes that `collect_cycles()` will not reclaim
- [ ] File an issue for send cost not recovering after `unlisten()` (27 → 327 allocs, sticky)
- [ ] Link both issues from the `#[ignore]` attributes
- [ ] Check the assertions hold on macOS and Windows, not just Linux

## Phase 2 — combinator ledger

- [ ] `iai-callgrind = "0.16"` dev-dependency; `[[bench]] name = "combinators", harness = false`
- [ ] Pin `iai-callgrind-runner` to the same patch version; document the skew failure mode
- [ ] Ledger arms for the 12 stream + 5 cell combinators already measured
- [ ] Arms for `hold`, `once`, `collect`, `filter_option`, `split_opt`, `split_res`, `split_enum2`, `split_enum3`
- [ ] Arms for `snapshot1`, `snapshot3`..`snapshot6`, `lift4`..`lift6`
- [ ] Arms for `Cell::value`, `Operational::updates`, `Operational::value`
- [ ] Arms for `Router`, `StreamLoop` feedback, `CellLoop` feedback
- [ ] Arm for `new_stream_sink_with_coalescer`
- [ ] Arms for the `_lazy` variants: `hold_lazy`, `collect_lazy`, `accum_lazy`
- [ ] Arms for `Cell::sample`, `sample_lazy`, `Stream::split`, `listen_weak`
- [ ] Arms for `SodiumCtx::post` and an explicit `new_transaction` scope
- [ ] One representative `*_with_deps` pair, to settle whether deps cost anything at fire time
- [ ] A comment in the bench file for every public operation with no arm, saying why
- [ ] CI job: Linux, install valgrind, install pinned runner, `cargo bench --bench combinators`
- [ ] Decide report-only versus gating (ADR open question)
- [ ] Decide where baselines live (ADR open question)

## Phase 3 — structural scaling

- [ ] `benches/structure.rs`, criterion, `BenchmarkId` throughout, construction hoisted out of `b.iter()`
- [ ] Port `graph-growth.rs` from `node-count-benches`
- [ ] Fix its confound: equal listener counts across the linear and tree arms
- [ ] `depth`, `width`, `fan_in` groups
- [ ] `listeners` group
- [ ] `batch` group (sends per transaction)
- [ ] `switch_upstream` and `switch_downstream` groups
- [ ] `live_subgraphs` group
- [ ] `churn` group — must reproduce the degradation recorded in the ADR
- [ ] Document the `--save-baseline` / `critcmp` workflow in the bench file header

## Phase 4 — book graph shapes

### Wave 1 — structural idioms

- [ ] `fold_lift2`, left-leaning, N in {1,2,4,8,16,32,64,128}
- [ ] `fold_lift2` contrast arm: balanced binary fold, same N
- [ ] `broadcast_demux`, N in {1,2,4,8,16,32}
- [ ] `broadcast_demux` contrast arm: `Router` + `filter_matches`, same N
- [ ] `state_machine_loop`, N nozzles in {1,2,4,8}
- [ ] `self_reading_accum`
- [ ] `self_reading_accum` contrast arm: `Stream::accum` rewrite
- [ ] `wide_feedback_ring`, N in {1,2,4,8,16,32,64,128}

### Wave 2 — dynamic graphs and one composite

- [ ] `population_churn`, N in {10,100,1000} — the scenario the whole tier exists for
- [ ] `gesture_switch_churn`
- [ ] `junction_rebuild`, N in {1,4,16,64,256}, left-leaning and balanced fold arms
- [ ] `eager_recompute_ratio`, ratio in {1:1, 1:10, 1:100, 1:1000}
- [ ] `petrol_pump` at the book's six section sizes: 7, 24, 30, 44, 65, 77 nodes

### Port hazards to check while writing the above

- [ ] Every Java in-closure `sample()` converted to `snapshot3` / `*_with_deps`, not dropped
- [ ] `Vec<Listener>` in the harness in place of Java's `Listener::append`
- [ ] Clock sent as the first event of each frame (no `TimerSystem`, `post` runs after)
- [ ] File an issue for the hidden `Cell::map` inside `switch_s` / `switch_c` (2 nodes per site)
- [ ] `Sync` bound on payloads in any `Stream::split` scenario


- [ ] Move `coz-driver`'s prime sieve into `bench-support` and have the driver call it
- [ ] Assert each scenario's node count in tier 4, so a scenario cannot silently change shape

## Phase 5 — retire the old file

- [ ] Confirm every `benches/sodium.rs` group has a replacement that has actually been run
- [ ] Delete `benches/sodium.rs`, commit message naming where each group went
- [ ] Drop the `#![allow(clippy::incompatible_msrv)]` header if the new targets do not need it

## Deferred

- [ ] Multi-threaded contention benchmarks (needs a stable single-threaded baseline first)
- [ ] Peak RSS under churn
- [ ] Cross-library comparison
