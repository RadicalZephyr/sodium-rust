# Getting Started with Sodium (in Rust)

So you want to give Sodium and functional reactive programming a
try. To give you an idea of what developing a Sodium app looks like,
let's build a simple text-based Tic Tac Toe game.

Start off by creating a new project and adding Sodium as a dependency.

```shell
cargo new sodium-tic-tac-toe
cd sodium-tic-tac-toe
cargo add sodium-rust
```



# Notes on Structure

It's extremely hard to read Sodium code because of the amount of boilerplate.
