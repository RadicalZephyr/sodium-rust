use crate::cell::Cell;
use crate::impl_::cell_sink::CellSink as CellSinkImpl;
use crate::sodium_ctx::SodiumCtx;

/// An object for pushing new values into a [`Cell`].
///
/// This is an operational primitive that should act as an interface
/// between the world of I/O and the world of FRP.
///
/// ## Note:
///
/// This should only be used from _outside_ the context of the Sodium
/// system to inject data from I/O into the reactive system. That
/// includes not calling this method from inside of the callbacks
/// passed to [`Cell::listen`] or
/// [`Stream::listen`][crate::stream::Stream].
pub struct CellSink<A> {
    pub impl_: CellSinkImpl<A>,
}

impl<A> Clone for CellSink<A> {
    fn clone(&self) -> Self {
        CellSink {
            impl_: self.impl_.clone(),
        }
    }
}

impl<A: Clone + Send + 'static> CellSink<A> {
    /// Create a new `CellSink` in the given context with the given value.
    pub fn new(sodium_ctx: &SodiumCtx, a: A) -> CellSink<A> {
        CellSink {
            impl_: CellSinkImpl::new(&sodium_ctx.impl_, a),
        }
    }

    /// Return a [`Cell`] that can be used to create Sodium logic that
    /// will read the values pushed into this [`CellSink`] from the I/O
    /// world.
    pub fn cell(&self) -> Cell<A> {
        Cell {
            impl_: self.impl_.cell(),
        }
    }

    /// Send a value, modifying the value of the cell.
    ///
    /// This method may not be called in handlers registered with
    /// [`Stream::listen`][crate::Stream::listen] or [`Cell::listen`].
    ///
    /// [`CellSink`] is an operational primitive, meant for interfacing
    /// I/O to FRP only. You aren't meant to use this to define your
    /// own primitives.
    pub fn send(&self, a: A) {
        self.impl_.send(a);
    }
}
