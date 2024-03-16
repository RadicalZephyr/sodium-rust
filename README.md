# Sodium FRP

A Functional Reactive Programming (FRP) library for Rust. Express your
application logic as a [reactive] directed graph of [functional]
transformations to your data. Sodium is great for creating a
[Boundaries]-style, [functional-core/imperative shell architecture][architecture].

[compositional]: doc/guides/compositional.md
[functional]: doc/guides/functional.md
[Boundaries]: https://www.destroyallsoftware.com/talks/boundaries
[architecture]: doc/guides/architecture.md

[![Test sodium-rust](https://github.com/SodiumFRP/sodium-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/SodiumFRP/sodium-rust/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/sodium-rust.svg)](https://crates.io/crates/sodium-rust)

## Getting Started

Available on crates.io: https://crates.io/crates/sodium-rust

Add it to your program with

```bash
cargo add sodium-rust
```

## Usage

Now you can write a Hello World program in Sodium:

```rust
use sodium_rust as na;

fn main() {
    let ctx = na::SodiumCtx::new();

    // Start an explicit transaction for creating the whole Sodium
    // system at once.
    let tx = na::Transaction::new(&ctx);

    // Create stream sinks so you can provide input to your Sodium program.
    let hello_sink = ctx.new_stream_sink();

    // Get the Stream connected to your sink to react to input events.
    let hello_s = hello_sink.stream();

    // Constant data can be stored in a `Cell`.
    let name_c = ctx.new_cell(String::from("World"));


    // Combine input events and program state to produce new output.
    let hello_name_s = hello_s.snapshot(&name_c, |greeting: &String, name: &String| {
        format!("{greeting} {name}!")
    });

    // End the creation transaction by dropping it.
    drop(tx);

    // Register listeners to allow your Sodium program to produce output.
    let mut listeners = Vec::new();
    listeners.push(hello_name_s.listen(|message: &String| println!("{message}")));

    // Send an "input" event because this is hello world.
    hello_sink.send(String::from("Hello"));

    // Clean up all of our listeners now that we no longer need them
    // so the whole system can be garbage collected.
    for l in listeners {
        l.unlisten();
    }
}
```

If this looks like a lot of overhead, you're right. Keep in mind, like
most frameworks Sodium is made for managing the complexity of a large
event-driven programs, which Hello World very much is not.

### Examples

See other example programs in `examples`. Specific usage examples for
all API functions can be found in `src/tests`.


## Implementation Details

Sodium objects within lambda expressions are traced via lambda1,
lambda2, etc. just like the TypeScript version does.

### Pitfalls

#### No Global State

You must create a SodiumCtx for your application and keep passing it
around in order to create sodium objects.

#### Node Count

From the benchmarking we've done it seems like the performance of a
Sodium program is directly correlated to the number of nodes in the
graph. If your program is suffering from performance issues try to
reduce the number of different nodes by using some of the built in
helpers. If Sodium is still too slow for you, [file an issue][Github
Issues] and tell us about what you're trying to do with Sodium so we
can create a new benchmark that captures your use-case.

## Contributing

Please note that Sodium Rust is released with a [Contributor Code of
Conduct][covenant]. By participating in this project you agree to
abide by its terms.

If you believe someone is violating the code of conduct, we ask that
you report it by emailing us. For more details please see our [Reporting
Guide][reporting].

To get started, please take a look at our [Contribution
Guidelines][contributing].  Next, probably check out our
[project][project] board, and look at the issues in the To-Do
column. From there, the standard "[fork], [branch], [code], [pull
request]" workflow works well.

Another great way to contribute is code review of any open PRs, trying
to reproduce open issues, and giving feedback on how you use Sodium
and how it could be more helpful.

[covenant]: https://github.com/SodiumFRP/sodium-rust/blob/master/CODE_OF_CONDUCT.md
[contributing]: https://github.com/SodiumFRP/sodium-rust/blob/master/CONTRIBUTING.md
[reporting]: https://github.com/SodiumFRP/sodium-rust/blob/master/doc/conduct/reporting-guide.md
[fork]: https://help.github.com/articles/fork-a-repo/
[branch]: https://help.github.com/articles/creating-and-deleting-branches-within-your-repository/
[code]: http://stackoverflow.com/questions/tagged/rust
[pull request]: https://help.github.com/articles/creating-a-pull-request/
[Github Issues]: https://github.com/SodiumFRP/sodium-rust/issues

## License

Sodium Rust is licensed under the [BSD 3-Clause License][bsd-3].

[bsd-3]: https://choosealicense.com/licenses/bsd-3-clause/
