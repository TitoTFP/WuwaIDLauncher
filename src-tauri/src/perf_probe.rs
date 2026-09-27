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
//! WebView2 process accounting, UAC elevation, the self-update restart, and
//! the per-request `media://` protocol cost. Those stay Windows-only, in the matrix.
//!
//! # Budgets, not timings
//!
//! A scenario asserts on allocations and bytes, never on microseconds. Wall
//! clock moves with the CPU, the filesystem and whatever else the machine is
//! doing, so a scenario that failed on it would be a coin flip. `measure` still
//! records it and `report` still prints it, for a human to read between two
//! runs, and no assertion anywhere reads it. A flaky performance gate is worse
//! than none, because the next gate gets ignored too.
//!
//! # Why the budget is the cheapest iteration, not the average
//!
//! The window is process-wide, and a process is never quiet. Every
//! `tauri::async_runtime::block_on` builds a multi-threaded tokio runtime on
//! first use, and those worker threads allocate on their first scheduling pass
//! — on a thread the measured body is not even running on, at a moment the body
//! does not control. That traffic lands in the armed window whenever it
//! happens to overlap it, which is why the same scenario measured 4,711
//! bytes/run on a warm run and 6,838 on a cold one: the extra 2,127 bytes were
//! `tokio-rt-worker` boot allocations, not launcher work.
//!
//! Averaging cannot fix that, because the contamination is unbounded: it adds
//! a fixed one-off cost divided by the iteration count, so it moves the figure
//! the budget is written against. `measure` therefore keeps every iteration's
//! own cost separately, and a budget is asserted against the **cheapest**
//! iteration in the window — `Metrics::min_allocs_per_run` and
//! `Metrics::min_bytes_per_run`.
//!
//! The cheapest iteration is the right figure because it is the only one that
//! is guaranteed to describe one launch doing nothing but the work under test.
//! A one-off initialisation, wherever it lands, can only ever make an
//! iteration *more* expensive, so it cannot raise the minimum. A real
//! regression is different in kind: a second hash pass, a copied body, a
//! per-tick settings read is paid by *every* iteration, so it raises the
//! minimum by exactly its own cost. The rule is not a way to make budgets
//! easier to pass — it is a way to make them describe the same thing every
//! run.
//!
//! One scenario, `startup.manifest_fetch_body`, asserts on the **median**
//! instead, because its per-iteration cost genuinely varies: the socket
//! decides how a response body arrives, so the cheapest of sixteen fetches
//! samples the luckiest read rather than a typical one. The reason is written
//! out at that scenario.
//!
//! The cheapest iteration is also, by construction, the honest answer to
//! "what does one launch of this path cost". `report` still prints the
//! window's average, because that is the figure `docs/launcher-performance.md`
//! has recorded all along and two runs have to stay diffable.
//!
//! # What the cheapest iteration cannot do
//!
//! The rule discounts a *one-off*. It cannot discount a thread that allocates
//! for the whole window: then every iteration carries some of that traffic, so
//! the minimum carries it too. Measured directly, one continuously allocating
//! background thread over a 320 ms window moves the cheapest of sixteen
//! iterations from 3 allocations to 247,683.
//!
//! So the minimum is not a defence against a busy process — it is a reason the
//! scenarios must run in a quiet one. Each scenario is `#[ignore]`d for exactly
//! that reason, and the dedicated job runs them on their own with
//! `--test-threads=1 --nocapture`:
//!
//! ```text
//! cargo test --lib perf_scenarios -- --ignored --test-threads=1 --nocapture
//! ```

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

/// One armed window: the window's totals, and what each iteration in it cost
/// on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metrics {
    /// Allocation events, armed window total.
    pub allocs: u64,
    /// Bytes handed out, armed window total.
    pub bytes: u64,
    /// The most live payload bytes at any single instant of the window.
    pub peak_live: u64,
    /// Wall clock for the window. Printed for a human, never asserted on.
    pub micros: u64,
    /// Allocation events per iteration, in the order they ran. A budget is
    /// written against the cheapest of these, never their sum.
    per_iteration_allocs: Vec<u64>,
    /// Bytes per iteration, likewise.
    per_iteration_bytes: Vec<u64>,
}

impl Metrics {
    /// Allocation events per iteration averaged over the window, which is what
    /// `report` prints. Every iteration is included, so a one-off cost from
    /// another thread is spread across them — readable, and not what a budget
    /// should be written against. See the module note.
    pub fn allocs_per_run(&self, iterations: u64) -> u64 {
        self.allocs / iterations
    }

    /// Bytes per iteration averaged over the window, likewise.
    pub fn bytes_per_run(&self, iterations: u64) -> u64 {
        self.bytes / iterations
    }

