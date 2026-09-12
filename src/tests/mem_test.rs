use crate::Cell;
use crate::CellSink;
use crate::SodiumCtx;
use crate::StreamSink;

use crate::tests::init;

// SET RUST_LOG=trace
#[test]
fn log_test() {
    init();
    log::info!("a");
    log::debug!("a");
    log::debug!("b");
}

#[test]
fn mem() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let s = sodium_ctx.new_stream::<i32>();
        let s2 = s.map_to(5);
        let s3 = s2.map_to(3);
        let l = s3.listen_weak(|_: &i32| {});
        l.unlisten();
    }
    sodium_ctx.impl_.collect_cycles();
    let node_count = sodium_ctx.impl_.node_count();
    let node_ref_count = sodium_ctx.impl_.node_ref_count();
    println!();
    println!("node_count {}", node_count);
    println!("node_ref_count {}", node_ref_count);
    assert_eq!(node_count, 0);
}

#[test]
fn map_s_mem() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let ss1: StreamSink<i32> = sodium_ctx.new_stream_sink();
        let s1 = ss1.stream();
        let _s2 = s1.map_to(5);
        let l = _s2.listen_weak(|_: &u32| {});
        println!("l: {:?}", l.impl_);
        l.unlisten();
    }
    sodium_ctx.impl_.collect_cycles();
    let node_count = sodium_ctx.impl_.node_count();
    let node_ref_count = sodium_ctx.impl_.node_ref_count();
    println!();
    println!("node_count {}", node_count);
    println!("node_ref_count {}", node_ref_count);
    assert_eq!(node_count, 0);
}

#[test]
fn map_c_mem() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;
    {
        let cs1: CellSink<i32> = sodium_ctx.new_cell_sink(3);
        let c1 = cs1.cell();
        let _c2 = c1.map(|a: &i32| a + 5);
        //let l = _c2.listen_weak(|_:&i32| {});
        //l.unlisten();
    }
    sodium_ctx.impl_.collect_cycles();
    let node_count = sodium_ctx.impl_.node_count();
    let node_ref_count = sodium_ctx.impl_.node_ref_count();
    println!();
    println!("node_count {}", node_count);
    println!("node_ref_count {}", node_ref_count);
    assert_eq!(node_count, 0);
}

/// `Cell::switch_s` and `Cell::switch_c` used to build a derived cell, via
/// `Cell::map`, whose only job was to unwrap the public newtype into the
/// `impl_` type underneath. The selector is only ever read through `sample`
/// and `updates`, so the unwrapping happens at those points instead, and the
/// intermediate cell is gone.
///
/// These counts are what stops it creeping back. They are not a contract —
/// change them when the switch machinery genuinely changes shape — but a
/// change here should be a change somebody meant to make.
#[test]
fn switch_node_count() {
    init();
    let sodium_ctx = SodiumCtx::new();
    let sodium_ctx = &sodium_ctx;

    let ss: StreamSink<i32> = sodium_ctx.new_stream_sink();
    let which_s = sodium_ctx.new_cell_sink(ss.stream());
    let before = sodium_ctx.impl_.node_count();
    let switched_s = Cell::switch_s(&which_s.cell());
    let switch_s_nodes = sodium_ctx.impl_.node_count() - before;

    let cs: CellSink<i32> = sodium_ctx.new_cell_sink(3);
    let which_c = sodium_ctx.new_cell_sink(cs.cell());
    let before = sodium_ctx.impl_.node_count();
    let switched_c = Cell::switch_c(&which_c.cell());
    let switch_c_nodes = sodium_ctx.impl_.node_count() - before;

    println!();
    println!("switch_s nodes {switch_s_nodes}");
    println!("switch_c nodes {switch_c_nodes}");
    assert_eq!(switch_s_nodes, 2, "Cell::switch_s node count");
    assert_eq!(switch_c_nodes, 3, "Cell::switch_c node count");

    drop(switched_s);
    drop(switched_c);
}
