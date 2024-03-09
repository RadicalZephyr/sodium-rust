# Options in Cells

I think `Options` are much harder to work with in `Cells` than
`Streams`. In a `Stream` you can filter out `None` values, but in a
`Cell` you just always have to deal with the value potentially being
None.

The typical use for an `Option` in idiomatic Rust programming is as a
value that may or may not be there at any point in time. But they are
poorly suited to representing a value that needs initialization but
after that is always present.

In essence, this is why a `OnceCell` is such a helpful abstraction
because it just requires initialization once and then can be used as
much as you want.
