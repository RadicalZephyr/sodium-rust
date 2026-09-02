//! Guards the type requirements of the public API.
//!
//! A value flowing through a [`Stream`] needs to be `Send + 'static`, and no
//! more. `Clone` is required only by the combinators that forward one value
//! to more than one place, and `Sync` is never required of a value or of a
//! closure.
//!
//! Every function here is concrete, so rustc checks its bounds whether or not
//! it runs; the `#[test]`s then confirm the runtime behaviour. If a blanket
//! `Clone` or `Sync` bound creeps back into the API, this file stops
//! compiling.

use sodium::{Cell, Enum2, Router, SodiumCtx, Stream};
use std::cell::Cell as StdCell;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

/// A value that is deliberately neither `Clone` nor `Copy`.
struct Token(u32);

/// `Send` but not `Sync`.
#[derive(Clone)]
struct NotSync(StdCell<u32>);

/// A `Send + !Sync` router key.
#[derive(PartialEq, Eq, Hash, Clone)]
struct Key(u32, PhantomData<StdCell<()>>);

/// A `Clone` collection whose items are not `Clone`. `Stream::split`
/// needs the former and not the latter.
#[derive(Clone)]
struct Tokens(u32);

impl IntoIterator for Tokens {
    type Item = Token;
    type IntoIter = std::iter::Map<std::ops::Range<u32>, fn(u32) -> Token>;

    fn into_iter(self) -> Self::IntoIter {
        (0..self.0).map(Token)
    }
}

fn assert_memory_freed(ctx: &SodiumCtx) {
    ctx.impl_.collect_cycles();
    assert_eq!(ctx.impl_.node_count(), 0);
}

fn assert_send_sync<T: Send + Sync>() {}

/// Relaxing the bounds must not cost the thread safety of the graph itself.
#[test]
fn context_and_nodes_remain_thread_safe() {
    assert_send_sync::<SodiumCtx>();
    assert_send_sync::<Stream<Token>>();
    assert_send_sync::<Stream<NotSync>>();
    assert_send_sync::<Cell<Token>>();
    assert_send_sync::<Router<u32, Key>>();
}

/// The core pipeline: sink, `map`, `filter_map`, `snapshot`, `accum`,
/// `collect` and `listen`, with an element type that is not `Clone`.
#[test]
fn non_clone_values_flow_through_a_pipeline() {
    let ctx = SodiumCtx::new();
    let snapped_out: Arc<Mutex<Vec<u32>>> = Default::default();
    let collected_out: Arc<Mutex<Vec<u32>>> = Default::default();
    let total;
    {
        let sink = ctx.new_stream_sink::<Token>();
        let scale = ctx.new_cell_sink(10u32);

        let s: Stream<Token> = sink.stream();
        let mapped: Stream<Token> = s.map(|t| Token(t.0 + 1));
        let evens: Stream<Token> = mapped.filter_map(|t| (t.0 % 2 == 0).then_some(Token(t.0)));
        let snapped: Stream<Token> = evens.snapshot(&scale.cell(), |t, k| Token(t.0 * *k));
        total = snapped.accum(0u32, |t, acc| *acc + t.0);
        let collected: Stream<u32> = snapped.collect(0u32, |t, n| (t.0 + *n, *n + 1));

        let sunk = snapped_out.clone();
        let l1 = snapped.listen(move |t| sunk.lock().unwrap().push(t.0));
        let sunk = collected_out.clone();
        let l2 = collected.listen(move |v| sunk.lock().unwrap().push(*v));

        sink.send(Token(1)); // 2, even: 20
        sink.send(Token(2)); // 3, odd: dropped
        sink.send(Token(3)); // 4, even: 40

        assert_eq!(*snapped_out.lock().unwrap(), vec![20, 40]);
        assert_eq!(*collected_out.lock().unwrap(), vec![20, 41]);
        assert_eq!(total.sample(), 60);
        l1.unlisten();
        l2.unlisten();
    }
    drop(total);
    assert_memory_freed(&ctx);
}

/// `split_enum2` produces owned values from its closure, so neither the
/// input nor the outputs need to be `Clone`.
#[test]
fn split_enum2_routes_non_clone_values() {
    let ctx = SodiumCtx::new();
    let small_out: Arc<Mutex<Vec<u32>>> = Default::default();
    let large_out: Arc<Mutex<Vec<u32>>> = Default::default();

    let sink = ctx.new_stream_sink::<Token>();
    let (small, large): (Stream<Token>, Stream<Token>) = sink.stream().split_enum2(|t| {
        if t.0 < 10 {
            Enum2::A(Token(t.0))
        } else {
            Enum2::B(Token(t.0))
        }
    });

    let sunk = small_out.clone();
    let l1 = small.listen(move |t| sunk.lock().unwrap().push(t.0));
    let sunk = large_out.clone();
    let l2 = large.listen(move |t| sunk.lock().unwrap().push(t.0));

    for v in [1, 20, 3, 40] {
        sink.send(Token(v));
    }

    assert_eq!(*small_out.lock().unwrap(), vec![1, 3]);
    assert_eq!(*large_out.lock().unwrap(), vec![20, 40]);
    l1.unlisten();
    l2.unlisten();
}