    /// The cheapest iteration's allocation count: the cost of one launch that
    /// did nothing but the work under test. This is what a budget asserts on.
    pub fn min_allocs_per_run(&self) -> u64 {
        self.per_iteration_allocs
            .iter()
            .copied()
            .min()
            .unwrap_or_default()
    }

    /// The cheapest iteration's bytes, likewise.
    pub fn min_bytes_per_run(&self) -> u64 {
        self.per_iteration_bytes
            .iter()
            .copied()
            .min()
            .unwrap_or_default()
    }

    /// The median iteration's allocation count. Used only where a scenario's
    /// per-iteration cost genuinely varies — see the note on the manifest
    /// fetch — because there the cheapest iteration is the luckiest of the
    /// window rather than a representative launch.
    pub fn median_allocs_per_run(&self) -> u64 {
        median(&self.per_iteration_allocs)
    }

    /// The median iteration's bytes, likewise.
    pub fn median_bytes_per_run(&self) -> u64 {
        median(&self.per_iteration_bytes)
    }

    /// The dearest iteration's allocation count, for reading.
    pub fn max_allocs_per_run(&self) -> u64 {
        self.per_iteration_allocs
            .iter()
            .copied()
            .max()
            .unwrap_or_default()
    }

    /// The dearest iteration's bytes, for reading.
    pub fn max_bytes_per_run(&self) -> u64 {
        self.per_iteration_bytes
            .iter()
            .copied()
            .max()
            .unwrap_or_default()
    }

    /// Microseconds per iteration. For reading, not for asserting.
    pub fn micros_per_run(&self, iterations: u64) -> u64 {
        self.micros / iterations
    }
}

/// The middle of a set of per-iteration costs, rounding up. A minority of
/// contaminated iterations cannot move it: with `n` iterations, at most half
/// can sit on either side of the middle.
fn median(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

/// Runs `body` once to warm up, then `iterations` times inside an armed
/// window, and reports what the window cost.
///
/// The warm-up call is not counted on purpose: the first call into any of these
/// paths faults in pages, initialises statics and warms the allocator's free
/// lists, and none of that is the cost the scenario is about. One call is
/// enough for that, and no more is used to paper over a second entry into a
/// lazily-built dependency: the per-iteration minimum already discounts a
/// one-off cost wherever it lands, so buying determinism with extra warm-up
/// iterations would only make the window slower without making it stricter.
pub fn measure<F: FnMut()>(iterations: u64, mut body: F) -> Metrics {
    assert!(iterations > 0, "a probe needs at least one iteration");

    body();

    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    LIVE.store(0, Ordering::Relaxed);
    PEAK_LIVE.store(0, Ordering::Relaxed);

    // Allocated before the window arms, so recording an iteration's own cost
    // is never itself counted as part of it.
    let mut per_iteration_allocs = Vec::with_capacity(iterations as usize);
    let mut per_iteration_bytes = Vec::with_capacity(iterations as usize);

    let started = Instant::now();
    ARMED.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        let allocs_before = ALLOCS.load(Ordering::Relaxed);
        let bytes_before = BYTES.load(Ordering::Relaxed);
        body();
        per_iteration_allocs.push(ALLOCS.load(Ordering::Relaxed) - allocs_before);
        per_iteration_bytes.push(BYTES.load(Ordering::Relaxed) - bytes_before);
    }
    ARMED.store(false, Ordering::Relaxed);
    // Read the counters after disarming, so the probe never counts itself.
    Metrics {
        allocs: ALLOCS.load(Ordering::Relaxed),
        bytes: BYTES.load(Ordering::Relaxed),
        peak_live: PEAK_LIVE.load(Ordering::Relaxed),
        micros: started.elapsed().as_micros() as u64,
        per_iteration_allocs,
        per_iteration_bytes,
    }
}

/// One `WUL1|` line per scenario, so two runs of the probe can be diffed
/// directly. The `allocs/run` and `bytes/run` columns stay the window's
/// average, which is what `docs/launcher-performance.md` has always recorded;
/// the `min_*` and `median_*` columns are the per-iteration figures budgets
/// are written against, and `max_*` is printed so a spread across a window is
/// visible rather than inferred.
pub fn report(label: &str, iterations: u64, metrics: &Metrics) {
    println!(
        "WUL1|{label}|iters={iterations}|allocs/run={}|bytes/run={}|min_allocs/run={}|min_bytes/run={}|median_allocs/run={}|median_bytes/run={}|max_allocs/run={}|max_bytes/run={}|peak_live={}|us/run={}",
        metrics.allocs_per_run(iterations),
        metrics.bytes_per_run(iterations),
        metrics.min_allocs_per_run(),
        metrics.min_bytes_per_run(),
        metrics.median_allocs_per_run(),
        metrics.median_bytes_per_run(),
        metrics.max_allocs_per_run(),
        metrics.max_bytes_per_run(),
        metrics.peak_live,
        metrics.micros_per_run(iterations),
    );
}
