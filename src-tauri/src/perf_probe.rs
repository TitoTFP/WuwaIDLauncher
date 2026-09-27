//! A counting global allocator for the launcher's own hot paths.
//!
//! The Windows matrix in `docs/launcher-performance.md` is the authoritative
//! gate: it samples a real release launcher together with its WebView2 process
//! tree. This probe is the portable half. It needs no desktop, no game and no
//! Windows, so it runs in CI and on any developer machine, and it answers a
//! question the matrix cannot at the size of a single function call: how much
//! memory does *this code path* hand out, and does the next change hand out
//! more than the last one did.
//!
//! What it measures: allocation events, bytes handed out, the peak live
//! payload between allocations, and wall-clock time, across a window in which
//! only the measured body runs.
//!
//! What it deliberately does not measure, because no in-process probe can:
//! WebView2 process accounting, UAC elevation, the self-update restart, and the
//! per-request `media://` protocol cost. Those stay Windows-only, in the matrix.
//!
//! # Budgets, not timings
//!
//! A scenario asserts on allocations and bytes, never on microseconds. Those
//! two are deterministic — the same body allocates the same number of times on
//! every run on every machine — so a budget over them catches a real
//! regression. Wall-clock time is the opposite: it moves with the CPU, the
//! filesystem and whatever else the machine is doing, so a scenario that failed
//! on it would be a coin flip. `measure` still records it and `report` still
//! prints it, for a human to read between two runs, and no assertion anywhere
//! reads it. A flaky performance gate is worse than none, because the next
//! gate gets ignored too.
//!
//! `ARMED` is process-wide, so the scenarios are single-threaded on purpose:
//! `cargo test --lib perf_scenarios -- --test-threads=1 --nocapture`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// The system allocator plus a counter. It is the global allocator of the test
/// binary only: nothing here is compiled into the shipped library.
pub struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Whether the counters are collecting. Unarmed, they are not touched at all,
/// so the price is one relaxed load per allocation and nothing else.
static ARMED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
/// Live payload bytes, relative to the start of the window rather than to the
/// process: a scenario is about what its body does, not about what the test
/// binary was already holding.
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE: AtomicU64 = AtomicU64::new(0);

impl CountingAllocator {
    #[inline]
    fn record_alloc(size: usize) {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
        let live = LIVE.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
        PEAK_LIVE.fetch_max(live, Ordering::Relaxed);
    }

    #[inline]
    fn record_dealloc(size: usize) {
        // A block that was allocated before the window armed is still freed
        // inside it, while `LIVE` counts from zero. A plain `fetch_sub` wraps
        // and panics — inside the allocator, which is not a place a panic can
        // be caught or reasoned about — so the subtraction saturates.
        let _ = LIVE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
            Some(live.saturating_sub(size as u64))
        });
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && ARMED.load(Ordering::Relaxed) {
            Self::record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if ARMED.load(Ordering::Relaxed) {
            Self::record_dealloc(layout.size());
        }
        unsafe { System.dealloc(pointer, layout) }
    }

    /// A reallocation is a free followed by an allocation, and it is counted as
    /// both: that is what a caller sees, and it is what a buffer that keeps
    /// growing in a loop costs.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let grown = unsafe { System.realloc(pointer, layout, new_size) };
        if !grown.is_null() && ARMED.load(Ordering::Relaxed) {
            Self::record_dealloc(layout.size());
            Self::record_alloc(new_size);
        }
        grown
    }
}

/// One armed window, as totals across every iteration in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// Allocation events, armed window total.
    pub allocs: u64,
    /// Bytes handed out, armed window total.
    pub bytes: u64,
    /// The most live payload bytes at any single instant of the window.
    pub peak_live: u64,
    /// Wall clock for the window. Printed for a human, never asserted on.
    pub micros: u64,
}

impl Metrics {
    /// Allocation events per iteration, which is the unit a budget is written in.
    pub fn allocs_per_run(&self, iterations: u64) -> u64 {
        self.allocs / iterations
    }

    /// Bytes per iteration, likewise.
    pub fn bytes_per_run(&self, iterations: u64) -> u64 {
        self.bytes / iterations
    }

    /// Microseconds per iteration. For reading, not for asserting.
    pub fn micros_per_run(&self, iterations: u64) -> u64 {
        self.micros / iterations
    }
}

/// Runs `body` once to warm up, then `iterations` times inside an armed
/// window, and reports what the window cost.
///
/// The warm-up call is not counted on purpose: the first call into any of these
/// paths faults in pages, initialises statics and warms the allocator's free
/// lists, and none of that is the cost the scenario is about.
pub fn measure<F: FnMut()>(iterations: u64, mut body: F) -> Metrics {
    assert!(iterations > 0, "a probe needs at least one iteration");

    body();

    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    LIVE.store(0, Ordering::Relaxed);
    PEAK_LIVE.store(0, Ordering::Relaxed);

    let started = Instant::now();
    ARMED.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        body();
    }
    ARMED.store(false, Ordering::Relaxed);
    // Read the counters after disarming, so the probe never counts itself.
    Metrics {
        allocs: ALLOCS.load(Ordering::Relaxed),
        bytes: BYTES.load(Ordering::Relaxed),
        peak_live: PEAK_LIVE.load(Ordering::Relaxed),
        micros: started.elapsed().as_micros() as u64,
    }
}

/// One `WUL1|` line per scenario, so two runs of the probe can be diffed
/// directly.
pub fn report(label: &str, iterations: u64, metrics: Metrics) {
    println!(
        "WUL1|{label}|iters={iterations}|allocs/run={}|bytes/run={}|peak_live={}|us/run={}",
        metrics.allocs_per_run(iterations),
        metrics.bytes_per_run(iterations),
        metrics.peak_live,
        metrics.micros_per_run(iterations),
    );
}
