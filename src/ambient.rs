//! The ambient [`SodiumCtx`] for the current thread.
//!
//! Every Sodium object belongs to a [`SodiumCtx`], and objects from two
//! different contexts cannot be combined. Passing a context explicitly
//! through an application is precise but noisy, and it is the one place
//! where this port's API diverges sharply from the other Sodium ports,
//! which construct FRP objects with no visible context at all.
//!
//! This module supplies the missing default. Each thread has its own
//! ambient context, created on first use, and constructors like
//! [`StreamSink::new`][crate::StreamSink::new] use it. An explicit context
//! is still available everywhere via the `*_in` constructors, and can be
//! made ambient for a scope with [`SodiumCtx::enter`].
//!
//! # Why per-thread, not global
//!
//! A `SodiumCtx` keeps its transaction depth in a single counter. Two
//! threads running transactions on one context interleave their
//! increments, so a transaction can be ended by the wrong thread and
//! updates are lost. A context is therefore a single-threaded object in
//! practice, and the ambient default follows that: one context per
//! thread, so two threads never share transaction state by accident.
//!
//! The cost is that FRP objects built on two threads belong to two
//! different graphs. Wiring them together panics rather than silently
//! misbehaving -- see [`SodiumCtx::enter`] for how to build one graph
//! from several threads.

use std::cell::RefCell;

use crate::SodiumCtx;

thread_local! {
    /// The innermost entered context, then the thread's lazily created
    /// default. Empty until either happens.
    static AMBIENT: RefCell<Vec<SodiumCtx>> = const { RefCell::new(Vec::new()) };
}

/// A guard returned by [`SodiumCtx::enter`].
///
/// The entered context is ambient on this thread until the guard is
/// dropped. Guards must be dropped in reverse order of creation, which
/// is what happens naturally when they are held in nested scopes.
#[must_use = "the context is only ambient while the guard is alive"]
pub struct CtxGuard {
    depth: usize,
    // Not `Send`: the ambient stack belongs to the thread that pushed it.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Drop for CtxGuard {
    fn drop(&mut self) {
        AMBIENT.with(|stack| {
            let mut stack = stack.borrow_mut();
            debug_assert_eq!(
                stack.len(),
                self.depth,
                "SodiumCtx guards dropped out of order"
            );
            stack.pop();
        });
    }
}

impl SodiumCtx {
    /// The ambient context for the current thread.
    ///
    /// This is the innermost context [entered][SodiumCtx::enter] on this
    /// thread, or, if none has been entered, a context created on first
    /// use and reused for the rest of the thread's life.
    ///
    /// ```
    /// use sodium::SodiumCtx;
    ///
    /// let a = SodiumCtx::current();
    /// let b = SodiumCtx::current();
    /// assert!(a.ptr_eq(&b));
    /// ```
    pub fn current() -> SodiumCtx {
        AMBIENT.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(ctx) = stack.last() {
                return ctx.clone();
            }
            let ctx = SodiumCtx::new();
            stack.push(ctx.clone());
            ctx
        })
    }

    /// Make this context ambient on the current thread until the returned
    /// guard is dropped.
    ///
    /// Use this to give a thread a context other than its own default --
    /// most importantly, to build one FRP graph from more than one
    /// thread:
    ///
    /// ```
    /// use sodium::{SodiumCtx, StreamSink};
    ///
    /// let ctx = SodiumCtx::new();
    /// let sink: StreamSink<i32> = ctx.new_stream_sink();
    ///
    /// std::thread::scope(|scope| {
    ///     scope.spawn(|| {
    ///         let _guard = ctx.enter();
    ///         // Built in `ctx`, not in this thread's own default, so it
    ///         // can be wired to `sink`'s graph.
    ///         let doubled = sink.stream().map(|a: &i32| a * 2);
    ///         let _keep = doubled.listen(|_: &i32| {});
    ///     });
    /// });
    /// ```
    ///
    /// Entering a context does not make it safe for two threads to run
    /// transactions on it at the same time; it only decides which context
    /// new objects are built in.
    pub fn enter(&self) -> CtxGuard {
        AMBIENT.with(|stack| {
            let mut stack = stack.borrow_mut();
            stack.push(self.clone());
            CtxGuard {
                depth: stack.len(),
                _not_send: std::marker::PhantomData,
            }
        })
    }

    /// Are these two handles clones of the same underlying context?
    pub fn ptr_eq(&self, other: &SodiumCtx) -> bool {
        self.impl_.ptr_eq(&other.impl_)
    }
}

/// Run `k` inside a single transaction on the ambient context.
///
/// The context-free counterpart of [`SodiumCtx::transaction`].
///
/// ```
/// use sodium::{StreamSink, transaction};
///
/// let a: StreamSink<i32> = StreamSink::new();
/// let b: StreamSink<i32> = StreamSink::new();
/// transaction(|| {
///     a.send(1);
///     b.send(2);
/// });
/// ```
pub fn transaction<R, K: FnOnce() -> R>(k: K) -> R {
    SodiumCtx::current().transaction(k)
}

/// Run `k` after the ambient context's current transaction closes, or
/// immediately if there is no transaction open.
///
/// The context-free counterpart of [`SodiumCtx::post`].
pub fn post<K: FnMut() + Send + 'static>(k: K) {
    SodiumCtx::current().post(k)
}
