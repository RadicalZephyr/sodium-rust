# Sodium FRP

A Functional Reactive Programming (FRP) library for Rust. Express your
application logic as a [reactive] directed graph of [functional]
transformations to your data. Sodium is great for creating a
[Boundaries]-style, [functional-core/imperative shell architecture][architecture].

[compositional]: doc/guides/compositional.md
[functional]: doc/guides/functional.md
[Boundaries]: https://www.destroyallsoftware.com/talks/boundaries
[architecture]: doc/guides/architecture.md

## Getting Started

Avaliable on crates.io: https://crates.io/crates/sodium-rust

### Examples

See tests under src/tests for example usage. Sodium objects within
lambda expressions are traced via lambda1, lambda2, etc. just like the
TypeScript version does.

## Pitfalls

### No Global State

You must create a SodiumCtx for your application and keep passing it
around in order to create sodium objects.

### Node Count

From the benchmarking we've done it seems like the performance of a
Sodium graph is heavily bounded by the number of nodes
