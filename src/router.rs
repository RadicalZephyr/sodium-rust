use crate::impl_::router::Router as RouterImpl;
use crate::SodiumCtx;
use crate::Stream;
use std::hash::Hash;

/// Create a new Router that routes event items of type `A` to zero or
/// more [`Stream`]s of type `K` according to a given selector
/// function.
///
/// [`Router`] is purely a tool for improving performance of your FRP
/// programs.  When you find yourself filtering out many different
/// types of values from the same [`Stream`] and the system starts to
/// become too slow, then you should consider using [`Router`] to
/// provide that filtering. The purpose of Router is to provide a more
/// efficient [`Stream::filter`] for when your filtering a lot off the
/// same stream. It creates less work for the backend when updating
/// the graph. It is a last resort when struggling with performance.
pub struct Router<A, K> {
    impl_: RouterImpl<A, K>,
}

impl<A, K> Router<A, K> {
    /// Create a new `Router` from the given input stream and selector
    /// function.
    pub fn new(
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
