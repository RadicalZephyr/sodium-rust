//! Does the `O(1)` mutable-collection trick survive being measured?
//!
//! Evidence for [ADR-0003](../../../0003-fn-bounds-on-combinators.md).
//!
//! Upstream issue #48 proposed bounding the combinators on `Fn`. It stalled in
//! 2020 on a counter-argument: that `Stream::map` is deliberately allowed to
//! mutate, because updating a collection in place is `O(1)` where rebuilding an
//! immutable one is `O(log n)`, and that doing so is "indistinguishable from
//! the immutable collection version in how it operates".
//!
//! That last claim is the one worth testing, because it is the one that makes
//! the trick safe rather than merely fast. Its content is: a value handed to an
//! observer in transaction N still reads, later, as it did in transaction N.
//! An immutable collection gives that for free.
//!
//! So this measures each variant of the trick twice -- once at the moment the
//! event arrives, once after the run is over -- and prints both. Agreement
//! means the claim holds; disagreement means past values moved.
//!
//! Run from the repository root:
//!
//! ```shell
//! cargo run --release -p adr-research --bin 0003-collection-drift
//! ```

use sodium_rust::{Listener, SodiumCtx, Stream};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Sizes recorded as each event arrived, and the values themselves, kept so the
/// same measurement can be repeated once the run is over.
type Probe<M> = (Arc<Mutex<Vec<usize>>>, Arc<Mutex<Vec<M>>>, Listener);

fn probe<M, F>(s: &Stream<M>, size_of: F) -> Probe<M>
where
    M: Clone + Send + 'static,
    F: Fn(&M) -> usize + Send + Sync + 'static,
{
    let at_event: Arc<Mutex<Vec<usize>>> = Default::default();
    let kept: Arc<Mutex<Vec<M>>> = Default::default();
    let (a, k) = (at_event.clone(), kept.clone());
    let l = s.listen(move |m| {
        a.lock().unwrap().push(size_of(m));
        k.lock().unwrap().push(m.clone());
    });
    (at_event, kept, l)
}

const EVENTS: [(&str, i32); 3] = [("a", 1), ("b", 2), ("c", 3)];

/// Variant A -- return the collection by value, as the issue's sketch does.
fn owned_by_value() -> (Vec<usize>, Vec<usize>) {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<(String, i32)>();

    let table: Arc<Mutex<HashMap<String, i32>>> = Default::default();
    let t = table.clone();
    let updated = sink.stream().map(move |(k, v): &(String, i32)| {
        let mut table = t.lock().unwrap();
        table.insert(k.clone(), *v);
        // The combinator's return type is owned, so the collection has to be
        // copied out to produce it. This is the O(n) the trick meant to avoid.
        table.clone()
    });

    let (at_event, kept, l) = probe(&updated, |m: &HashMap<String, i32>| m.len());
    for (k, v) in EVENTS {
        sink.send((k.to_string(), v));
    }
    let after = kept.lock().unwrap().iter().map(|m| m.len()).collect();
    let during = at_event.lock().unwrap().clone();
    l.unlisten();
    (during, after)
}

/// Variant B -- hand downstream a shared handle, so nothing is copied.
fn shared_handle() -> (Vec<usize>, Vec<usize>) {
    let ctx = SodiumCtx::new();
    let sink = ctx.new_stream_sink::<(String, i32)>();

    let table: Arc<Mutex<HashMap<String, i32>>> = Default::default();
    let t = table.clone();
    let updated = sink.stream().map(move |(k, v): &(String, i32)| {
        t.lock().unwrap().insert(k.clone(), *v);
        t.clone() // O(1): clones the Arc, not the map
    });

    let (at_event, kept, l) = probe(&updated, |m: &Arc<Mutex<HashMap<String, i32>>>| {
        m.lock().unwrap().len()
    });
    for (k, v) in EVENTS {
        sink.send((k.to_string(), v));
    }
    let after = kept
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.lock().unwrap().len())
        .collect();
    let during = at_event.lock().unwrap().clone();
    l.unlisten();
    (during, after)
}

fn report(name: &str, cost: &str, (during, after): (Vec<usize>, Vec<usize>)) {
    let verdict = if during == after {
        "values held"
    } else {
        "VALUES DRIFTED"
    };
    println!("{name:<22} {cost:<6} at event {during:?}  observed later {after:?}  -- {verdict}");
}

fn main() {
    println!("Collection size as each event arrived, and again after the run.\n");
    report("owned, by value", "O(n)", owned_by_value());
    report("shared handle", "O(1)", shared_handle());
    println!(
        "\nNeither variant depends on the combinator's bound: a closure cannot lend\n\
         out a borrow of its own state whatever the bound is, because the return\n\
         type is owned. Both closures above reach their state through a shared\n\
         reference, so both are already `Fn`."
    );
}
