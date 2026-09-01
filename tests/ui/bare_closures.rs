// The headline guarantee, checked the way a downstream crate would hit it:
// every combinator takes a bare, unannotated closure.
//
// This is the regression guard for the `FnMut`/`Fn` bounds. If any of these
// methods is moved back onto an `IsLambda`-style bound, this stops compiling.

use sodium::{Cell, Dep, Listener, Stream};

pub fn chained(s: &Stream<i32>) -> Stream<i32> {
    s.map(|a| *a + 1)
        .filter(|a| *a % 2 == 0)
        .filter_map(|a| Some(*a * 3))
}

pub fn merged(s: &Stream<i32>, t: &Stream<i32>) -> Stream<i32> {
    s.merge(t, |a, b| *a + *b)
}

pub fn stateful(s: &Stream<i32>) -> (Cell<i32>, Stream<i32>) {
    let total = s.accum(0, |a, t| *a + *t);
    let doubled = s.collect(0, |a, t| (*a * 2, *a + *t));
    (total, doubled)
}

pub fn snapshots(
    s: &Stream<i32>,
    c1: &Cell<i32>,
    c2: &Cell<i32>,
    c3: &Cell<i32>,
    c4: &Cell<i32>,
    c5: &Cell<i32>,
) -> Stream<i32> {
    let two = s.snapshot(c1, |a, b| *a + *b);
    let three = s.snapshot3(c1, c2, |a, b, x| *a + *b + *x);
    let four = s.snapshot4(c1, c2, c3, |a, b, x, y| *a + *b + *x + *y);
    let five = s.snapshot5(c1, c2, c3, c4, |a, b, x, y, z| *a + *b + *x + *y + *z);
    let six = s.snapshot6(c1, c2, c3, c4, c5, |a, b, w, x, y, z| {
        *a + *b + *w + *x + *y + *z
    });
    two.merge(&three, |a, b| *a + *b)
        .merge(&four, |a, b| *a + *b)
        .merge(&five, |a, b| *a + *b)
        .merge(&six, |a, b| *a + *b)
}

pub fn lifted(
    a: &Cell<i32>,
    b1: &Cell<i32>,
    b2: &Cell<i32>,
    b3: &Cell<i32>,
    b4: &Cell<i32>,
    b5: &Cell<i32>,
) -> Cell<i32> {
    let two = a.lift2(b1, |x, y| *x + *y);
    let three = a.lift3(b1, b2, |x, y, z| *x + *y + *z);
    let four = a.lift4(b1, b2, b3, |w, x, y, z| *w + *x + *y + *z);
    let five = a.lift5(b1, b2, b3, b4, |v, w, x, y, z| *v + *w + *x + *y + *z);
    let six = a.lift6(b1, b2, b3, b4, b5, |u, v, w, x, y, z| {
        *u + *v + *w + *x + *y + *z
    });
    two.lift5(&three, &four, &five, &six, |p, q, r, s, t| {
        *p + *q + *r + *s + *t
    })
    .map(|x| *x)
}

pub fn listened(s: &Stream<i32>, c: &Cell<i32>) -> Vec<Listener> {
    vec![
        s.listen(|a| println!("{}", *a + 1)),
        s.listen_weak(|a| println!("{}", *a + 1)),
        c.listen(|a| println!("{}", *a + 1)),
        c.listen_weak(|a| println!("{}", *a + 1)),
    ]
}

// The `*_with_deps` siblings infer just the same.
pub fn with_deps(s: &Stream<i32>, c: &Cell<i32>) -> Stream<i32> {
    let deps: Vec<Dep> = vec![c.to_dep()];
    let captured = c.clone();
    s.map_with_deps(move |a| *a + captured.sample(), deps)
}

// A closure body that constrains only an associated type of the argument
// (`<?U as Neg>::Output`) rather than the argument itself. Under the old
// `IsLambda` bounds even `|a: &_|` was not enough here -- it needed `&i32`.
pub fn negated(s: &Stream<i32>) -> Listener {
    s.listen(|a| println!("{}", -*a))
}

// Nothing here needs to run: trybuild compiles *and* executes a `pass` case,
// and it is the type checking that is under test. The functions above are
// concrete, so rustc checks their bodies -- and therefore infers every closure
// signature -- whether or not they are ever called. Runtime behaviour is
// covered by tests/closure_type_inference.rs.
fn main() {}
