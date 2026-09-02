# Insights

The basic structure in Sodium is the `Node`. If a new computation
needs to happen, then you need a new `Node`.

A `Node` does something with the update closure passed to `Node::new`.
Within that, to get the potential firing value, you use
`Stream::with_firing_op` from the `Stream` that feeds this new `Node`.

You then do some computation using whatever user closure supplied for
this `Node`, then call `Stream::_send` on the appropriate `Stream`.

In the forward direction, i.e. the direction that events propagate,
you use weak references, and you register yourself as a dependency of
the nodes that are "downstream."

This dependency ordering is because the continued existence of a
`Listener` is what roots a chain of `Nodes`. When a `Listener` goes
out of scope, then anything that listener depended on (that no longer
has any dependencies) is free to be cleaned up.

## Why values are `Send + 'static`, and closures are not `Sync`

Every `Node` is stored type-erased, as a `Box<dyn IsNode + Send +
Sync>`, in a `SodiumCtx` that is itself `Send + Sync` and can be
cloned into any thread. A node's update closure captures the
`Stream<A>`s it reads from, so the closure, and therefore `A`, has to
be `Send`; and it has to be `'static` because nothing bounds how long
the graph lives. There is no way to admit a `!Send` value without
giving up a thread-safe `SodiumCtx` for everyone, because the type
erasure discards `A`.

`A: Send` is all it takes for `Stream<A>` to be `Send + Sync`: the
firing value sits behind a `Mutex`, and `Mutex<T>: Sync` needs only
`T: Send`. The same fact is why user closures need not be `Sync`. The
update closure is kept in `NodeData::update` behind a `Mutex`, not an
`RwLock` (whose contents would have to be `Sync`), and it is only ever
called through exclusive access anyway, since it is an `FnMut`.

`Clone` is different: it is not structural, it is per operation. A
stream's firing value lives in that stream's `StreamData` for the
length of the transaction, and any number of dependents read it by
reference. A dependent that forwards the value unchanged (`filter`,
`or_else`, `hold`, `once`, and so on) has to put a copy in its own
`StreamData`, so those, and only those, require `A: Clone`.

The alternative would be to store firings as `Arc<A>` and share them.
That would drop `Clone` almost everywhere, `Cell` included, and would
stop `snapshot` and `hold` from deep-copying large values. It costs an
allocation per event, and it would require `A: Sync`, because
`Arc<A>: Send` needs `A: Sync` (sharing hands out `&A` on several
threads at once). Wrapping a value in an `Arc` at the call site buys
the same thing, only where it is wanted.