/// `split` iterates a clone of the collection and moves each item into
/// its own transaction, so the items themselves need not be `Clone`.
#[test]
fn split_moves_non_clone_items_out_of_a_clone_collection() {
    let ctx = SodiumCtx::new();
    let out: Arc<Mutex<Vec<u32>>> = Default::default();

    let sink = ctx.new_stream_sink::<Tokens>();
    let items: Stream<Token> = sink.stream().split();
    let sunk = out.clone();
    let l = items.listen(move |t| sunk.lock().unwrap().push(t.0));

    sink.send(Tokens(3));

    assert_eq!(*out.lock().unwrap(), vec![0, 1, 2]);
    l.unlisten();
}

/// The constant handed to `map_to` is cloned per event, never shared
/// between threads, so it needs `Clone` but not `Sync`.
#[test]
fn map_to_accepts_a_value_that_is_not_sync() {
    let ctx = SodiumCtx::new();
    let out: Arc<Mutex<Vec<u32>>> = Default::default();

    let sink = ctx.new_stream_sink::<()>();
    let s: Stream<NotSync> = sink.stream().map_to(NotSync(StdCell::new(7)));
    let sunk = out.clone();
    let l = s.listen(move |v| sunk.lock().unwrap().push(v.0.get()));

    sink.send(());
    sink.send(());

    assert_eq!(*out.lock().unwrap(), vec![7, 7]);
    l.unlisten();
}

/// Closures handed to the API need to be `Send`, not `Send + Sync`. A
/// boxed `FnMut` is the everyday example of a capture that is `Send`
/// but not `Sync`; a `std::cell::Cell` is another.
#[test]
fn closures_need_not_be_sync() {
    let ctx = SodiumCtx::new();
    let listened: Arc<Mutex<Vec<u32>>> = Default::default();
    let mapped_out: Arc<Mutex<Vec<u32>>> = Default::default();
    let cell_out: Arc<Mutex<Vec<u32>>> = Default::default();

    let sink = ctx.new_stream_sink::<u32>();

    let sunk = listened.clone();
    let mut callback: Box<dyn FnMut(u32) + Send> = Box::new(move |v| sunk.lock().unwrap().push(v));
    let l1 = sink.stream().listen(move |v| callback(*v));

    let running = StdCell::new(0u32);
    let running_total: Stream<u32> = sink.stream().map(move |v| {
        running.set(running.get() + *v);
        running.get()
    });
    let sunk = mapped_out.clone();
    let l2 = running_total.listen(move |v| sunk.lock().unwrap().push(*v));

    let held: Cell<u32> = sink.stream().hold(0);
    let sunk = cell_out.clone();
    let mut callback: Box<dyn FnMut(u32) + Send> = Box::new(move |v| sunk.lock().unwrap().push(v));
    let l3 = held.listen(move |v| callback(*v));

    sink.send(1);
    sink.send(2);
    sink.send(3);

    assert_eq!(*listened.lock().unwrap(), vec![1, 2, 3]);
    assert_eq!(*mapped_out.lock().unwrap(), vec![1, 3, 6]);
    assert_eq!(*cell_out.lock().unwrap(), vec![0, 1, 2, 3]);
    for l in [l1, l2, l3] {
        l.unlisten();
    }
}

/// The router's table lives behind a `Mutex`, so its keys need only be
/// `Send`.
#[test]
fn router_keys_need_not_be_sync() {
    let ctx = SodiumCtx::new();
    let out: Arc<Mutex<Vec<u32>>> = Default::default();

    let sink = ctx.new_stream_sink::<u32>();
    let router: Router<u32, Key> =
        ctx.new_router(&sink.stream(), |v| vec![Key(*v % 2, PhantomData)]);
    let evens = router.filter_matches(&Key(0, PhantomData));
    let sunk = out.clone();
    let l = evens.listen(move |v| sunk.lock().unwrap().push(*v));

    for v in 1..=4 {
        sink.send(v);
    }

    assert_eq!(*out.lock().unwrap(), vec![2, 4]);
    l.unlisten();
}
