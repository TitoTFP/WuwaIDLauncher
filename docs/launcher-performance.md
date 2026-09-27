# Launcher Performance Matrix

This document defines the deterministic Windows performance gate for the launcher.
It measures the launcher and WebView2 in two runtime states without requiring a
real Wuthering Waves installation.

## States

### Visible foreground

The release launcher starts with no game process. The test waits for the main
window and an enabled **Mainkan Game** button, then samples the visible steady
state.

### System tray

The runner creates a disposable fixture game directory and copies
`wut-game-lifecycle-fixture.exe` to the expected
`Client/Binaries/Win64/Client-Win64-Shipping.exe` path. The launcher starts that
fixture through its normal UI flow, enters the tray, and samples the hidden
steady state. The fixture lifetime is controlled by
`WUWAID_LAUNCHER_FIXTURE_CHILD_LIFETIME_SECONDS`.

After the tray sample, the fixture process tree is terminated and the runner
measures the time until the launcher window is visible again.

## Measurements and thresholds

The defaults are deliberately broad enough for a shared GitHub-hosted runner,
while still catching a runaway process or a broken lifecycle transition.

| Metric | Gate |
| --- | ---: |
| Launcher CPU, every sample | ≤ 10% |
| WebView2 CPU, every sample | ≤ 10% |
| Launcher private memory | ≤ 512 MB |
| Launcher working set | ≤ 512 MB |
| WebView2 private memory (summed) | ≤ 1024 MB |
| WebView2 working set (summed) | ≤ 1024 MB |
| Launcher private-memory growth per state sample | ≤ 32 MB |
| Launcher read I/O | ≤ 8 MiB/s |
| Launcher write I/O | ≤ 4 MiB/s |
| WebView2 read I/O | ≤ 16 MiB/s |
| WebView2 write I/O | ≤ 8 MiB/s |
| Sampling cadence jitter | ≤ 1000 ms |
| Startup to main window | ≤ 60 s |
| Launcher ready after window | ≤ 120 s |
| Visible-to-tray transition | ≤ 60 s |
| Tray-to-visible restore | ≤ 60 s |

The raw sampler records CPU P95 as well as the enforced maximum. Every sample
must include a WebView2 process and the expected window visibility.

Default sampling is 20 seconds for visible foreground and 30 seconds for tray,
with a two-second interval. The matrix emits enough samples to detect a broken
steady state without making CI unnecessarily long.

## Reproduction

Run on Windows with an interactive desktop, PowerShell 7, WebView2, and the
release launcher build:

```powershell
npm run build
pwsh -NoProfile -File scripts/acceptance/run-windows-fixture-performance.ps1 `
  -OutputRoot .\performance-evidence
```

The runner builds `wut-game-lifecycle-fixture.exe` automatically when it is not
already present. It uses isolated fixture files and an isolated local-app-data
folder, and cleans up the launcher and fixture process tree in `finally`.

Evidence files include:

- `summary.json` — scenario metrics, latency measurements, thresholds, and status;
- `resource-visible.csv` and `resource-tray.csv` — raw per-sample data;
- `resource-visible.log` and `resource-tray.log` — sampler output.

## GitHub Actions

The matrix runs in a dedicated GitHub-hosted `windows-latest` Windows fixture
performance job. The workflow uploads `performance-evidence` as a retained
artifact even when the separate Windows regression job fails. No self-hosted
runner is permitted by the workflow contract.

The hosted run still needs to create a real Tauri window and WebView2 process;
the test must fail rather than silently switching to a headless or synthetic
measurement if that desktop capability is unavailable.

## Boundaries and limitations

- The fixture is not the real game and does not prove real-game rendering,
  patch compatibility, GPU behavior, or game CPU/resource usage.
- Real game acceptance remains manual and is not replaced by this matrix.
- UAC, administrator elevation, self-update restart, and other destructive or
  interactive flows remain manual or existing contract coverage.
- Other operation states such as install, media sync, and download throughput
  are outside this two-state performance matrix.
- Results are runner-specific observations, not a universal hardware
  benchmark. Compare artifacts from equivalent runner images before changing
  thresholds.

## Portable allocation probe

The matrix above is the authoritative gate and is not replaced, weakened, or
second-guessed by anything here. It samples a release launcher and its WebView2
process tree on a real Windows runner. What it cannot do is run on a Linux CI
box or a developer laptop, and it measures whole processes rather than the cost
of one function.

`src-tauri/src/perf_probe.rs` is a counting `GlobalAlloc` plus a window
mechanism, and `src-tauri/src/perf_scenarios.rs` is one measured scenario per
launcher state. Both are `#[cfg(test)]` modules of the library: the allocator
exists only in the test binary, so there is no feature flag, no dependency, and
no `cfg` in a shipped code path. They are in-crate rather than in
`src-tauri/tests/` on purpose — an integration test links the library compiled
*without* `cfg(test)`, so the allocator would be invisible, and the scenarios
could not reach the private seams they measure.

What a scenario reports, per iteration:

| Field | Meaning |
| --- | --- |
| `allocs/run` | Allocation events inside the armed window |
| `bytes/run` | Bytes handed out inside the armed window |
| `peak_live` | Most live payload bytes at any instant of the window |
| `us/run` | Wall clock, printed for a human to read |

