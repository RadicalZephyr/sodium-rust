extern crate topological_sort;

pub mod rusty_frp;

#[cfg(test)]
mod tests {
    use rusty_frp::Cell;
    use rusty_frp::CellSink;
    use rusty_frp::CellTrait;
    use rusty_frp::FrpContext;
    use rusty_frp::StreamSink;
    use rusty_frp::StreamTrait;
    use rusty_frp::WithFrpContext;

    #[test]
    fn test_via_console() {
        // use: cargo test -- --nocapture
        test_cell_sink();
        test_cell_map();
        test_stream_map();
        test_lift2();
        test_cell_loop();
    }

    fn test_cell_sink() {
        println!("test_cell_sink");
        struct Env {
            frp_context: FrpContext<Env>
        }
        let mut env = Env { frp_context: FrpContext::new() };
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let cs1 = env.frp_context.new_cell_sink(1u32);
        cs1.observe(&mut env, &with_frp_context, |_, value| { println!("cs1 = {}", value); });
        cs1.change_value(&mut env, &with_frp_context, 2);
        cs1.change_value(&mut env, &with_frp_context, 3);
        cs1.change_value(&mut env, &with_frp_context, 4);
    }

    fn test_cell_map() {
        println!("test_cell_map");
        struct Env {
            frp_context: FrpContext<Env>
        }
        let mut env = Env { frp_context: FrpContext::new() };
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let cs1 = env.frp_context.new_cell_sink(1u32);
        let c2 = env.frp_context.map_c(&cs1, |value| { value + 1 });
        c2.observe(&mut env, &with_frp_context, |_, value| { println!("c2 = {}", value); });
        cs1.change_value(&mut env, &with_frp_context, 2);
        cs1.change_value(&mut env, &with_frp_context, 3);
        cs1.change_value(&mut env, &with_frp_context, 4);
    }

    fn test_stream_map() {
        println!("test_stream_map");
        struct Env {
            frp_context: FrpContext<Env>
        }
        let mut env = Env { frp_context: FrpContext::new() };
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let ss1 = env.frp_context.new_stream_sink();
        let s2 = env.frp_context.map_s(&ss1, |value| { value + 1 });
        s2.observe(&mut env, &with_frp_context, |_, value| { println!("c2 = {}", value); });
        ss1.send(&mut env, &with_frp_context, 2);
        ss1.send(&mut env, &with_frp_context, 3);
        ss1.send(&mut env, &with_frp_context, 4);
    }

    fn test_lift2() {
        println!("test_lift2");
        struct Env {
            frp_context: FrpContext<Env>
        }
        let mut env = Env { frp_context: FrpContext::new() };
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let cs1 = env.frp_context.new_cell_sink(1u32);
        let cs2 = env.frp_context.new_cell_sink(1u32);
        let c3 =
            env.frp_context.lift2_c(
                |a, b| a + b,
                &cs1,
                &cs2
            );
        c3.observe(&mut env, &with_frp_context, |_, value| { println!("c3 = {}", value); });
        cs1.change_value(&mut env, &with_frp_context, 2);
        cs2.change_value(&mut env, &with_frp_context, 3);
        cs1.change_value(&mut env, &with_frp_context, 4);
    }

