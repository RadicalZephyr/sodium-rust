//! Support code for the recorded experiments in `src/bin`.
//!
//! See `README.md` for what this crate is for and how to add to it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use sodium_rust::{Listener, Stream, StreamSink};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

/// A [`GlobalAlloc`] that counts allocations, so an experiment can report a
/// number that is exact rather than a timing that is not.
///
/// A global allocator has to be declared by the binary that uses it, so each
/// experiment that wants one writes:
///
/// ```ignore
/// #[global_allocator]
/// static ALLOC: research::CountingAlloc = research::CountingAlloc::new();
/// ```
///
/// Only deallocation is left uncounted: every question these experiments ask
/// is about how much work one event causes, and a paired free tells us nothing
/// the allocation did not.
pub struct CountingAlloc;

impl CountingAlloc {
    pub const fn new() -> CountingAlloc {
        CountingAlloc
    }
}

impl Default for CountingAlloc {
    fn default() -> CountingAlloc {
        CountingAlloc::new()
    }
}

// SAFETY: every method forwards to `System`, which is a valid allocator, and
// the counters are plain atomics that allocate nothing themselves.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Allocations counted so far. Meaningless unless [`CountingAlloc`] is the
/// binary's `#[global_allocator]`.
pub fn allocs() -> usize {
    ALLOCS.load(Ordering::Relaxed)
}

/// Bytes requested so far, on the same terms as [`allocs`].
pub fn bytes() -> usize {
    BYTES.load(Ordering::Relaxed)
}

/// What one run of a workload cost.
pub struct Cost {
    pub allocs: f64,
    pub bytes: f64,
    pub nanos: f64,
}

/// Run `body` `n` times and report the per-iteration cost.
///
/// Call it on a warmed graph: the first event through a fresh graph pays
/// one-off costs that would otherwise be smeared across the average.
pub fn measure(n: usize, mut body: impl FnMut()) -> Cost {
    let a0 = allocs();
    let b0 = bytes();
    let t = Instant::now();
    for _ in 0..n {
        body();
    }
    let elapsed = t.elapsed();
    Cost {
        allocs: (allocs() - a0) as f64 / n as f64,
        bytes: (bytes() - b0) as f64 / n as f64,
        nanos: elapsed.as_secs_f64() / n as f64 * 1e9,
    }
}

/// Somewhere for fired values to land.
///
/// Every experiment needs a listener body that the optimiser cannot delete and
/// that does not itself allocate. The obvious choice, pushing into a `Vec`,
/// reallocates as it grows; an atomic add does not, which keeps the listener
/// out of the measurement. `adr0001_timed_region` checks that this choice does
/// not change the answer at the sizes we use.
#[derive(Clone, Default)]
pub struct Observer(Arc<AtomicU64>);

impl Observer {
    pub fn new() -> Observer {
        Observer(Arc::new(AtomicU64::new(0)))
    }

    /// Attach to a stream, returning the listener that keeps it alive.
    pub fn listen(&self, s: &Stream<u16>) -> Listener {
        let sink = self.0.clone();
        s.listen(move |v: &u16| {
            sink.fetch_add(u64::from(*v), Ordering::Relaxed);
        })
    }

    /// Read the total, so the graph is observably alive.
    pub fn total(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// `sink -> map -> map -> ... -> map`, `n` deep. `n == 0` is the bare sink.
pub fn chain(sink: &StreamSink<u16>, n: usize) -> Stream<u16> {
    let mut s = sink.stream();
    for _ in 0..n {
        s = s.map(|v: &u16| v.wrapping_add(1));
    }
    s
}

/// Print a row of an experiment's output table.
pub fn row(cells: &[String]) {
    let mut line = String::new();
    for (i, c) in cells.iter().enumerate() {
        if i == 0 {
            line.push_str(&format!("{c:<38}"));
        } else {
            line.push_str(&format!("{c:>16}"));
        }
    }
    println!("{line}");
}

/// Print a heading followed by a column header row.
pub fn heading(title: &str, columns: &[&str]) {
    println!();
    println!("{title}");
    row(&columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>());
}
