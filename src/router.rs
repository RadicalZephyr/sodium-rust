use crate::impl_::router::Router as RouterImpl;
use crate::SodiumCtx;
use crate::Stream;
use std::hash::Hash;

/// Create a new Router that routes event items of type `A` to zero or
/// more [`Stream`]s of type `K` according to a given selector
/// function.
pub struct Router<A, K> {
    impl_: RouterImpl<A, K>,
}

impl<A, K> Router<A, K> {
    /// Create a new `Router` from the given input stream and selector
    /// function.
    ///
    /// The `Router` is created in `in_stream`'s context; a `Router` on a
    /// stream from some other context would be a broken graph, so there
    /// is nothing for a separate context argument to say.
    pub fn new(
        in_stream: &Stream<A>,
        selector: impl Fn(&A) -> Vec<K> + Send + Sync + 'static,
    ) -> Router<A, K>
    where
        A: Clone + Send + 'static,
        K: Send + Sync + Eq + Hash + 'static,
    {
        Router::new_in(&in_stream.sodium_ctx(), in_stream, selector)
    }

    /// A variant of [`new`][Router::new] that takes the context
    /// explicitly. It must be the context `in_stream` belongs to.
    pub fn new_in(
        sodium_ctx: &SodiumCtx,
        in_stream: &Stream<A>,
        selector: impl Fn(&A) -> Vec<K> + Send + Sync + 'static,
    ) -> Router<A, K>
    where
        A: Clone + Send + 'static,
        K: Send + Sync + Eq + Hash + 'static,
    {
        Router {
            impl_: RouterImpl::new(&sodium_ctx.impl_, &in_stream.impl_, selector),
        }
    }

    /// Create a Stream that is subscribed to event values that the
    /// selector function routes to the given `K` value.
    pub fn filter_matches(&self, k: &K) -> Stream<A>
    where
        A: Clone + Send + 'static,
        K: Clone + Send + Sync + Eq + Hash + 'static,
    {
        Stream {
            impl_: self.impl_.filter_matches(k),
        }
    }
}
