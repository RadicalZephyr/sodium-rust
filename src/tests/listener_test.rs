//! Ports of the listener lifecycle tests from the .NET binding's
//! `Sodium.Frp.Tests/StreamTests.cs`.
//!
//! .NET distinguishes strong from weak listeners and drops the weak ones
//! at a forced GC. Here dropping a [`Listener`] only decrements its
//! node's reference count -- the node is freed by the cycle collector --
//! so the inner blocks below stand in for going out of scope and the
//! explicit `collect_cycles` calls for the upstream `GC.Collect`.

use crate::SodiumCtx;

use std::sync::{Arc, Mutex};

#[test]
fn unlisten() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<i32>();
    let out = Arc::new(Mutex::new(Vec::new()));
    {
        let l;
        {
            let out = out.clone();
            l = s
                .stream()
                .listen(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
        }
        s.send(1);
        l.unlisten();
        s.send(2);
    }
    s.send(3);
    s.send(4);
    let lock = out.lock();
    let out: &Vec<i32> = lock.as_ref().unwrap();
    assert_eq!(vec![1], *out);
}

#[test]
fn unlisten_weak() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<i32>();
    let out = Arc::new(Mutex::new(Vec::new()));
    {
        let l;
        {
            let out = out.clone();
            l = s
                .stream()
                .listen_weak(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
        }
        s.send(1);
        l.unlisten();
        s.send(2);
    }
    s.send(3);
    s.send(4);
    let lock = out.lock();
    let out: &Vec<i32> = lock.as_ref().unwrap();
    assert_eq!(vec![1], *out);
}

// Unlistening more than once must be harmless.
#[test]
fn multiple_unlisten() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<i32>();
    let out = Arc::new(Mutex::new(Vec::new()));
    {
        let l;
        {
            let out = out.clone();
            l = s
                .stream()
                .listen(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
        }
        s.send(1);
        l.unlisten();
        l.unlisten();
        s.send(2);
        l.unlisten();
    }
    s.send(3);
    s.send(4);
    let lock = out.lock();
    let out: &Vec<i32> = lock.as_ref().unwrap();
    assert_eq!(vec![1], *out);
}

#[test]
fn multiple_unlisten_weak() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<i32>();
    let out = Arc::new(Mutex::new(Vec::new()));
    {
        let l;
        {
            let out = out.clone();
            l = s
                .stream()
                .listen_weak(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
        }
        s.send(1);
        l.unlisten();
        l.unlisten();
        s.send(2);
        l.unlisten();
    }
    s.send(3);
    s.send(4);
    let lock = out.lock();
    let out: &Vec<i32> = lock.as_ref().unwrap();
    assert_eq!(vec![1], *out);
}

// A weak listener stops firing once its handle is dropped.
#[test]
fn listen_weak() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<i32>();
    let out = Arc::new(Mutex::new(Vec::new()));
    {
        let out = out.clone();
        let _l = s
            .stream()
            .listen_weak(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
        s.send(1);
        s.send(2);
    }
    // The upstream test forces a GC here; the analogue is collecting
    // cycles, which is what actually frees a dropped listener's node.
    sodium_ctx.impl_.collect_cycles();
    s.send(3);
    s.send(4);
    let lock = out.lock();
    let out: &Vec<i32> = lock.as_ref().unwrap();
    assert_eq!(2, out.len());
}

// Dropping one weak listener must not disturb a second one attached to
// the same intermediate stream afterwards.
#[test]
fn listen_weak_with_map() {
    let sodium_ctx = SodiumCtx::new();
    let s = sodium_ctx.new_stream_sink::<i32>();
    let out = Arc::new(Mutex::new(Vec::new()));
    {
        let s2 = s.stream().map(|v: &i32| v + 1);
        {
            let out = out.clone();
            let _l = s2.listen_weak(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
            s.send(1);
            s.send(2);
        }
        sodium_ctx.impl_.collect_cycles();
        {
            let out = out.clone();
            let _l = s2.listen_weak(move |v: &i32| out.lock().as_mut().unwrap().push(*v));
            s.send(3);
            s.send(4);
            s.send(5);
        }
    }
    sodium_ctx.impl_.collect_cycles();
    s.send(6);
    let lock = out.lock();
    let out: &Vec<i32> = lock.as_ref().unwrap();
    assert_eq!(vec![2, 3, 4, 5, 6], *out);
}