What it does **not** measure, and cannot: WebView2 process accounting, UAC
elevation, the self-update restart, and the per-request `media://` protocol
cost. Those stay Windows-only, in the matrix above. A scenario also cannot see
I/O the operating system serves from cache, nor a hashing pass that streams
through a fixed buffer — those show up in the matrix's read-I/O and CPU rows and
in the printed `us/run`, not in an allocation budget.

### Budgets, not timings

Every scenario asserts on `allocs` and `bytes` only. Those are deterministic:
the same body allocates the same number of times, in the same order, on every
run. `us/run` is recorded and printed under `--nocapture` and is deliberately
never asserted on — wall-clock time moves with the CPU, the filesystem, and
whatever else the machine is doing, and a gate that fails at random stops being
believed.

Run them single-threaded and with output captured:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib perf_scenarios -- --test-threads=1 --nocapture
```

`--test-threads=1` is mandatory: the probe's arm flag is process-wide, so two
scenarios at once would count each other's allocations.

### Measured on a Linux x64 debug worktree

Before and after the first optimisation batch, as `allocs/run` / `bytes/run` /
`us/run`:

```text
idle.monitor_tick                 42 / 1590     / 18       ->  42 / 1590     / 18
idle.settings_read_parse          26 / 966      / 11       ->  26 / 966      / 11
startup.cached_media_validate     40 / 5047     / 415614   ->  32 / 4543     / 203598
startup.theme_cache_read          32 / 67368    / 907      ->  25 / 18012    / 1940
startup.manifest_fetch_body      129 / 6500470  / 5160     -> 128 / 5419148  / 5202
update_check.read_game_field_only 44 / 6224     / 12       ->  16 / 2176     / 9
```

The states no change in this batch are bounded too, so a later change to them
is caught rather than assumed: `download.fs_read_32mib` 1 alloc / 33,554,432
bytes, `download.compute_sha256_32mib` 2 / 128, `download.validate_archive`
19 / 108,250, `install.sha256_16mib_once` 2 / 128, and
`install.repak_round_trip` 133 / 49,522.

### The second batch: the idle, update, download and install paths

The states the first batch did not touch are now bounded by what the code
actually does, measured on the same Linux x64 debug worktree, as
`allocs/run` / `bytes/run` / `us/run`:

```text
idle.monitor_tick                 42 / 1590     / 21       ->  3 / 108       / 0
download.validate_archive         19 / 108250   / 10       ->  19 / 108250   / 10
install.repak_round_trip         133 / 49522    / 300      ->  133 / 49522    / 317
download.resume_prefix_digest     --            --        ->  1 / 64         / 327394
```

The budgets re-cut with them, and why:

| Scenario | Was | Now | Justification |
| --- | ---: | ---: | --- |
| `idle.monitor_tick` | 48 allocs | 8 allocs / 1,024 bytes | The tick reads a resolved path out of `RuntimeCoordinator` instead of re-reading and re-parsing `settings.json`, canonicalizing and stat-ing on every tick. Measured 3 allocations and 108 bytes: a lock, a `PathBuf` clone and a `join`. At the unchanged two-second cadence that is ~90 allocations a minute instead of ~1,260. |
| `download.validate_archive` | 24 allocs | 21 allocs | `perform_launcher_update` no longer calls `validate_update_archive` before `extract_zip_update`, which validates as its first statement and rejects with the same strings. One walk is 19 allocations; two walks are 38, which is what this budget now refuses. |
| `install.repak_round_trip` | 200 allocs | 160 allocs | The install transaction body did not change — it moved onto a blocking thread — so the allocation count is the same 133. The budget is re-cut around that measured value because what it now bounds is the work parked on the thread the install blocks on. |

`download.resume_prefix_digest` is new and covers the digest a resumed
download carries: the bytes a resume skips are hashed from the partial file
through the same fixed stack buffer, so the cost is 1 allocation and 64 bytes
whatever the file weighs, and a resumed download produces the same digest as a
single-shot one. Both halves are asserted — the budget, and the digest itself
against the digest of the same bytes hashed in memory.

What this batch removed, in the update path: a second full read of the archive
(`compute_sha256` over the file the download had just written, now carried out
in the write loop that produced those bytes) and a second walk of the same
central directory. What it moved, in the install path: the snapshot, copy and
hash passes of `install_patch_transaction_with_commit` off the async runtime,
so a 10 ms heartbeat scheduled on the same runtime is delayed 6,177 ms by the
pre-fix shape and 11 ms by the fixed one. The probe cannot see that difference
— it counts allocations, and a thread change allocates nothing — so the install
scenario is bounded by its allocations and the thread placement is bounded by
the install's own transaction and rollback tests.

### The probe in CI

The Windows matrix above is still the authoritative gate and is unchanged. The
probe runs beside it in a dedicated `ubuntu-latest` job, because a question
about the cost of one function does not need a desktop, a game or WebView2:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib perf_scenarios -- --test-threads=1 --nocapture
```

The job prints the `WUL1|` lines into its log and uploads the whole run output
as the retained `performance-evidence` artifact, next to the Windows matrix's
own evidence, so two runs of either can be diffed directly. A budget breach
fails the job: the run is piped through `tee` under `set -o pipefail`, so the
test exit code is not swallowed by the pipe.
