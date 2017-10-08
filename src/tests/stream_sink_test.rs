use sodium::IsStream;
use sodium::SodiumCtx;
use sodium::Stream;
use sodium::StreamSink;
use sodium::Transaction;
use tests::assert_memory_freed;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn map() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s: StreamSink<i32> = StreamSink::new(sodium_ctx);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l = s.map(sodium_ctx, |a| *a + 1)
                .listen(
                    sodium_ctx,
                    move |a| {
                        (*out).borrow_mut().push(a.clone())
                    }
                );
        }
        s.send(sodium_ctx, &7);
        assert_eq!(vec![8], *(*out).borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn map_to() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s = StreamSink::new(sodium_ctx);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l =
                s.map_to(sodium_ctx, "fusebox")
                    .listen(
                        sodium_ctx,
                        move |a|
                            (*out).borrow_mut().push(*a)
                    );
        }
        s.send(sodium_ctx, &7);
        s.send(sodium_ctx, &9);
        assert_eq!(vec!["fusebox", "fusebox"], *(*out).borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn merge_non_simultaneous() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s1 = StreamSink::new(sodium_ctx);
        let s2 = StreamSink::new(sodium_ctx);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l =
                s2.or_else(sodium_ctx, &s1)
                    .listen(
                        sodium_ctx,
                        move |a|
                            (*out).borrow_mut().push(*a)
                    );
        }
        s1.send(sodium_ctx, &7);
        s2.send(sodium_ctx, &9);
        s1.send(sodium_ctx, &8);
        assert_eq!(vec![7, 9, 8], *(*out).borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn merge_simultaneous() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s1 = StreamSink::new_with_coalescer(sodium_ctx, |l, r| *r);
        let s2 = StreamSink::new_with_coalescer(sodium_ctx, |l, r| *r);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l =
                s2.or_else(sodium_ctx, &s1)
                    .listen(
                        sodium_ctx,
                        move |a|
                            (*out).borrow_mut().push(*a)
                    );
        }
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s1.send(sodium_ctx, &7);
                s2.send(sodium_ctx, &60);
            }
        );
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s1.send(sodium_ctx, &9);
            }
        );
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s1.send(sodium_ctx, &7);
                s1.send(sodium_ctx, &60);
                s2.send(sodium_ctx, &8);
                s2.send(sodium_ctx, &90);
            }
        );
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s2.send(sodium_ctx, &8);
                s2.send(sodium_ctx, &90);
                s1.send(sodium_ctx, &7);
                s1.send(sodium_ctx, &60);
            }
        );
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s2.send(sodium_ctx, &8);
                s1.send(sodium_ctx, &7);
                s2.send(sodium_ctx, &90);
                s1.send(sodium_ctx, &60);
            }
        );
        assert_eq!(vec![60, 9, 90, 90, 90], *(*out).borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn coalesce() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s = StreamSink::new_with_coalescer(sodium_ctx, |a, b| *a + *b);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l = s.listen(
                sodium_ctx,
                move |a|
                    out.borrow_mut().push(*a)
            );
        }
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s.send(sodium_ctx, &2);
            }
        );
        Transaction::run(
            sodium_ctx,
            |sodium_ctx| {
                s.send(sodium_ctx, &8);
                s.send(sodium_ctx, &40);
            }
        );
        assert_eq!(vec![2, 48], *out.borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn filter() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s = StreamSink::new(sodium_ctx);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l = s
                .filter(sodium_ctx, |a| *a < 10)
                .listen(
                    sodium_ctx,
                    move |a|
                        out.borrow_mut().push(*a)
                );
        }
        s.send(sodium_ctx, &2);
        s.send(sodium_ctx, &16);
        s.send(sodium_ctx, &9);
        assert_eq!(vec![2, 9], *out.borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn filter_option() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s = StreamSink::new(sodium_ctx);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l = Stream::filter_option(sodium_ctx, &s)
                .listen(
                    sodium_ctx,
                    move |a|
                        out.borrow_mut().push(*a)
                );
        }
        s.send(sodium_ctx, &Some("tomato"));
        s.send(sodium_ctx, &None);
        s.send(sodium_ctx, &Some("peach"));
        assert_eq!(vec!["tomato", "peach"], *out.borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn merge() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let sa = StreamSink::new(sodium_ctx);
        let sb =
            sa
                .map(sodium_ctx, |x| *x / 10)
                .filter(sodium_ctx, |x| *x != 0);
        let sc =
            sa
                .map(sodium_ctx, |x| *x % 10)
                .merge(sodium_ctx, &sb, |x, y| *x + *y);
        let out = Rc::new(RefCell::new(Vec::new()));
        let l;
        {
            let out = out.clone();
            l = sc.listen(
                sodium_ctx,
                move |a|
                    out.borrow_mut().push(*a)
            );
        }
        sa.send(sodium_ctx, &2);
        sa.send(sodium_ctx, &52);
        assert_eq!(vec![2, 7], *out.borrow());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

/*
import { expect } from 'chai';

import {
  lambda1,
  StreamSink,
  StreamLoop,
  CellSink,
  Transaction,
  Tuple2,
  Operational,
  Cell,
  CellLoop,
  getTotalRegistrations
} from '../../lib/Sodium';

export class StreamSinkTest {

    afterEach() {
      if (getTotalRegistrations() != 0) {
        throw new Error('listeners were not deregistered');
      }
    };

    'should test map()' (done) {
      const s = new StreamSink<number>();
      const out: number[] = [];
      const kill = s.map(a => a + 1)
        .listen(a => {
          out.push(a);
          done();
        });
      s.send(7);
      kill();

      expect([8]).to.deep.equal(out);
    };

    'should throw an error send_with_no_listener_1' () {
      const s = new StreamSink<number>();

      try {
        s.send(7);
      } catch (e) {
        expect(e.message).to.equal('send() was invoked before listeners were registered');
      }

    };

    'should (not?) throw an error send_with_no_listener_2' () {
      const s = new StreamSink<number>();
      const out: number[] = [];
      const kill = s.map(a => a + 1)
        .listen(a => out.push(a));

      s.send(7);
      kill();

      try {
        // TODO: the message below is bit misleading, need to verify with Stephen B.
        //       - "this should not throw, because once() uses this mechanism"
        s.send(9);
      } catch (e) {
        expect(e.message).to.equal('send() was invoked before listeners were registered');
      }
    };

    'should map_tack' (done) {
      const s = new StreamSink<number>(),
        t = new StreamSink<string>(),
        out: number[] = [],
        kill = s.map(lambda1((a: number) => a + 1, [t]))
          .listen(a => {
            out.push(a);
            done();
          });

      s.send(7);
      t.send("banana");
      kill();

      expect([8]).to.deep.equal(out);
    };

    'should test mapTo()' (done) {
      const s = new StreamSink<number>(),
        out: string[] = [],
        kill = s.mapTo("fusebox")
          .listen(a => {
            out.push(a);
            if(out.length === 2) {
              done();
            }
          });

      s.send(7);
      s.send(9);
      kill();

      expect(['fusebox', 'fusebox']).to.deep.equal(out);
    };

    'should do mergeNonSimultaneous' (done) {
      const s1 = new StreamSink<number>(),
        s2 = new StreamSink<number>(),
        out: number[] = [];

      const kill = s2.orElse(s1)
        .listen(a => {
          out.push(a);
          if(out.length === 3) {
            done();
          }
        });

      s1.send(7);
      s2.send(9);
      s1.send(8);
      kill();

      expect([7, 9, 8]).to.deep.equal(out);
    };

    'should do mergeSimultaneous' (done) {
      const s1 = new StreamSink<number>((l: number, r: number) => { return r; }),
        s2 = new StreamSink<number>((l: number, r: number) => { return r; }),
        out: number[] = [],
        kill = s2.orElse(s1)
          .listen(a => {
            out.push(a);
            if(out.length === 5) {
              done();
            }
          });

      Transaction.run<void>(() => {
        s1.send(7);
        s2.send(60);
      });
      Transaction.run<void>(() => {
        s1.send(9);
      });
      Transaction.run<void>(() => {
        s1.send(7);
        s1.send(60);
        s2.send(8);
        s2.send(90);
      });
      Transaction.run<void>(() => {
        s2.send(8);
        s2.send(90);
        s1.send(7);
        s1.send(60);
      });
      Transaction.run<void>(() => {
        s2.send(8);
        s1.send(7);
        s2.send(90);
        s1.send(60);
      });
      kill();

      expect([60, 9, 90, 90, 90]).to.deep.equal(out);
    };

    'should do coalesce' (done) {
      const s = new StreamSink<number>((a, b) => a + b),
        out: number[] = [],
        kill = s.listen(a => {
          out.push(a);
          if(out.length === 2) {
            done();
          }
        });

      Transaction.run<void>(() => {
        s.send(2);
      });
      Transaction.run<void>(() => {
        s.send(8);
        s.send(40);
      });
      kill();

      expect([2, 48]).to.deep.equal(out);
    };

    'should test filter()' (done) {
      const s = new StreamSink<number>(),
        out: number[] = [],
        kill = s.filter(a => a < 10)
          .listen(a => {
            out.push(a);
            if(out.length === 2) {
              done();
            }
          });

      s.send(2);
      s.send(16);
      s.send(9);
      kill();

      expect([2, 9]).to.deep.equal(out);
    };

    'should test filterNotNull()' (done) {
      const s = new StreamSink<string>(),
        out: string[] = [],
        kill = s.filterNotNull()
          .listen(a => {
            out.push(a);
            if(out.length === 2) {
              done();
            }
          });

      s.send("tomato");
      s.send(null);
      s.send("peach");
      kill();

      expect(["tomato", "peach"]).to.deep.equal(out);
    };

    'should test merge()' (done) {
      const sa = new StreamSink<number>(),
        sb = sa.map(x => Math.floor(x / 10))
          .filter(x => x != 0),
        sc = sa.map(x => x % 10)
          .merge(sb, (x, y) => x + y),
        out: number[] = [],
        kill = sc.listen(a => {
          out.push(a);
          if(out.length === 2) {
            done();
          }
        });

      sa.send(2);
      sa.send(52);
      kill();

      expect([2, 7]).to.deep.equal(out);
    };

    'should test loop()' (done) {
      const sa = new StreamSink<number>(),
        sc = Transaction.run(() => {
          const sb = new StreamLoop<number>(),
            sc_ = sa.map(x => x % 10).merge(sb,
              (x, y) => x + y),
            sb_out = sa.map(x => Math.floor(x / 10))
              .filter(x => x != 0);
          sb.loop(sb_out);
          return sc_;
        }),
        out: number[] = [],
        kill = sc.listen(a => {
          out.push(a);
          if(out.length === 2) {
            done();
          }
        });

      sa.send(2);
      sa.send(52);
      kill();

      expect([2, 7]).to.deep.equal(out);
    };

    'should test gate()' (done) {
      const s = new StreamSink<string>(),
        pred = new CellSink<boolean>(true),
        out: string[] = [],
        kill = s.gate(pred).listen(a => {
          out.push(a);
          if(out.length === 2) {
            done();
          }
        });

      s.send("H");
      pred.send(false);
      s.send('O');
      pred.send(true);
      s.send('I');
      kill();

      expect(["H", "I"]).to.deep.equal(out);
    };

    'should test collect()' (done) {
      const ea = new StreamSink<number>(),
        out: number[] = [],
        sum = ea.collect(0, (a, s) => new Tuple2(a + s + 100, a + s)),
        kill = sum.listen(a => {
          out.push(a);
          if(out.length === 5) {
            done();
          }
        });

      ea.send(5);
      ea.send(7);
      ea.send(1);
      ea.send(2);
      ea.send(3);
      kill();

      expect([105, 112, 113, 115, 118]).to.deep.equal(out);
    };

    'should test accum()' (done) {
      const ea = new StreamSink<number>(),
        out: number[] = [],
        sum = ea.accum(100, (a, s) => a + s),
        kill = sum.listen(a => {
          out.push(a);
          if(out.length === 6) {
            done();
          }
        });

      ea.send(5);
      ea.send(7);
      ea.send(1);
      ea.send(2);
      ea.send(3);
      kill();

      expect([100, 105, 112, 113, 115, 118]).to.deep.equal(out);
    };

    'should test once()' (done) {
      const s = new StreamSink<string>(),
        out: string[] = [],
        kill = s.once().listen(a => {
          out.push(a);
          done();
        });

      s.send("A");
      s.send("B");
      s.send("C");
      kill();

      expect(["A"]).to.deep.equal(out);
    };

    'should test defer()' (done) {
      const s = new StreamSink<string>(),
        c = s.hold(" "),
        out: string[] = [],
        kill = Operational.defer(s).snapshot1(c)
          .listen(a => {
            out.push(a);
            if(out.length === 3) {
              done();
            }
          });

      s.send("C");
      s.send("B");
      s.send("A");
      kill();

      expect(["C", "B", "A"]).to.deep.equal(out);
    };

    'should test hold()' (done) {
      const s = new StreamSink<number>(),
        c = s.hold(0),
        out: number[] = [],
        kill = Operational.updates(c)
          .listen(a => {
            out.push(a);
            if(out.length === 2) {
              done();
            }
          });

      s.send(2);
      s.send(9);
      kill();

      expect([2, 9]).to.deep.equal(out);
    };

    'should do holdIsDelayed' (done) {
      const s = new StreamSink<number>(),
        h = s.hold(0),
        sPair = s.snapshot(h, (a, b) => a + " " + b),
        out: string[] = [],
        kill = sPair.listen(a => {
          out.push(a);
          if(out.length === 2) {
            done();
          }
        });

      s.send(2);
      s.send(3);
      kill();

      expect(["2 0", "3 2"]).to.deep.equal(out);
    };

    'should test switchC()' (done) {
      class SC {
        constructor(a: string, b: string, sw: string) {
          this.a = a;
          this.b = b;
          this.sw = sw;
        }

        a: string;
        b: string;
        sw: string;
      }

      const ssc = new StreamSink<SC>(),
        // Split each field out of SC so we can update multiple cells in a
        // single transaction.
        ca = ssc.map(s => s.a).filterNotNull().hold("A"),
        cb = ssc.map(s => s.b).filterNotNull().hold("a"),
        csw_str = ssc.map(s => s.sw).filterNotNull().hold("ca"),
        // ****
        // NOTE! Because this lambda contains references to Sodium objects, we
        // must declare them explicitly using lambda1() so that Sodium knows
        // about the dependency, otherwise it can't manage the memory.
        // ****
        csw = csw_str.map(lambda1(s => s == "ca" ? ca : cb, [ca, cb])),
        co = Cell.switchC(csw),
        out: string[] = [],
        kill = co.listen(c => {
          out.push(c);
          if(out.length === 11) {
            done();
          }
        });

      ssc.send(new SC("B", "b", null));
      ssc.send(new SC("C", "c", "cb"));
      ssc.send(new SC("D", "d", null));
      ssc.send(new SC("E", "e", "ca"));
      ssc.send(new SC("F", "f", null));
      ssc.send(new SC(null, null, "cb"));
      ssc.send(new SC(null, null, "ca"));
      ssc.send(new SC("G", "g", "cb"));
      ssc.send(new SC("H", "h", "ca"));
      ssc.send(new SC("I", "i", "ca"));
      kill();

      expect(["A", "B", "c", "d", "E", "F", "f", "F", "g", "H", "I"]).to.deep.equal(out);

    };

    'should test switchS()' (done) {
      class SS {
        constructor(a: string, b: string, sw: string) {
          this.a = a;
          this.b = b;
          this.sw = sw;
        }

        a: string;
        b: string;
        sw: string;
      }

      const sss = new StreamSink<SS>(),
        sa = sss.map(s => s.a),
        sb = sss.map(s => s.b),
        csw_str = sss.map(s => s.sw).filterNotNull().hold("sa"),
        // ****
        // NOTE! Because this lambda contains references to Sodium objects, we
        // must declare them explicitly using lambda1() so that Sodium knows
        // about the dependency, otherwise it can't manage the memory.
        // ****
        csw = csw_str.map(lambda1(sw => sw == "sa" ? sa : sb, [sa, sb])),
        so = Cell.switchS(csw),
        out: string[] = [],
        kill = so.listen(x => {
          out.push(x);
          if(out.length === 9) {
            done();
          }
        });

      sss.send(new SS("A", "a", null));
      sss.send(new SS("B", "b", null));
      sss.send(new SS("C", "c", "sb"));
      sss.send(new SS("D", "d", null));
      sss.send(new SS("E", "e", "sa"));
      sss.send(new SS("F", "f", null));
      sss.send(new SS("G", "g", "sb"));
      sss.send(new SS("H", "h", "sa"));
      sss.send(new SS("I", "i", "sa"));
      kill();

      expect(["A", "B", "C", "d", "e", "F", "G", "h", "I"]).to.deep.equal(out);
    };

    'should do switchSSimultaneous' (done) {
      class SS2 {
        s: StreamSink<number> = new StreamSink<number>();
      }

      const ss1 = new SS2(),
        ss2 = new SS2(),
        ss3 = new SS2(),
        ss4 = new SS2(),
        css = new CellSink<SS2>(ss1),
        // ****
        // NOTE! Because this lambda contains references to Sodium objects, we
        // must declare them explicitly using lambda1() so that Sodium knows
        // about the dependency, otherwise it can't manage the memory.
        // ****
        so = Cell.switchS(css.map(lambda1((b: SS2) => b.s, [ss1.s, ss2.s, ss3.s, ss4.s]))),
        out: number[] = [],
        kill = so.listen(c => {
          out.push(c);
          if(out.length === 10) {
            done();
          }
        });

      ss1.s.send(0);
      ss1.s.send(1);
      ss1.s.send(2);
      css.send(ss2);
      ss1.s.send(7);
      ss2.s.send(3);
      ss2.s.send(4);
      ss3.s.send(2);
      css.send(ss3);
      ss3.s.send(5);
      ss3.s.send(6);
      ss3.s.send(7);
      Transaction.run(() => {
        ss3.s.send(8);
        css.send(ss4);
        ss4.s.send(2);
      });
      ss4.s.send(9);
      kill();

      expect([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]).to.deep.equal(out);
    };

    'should test loopCell' (done)  {
      const sa = new StreamSink<number>(),
        sum_out = Transaction.run(() => {
          const sum = new CellLoop<number>(),
            sum_out_ = sa.snapshot(sum, (x, y) => x + y).hold(0);
          sum.loop(sum_out_);
          return sum_out_;
        }),
        out: number[] = [],
        kill = sum_out.listen(a => {
          out.push(a);
          if(out.length === 4) {
            done();
          }
        });

      sa.send(2);
      sa.send(3);
      sa.send(1);
      kill();

      expect([0, 2, 5, 6]).to.deep.equal(out);
      expect(6).to.equal(sum_out.sample());
    };
  }
*/