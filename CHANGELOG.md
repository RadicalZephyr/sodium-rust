# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- This CHANGELOG file.
- Dependency on [parking-lot].
- New `filter_map` combinator.

[parking-lot]: https://crates.io/crates/parking-lot

### Changed

- **Breaking:** combinators that take a function are now bounded on `FnMut`/`Fn`
  directly instead of on the `IsLambda1`..`IsLambda6` traits. Closure arguments
  now infer, so call sites no longer need a type annotation on every closure
  parameter (`stream.map(|a| *a + 1)` instead of `stream.map(|a: &_| *a + 1)`).

  The previous bounds defeated rustc's closure signature deduction, which only
  looks through the `Fn` family and not through user-defined traits, so a bare
  closure failed with `error[E0282]: type annotations needed` even though the
  element type was fully determined by the receiver.
- **Breaking:** passing a `Lambda` built with `lambda1`..`lambda6` to a
  combinator no longer compiles. Use the new `*_with_deps` variant instead:

  ```rust
  // before
  stream.map(lambda1(move |a: &A| *a + c.sample(), vec![c.to_dep()]))
  // after
  stream.map_with_deps(move |a| *a + c.sample(), vec![c.to_dep()])
  ```

- The `Clone` bound on event values is now per method rather than blanket.
  A `Stream<A>` needs only `A: Send + 'static`. `Clone` is required by the
  combinators that forward one value to more than one place (`filter`,
  `merge` and `or_else`, `hold`, `gate`, `once`, `Operational::defer`,
  `StreamLoop`, `Router`) and by the `Option`/`Result` projections
  (`filter_option`, `split_opt`, `split_res`). The sinks, `map`,
  `filter_map`, `snapshot*`, `split_enum*`, `listen*`, and the inputs of
  `collect` and `accum` accept values that are not `Clone`, and the values
  produced by `map`, `filter_map` and `snapshot*` need not be `Clone` either.
  `Cell` values still need `Clone`: a cell is read from many places, and each
  reader gets its own copy.
- Closures passed to combinators and listeners need only be `Send`, not
  `Send + Sync`. The `Sync` requirement was an artefact of storing node
  closures in an `RwLock`; they now sit behind a `Mutex`, which needs its
  contents to be `Send` only. A listener can now capture, for instance, a
  `Box<dyn FnMut(..) + Send>`.
- The value given to `Stream::map_to`, the items produced by `Stream::split`
  and the keys of a `Router` no longer need to be `Sync`.
- `Stream::split` no longer clones each item; items are moved into the
  post-transaction send instead.
- `IsLambda1`..`IsLambda6`, `Lambda` and `lambda1`..`lambda6` remain exported but
  are now `#[doc(hidden)]`; they are the mechanism behind `*_with_deps` rather
  than part of the public surface.
- `Dep`, `Cell::to_dep` and `Stream::to_dep` are no longer `#[doc(hidden)]`,
  since they appear in the public `*_with_deps` signatures.
- `Cell::listen_weak` now honours declared dependencies, matching
  `Stream::listen_weak` and `Cell::listen`.
- Various small performance improvements.

### Added

- `*_with_deps` variants of every function-taking combinator, for closures that
  capture FRP nodes Sodium cannot otherwise see: `Stream::map_with_deps`,
  `filter_with_deps`, `filter_map_with_deps`, `merge_with_deps`,
  `snapshot_with_deps`, `snapshot3_with_deps`..`snapshot6_with_deps`,
  `collect_with_deps`, `collect_lazy_with_deps`, `accum_with_deps`,
  `accum_lazy_with_deps`, `listen_with_deps`, `listen_weak_with_deps`, and
  `Cell::map_with_deps`, `lift2_with_deps`..`lift6_with_deps`,
  `listen_with_deps`, `listen_weak_with_deps`.

## [2.1.2] - 2022-11-27

### Added

- Added basic documentation for every public API [#62]

### Fixed

- Removed several API items related to internal dependency tracking
  from the documentation.
- Fixed garbage collector bug that was causing the time taken by every
  transaction to increase exponentially with the number of nodes in
  the graph. [#62]

[#62]: https://github.com/SodiumFRP/sodium-rust/pull/62


## [2.1.1] - 2020-10-27

### Fixed

- Fixed `Listener` memory management when using `Operational::defer`.


## [2.1.0] - 2020-07-28

### Added

- Added methods for sampling up to 6 `Cell`s from one
  `Stream::snapshot`. [#43]
- Add a Drop-based `Transaction` scope. [#53]
- Added `Stream::split` method for flattening a `Stream<impl
  IntoIterator<Item = T>` into a `Stream<T>`. [#44]
- Added missing public `SodiumCtx::post` method. [#42]
- Added `Router`, a more performant way of splitting one `Stream` into
  many `Stream`s.

[#43]: https://github.com/SodiumFRP/sodium-rust/issues/43
[#53]: https://github.com/SodiumFRP/sodium-rust/issues/53
[#44]: https://github.com/SodiumFRP/sodium-rust/issues/44
[#42]: https://github.com/SodiumFRP/sodium-rust/issues/42
[#57]: https://github.com/SodiumFRP/sodium-rust/issues/57


## [2.0.2] - 2020-07-25

### Fixed

- Correctly use `Lazy` values in `CellLoop::loop`. [#45]

[#45]: https://github.com/SodiumFRP/sodium-rust/issues/45


## [2.0.1] - 2020-04-24

### Added

- Added missing `Clone` impl for `SodiumCtx`.

### Fixed

- Track dependencies correctly in `Stream::listen`.


## [2.0.0] - 2020-04-21

Version 2 rewrite.


## [1.0.1] - 2018-11-24

### Added

- Added `Transaction::is_active` method. [#26]

### Changed

- Disallow calling `send` in Sodium callbacks, use `post` to make it
  happen in the next transaction instead. [#32]

### Fixed

- Fixed `SampleLazy` bug. [#23]
- Fixed `IsCell::listen_weak` now correctly uses `Cell::listen_weak`. [#30]
- Keep `SodiumCtx` alive in memory when any Sodium objects are alive. [#29]

[#23]: https://github.com/SodiumFRP/sodium-rust/issues/23
[#26]: https://github.com/SodiumFRP/sodium-rust/issues/26
[#29]: https://github.com/SodiumFRP/sodium-rust/issues/29
[#30]: https://github.com/SodiumFRP/sodium-rust/issues/30
[#32]: https://github.com/SodiumFRP/sodium-rust/issues/32
[#34]: https://github.com/SodiumFRP/sodium-rust/issues/34


## [1.0.0] - 2018-11-17

Initial release.

[Unreleased]: https://github.com/SodiumFRP/sodium-rust/compare/v2.1.2...HEAD
[2.1.2]: https://github.com/SodiumFRP/sodium-rust/compare/2.1.1...2.1.2
[2.1.1]: https://github.com/SodiumFRP/sodium-rust/compare/2.1.0...2.1.1
[2.1.0]: https://github.com/SodiumFRP/sodium-rust/compare/2.0.2...2.1.0
[2.0.2]: https://github.com/SodiumFRP/sodium-rust/compare/2.0.1...2.0.2
[2.0.1]: https://github.com/SodiumFRP/sodium-rust/compare/2.0.0...2.0.1
[2.0.0]: https://github.com/SodiumFRP/sodium-rust/compare/1.0.1...2.0.0
[1.0.1]: https://github.com/SodiumFRP/sodium-rust/compare/1.0.0...1.0.1
[1.0.0]: https://github.com/SodiumFRP/sodium-rust/releases/tag/v1.0.0
