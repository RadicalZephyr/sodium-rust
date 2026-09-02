use crate::tests::init;
use crate::{transaction, Cell, CellSink, SodiumCtx, Stream, StreamSink, Transaction};

use std::sync::{Arc, Mutex};
use std::thread;

#[test]
fn current_is_stable_within_a_thread() {
    init();
    let a = SodiumCtx::current();
    let b = SodiumCtx::current();
    assert!(a.ptr_eq(&b));
}

#[test]
fn ambient_constructors_build_one_working_graph() {
    init();
    let clicks: StreamSink<()> = StreamSink::new();
    let bias: CellSink<i32> = CellSink::new(10);
    let count = clicks.stream().accum(0, |_: &(), n: &i32| n + 1);
    let total = count.lift2(&bias.cell(), |n: &i32, b: &i32| n + b);

    let out = Arc::new(Mutex::new(Vec::new()));
    let l = {
        let out = out.clone();
        total.listen(move |n: &i32| out.lock().unwrap().push(*n))
    };

    clicks.send(());
    bias.send(100);
    l.unlisten();

    assert_eq!(*out.lock().unwrap(), vec![10, 11, 101]);
}

#[test]
fn free_transaction_batches_on_the_ambient_context() {
    init();
    let a: StreamSink<i32> = StreamSink::new();
    let b: StreamSink<i32> = StreamSink::new();
    let sum = a.stream().merge(&b.stream(), |x: &i32, y: &i32| x + y);

    let fired = Arc::new(Mutex::new(Vec::new()));
    let l = {
        let fired = fired.clone();
        sum.listen(move |n: &i32| fired.lock().unwrap().push(*n))
    };

    // One transaction, so both sends are simultaneous and the merged
    // stream fires once with the coalesced value.
    transaction(|| {
        a.send(1);
        b.send(2);
    });
    l.unlisten();

    assert_eq!(*fired.lock().unwrap(), vec![3]);
}

#[test]
fn scoped_transaction_uses_the_ambient_context() {
    init();
    let sl = {
        let _t = Transaction::new();
        let sl = crate::StreamLoop::<i32>::new();
        let ss: StreamSink<i32> = StreamSink::new();
        sl.loop_(&ss.stream());
        ss.send(1);
        sl
    };
    // Resolving the loop inside the scoped transaction is enough; using
    // the stream afterwards must not panic.
    let _ = sl.stream().map(|a: &i32| a + 1);
}

#[test]
fn enter_overrides_the_thread_default() {
    init();
    let default = SodiumCtx::current();
    let other = SodiumCtx::new();
    {
        let _guard = other.enter();
        assert!(SodiumCtx::current().ptr_eq(&other));
        let s: Stream<i32> = Stream::new();
        assert!(s.sodium_ctx().ptr_eq(&other));
    }
    assert!(SodiumCtx::current().ptr_eq(&default));
}

#[test]
fn enter_nests_and_restores_in_order() {
    init();
    let outer = SodiumCtx::new();
    let inner = SodiumCtx::new();
    let _o = outer.enter();
    assert!(SodiumCtx::current().ptr_eq(&outer));
    {
        let _i = inner.enter();
        assert!(SodiumCtx::current().ptr_eq(&inner));
    }
    assert!(SodiumCtx::current().ptr_eq(&outer));
}

#[test]
fn each_thread_gets_its_own_default_context() {
    init();
    let here = SodiumCtx::current();
    let there = thread::spawn(SodiumCtx::current).join().unwrap();
    assert!(!here.ptr_eq(&there));
}

#[test]
fn enter_lets_a_second_thread_build_in_the_same_graph() {
    init();
    let ctx = SodiumCtx::new();
    let ss: StreamSink<i32> = {
        let _g = ctx.enter();
        StreamSink::new()
    };

    let out = Arc::new(Mutex::new(Vec::new()));
    let l = thread::scope(|scope| {
        scope
            .spawn(|| {
                let _g = ctx.enter();
                // A root node built on this thread, wired to a graph built
                // on the parent thread. Without `enter` this would panic.
                let offset: Cell<i32> = Cell::new(1000);
                let sum = ss.stream().snapshot(&offset, |a: &i32, b: &i32| a + b);
                let out = out.clone();
                sum.listen(move |n: &i32| out.lock().unwrap().push(*n))
            })
            .join()
            .unwrap()
    });

    ss.send(5);
    l.unlisten();
    assert_eq!(*out.lock().unwrap(), vec![1005]);
}

#[test]
#[should_panic(expected = "two different SodiumCtx")]
fn combining_two_contexts_panics() {
    init();
    let other = SodiumCtx::new();
    let mine: StreamSink<i32> = StreamSink::new();
    let theirs: StreamSink<i32> = StreamSink::new_in(&other);
    let _ = mine
        .stream()
        .merge(&theirs.stream(), |a: &i32, b: &i32| a + b);
}

#[test]
#[should_panic(expected = "two different SodiumCtx")]
fn a_root_built_on_another_thread_panics_when_wired_in() {
    init();
    let ss: StreamSink<i32> = StreamSink::new();
    // No `enter`, so the spawned thread builds in its own default context.
    let stray: Cell<i32> = thread::spawn(|| Cell::new(1)).join().unwrap();
    let _ = ss.stream().snapshot(&stray, |a: &i32, b: &i32| a + b);
}
