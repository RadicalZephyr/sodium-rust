//! A cell holds its value whether or not anything is listening to it.
//!
//! These pin the behaviour the denotational semantics requires, where
//! `steps (Hold a s t0)` filters the source's occurrences by time alone and no
//! listener, subscription or refcount appears in the model. Java `nz.sodium`
//! agrees. `sodium-typescript` currently fails every one of these, so they are
//! also the specification a fix there should satisfy.

use crate::tests::assert_memory_freed;
use crate::{Cell, SodiumCtx};

#[test]
fn cell_sink_keeps_value_sent_while_unlistened() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let c = sodium_ctx.new_cell_sink(0);
        sodium_ctx.transaction(|| c.send(42));
        assert_eq!(42, c.cell().sample());
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn cell_sink_keeps_that_value_when_later_listened_to() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let c = sodium_ctx.new_cell_sink(0);
        sodium_ctx.transaction(|| c.send(42));
        let l = c.cell().listen(|_: &i32| {});
        assert_eq!(42, c.cell().sample());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn hold_keeps_update_while_its_output_is_unlistened() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let s = sodium_ctx.new_stream_sink::<i32>();
        let c = s.stream().hold(0);
        sodium_ctx.transaction(|| s.send(42));
        assert_eq!(42, c.sample());
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn mapped_cell_stays_current_while_unlistened() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let c = sodium_ctx.new_cell_sink(0);
        let keep_source = c.cell().listen(|_: &i32| {});
        let m = c.cell().map(|x: &i32| x + 1);
        sodium_ctx.transaction(|| c.send(42));
        assert_eq!(43, m.sample());
        keep_source.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn unlistened_lift_does_not_glitch() {
    // Both sources live, both sent in one transaction, so there is no
    // intermediate state the result may take. sodium-typescript yields 21 here,
    // which is neither the old value (3) nor the new one (30).
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let a = sodium_ctx.new_cell_sink(1);
        let b = sodium_ctx.new_cell_sink(2);
        let ka = a.cell().listen(|_: &i32| {});
        let kb = b.cell().listen(|_: &i32| {});
        let l = a.cell().lift2(&b.cell(), |x: &i32, y: &i32| x + y);
        sodium_ctx.transaction(|| {
            a.send(10);
            b.send(20);
        });
        assert_eq!(30, l.sample());
        ka.unlisten();
        kb.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn switch_c_binds_a_cell_sent_before_anything_listens() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let sw = sodium_ctx.new_cell_sink(sodium_ctx.new_cell(0));
        let out = Cell::switch_c(&sw.cell());
        let real = sodium_ctx.new_cell_sink(42);
        sodium_ctx.transaction(|| sw.send(real.cell()));
        let l = out.listen(|_: &i32| {});
        assert_eq!(42, out.sample());
        l.unlisten();
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn switch_s_binds_a_stream_sent_before_anything_listens() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let sw = sodium_ctx.new_cell_sink(sodium_ctx.new_stream::<i32>());
        let out = Cell::switch_s(&sw.cell());
        let src = sodium_ctx.new_stream_sink::<i32>();
        sodium_ctx.transaction(|| sw.send(src.stream()));
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let l = {
            let seen = seen.clone();
            out.listen(move |a: &i32| seen.lock().unwrap().push(*a))
        };
        src.send(1);
        l.unlisten();
        assert_eq!(vec![1], *seen.lock().unwrap());
    }
    assert_memory_freed(sodium_ctx);
}

#[test]
fn send_in_the_construction_transaction_is_kept() {
    let mut sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &mut sodium_ctx;
    {
        let c = sodium_ctx.transaction(|| {
            let c = sodium_ctx.new_cell_sink(0);
            c.send(42);
            c
        });
        assert_eq!(42, c.cell().sample());
    }
    assert_memory_freed(sodium_ctx);
}
