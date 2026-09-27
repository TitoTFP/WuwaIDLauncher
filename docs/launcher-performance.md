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

Three of these budgets are written for the shape that exists today and will need
re-cutting when a later batch lands: `idle.monitor_tick` (caching settings
across ticks moves it a long way), `download.validate_archive` (a second
validation pass roughly doubles it), and `install.repak_round_trip` (moving the
extract off the async runtime changes its numbers).
