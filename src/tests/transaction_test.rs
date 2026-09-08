//! Ports of the transaction and `post` tests from the .NET binding's
//! `Sodium.Frp.Tests/TransactionTests.cs`.
//!
//! `SodiumCtx::post` had no coverage at all before these.

use crate::SodiumCtx;

use std::sync::{Arc, Mutex};

// FIXME: `SodiumCtx::post` documents that it runs the closure
// "immediately if there is no current transaction", and that is what the
// Java and .NET bindings do. It does not: `post` unconditionally pushes
// onto the queue, which is only drained by `end_of_transaction`, so a
// closure posted outside a transaction sits there until some later
// transaction happens to close.
#[ignore = "post outside a transaction does not run until the next transaction closes"]
#[test]
fn post() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    let cell = sodium_ctx.transaction(|| {
        let s = sodium_ctx.new_stream_sink::<i32>();
        s.send(2);
        s.stream().hold(1)
    });
    let value = Arc::new(Mutex::new(0));
    {
        let value = value.clone();
        sodium_ctx.post(move || **value.lock().as_mut().unwrap() = cell.sample());
    }
    assert_eq!(2, **value.lock().as_ref().unwrap());
}

// A `post` registered from inside a `post` still runs.
#[test]
fn nested_post() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    let cell = sodium_ctx.transaction(|| {
        let s = sodium_ctx.new_stream_sink::<i32>();
        s.send(2);
        {
            let s = s.clone();
            let sodium_ctx = sodium_ctx.clone();
            sodium_ctx.clone().post(move || {
                s.send(3);
                let s = s.clone();
                sodium_ctx.post(move || s.send(5));
            });
        }
        {
            let s = s.clone();
            sodium_ctx.post(move || s.send(4));
        }
        s.stream().hold(1)
    });
    assert_eq!(5, cell.sample());
}

// FIXME: the closure does run at the right time, but it observes the
// cell's *old* value, so this asserts 2 and gets 1.
//
// `Stream::hold` applies its pending value through `SodiumCtx::post` too
// (impl_::cell, NodeName::CELL_HOLD), and that post is only enqueued
// during the node-update phase of `end_of_transaction` -- after the
// transaction body has run. A user `post` registered in the body is
// therefore always ahead of it in the same FIFO queue and runs first, on
// the stale value. Java and .NET apply cell updates in an earlier phase,
// so a posted closure sees the transaction's results, which is the whole
// point of posting.
#[ignore = "a posted closure runs before cell updates are applied"]
#[test]
fn post_in_transaction() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    let value = Arc::new(Mutex::new(0));
    sodium_ctx.transaction(|| {
        let s = sodium_ctx.new_stream_sink::<i32>();
        s.send(2);
        let c = s.stream().hold(1);
        {
            let value = value.clone();
            sodium_ctx.post(move || **value.lock().as_mut().unwrap() = c.sample());
        }
        assert_eq!(0, **value.lock().as_ref().unwrap());
    });
    assert_eq!(2, **value.lock().as_ref().unwrap());
}

// The nesting itself works -- the closure waits for the outermost
// transaction -- but it hits the same stale-value problem as
// post_in_transaction.
#[ignore = "a posted closure runs before cell updates are applied"]
#[test]
fn post_in_nested_transaction() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    let value = Arc::new(Mutex::new(0));
    sodium_ctx.transaction(|| {
        let s = sodium_ctx.new_stream_sink::<i32>();
        s.send(2);
        sodium_ctx.transaction(|| {
            let c = s.stream().hold(1);
            let value = value.clone();
            sodium_ctx.post(move || **value.lock().as_mut().unwrap() = c.sample());
        });
        assert_eq!(0, **value.lock().as_ref().unwrap());
    });
    assert_eq!(2, **value.lock().as_ref().unwrap());
}
