//! What one uncontended synchronisation operation costs, for every mechanism
//! ADR-0002 considers.
//!
//! The question the record turns on is not whether a permission token is
//! cheaper than a lock -- it plainly is -- but by how much, against a floor of
//! doing no permission check at all. So every mechanism is timed in the same
//! loop against the same floor: a plain field write through `&mut`.
//!
//! `qcell::LCell` is GhostCell; its `ro`/`rw` are type coercions and should
//! land on the floor. `qcell::QCell` compares owner ids and branches to a
//! panic, so it should land just above it. The gap between those two is the
//! price of keeping `Stream<A>`'s shape, which is the trade ADR-0002 is about.
//!
//! Read the table for its order of magnitude, not its third decimal. The
//! separation that matters -- tens of nanoseconds against fractions of one --
//! is far outside the noise. The differences *among* the floor, `QCell` and
//! `LCell` are not: they are a fraction of a nanosecond on a loop the
//! optimiser is free to pipeline, so their ordering flips between runs. The
//! honest reading is that all three are indistinguishable here, which is
//! itself the finding -- `QCell`'s owner check does not show up.
//!
//! Every container is reached through a `black_box`ed reference so the
//! optimiser cannot prove it local and sink the loop to a single store. Doing
//! that to only some of them is how this binary first reported a floor slower
//! than the cells it was the floor for.
//!
//! ```shell
//! cargo run --release -p adr-research --bin 0002-sync-primitive-cost
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use qcell::{LCell, LCellOwner, QCell, QCellOwner};

/// Enough iterations that the loop dominates the timer's resolution, and few
/// enough that the whole binary runs in a few seconds.
const OPS: u64 = 20_000_000;

fn ns_per_op(elapsed: Duration) -> f64 {
    elapsed.as_nanos() as f64 / OPS as f64
}

fn main() {
    let mut rows: Vec<(&str, f64)> = Vec::new();

    // The floor: the same store the cell loops below perform, reached through
    // a plain `&mut` with no permission check at all. Routing the reference
    // through `black_box` stops the optimiser from proving the location does
    // not escape and sinking the loop to a single store, which would make the
    // floor artificially fast and every other row look worse than it is.
    let mut plain = 0u64;
    let slot: &mut u64 = black_box(&mut plain);
    let t = Instant::now();
    for i in 0..OPS {
        *slot = black_box(i);
    }
    let floor = ns_per_op(t.elapsed());
    rows.push(("plain &mut write (floor)", floor));

    let t = Instant::now();
    for _ in 0..OPS {
        black_box(*slot);
    }
    rows.push(("plain &mut read", ns_per_op(t.elapsed())));

    let rw_owned = RwLock::new(0u64);
    let rw: &RwLock<u64> = black_box(&rw_owned);
    let t = Instant::now();
    for i in 0..OPS {
        *rw.write() = black_box(i);
    }
    rows.push(("parking_lot::RwLock::write", ns_per_op(t.elapsed())));

    let t = Instant::now();
    for _ in 0..OPS {
        black_box(*rw.read());
    }
    rows.push(("parking_lot::RwLock::read", ns_per_op(t.elapsed())));

    let mutex_owned = Mutex::new(0u64);
    let mutex: &Mutex<u64> = black_box(&mutex_owned);
    let t = Instant::now();
    for i in 0..OPS {
        *mutex.lock() = black_box(i);
    }
    rows.push(("parking_lot::Mutex::lock", ns_per_op(t.elapsed())));

    let mut owner = QCellOwner::new();
    let qcell_owned = QCell::new(&owner, 0u64);
    let qcell: &QCell<u64> = black_box(&qcell_owned);
    let t = Instant::now();
    for i in 0..OPS {
        *qcell.rw(&mut owner) = black_box(i);
    }
    rows.push(("qcell::QCell::rw", ns_per_op(t.elapsed())));

    let t = Instant::now();
    for _ in 0..OPS {
        black_box(*qcell.ro(&owner));
    }
    rows.push(("qcell::QCell::ro", ns_per_op(t.elapsed())));

    LCellOwner::scope(|mut owner| {
        let lcell_owned = LCell::new(0u64);
        let lcell: &LCell<u64> = black_box(&lcell_owned);
        let t = Instant::now();
        for i in 0..OPS {
            *lcell.rw(&mut owner) = black_box(i);
        }
        rows.push(("qcell::LCell::rw (GhostCell)", ns_per_op(t.elapsed())));

        let t = Instant::now();
        for _ in 0..OPS {
            black_box(*lcell.ro(&owner));
        }
        rows.push(("qcell::LCell::ro (GhostCell)", ns_per_op(t.elapsed())));
    });

    println!("{OPS} iterations each\n");
    println!(
        "{:<30} {:>10} {:>14}",
        "mechanism", "ns per op", "over floor"
    );
    for (name, ns) in &rows {
        println!("{:<30} {:>10.3} {:>+14.3}", name, ns, ns - floor);
    }
}
