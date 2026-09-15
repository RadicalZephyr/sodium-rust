//! Ports of the cross-language "common tests".
//!
//! These are the denotational semantics tests that the upstream Sodium
//! repository generates for every language binding from
//! `common-tests/SemanticTests.hs`. Each one listens to a stream in a
//! fresh transaction, fires a round of events, then unlistens, so that
//! the exact set of events observed in each transaction is pinned down.

use crate::{Operational, SodiumCtx, Stream};

use std::sync::{Arc, Mutex};

/// Listen to `s` inside a transaction, fire a round of events with
/// `k`, then unlisten and return everything the stream fired.
fn round<A, K>(sodium_ctx: &SodiumCtx, s: &Stream<A>, k: K) -> Vec<A>
where
    A: Clone + Send + 'static,
    K: FnOnce(),
{
    let out = Arc::new(Mutex::new(Vec::new()));
    let l = {
        let out = out.clone();
        sodium_ctx
            .transaction(|| s.listen(move |a: &A| out.lock().as_mut().unwrap().push(a.clone())))
    };
    sodium_ctx.transaction(k);
    l.unlisten();
    let lock = out.lock();
    let out: &Vec<A> = lock.as_ref().unwrap();
    out.clone()
}

#[test]
fn base_send1() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let s = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<&'static str>());
        let out = Arc::new(Mutex::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l = sodium_ctx.transaction(|| {
                s.stream()
                    .listen(move |a: &&'static str| out.lock().as_mut().unwrap().push(*a))
            });
        }
        sodium_ctx.transaction(|| s.send("a"));
        sodium_ctx.transaction(|| s.send("b"));
        l.unlisten();
        {
            let lock = out.lock();
            let out: &Vec<&'static str> = lock.as_ref().unwrap();
            assert_eq!(vec!["a", "b"], *out);
        }
    }
    super::assert_memory_freed(sodium_ctx);
}

#[test]
fn operational_split() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let a = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<Vec<&'static str>>());
        let b = sodium_ctx.transaction(|| a.stream().split());
        assert_eq!(
            vec!["a", "b"],
            round(sodium_ctx, &b, || a.send(vec!["a", "b"]))
        );
    }
    super::assert_memory_freed(sodium_ctx);
}

#[test]
fn operational_defer1() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let a = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<&'static str>());
        let b = sodium_ctx.transaction(|| Operational::defer(&a.stream()));
        assert_eq!(vec!["a"], round(sodium_ctx, &b, || a.send("a")));
        assert_eq!(vec!["b"], round(sodium_ctx, &b, || a.send("b")));
    }
    super::assert_memory_freed(sodium_ctx);
}

#[test]
fn operational_defer2() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let a = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<&'static str>());
        let b = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<&'static str>());
        let c = sodium_ctx.transaction(|| Operational::defer(&a.stream()).or_else(&b.stream()));
        assert_eq!(vec!["a"], round(sodium_ctx, &c, || a.send("a")));
        // `b` is not deferred, so it arrives first even though `a` was
        // sent first.
        assert_eq!(
            vec!["B", "b"],
            round(sodium_ctx, &c, || {
                a.send("b");
                b.send("B");
            })
        );
    }
    super::assert_memory_freed(sodium_ctx);
}

#[test]
fn stream_or_else1() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let a = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<i32>());
        let b = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<i32>());
        let c = sodium_ctx.transaction(|| a.stream().or_else(&b.stream()));
        assert_eq!(vec![0], round(sodium_ctx, &c, || a.send(0)));
        assert_eq!(vec![10], round(sodium_ctx, &c, || b.send(10)));
        // Simultaneous: the left hand side wins.
        assert_eq!(
            vec![2],
            round(sodium_ctx, &c, || {
                a.send(2);
                b.send(20);
            })
        );
        assert_eq!(vec![30], round(sodium_ctx, &c, || b.send(30)));
    }
    super::assert_memory_freed(sodium_ctx);
}

// FIXME: this test encodes the reference semantics and currently fails
// on sodium-rust, which reports ["b", "B"] for the second round.
//
// `Operational::defer` is built on `SodiumCtx::post`, and
// `SodiumCtx::end_of_transaction` drains the post queue by calling each
// closure in turn. Every closure calls `StreamSink::send`, which opens a
// transaction of its own, so N deferred sends produce N separate child
// transactions. The Java and C++ implementations run all deferred sends
// inside a *single* child transaction, which is what makes the two
// `defer`s simultaneous and lets `or_else` drop the right hand side.
#[ignore = "sodium-rust puts each deferred send in its own transaction"]
#[test]
fn operational_defer_simultaneous() {
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let a = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<&'static str>());
        let b = sodium_ctx.transaction(|| sodium_ctx.new_stream_sink::<&'static str>());
        let c = sodium_ctx.transaction(|| {
            Operational::defer(&a.stream()).or_else(&Operational::defer(&b.stream()))
        });
        assert_eq!(vec!["A"], round(sodium_ctx, &c, || b.send("A")));
        // Both are deferred into the same child transaction, so
        // `or_else` picks the left hand side.
        assert_eq!(
            vec!["b"],
            round(sodium_ctx, &c, || {
                a.send("b");
                b.send("B");
            })
        );
    }
    super::assert_memory_freed(sodium_ctx);
}
