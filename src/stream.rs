use crate::cell::Cell;
use crate::impl_::dep::Dep;
use crate::impl_::enum_::{Enum2, Enum3};
use crate::impl_::lambda::{lambda1, lambda2};
use crate::impl_::stream::Stream as StreamImpl;
use crate::listener::Listener;
use crate::sodium_ctx::SodiumCtx;
use crate::Lazy;

/// Represents a stream of discrete events/firings containing values
/// of type `A`.
///
/// Also known in other FRP systems as an _event_ (which would contain
/// _event occurrences_), an _event stream_, an _observable_, or a
/// _signal_.
///
/// # Type requirements
///
/// An event value needs to be `Send + 'static`, and nothing more. The
/// graph is reference-counted, shared, and usable from any thread, so
/// every value it stores has to be able to move between threads and
/// to outlive the scope that created it.
///
/// `Clone` is only required by the combinators that hand the same
/// value on to more than one place: [`filter`][Stream::filter],
/// [`merge`][Stream::merge] and [`or_else`][Stream::or_else],
/// [`hold`][Stream::hold], [`gate`][Stream::gate],
/// [`once`][Stream::once],
/// [`Operational::defer`][crate::Operational::defer],
/// [`StreamLoop`][crate::StreamLoop] and [`Router`][crate::Router].
/// Combinators whose closure produces a fresh value, such as
/// [`map`][Stream::map], [`filter_map`][Stream::filter_map],
/// [`snapshot`][Stream::snapshot] and
/// [`split_enum2`][Stream::split_enum2], do not need it, and neither
/// do the sinks or the listeners.
///
/// A [`Cell`] does need `Clone`, because its value is read from many
/// places and each reader gets its own copy. That includes the state
/// of [`accum`][Stream::accum] and [`collect`][Stream::collect]. To
/// carry a type that is not `Clone`, or is expensive to clone, through
/// a cell, wrap it in an [`Arc`][std::sync::Arc].
pub struct Stream<A> {
    pub impl_: StreamImpl<A>,
}

impl<A> Clone for Stream<A> {
    fn clone(&self) -> Self {
        Stream {
            impl_: self.impl_.clone(),
        }
    }
}

impl<A: Send + 'static> Stream<A> {
    pub fn split_enum2<B, C, FN>(&self, f: FN) -> (Stream<B>, Stream<C>)
    where
        B: Send + 'static,
        C: Send + 'static,
        FN: Fn(&A) -> Enum2<B, C> + Send + 'static,
    {
        let (b, c) = self.impl_.split_enum2(f);

        let b = Stream { impl_: b };
        let c = Stream { impl_: c };
        (b, c)
    }

    pub fn split_enum3<B, C, D, FN>(&self, f: FN) -> (Stream<B>, Stream<C>, Stream<D>)
    where
        B: Send + 'static,
        C: Send + 'static,
        D: Send + 'static,
        FN: Fn(&A) -> Enum3<B, C, D> + Send + 'static,
    {
        let (b, c, d) = self.impl_.split_enum3(f);

        let b = Stream { impl_: b };
        let c = Stream { impl_: c };
        let d = Stream { impl_: d };
        (b, c, d)
    }
}

impl<A: Clone + Send + 'static> Stream<Option<A>> {
    /// Return a `Stream` that only outputs events that have present
    /// values, removing the `Option` wrapper and discarding empty
    /// values.
    pub fn filter_option(&self) -> Stream<A> {
        self.filter_map(|a| a.clone())
    }

    pub fn split_opt(&self) -> (Stream<A>, Stream<()>) {
        let (a, b) = self.impl_.split_enum2(|opt: &Option<A>| match opt {
            Some(val) => Enum2::A(val.clone()),
            None => Enum2::B(()),
        });

        let a = Stream { impl_: a };
        let b = Stream { impl_: b };
        (a, b)
    }
}

impl<T, E> Stream<Result<T, E>>
where
    T: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    pub fn split_res(&self) -> (Stream<T>, Stream<E>) {
        let (a, b) = self.impl_.split_enum2(|opt: &Result<T, E>| match opt {
            Ok(val) => Enum2::A(val.clone()),
            Err(e) => Enum2::B(e.clone()),
        });

        let a = Stream { impl_: a };
        let b = Stream { impl_: b };
        (a, b)
    }
}