    fn test_cell_loop() {
        println!("test_cell_loop");
        struct Env {
            frp_context: FrpContext<Env>
        }
        let mut env = Env { frp_context: FrpContext::new() };
        #[derive(Copy,Clone)]
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let cs_pulse: CellSink<Env,Option<()>> = env.frp_context.new_cell_sink(None);
        let c_pulse = cs_pulse.clone();
        let c: Cell<Env,u32> =
            env.frp_context.loop_c(
                0u32,
                move |frp_context, c| {
                    frp_context.lift2_c(
                        |a: &u32, pulse| {
                            match pulse {
                                &Some(_) => a.clone() + 1,
                                &None => a.clone()
                            }
                        },
                        c,
                        &c_pulse
                    )
                }
            );
        c.observe(&mut env, &with_frp_context, |_,value| { println!("c = {}", value); });
        // pulse 1
        cs_pulse.change_value(&mut env, &with_frp_context, Some(()));
        // pulse 2
        cs_pulse.change_value(&mut env, &with_frp_context, Some(()));
        // pulse 3
        cs_pulse.change_value(&mut env, &with_frp_context, Some(()));
    }
/*
function shouldThrow(substr : string, f : () => void) : void {
    try {
        f();
    }
    catch (err) {
        if (err.message.search(substr) >= 0)
            return;
        else
            fail("unexpected exception: "+err);
    }
    fail("should throw exception");
}

let current_test : string = null;

function checkMemory() : void {
    if (getTotalRegistrations() != 0)
        throw new Error("listeners were not deregistered!");
}

function test(name : string, t : () => void)
{
    current_test = name;
    let pass = true;
    try {
        t();
        checkMemory();
        current_test = null
        console.log(name + " - PASS");
    }
    catch (err) {
        console.log(name + " - FAIL:");
        if (err.stack !== undefined)
            console.log(err.stack);
        else
            console.log(err);
        pass = false;
        current_test = null;
    }
}*/

    #[test]
    fn map() {
        struct Env {
            frp_context: FrpContext<Env>,
            out: Vec<u32>
        }
        let mut env = Env { frp_context: FrpContext::new(), out: Vec::new() };
        #[derive(Copy,Clone)]
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let s: StreamSink<Env,u32> = env.frp_context.new_stream_sink();
        let s2 = env.frp_context.map_s(&s, |a| a + 1);
        s2.observe(
            &mut env,
            &with_frp_context,
            |env:&mut Env, value:&u32|
                env.out.push(value.clone())
        );
        s.send(&mut env, &with_frp_context, 7);
        assert_eq!(env.out, vec![8]);
    }

/*
test("send_with_no_listener_1", () => {
    shouldThrow("invoked before listeners",
        () => {
            const s = new StreamSink<number>();
            s.send(7);
        }
    );
});

test("send_with_no_listener_2", () => {
    () => {
        const s = new StreamSink<number>();
        const out : number[] = [];
        const kill = s.map(a => a + 1)
                    .listen(a => out.push(a));
        s.send(7);
        kill();
        s.send(9);  // this should not throw, because once() uses this mechanism
    }
});

test("map_track", () => {
    const s = new StreamSink<number>(),
        t = new StreamSink<string>(),
        out : number[] = [],
        kill = s.map(lambda1((a : number) => a + 1, [t]))
                .listen(a => out.push(a));
    s.send(7);
    t.send("banana");
    kill();
    assertEquals([8], out);
});

test("mapTo", () => {
    const s = new StreamSink<number>(),
        out : string[] = [],
        kill = s.mapTo("fusebox")
                .listen(a => out.push(a));
    s.send(7);
    s.send(9);
    kill();
    assertEquals(["fusebox", "fusebox"], out);
});
*/

    #[test]
    fn merge_non_simultaneous() {
        struct Env {
            frp_context: FrpContext<Env>,
            out: Vec<u32>
        }
        let mut env = Env { frp_context: FrpContext::new(), out: Vec::new() };
        #[derive(Copy,Clone)]
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let s1: StreamSink<Env,u32> = env.frp_context.new_stream_sink();
        let s2: StreamSink<Env,u32> = env.frp_context.new_stream_sink();
        let s3 = env.frp_context.or_else(&s2, &s1);
        s3.observe(&mut env, &with_frp_context, |env,value| env.out.push(value.clone()));
        s1.send(&mut env, &with_frp_context, 7);
        s2.send(&mut env, &with_frp_context, 9);
        s1.send(&mut env, &with_frp_context, 8);
        assert_eq!(vec!(7,9,8), env.out);
    }

/*
test("mergeSimultaneous", () => {
    const s1 = new StreamSink<number>((l : number, r : number) => { return r; }),
        s2 = new StreamSink<number>((l : number, r : number) => { return r; }),
        out : number[] = [],
        kill = s2.orElse(s1)
                 .listen(a => out.push(a));
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
    assertEquals([60,9,90,90,90], out);
});

test("coalesce", () => {
    const s = new StreamSink<number>((a, b) => a+b),
        out : number[] = [],
        kill = s.listen(a => out.push(a));
    Transaction.run<void>(() => {
        s.send(2);
    });
    Transaction.run<void>(() => {
        s.send(8);
        s.send(40);
    });
    kill();
    assertEquals([2, 48], out);
});
*/

