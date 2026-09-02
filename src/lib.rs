//! Sodium is a library for doing Functional Reactive Programming
//! (FRP) in Rust.
//!
//! Every Sodium object belongs to a [`SodiumCtx`], and the objects in one
//! FRP graph must all come from the same one. You do not have to name it:
//! each thread has an ambient context, created on first use, and the
//! constructors here use it.
//!
//! ```
//! use sodium::{transaction, StreamSink};
//!
//! let clicks: StreamSink<()> = StreamSink::new();
//! let count = clicks.stream().accum(0, |_click: &(), n: &i32| n + 1);
//! let listener = count.listen(|n: &i32| println!("clicked {n} times"));
//!
//! transaction(|| clicks.send(()));
//! listener.unlisten();
//! ```
//!
//! For an explicit context -- independent graphs in one thread, or one
//! graph built from several threads -- every constructor has an `*_in`
//! sibling that takes one, and [`SodiumCtx::enter`] makes a context
//! ambient for a scope. See [`SodiumCtx::current`] for the details, and
//! why the default is per thread rather than per process.

mod ambient;
mod cell;
mod cell_loop;
mod cell_sink;
mod impl_;
mod listener;
mod operational;
mod router;
mod sodium_ctx;
mod stream;
mod stream_loop;
mod stream_sink;
mod transaction;

pub use self::ambient::post;
pub use self::ambient::transaction;
pub use self::ambient::CtxGuard;
pub use self::cell::Cell;
pub use self::cell_loop::CellLoop;
pub use self::cell_sink::CellSink;
pub use self::impl_::dep::Dep;
pub use self::impl_::enum_::Enum2;
pub use self::impl_::enum_::Enum3;
#[doc(hidden)]
pub use self::impl_::lambda::lambda1;
#[doc(hidden)]
pub use self::impl_::lambda::lambda2;
#[doc(hidden)]
pub use self::impl_::lambda::lambda3;
#[doc(hidden)]
pub use self::impl_::lambda::lambda4;
#[doc(hidden)]
pub use self::impl_::lambda::lambda5;
#[doc(hidden)]
pub use self::impl_::lambda::lambda6;
#[doc(hidden)]
pub use self::impl_::lambda::IsLambda1;
#[doc(hidden)]
pub use self::impl_::lambda::IsLambda2;
#[doc(hidden)]
pub use self::impl_::lambda::IsLambda3;
#[doc(hidden)]
pub use self::impl_::lambda::IsLambda4;
#[doc(hidden)]
pub use self::impl_::lambda::IsLambda5;
#[doc(hidden)]
pub use self::impl_::lambda::IsLambda6;
#[doc(hidden)]
pub use self::impl_::lambda::Lambda;
pub use self::impl_::lazy::Lazy;
#[doc(hidden)]
pub use self::impl_::node::Node;
pub use self::listener::Listener;
pub use self::operational::Operational;
pub use self::router::Router;
pub use self::sodium_ctx::SodiumCtx;
pub use self::stream::Stream;
pub use self::stream_loop::StreamLoop;
pub use self::stream_sink::StreamSink;
pub use self::transaction::Transaction;
#[cfg(test)]
mod tests;
