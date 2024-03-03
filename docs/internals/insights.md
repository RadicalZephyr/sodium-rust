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