    #[test]
    fn filter() {
        struct Env {
            frp_context: FrpContext<Env>,
            out: Vec<u32>
        }
        let mut env = Env { frp_context: FrpContext::new(), out: Vec::new() };
        #[derive(Copy,Clone)]
        struct WithFrpContextForEnv {}
        impl WithFrpContext<Env> for WithFrpContextForEnv {
            fn with_frp_context<'r>(&self, env: &'r mut Env) -> &'r mut FrpContext<Env> {
                return &mut env.frp_context;
            }
        }
        let with_frp_context = WithFrpContextForEnv {};
        let s: StreamSink<Env,u32> = env.frp_context.new_stream_sink();
        let s2 = env.frp_context.filter(|a| a.clone() < 10, &s);
        s2.observe(&mut env, &with_frp_context, |env, value| env.out.push(value.clone()));
        s.send(&mut env, &with_frp_context, 2);
        s.send(&mut env, &with_frp_context, 16);
        s.send(&mut env, &with_frp_context, 9);
        assert_eq!(vec![2, 9], env.out);
    }
    
/*
test("filterNotNull", () => {
    const s = new StreamSink<string>(),
        out : string[] = [],
        kill = s.filterNotNull()
                .listen(a => out.push(a));
    s.send("tomato");
    s.send(null);
    s.send("peach");
    kill();
    assertEquals(["tomato", "peach"], out);
});

test("merge2", () => {
    const sa = new StreamSink<number>(),
        sb = sa.map(x => Math.floor(x / 10))
                   .filter(x => x != 0),
        sc = sa.map(x => x % 10).merge(sb,
            (x,y) => x+y),
        out : number[] = [],
        kill = sc.listen(a => out.push(a));
    sa.send(2);
    sa.send(52);
    kill();
    assertEquals([2, 7], out);
});

test("loopStream", () => {
    const sa = new StreamSink<number>(),
        sc = Transaction.run(() => {
            const sb = new StreamLoop<number>(),
                sc_ = sa.map(x => x % 10).merge(sb,
                    (x, y) => x+y),
                sb_out = sa.map(x => Math.floor(x / 10))
                           .filter(x => x != 0);
            sb.loop(sb_out);
            return sc_;
        }),
        out : number[] = [],
        kill = sc.listen(a => out.push(a));
    sa.send(2);
    sa.send(52);
    kill();
    assertEquals([2, 7], out);
});

test("gate", () => {
    const s = new StreamSink<string>(),
        pred = new CellSink<boolean>(true),
        out : string[] = [],
        kill = s.gate(pred).listen(a => out.push(a));
    s.send("H");
    pred.send(false);
    s.send('O');
    pred.send(true);
    s.send('I');
    kill();
    assertEquals(["H", "I"], out);
});

test("collect", () => {
    const ea = new StreamSink<number>(),
        out : number[] = [],
        sum = ea.collect(0, (a, s) => new Tuple2(a+s+100, a+s)),
        kill = sum.listen(a => out.push(a));
    ea.send(5);
    ea.send(7);
    ea.send(1);
    ea.send(2);
    ea.send(3);
    kill();
    assertEquals([105,112,113,115,118], out);
});

test("accum", () => {
    const ea = new StreamSink<number>(),
        out : number[] = [],
        sum = ea.accum(100, (a, s) => a + s),
        kill = sum.listen(a => out.push(a));
    ea.send(5);
    ea.send(7);
    ea.send(1);
    ea.send(2);
    ea.send(3);
    kill();
    assertEquals([100,105,112,113,115,118], out);
});

test("once", () => {

    const s = new StreamSink<string>(),
        out : string[] = [],
        kill = s.once().listen(a => out.push(a));
    s.send("A");
    s.send("B");
    s.send("C");
    kill();
    assertEquals(["A"], out);
});

test("defer", () => {
    const s = new StreamSink<string>(),
        c = s.hold(" "),
        out : string[] = [],
        kill = Operational.defer(s).snapshot1(c)
               .listen(a => out.push(a));
    s.send("C");
    s.send("B");
    s.send("A");
    kill();
    assertEquals(["C","B","A"], out);
});

test("hold", () => {
    const s = new StreamSink<number>(),
        c = s.hold(0),
        out : number[] = [],
        kill = Operational.updates(c)
              .listen(a => out.push(a));
    s.send(2);
    s.send(9);
    kill();
    assertEquals([2, 9], out);
});

test("snapshot", () => {
    const c = new CellSink<number>(0),
        s = new StreamSink<number>(),
        out : string[] = [],
        kill = s.snapshot(c, (x, y) => x + " " + y)
                .listen(a => out.push(a));
    s.send(100);
    c.send(2);
    s.send(200);
    c.send(9);
    c.send(1);
    s.send(300);
    kill();
    assertEquals(["100 0", "200 2", "300 1"], out);
});

test("values", () => {
    const c = new CellSink<number>(9),
        out : number[] = [],
        kill = c.listen(a => out.push(a));
    c.send(2);
    c.send(7);
    kill();
    assertEquals([9, 2, 7], out);
});

test("constantCell", () => {
    const c = new Cell<number>(12),
        out : number[] = [],
        kill = c.listen(a => out.push(a));
    kill();
    assertEquals([12], out);
});

test("mapC", () => {
    const c = new CellSink<number>(6),
        out : string[] = [],
        kill = c.map(a => ""+a)
                .listen(a => out.push(a));
    c.send(8);
    kill();
    assertEquals(["6", "8"], out);
});

test("mapCLateListen", () => {
    shouldThrow("invoked before listeners", () => {
        const c = new CellSink<number>(6),
            out : string[] = [],
            cm = c.map(a => ""+a);
        c.send(2);
        const kill = cm.listen(a => out.push(a));
        c.send(8);
        kill();
        assertEquals(["2", "8"], out);
    });
});

test("apply", () => {
    const cf = new CellSink<(a : number) => string>(a => "1 "+a),
        ca = new CellSink<number>(5),
        out : string[] = [],
        kill = Cell.apply(cf, ca).listen(a => out.push(a));
    cf.send(a => "12 " + a);
    ca.send(6);
    kill();
    assertEquals(["1 5", "12 5", "12 6"], out);
});

test("lift", () => {
    const a = new CellSink<number>(1),
        b = new CellSink<number>(5),
        out : string[] = [],
        kill = a.lift(b, (aa, bb) => aa + " " + bb)
                .listen(a => out.push(a));
    a.send(12);
    b.send(6);
    kill();
    assertEquals(["1 5", "12 5", "12 6"], out);
});

test("liftGlitch", () => {
    const a = new CellSink(1),
        a3 = a.map(x => x * 3),
        a5 = a.map(x => x * 5),
        b = a3.lift(a5, (x, y) => x + " " + y),
        out : string[] = [],
        kill = b.listen(x => out.push(x));
    a.send(2);
    kill();
    assertEquals(["3 5", "6 10"], out);
});

test("liftFromSimultaneous", () => {
    const t = Transaction.run(() => {
        const b1 = new CellSink(3),
            b2 = new CellSink(5);
        b2.send(7);
        return new Tuple2(b1, b2);
    });
    const b1 = t.a,
        b2 = t.b,
        out : number[] = [],
        kill = b1.lift(b2, (x, y) => x + y)
          .listen(a => out.push(a));
    kill();
    assertEquals([10], out);
});

test("holdIsDelayed", () => {
    const s = new StreamSink<number>(),
        h = s.hold(0),
        sPair = s.snapshot(h, (a, b) => a + " " + b),
        out : string[] = [],
        kill = sPair.listen(a => out.push(a));
    s.send(2);
    s.send(3);
    kill();
    assertEquals(["2 0", "3 2"], out);
});

class SC {
    constructor(a : string, b : string, sw : string) {
        this.a = a;
        this.b = b;
        this.sw = sw;
    }

    a : string;
    b : string;
    sw : string;
}

test("switchC", () => {
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
        out : string[] = [],
        kill = co.listen(c => out.push(c));
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
    assertEquals(["A", "B", "c", "d", "E", "F", "f", "F", "g", "H", "I"], out);
});

class SS {
    constructor(a : string, b : string, sw : string) {
        this.a = a;
        this.b = b;
        this.sw = sw;
    }

    a : string;
    b : string;
    sw : string;
}

test("switchS", () => {
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
          out : string[] = [],
          kill = so.listen(x => out.push(x));
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
    assertEquals(["A", "B", "C", "d", "e", "F", "G", "h", "I"], out);
});

class SS2 {
    s : StreamSink<number> = new StreamSink<number>();
}

test("switchSSimultaneous", () => {
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
          so = Cell.switchS(css.map(lambda1((b : SS2) => b.s, [ss1.s, ss2.s, ss3.s, ss4.s]))),
          out : number[] = [],
          kill = so.listen(c => out.push(c));
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
    assertEquals([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], out);
});

test("loopCell", () => {
    const sa = new StreamSink<number>(),
        sum_out = Transaction.run(() => {
                const sum = new CellLoop<number>(),
                      sum_out_ = sa.snapshot(sum, (x, y) => x + y).hold(0);
                sum.loop(sum_out_);
                return sum_out_;
            }),
        out : number[] = [],
        kill = sum_out.listen(a => out.push(a));
    sa.send(2);
    sa.send(3);
    sa.send(1);
    kill();
    assertEquals([0, 2, 5, 6], out);
    assertEquals(6, sum_out.sample());
});

test("accum", () => {
    const sa = new StreamSink<number>(),
        out : number[] = [],
        sum = sa.accum(100, (a, s) => a + s),
        kill = sum.listen(a => out.push(a));
    sa.send(5);
    sa.send(7);
    sa.send(1);
    sa.send(2);
    sa.send(3);
    kill();
    assertEquals([100, 105, 112, 113, 115, 118], out);
});

test("loopValueSnapshot", () => {
    const out : string[] = [],
        kill = Transaction.run(() => {
            const a = new Cell("lettuce"),
               b = new CellLoop<string>(),
               eSnap = Operational.value(a).snapshot(b, (aa, bb) => aa + " " + bb);
            b.loop(new Cell("cheese"));
            return eSnap.listen(x => out.push(x));
        });
    kill();
    assertEquals(["lettuce cheese"], out);
});

test("loopValueHold", () => {
    const out : string[] = [],
        value = Transaction.run(() => {
            const a = new CellLoop<string>(),
                value_ = Operational.value(a).hold("onion");
            a.loop(new Cell("cheese"));
            return value_;
        }),
        sTick = new StreamSink<Unit>(),
        kill = sTick.snapshot1(value).listen(x => out.push(x));
    sTick.send(Unit.UNIT);
    kill();
    assertEquals(["cheese"], out);
});

test("liftLoop", () => {
    const out : string[] = [],
        b = new CellSink("kettle"),
        c = Transaction.run(() => {
            const a = new CellLoop<string>(),
                c_ = a.lift(b, (aa, bb) => aa + " " + bb);
            a.loop(new Cell("tea"));
            return c_;
        }),
        kill = c.listen(x => out.push(x));
    b.send("caddy");
    kill();
    assertEquals(["tea kettle", "tea caddy"], out);
});

const name = "fromAsync",
     action = IOAction.fromAsync((a : number, result : (b : number) => void) => {
            setTimeout(() => {
                    result(a + 1);
                }, 1);
        }),
     out : number[] = [],
     sa = new StreamSink<number>(),
     kill = action(sa).listen(b => out.push(b));
sa.send(5);
assertEquals([], out);
setTimeout(() => {
        sa.send(9);
        assertEquals([6], out);
        setTimeout(() => {
            assertEquals([6, 10], out);
            kill();
            checkMemory();
            console.log(name + " - PASS");
        }, 100);
    }, 100);
*/
}