impl<A: Send + 'static, COLLECTION: IntoIterator<Item = A> + Clone + Send + 'static>
    Stream<COLLECTION>
{
    /// Flatten a `Stream` of a collection of `A` into a `Stream` of
    /// single `A`s.
    pub fn split(&self) -> Stream<A> {
        Stream {
            impl_: self.impl_.split(),
        }
    }
}

impl<A: Send + 'static> Stream<A> {
    /// Create a `Stream` that will never fire.
    pub fn new(sodium_ctx: &SodiumCtx) -> Stream<A> {
        Stream {
            impl_: StreamImpl::new(&sodium_ctx.impl_),
        }
    }

    /// Return a handle to this node for use as an explicit FRP
    /// dependency of a `*_with_deps` combinator.
    pub fn to_dep(&self) -> Dep {
        self.impl_.to_dep()
    }

    /// Return a stream whose events are the result of the combination
    /// of the event value and the current value of the cell using the
    /// specified function.
    ///
    /// Note that there is an implicit delay: state updates caused by
    /// event firings being held with [`Stream::hold`] don't become
    /// visible as the cell's current value until the following
    /// transaction. To put this another way, `snapshot` always sees
    /// the value of a cell as it wass before any state changes from
    /// the current transaction.
    pub fn snapshot<
        B: Clone + Send + 'static,
        C: Send + 'static,
        FN: FnMut(&A, &B) -> C + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        f: FN,
    ) -> Stream<C> {
        self.snapshot_with_deps(cb, f, Vec::new())
    }

    /// A variant of [`snapshot`][Stream::snapshot] that declares extra
    /// FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn snapshot_with_deps<
        B: Clone + Send + 'static,
        C: Send + 'static,
        FN: FnMut(&A, &B) -> C + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        f: FN,
        deps: Vec<Dep>,
    ) -> Stream<C> {
        Stream {
            impl_: self.impl_.snapshot(&cb.impl_, lambda2(f, deps)),
        }
    }

    /// A variant of [`snapshot`][Stream::snapshot] that captures the
    /// cell's value at the time of the event firing, ignoring the
    /// stream's value.
    pub fn snapshot1<B: Send + Clone + 'static>(&self, cb: &Cell<B>) -> Stream<B> {
        self.snapshot(cb, |_a, b| b.clone())
    }

    /// A variant of [`snapshot`][Stream::snapshot] that captures the
    /// value of two cells.
    pub fn snapshot3<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + 'static,
        FN: FnMut(&A, &B, &C) -> D + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        f: FN,
    ) -> Stream<D> {
        self.snapshot3_with_deps(cb, cc, f, Vec::new())
    }

    /// A variant of [`snapshot3`][Stream::snapshot3] that declares extra
    /// FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn snapshot3_with_deps<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + 'static,
        FN: FnMut(&A, &B, &C) -> D + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        mut f: FN,
        mut deps: Vec<Dep>,
    ) -> Stream<D> {
        let cc = cc.clone();
        deps.push(cc.to_dep());
        self.snapshot_with_deps(cb, move |a, b| f(a, b, &cc.sample()), deps)
    }

    /// A variant of [`snapshot`][Stream::snapshot] that captures the
    /// value of three cells.
    pub fn snapshot4<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + Clone + 'static,
        E: Send + 'static,
        FN: FnMut(&A, &B, &C, &D) -> E + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        cd: &Cell<D>,
        f: FN,
    ) -> Stream<E> {
        self.snapshot4_with_deps(cb, cc, cd, f, Vec::new())
    }

    /// A variant of [`snapshot4`][Stream::snapshot4] that declares extra
    /// FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn snapshot4_with_deps<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + Clone + 'static,
        E: Send + 'static,
        FN: FnMut(&A, &B, &C, &D) -> E + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        cd: &Cell<D>,
        mut f: FN,
        mut deps: Vec<Dep>,
    ) -> Stream<E> {
        let cc = cc.clone();
        let cd = cd.clone();
        deps.push(cc.to_dep());
        deps.push(cd.to_dep());
        self.snapshot_with_deps(cb, move |a, b| f(a, b, &cc.sample(), &cd.sample()), deps)
    }

    /// A variant of [`snapshot`][Stream::snapshot] that captures the
    /// value of four cells.
    pub fn snapshot5<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + Clone + 'static,
        E: Send + Clone + 'static,
        F: Send + 'static,
        FN: FnMut(&A, &B, &C, &D, &E) -> F + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        cd: &Cell<D>,
        ce: &Cell<E>,
        f: FN,
    ) -> Stream<F> {
        self.snapshot5_with_deps(cb, cc, cd, ce, f, Vec::new())
    }

    /// A variant of [`snapshot5`][Stream::snapshot5] that declares extra
    /// FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    #[allow(clippy::too_many_arguments)]
    pub fn snapshot5_with_deps<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + Clone + 'static,
        E: Send + Clone + 'static,
        F: Send + 'static,
        FN: FnMut(&A, &B, &C, &D, &E) -> F + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        cd: &Cell<D>,
        ce: &Cell<E>,
        mut f: FN,
        mut deps: Vec<Dep>,
    ) -> Stream<F> {
        let cc = cc.clone();
        let cd = cd.clone();
        let ce = ce.clone();
        deps.push(cc.to_dep());
        deps.push(cd.to_dep());
        deps.push(ce.to_dep());
        self.snapshot_with_deps(
            cb,
            move |a, b| f(a, b, &cc.sample(), &cd.sample(), &ce.sample()),
            deps,
        )
    }

    /// A variant of [`snapshot`][Stream::snapshot] that captures the
    /// value of five cells.
    pub fn snapshot6<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + Clone + 'static,
        E: Send + Clone + 'static,
        F: Send + Clone + 'static,
        G: Send + 'static,
        FN: FnMut(&A, &B, &C, &D, &E, &F) -> G + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        cd: &Cell<D>,
        ce: &Cell<E>,
        cf: &Cell<F>,
        f: FN,
    ) -> Stream<G> {
        self.snapshot6_with_deps(cb, cc, cd, ce, cf, f, Vec::new())
    }

    /// A variant of [`snapshot6`][Stream::snapshot6] that declares extra
    /// FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    #[allow(clippy::too_many_arguments)]
    pub fn snapshot6_with_deps<
        B: Send + Clone + 'static,
        C: Send + Clone + 'static,
        D: Send + Clone + 'static,
        E: Send + Clone + 'static,
        F: Send + Clone + 'static,
        G: Send + 'static,
        FN: FnMut(&A, &B, &C, &D, &E, &F) -> G + Send + 'static,
    >(
        &self,
        cb: &Cell<B>,
        cc: &Cell<C>,
        cd: &Cell<D>,
        ce: &Cell<E>,
        cf: &Cell<F>,
        mut f: FN,
        mut deps: Vec<Dep>,
    ) -> Stream<G> {
        let cc = cc.clone();
        let cd = cd.clone();
        let ce = ce.clone();
        let cf = cf.clone();
        deps.push(cc.to_dep());
        deps.push(cd.to_dep());
        deps.push(ce.to_dep());
        deps.push(cf.to_dep());
        self.snapshot_with_deps(
            cb,
            move |a, b| f(a, b, &cc.sample(), &cd.sample(), &ce.sample(), &cf.sample()),
            deps,
        )
    }

    /// Transform this `Stream`'s event values with the supplied
    /// function.
    ///
    /// The supplied function may construct FRP logic or use
    /// [`Cell::sample`], in which case it's equivalent to
    /// [`snapshot`][Stream::snapshot]ing the cell. In addition, the
    /// function must be referentially transparent.
    pub fn map<B: Send + 'static, FN: FnMut(&A) -> B + Send + 'static>(&self, f: FN) -> Stream<B> {
        self.map_with_deps(f, Vec::new())
    }

    /// A variant of [`map`][Stream::map] that declares extra FRP
    /// dependencies for the supplied function.
    ///
    /// Sodium builds its dependency graph from the shape of the FRP
    /// network, which it cannot see inside a closure. If `f` captures a
    /// [`Cell`] and calls [`sample`][Cell::sample] on it, that cell is a
    /// real dependency of the resulting stream, and passing it here via
    /// [`Cell::to_dep`] is what keeps the graph correct.
    ///
    /// This is a low-level escape hatch. Prefer expressing the
    /// dependency in the network itself, with
    /// [`snapshot`][Stream::snapshot] and friends, which do this
    /// bookkeeping for you.
    pub fn map_with_deps<B: Send + 'static, FN: FnMut(&A) -> B + Send + 'static>(
        &self,
        f: FN,
        deps: Vec<Dep>,
    ) -> Stream<B> {
        Stream {
            impl_: self.impl_.map(lambda1(f, deps)),
        }
    }

    /// Transform this `Stream`'s event values into the specified constant value.
    pub fn map_to<B: Send + Clone + 'static>(&self, b: B) -> Stream<B> {
        self.map(move |_| b.clone())
    }

    /// Return a `Stream` that only outputs events for which the predicate returns `true`.
    pub fn filter<PRED: FnMut(&A) -> bool + Send + 'static>(&self, pred: PRED) -> Stream<A>
    where
        A: Clone,
    {
        self.filter_with_deps(pred, Vec::new())
    }

    /// A variant of [`filter`][Stream::filter] that declares extra FRP
    /// dependencies for the supplied predicate.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn filter_with_deps<PRED: FnMut(&A) -> bool + Send + 'static>(
        &self,
        pred: PRED,
        deps: Vec<Dep>,
    ) -> Stream<A>
    where
        A: Clone,
    {
        Stream {
            impl_: self.impl_.filter(lambda1(pred, deps)),
        }
    }

    /// Return a `Stream` that both filters and maps.
    ///
    /// Only outputs events for which the supplied closure returns
    /// `Some(value)`.
    ///
    /// `filter_map` can be used to make chains of `filter` and `map`
    /// more concise and performant. The example below shows how a
    /// `map().filter().map()` can be shortened to a single call to
    /// `filter_map`.
    ///
    /// [`filter`]: Stream::filter
    /// [`map`]: Stream::map
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # use std::sync::{Arc, Mutex};
    /// # use sodium::SodiumCtx;
    /// #
    /// let mut sodium_ctx = SodiumCtx::new();
    /// let s = sodium_ctx.new_stream_sink();
    /// let out = Arc::new(Mutex::new(Vec::new()));
    /// let l = {
    ///     let out = out.clone();
    ///     s
    ///         .stream()
    ///         .filter_map(|a: &&'static str| a.parse().ok())
    ///         .listen(move |a| out.lock().unwrap().push(*a))
    /// };
    /// s.send("1");
    /// s.send("two");
    /// s.send("NaN");
    /// s.send("four");
    /// s.send("5");
    ///
    /// let out = out.lock().unwrap();
    /// assert_eq!([1, 5], &out[..]);
    /// l.unlisten();
    /// ```
    ///
    /// Here's the same example, but with [`filter`] and [`map`]:
    ///
    /// ```
    /// # use std::sync::{Arc, Mutex};
    /// # use sodium::SodiumCtx;
    /// #
    /// let mut sodium_ctx = SodiumCtx::new();
    /// let s = sodium_ctx.new_stream_sink();
    /// let out = Arc::new(Mutex::new(Vec::new()));
    /// let l = {
    ///     let out = out.clone();
    ///     s
    ///         .stream()
    ///         .map(|a: &&'static str| a.parse::<i32>())
    ///         .filter(|a| a.is_ok())
    ///         .map(|a| a.clone().unwrap())
    ///         .listen(move |a| out.lock().unwrap().push(*a))
    /// };
    /// s.send("1");
    /// s.send("two");
    /// s.send("NaN");
    /// s.send("four");
    /// s.send("5");
    ///
    /// let out = out.lock().unwrap();
    /// assert_eq!([1, 5], &out[..]);
    /// l.unlisten();
    /// ```
    pub fn filter_map<B: Send + 'static, FN: FnMut(&A) -> Option<B> + Send + 'static>(
        &self,
        f: FN,
    ) -> Stream<B> {
        self.filter_map_with_deps(f, Vec::new())
    }

    /// A variant of [`filter_map`][Stream::filter_map] that declares
    /// extra FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn filter_map_with_deps<B: Send + 'static, FN: FnMut(&A) -> Option<B> + Send + 'static>(
        &self,
        f: FN,
        deps: Vec<Dep>,
    ) -> Stream<B> {
        Stream {
            impl_: self.impl_.filter_map(lambda1(f, deps)),
        }
    }

    /// Variant of [`merge`][Stream::merge] that merges two streams.
    ///
    /// In the case where two events are simultaneous (both in the
    /// same transaction), the event taken from `self` takes
    /// precedenc, and the event from `s2` will be dropped.
    ///
    /// If you want to specify your own combining function use
    /// [`merge`][Stream::merge]. This function is equivalent to
    /// `s1.merge(s2, |l, _r| l)`. The name `or_else` is used instead
    /// of `merge` to make it clear that care should be taken because
    /// events can be dropped.
    pub fn or_else(&self, s2: &Stream<A>) -> Stream<A>
    where
        A: Clone,
    {
        self.merge(s2, |lhs, _rhs| lhs.clone())
    }

    /// Merge two streams of the same type into one, so that events on
    /// either input appear on the returned stream.
    ///
    /// If the events are simultaneous (that is, one event from `self`
    /// and one from `s2` occur in the same transaction), combine them
    /// into one using the specified combining function so that the
    /// returned stream is guaranteed only ever to have one event per
    /// transaction. The event from `self` will appear at the left
    /// input of the combining function, and the event from `s2` will
    /// appear at the right.
    pub fn merge<FN: FnMut(&A, &A) -> A + Send + 'static>(&self, s2: &Stream<A>, f: FN) -> Stream<A>
    where
        A: Clone,
    {
        self.merge_with_deps(s2, f, Vec::new())
    }

    /// A variant of [`merge`][Stream::merge] that declares extra FRP
    /// dependencies for the supplied combining function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn merge_with_deps<FN: FnMut(&A, &A) -> A + Send + 'static>(
        &self,
        s2: &Stream<A>,
        f: FN,
        deps: Vec<Dep>,
    ) -> Stream<A>
    where
        A: Clone,
    {
        Stream {
            impl_: self.impl_.merge(&s2.impl_, lambda2(f, deps)),
        }
    }

    /// Returns a cell with the specified initial value, which is
    /// updated by this stream's event values.
    pub fn hold(&self, a: A) -> Cell<A>
    where
        A: Clone,
    {
        Cell {
            impl_: self.impl_.hold(a),
        }
    }

    /// A variant of [`hold`][Stream::hold] that uses an initial value
    /// returned by [`Cell::sample_lazy`].
    pub fn hold_lazy(&self, a: Lazy<A>) -> Cell<A>
    where
        A: Clone,
    {
        Cell {
            impl_: self.impl_.hold_lazy(a),
        }
    }

    /// Return a stream that only outputs events from the input stream
    /// when the specified cell's value is true.
    pub fn gate(&self, cpred: &Cell<bool>) -> Stream<A>
    where
        A: Clone,
    {
        let cpred = cpred.clone();
        let cpred_dep = cpred.to_dep();
        self.filter_with_deps(move |_| cpred.sample(), vec![cpred_dep])
    }

    /// Return a stream that outputs only one value, which is the next
    /// event of the input stream, starting from the transaction in
    /// `once` was invoked.
    pub fn once(&self) -> Stream<A>
    where
        A: Clone,
    {
        Stream {
            impl_: self.impl_.once(),
        }
    }

    /// Transform an event with a generalized state loop (a Mealy
    /// machine). The function is passed the input and the old state
    /// and returns the new state and output value.
    pub fn collect<B, S, F>(&self, init_state: S, f: F) -> Stream<B>
    where
        B: Send + Clone + 'static,
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> (B, S) + Send + 'static,
    {
        self.collect_with_deps(init_state, f, Vec::new())
    }

    /// A variant of [`collect`][Stream::collect] that declares extra FRP
    /// dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn collect_with_deps<B, S, F>(&self, init_state: S, f: F, deps: Vec<Dep>) -> Stream<B>
    where
        B: Send + Clone + 'static,
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> (B, S) + Send + 'static,
    {
        self.collect_lazy_with_deps(Lazy::new(move || init_state.clone()), f, deps)
    }

    /// A variant of [`collect`][Stream::collect] that takes an
    /// initial state that is returned by [`Cell::sample_lazy`].
    pub fn collect_lazy<B, S, F>(&self, init_state: Lazy<S>, f: F) -> Stream<B>
    where
        B: Send + Clone + 'static,
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> (B, S) + Send + 'static,
    {
        self.collect_lazy_with_deps(init_state, f, Vec::new())
    }

    /// A variant of [`collect_lazy`][Stream::collect_lazy] that declares
    /// extra FRP dependencies for the supplied function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn collect_lazy_with_deps<B, S, F>(
        &self,
        init_state: Lazy<S>,
        f: F,
        deps: Vec<Dep>,
    ) -> Stream<B>
    where
        B: Send + Clone + 'static,
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> (B, S) + Send + 'static,
    {
        Stream {
            impl_: self.impl_.collect_lazy(init_state, lambda2(f, deps)),
        }
    }

    /// Accumulate on an input event, outputting the new state each time.
    ///
    /// As each event is received, the accumulating function `f` is
    /// called with the current state and the new event value. The
    /// accumulating function may construct FRP logic or use
    /// [`Cell::sample`], in which case it's equivalent to
    /// [`snapshot`][Stream::snapshot]ing the cell. In additon, the
    /// function must be referentially transparent.
    pub fn accum<S, F>(&self, init_state: S, f: F) -> Cell<S>
    where
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> S + Send + 'static,
    {
        self.accum_with_deps(init_state, f, Vec::new())
    }

    /// A variant of [`accum`][Stream::accum] that declares extra FRP
    /// dependencies for the supplied accumulating function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn accum_with_deps<S, F>(&self, init_state: S, f: F, deps: Vec<Dep>) -> Cell<S>
    where
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> S + Send + 'static,
    {
        self.accum_lazy_with_deps(Lazy::new(move || init_state.clone()), f, deps)
    }

    /// A variant of [`accum`][Stream::accum] that takes an initial
    /// state returned by [`Cell::sample_lazy`].
    pub fn accum_lazy<S, F>(&self, init_state: Lazy<S>, f: F) -> Cell<S>
    where
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> S + Send + 'static,
    {
        self.accum_lazy_with_deps(init_state, f, Vec::new())
    }

    /// A variant of [`accum_lazy`][Stream::accum_lazy] that declares
    /// extra FRP dependencies for the supplied accumulating function.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn accum_lazy_with_deps<S, F>(&self, init_state: Lazy<S>, f: F, deps: Vec<Dep>) -> Cell<S>
    where
        S: Send + Clone + 'static,
        F: FnMut(&A, &S) -> S + Send + 'static,
    {
        Cell {
            impl_: self.impl_.accum_lazy(init_state, lambda2(f, deps)),
        }
    }

    /// A variant of [`listen`][Stream::listen] that will deregister
    /// the listener automatically if the listener is
    /// garbage-collected.
    ///
    /// With [`listen`][Stream::listen] the listener is only
    /// deregistered if [`Listener::unlisten`] is called explicitly.
    pub fn listen_weak<K: FnMut(&A) + Send + 'static>(&self, k: K) -> Listener {
        self.listen_weak_with_deps(k, Vec::new())
    }

    /// A variant of [`listen_weak`][Stream::listen_weak] that declares
    /// extra FRP dependencies for the supplied handler.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn listen_weak_with_deps<K: FnMut(&A) + Send + 'static>(
        &self,
        k: K,
        deps: Vec<Dep>,
    ) -> Listener {
        Listener {
            impl_: self.impl_.listen_weak(lambda1(k, deps)),
        }
    }

    /// Listen for events/firings on this stream.
    ///
    /// This is the observer pattern. The returned [`Listener`] has an
    /// [`unlisten`][Listener::unlisten] method to cause the listener
    /// to be removed. This is an operational mechanism for
    /// interfacing between the world of I/O and FRP.
    ///
    /// The handler function for this listener should make no
    /// assumptions about what thread it will be called on, and the
    /// handler should not block. It also is not allowed to use
    /// [`CellSink::send`][crate::CellSink::send] or
    /// [`StreamSink::send`][crate::StreamSink::send] in the handler.
    pub fn listen<K: FnMut(&A) + Send + 'static>(&self, k: K) -> Listener {
        self.listen_with_deps(k, Vec::new())
    }

    /// A variant of [`listen`][Stream::listen] that declares extra FRP
    /// dependencies for the supplied handler.
    ///
    /// See [`map_with_deps`][Stream::map_with_deps] for when this is
    /// needed.
    pub fn listen_with_deps<K: FnMut(&A) + Send + 'static>(
        &self,
        k: K,
        deps: Vec<Dep>,
    ) -> Listener {
        Listener {
            impl_: self.impl_.listen(lambda1(k, deps)),
        }
    }
}
